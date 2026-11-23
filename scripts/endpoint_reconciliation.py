#!/usr/bin/env python3
"""Endpoint Reconciliation Matrix — the single source of truth for endpoint counts.

Review finding P0-01: the plan reported "46 implemented / 68 contract / 23
missing", and 46 + 23 = 69. The cause was a counting error, not a coverage
error: `GET /health` is NOT one of the 68 business contract endpoints, so the
implemented count was inflated by one.

SPE-QA-002 §5.2 / §12.3 finding (2026-11-23, SPEC-QA card): the old version of
this script set ``verification = "Verified" if key in rust_api else ...`` and
printed ``verification_coverage = implemented/total``. That derives a *test*
conclusion from *route existence* and produced the false 68/68 "Verified"
matrix. Per SPE-QA-002 §5.2 ("未测试不能标记为通过", "已实现不代表符合标准")
and §12.3 ("不要仅凭测试通过率得出 VERIFIED 结论"), implementation and
verification are now separate dimensions:

  * ``implementation`` — route / code exists in the Rust router;
  * ``verification``   — a registered test case actually covers the endpoint
                         AND its execution result is attested as PASS.

Evidence sources (no inference from routes):
  * test-case coverage: docs/contract-baseline/contract-coverage.json
    (produced by scripts/contract_capture.py -> contract_diff.py CASES roster)
  * execution attestation: optional --execution-evidence JSON with per-case
    PASS/FAIL results. With no attestation the honest state is
    COVERED_NOT_ATTESTED — never "Verified" just because a case name exists.

Usage:
    python3 scripts/endpoint_reconciliation.py            # human table
    python3 scripts/endpoint_reconciliation.py --json     # machine readable
    python3 scripts/endpoint_reconciliation.py --selftest # counterexample probe
    python3 scripts/endpoint_reconciliation.py --out docs/migration/reconciliation/endpoint-matrix-r1.md
    python3 scripts/endpoint_reconciliation.py --execution-evidence <results.json> --json
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
ROUTER = REPO / "crates/ica-api/src/routes.rs"
CONTRACT = REPO / "docs/rust-migration-contract.md"
COVERAGE_JSON = REPO / "docs/contract-baseline/contract-coverage.json"

# Routes that are deliberately outside the 68-route business contract.
NON_CONTRACT_ROUTES = {
    ("GET", "/health"),
}

# Modules whose endpoints belong to waves that are not implemented yet.
WAVE_BY_MODULE = {
    "ai": "W5",
    "settings": "W5",
    "audit-events": "W5",
    "backups": "W5",
    "auth": "W6",
    "shares": "W6",
}

# Verification states (SPE-QA-002 §5.2 / §12.3 vocabulary, project-local).
V_VERIFIED = "VERIFIED"                    # case exists + execution evidence PASS
V_COVERED = "COVERED_NOT_ATTESTED"          # case exists, no PASS evidence registered
V_NOT_TESTED = "NOT_TESTED"                 # no case covers it / no implementation


def normalise(path: str) -> str:
    """Put a path in one canonical form.

    The contract table writes paths without the `/api` prefix; the router
    writes them with it. Parameter names differ too ({id} vs
    {candidate_id}), so those are collapsed to `{}` for matching only — the
    original text is kept for display.
    """
    path = path.strip()
    if not path.startswith("/api"):
        path = "/api" + path
    return re.sub(r"\{[^}]+\}", "{}", path)


def read_rust_routes() -> list[tuple[str, str]]:
    """Extract every (method, path) the router actually registers."""
    src = ROUTER.read_text(encoding="utf-8")
    routes: list[tuple[str, str]] = []
    index = 0
    while True:
        start = src.find(".route(", index)
        if start < 0:
            break
        cursor = start + len(".route(")
        depth = 1
        while depth:
            if src[cursor] == "(":
                depth += 1
            elif src[cursor] == ")":
                depth -= 1
            cursor += 1
        body = src[start + 7:cursor - 1]
        path_match = re.search(r'"([^"]+)"', body)
        if path_match:
            path = path_match.group(1)
            for method in re.finditer(r"\b(get|post|put|patch|delete)\s*\(", body):
                routes.append((method.group(1).upper(), path))
        index = cursor
    return routes


def read_contract_rows() -> list[dict]:
    """Extract every numbered row of the contract endpoint table."""
    text = CONTRACT.read_text(encoding="utf-8")
    pattern = re.compile(
        r"^\| *(\d+) *\| *(GET|POST|PUT|PATCH|DELETE) *\| *`([^`]+)` *\| *`([^`]+)` *\| *([^|]*)\|",
        re.M,
    )
    rows = []
    for number, method, path, handler, module in pattern.findall(text):
        rows.append({
            "id": int(number),
            "method": method,
            "path": path,
            "handler": handler,
            "module": module.strip(),
        })
    return rows


def wave_of(path: str, module: str) -> str:
    """Which wave an endpoint belongs to, judged by its path prefix."""
    bare = path[1:] if path.startswith("/") else path
    head = bare.split("/")[0]
    return WAVE_BY_MODULE.get(head, "?")


# Routes the project moved OUT of the 68-endpoint contract by explicit ruling
# (ADR-0003 D-2, 2026-11-19): they are implemented in the Rust router (or owned
# by their own wave: W7 for confirm-candidate, AI-EXT for the ai/models CRUD)
# but are NOT part of the migration contract's 68. The gate would otherwise
# count them as "extra" — failing the build for a scope decision the project
# already made. The allowlist is explicit and commented so a route cannot
# quietly talk its way out of the contract.
AUXILIARY_ROUTES = {
    ("POST", "/api/imports/{}/confirm-candidate"),
}


def read_case_coverage(coverage_path: Path) -> dict:
    """Return {(METHOD, /api/normalised/path): coverage row}.

    A missing/unreadable file is NOT silently treated as "all covered": it
    yields an empty mapping, so every endpoint becomes NOT_TESTED. That is the
    fail-closed behaviour SPE-QA-002 §12.3 requires when evidence is absent.
    """
    if not coverage_path.is_file():
        return {}
    payload = json.loads(coverage_path.read_text(encoding="utf-8"))
    return {
        (row["method"].upper(), normalise(row["path"])): row
        for row in payload.get("rows", [])
    }


def read_execution_evidence(path: Path | None) -> dict[str, str]:
    """Return {case_name: PASS|FAIL} from an optional attestation file.

    Accepted shapes::

        {"case_results": {"case_name": "PASS"}}
        {"cases": [{"name": "case_name", "result": "PASS"}, ...]}

    No file => {} => no endpoint can be VERIFIED. Results other than PASS are
    kept so a FAIL case blocks the VERIFIED state instead of inflating it.
    """
    if path is None or not path.is_file():
        return {}
    payload = json.loads(path.read_text(encoding="utf-8"))
    results: dict[str, str] = {}
    mapping = payload.get("case_results")
    if isinstance(mapping, dict):
        results.update({str(k): str(v).upper() for k, v in mapping.items()})
    for case in payload.get("cases", []):
        results[str(case["name"])] = str(case.get("result", "UNKNOWN")).upper()
    return results


def verification_of(implemented: bool, coverage_row: dict | None,
                    evidence: dict[str, str]) -> tuple[str, list[str]]:
    """Derive verification from test evidence only — never from route presence."""
    if not implemented:
        return V_NOT_TESTED, ["endpoint not implemented in Rust router"]
    if not coverage_row or int(coverage_row.get("case_count", 0)) <= 0:
        return V_NOT_TESTED, ["no test case in contract-coverage.json"]
    names = [str(n) for n in coverage_row.get("case_names", [])]
    if not names:
        return V_NOT_TESTED, ["coverage row lists no case_names"]
    if not evidence:
        return V_COVERED, ["case(s) registered; no execution PASS evidence supplied"]
    missing = [n for n in names if n not in evidence]
    failed = [n for n in names if n in evidence and evidence[n] != "PASS"]
    if missing:
        return V_COVERED, ["no execution evidence for case(s): " + ", ".join(missing)]
    if failed:
        return V_COVERED, ["non-PASS execution result for case(s): " + ", ".join(failed)]
    return V_VERIFIED, [f"all {len(names)} case(s) attested PASS"]


def build(coverage_path: Path = COVERAGE_JSON,
          evidence: dict[str, str] | None = None,
          evidence_path: Path | None = None) -> dict:
    rust_routes = read_rust_routes()
    contract_rows = read_contract_rows()
    coverage = read_case_coverage(coverage_path)
    evidence = evidence if evidence is not None else read_execution_evidence(evidence_path)

    rust_api = {(m, normalise(p)) for m, p in rust_routes if p.startswith("/api")}
    rust_non_contract = {(m, p) for m, p in rust_routes if not p.startswith("/api")}
    contract_set = {(r["method"], normalise(r["path"])) for r in contract_rows}

    entries: list[dict] = []
    uncovered: list[dict] = []
    for row in contract_rows:
        key = (row["method"], normalise(row["path"]))
        implemented = key in rust_api
        coverage_row = coverage.get(key)
        verification, reasons = verification_of(implemented, coverage_row, evidence)
        entries.append({
            "contract_id": row["id"],
            "python_route": row["path"],
            "method": row["method"],
            "module": row["module"],
            "wave": wave_of(row["path"], row["module"]),
            "implementation": "Implemented" if implemented else "Missing",
            "verification": verification,
            "verification_reason": "; ".join(reasons),
            "case_count": int(coverage_row.get("case_count", 0)) if coverage_row else 0,
            "case_names": list(coverage_row.get("case_names", [])) if coverage_row else [],
            "scope": "Contract",
            "baseline_sha": "d39e321",
        })
        if verification == V_NOT_TESTED:
            uncovered.append({
                "contract_id": row["id"],
                "method": row["method"],
                "path": row["path"],
                "implementation": "Implemented" if implemented else "Missing",
                "reason": "; ".join(reasons),
            })

    auxiliary = 0
    for method, path in sorted(rust_api - contract_set):
        is_auxiliary = (method, path) in AUXILIARY_ROUTES
        if is_auxiliary:
            auxiliary += 1
        # Router-only routes are outside the 68-row contract and have no
        # contract-coverage.json row: they cannot be VERIFIED either.
        entries.append({
            "contract_id": None,
            "python_route": path,
            "method": method,
            "module": "(router only)",
            "wave": "W7" if is_auxiliary else "?",
            "implementation": "Auxiliary" if is_auxiliary else "Extra",
            "verification": V_NOT_TESTED,
            "verification_reason": "outside contract; no test-case coverage row",
            "case_count": 0,
            "case_names": [],
            "scope": "Auxiliary (D-2)" if is_auxiliary else "Auxiliary",
            "baseline_sha": "9ed173c",
        })

    implemented = sum(1 for e in entries if e["implementation"] == "Implemented")
    missing = sum(1 for e in entries if e["implementation"] == "Missing")
    extra = sum(1 for e in entries if e["implementation"] == "Extra")
    total = len(entries)
    contract_total = len(contract_rows)
    verified = sum(1 for e in entries if e["verification"] == V_VERIFIED)
    covered = sum(1 for e in entries if e["verification"] in (V_VERIFIED, V_COVERED))
    not_tested = sum(1 for e in entries if e["verification"] == V_NOT_TESTED)

    return {
        "generated_from": {
            "router": str(ROUTER.relative_to(REPO)),
            "contract": str(CONTRACT.relative_to(REPO)),
            "case_coverage": str(COVERAGE_JSON.relative_to(REPO)) if coverage_path == COVERAGE_JSON else str(coverage_path),
            "execution_evidence": str(evidence_path) if evidence_path else None,
        },
        "counts": {
            "contract_total": contract_total,
            "implemented": implemented,
            "missing": missing,
            "extra_routes": extra,
            "auxiliary_routes": auxiliary,
            "non_contract_routes": len(rust_non_contract),
            "matrix_rows": total,
            "verified": verified,
            "covered_not_attested": covered - verified,
            "not_tested": not_tested,
        },
        "coverage": {
            # Route/implementation dimension: "does code exist?"
            "route_coverage": f"{implemented}/{contract_total}",
            # Backward-compatible aliases for older readers.
            "contract_coverage": f"{implemented}/{contract_total}",
            "implementation_coverage": f"{implemented}/{total}",
            # Verification dimension: "was a case executed and attested PASS?"
            # Deliberately NOT equal to route_coverage by construction.
            "verification_coverage": f"{verified}/{contract_total}",
            "test_case_coverage": f"{covered}/{contract_total}",
            "not_tested": f"{not_tested}/{total}",
        },
        "uncovered_endpoints": uncovered,
        "non_contract": sorted(f"{m} {p}" for m, p in rust_non_contract),
        "entries": entries,
    }


def render_markdown(data: dict) -> str:
    counts = data["counts"]
    lines = [
        "# Endpoint Reconciliation Matrix R1.1",
        "",
        "**生成方式：** `python3 scripts/endpoint_reconciliation.py --out <file>`",
        "所有统计数字从本矩阵生成，不再分别用 grep、契约文档与 Work Item 手工统计。",
        "",
        "> **审计修正（P0-01）**：此前总方案写「已实现 46 条」，是把 `GET /health` "
        "计入了——它不属于 68 条业务契约。正确数字是 **45 / 68**。",
        "> **SPE-QA-002 §5.2 修正**：`verification` 不再由路由存在推导。"
        "实现存在只说明代码在；`VERIFIED` 需要用例覆盖 + 执行 PASS 证据。",
        "",
        "## 计数",
        "",
        "| 指标 | 值 |",
        "| --- | --- |",
        f"| 契约端点总数 | **{counts['contract_total']}** |",
        f"| 已实现 | **{counts['implemented']}** |",
        f"| 未实现 | **{counts['missing']}** |",
        f"| 契约外路由（Rust 有、契约表无） | {counts['extra_routes']} |",
        f"| 非契约路由（`/health` 等） | {counts['non_contract_routes']} |",
        f"| 矩阵行数 | {counts['matrix_rows']} |",
        f"| VERIFIED / COVERED_NOT_ATTESTED / NOT_TESTED | "
        f"{counts['verified']} / {counts['covered_not_attested']} / {counts['not_tested']} |",
        "",
        f"**校验：{counts['implemented']} + {counts['missing']} = "
        f"{counts['implemented'] + counts['missing']} "
        f"{'✓' if counts['implemented'] + counts['missing'] == counts['contract_total'] else '✗'}**",
        "",
        "两个独立维度（绝不合并）：",
        "",
        f"- Route Coverage（代码/路由存在）: **{data['coverage']['route_coverage']}**",
        f"- Test-case Coverage（有用例登记）: **{data['coverage']['test_case_coverage']}**",
        f"- Verification Coverage（用例登记 + 执行 PASS 证据）: **{data['coverage']['verification_coverage']}**",
        "",
        "> 未提供 `--execution-evidence` 时，有用例也只记为 COVERED_NOT_ATTESTED，"
        "不会被标成 VERIFIED。聚合的 `522 passed` 不能直接当作逐端点验证证据。",
        "",
        "## 未测试端点（反证清单）",
        "",
        "| Contract ID | Method | Path | Implementation | Reason |",
        "| --- | --- | --- | --- | --- |",
    ]
    if data["uncovered_endpoints"]:
        for e in data["uncovered_endpoints"]:
            lines.append(
                f"| {e['contract_id']} | {e['method']} | `{e['path']}` | "
                f"{e['implementation']} | {e['reason']} |"
            )
    else:
        lines.append("| — | — | — | — | 无 |")
    lines += [
        "",
        "## 非契约路由",
        "",
        "这些**不属于** 68 条业务契约，不计入实现率：",
        "",
    ]
    for item in data["non_contract"]:
        lines.append(f"- `{item}`")
    lines += [
        "",
        "## 逐条矩阵",
        "",
        "| Contract ID | Method | Python Route | Wave | Module | Implementation | Verification | Case Count | Scope | Baseline SHA |",
        "| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |",
    ]
    for e in data["entries"]:
        cid = e["contract_id"] if e["contract_id"] is not None else "—"
        lines.append(
            f"| {cid} | {e['method']} | `{e['python_route']}` | {e['wave']} | "
            f"`{e['module']}` | {e['implementation']} | {e['verification']} | "
            f"{e['case_count']} | {e['scope']} | `{e['baseline_sha']}` |"
        )
    lines += [
        "",
        "## 波次分布",
        "",
        "| Wave | 已实现 | 未实现 | VERIFIED |",
        "| --- | --- | --- | --- |",
    ]
    waves: dict[str, list[dict]] = {}
    for e in data["entries"]:
        waves.setdefault(e["wave"], []).append(e)
    for wave in sorted(waves):
        vals = waves[wave]
        done = sum(1 for v in vals if v["implementation"] == "Implemented")
        verified = sum(1 for v in vals if v["verification"] == V_VERIFIED)
        lines.append(f"| {wave} | {done} | {sum(1 for v in vals if v['implementation'] == 'Missing')} | {verified} |")
    lines.append("")
    return "\n".join(lines)


def selftest() -> int:
    """Counterexample: an implemented endpoint with no case must NOT be Verified.

    Injects an in-memory coverage map with one contract row deliberately
    stripped of its cases, then asserts all three separation properties.
    No repository file is modified.
    """
    baseline = build()
    victim = baseline["entries"][0]
    coverage_path = REPO / "docs/contract-baseline/contract-coverage.json"
    rows = json.loads(coverage_path.read_text(encoding="utf-8"))["rows"]
    key = (victim["method"], normalise(victim["python_route"]))
    injected_rows = [r for r in rows if (r["method"].upper(), normalise(r["path"])) != key]
    import tempfile
    with tempfile.TemporaryDirectory(prefix="ica-rust-spec-qa-") as tmp:
        scratch = Path(tmp) / "uncovered-probe.json"
        scratch.write_text(
            json.dumps({"rows": injected_rows, "summary": {"capture_cases": 0}}, ensure_ascii=False),
            encoding="utf-8",
        )
        probe = build(coverage_path=scratch)

    got = next(e for e in probe["entries"] if e["contract_id"] == victim["contract_id"])
    checks = [
        ("injected endpoint still implemented (route exists)",
         got["implementation"] == "Implemented", got["implementation"]),
        ("injected endpoint verification == NOT_TESTED (not Verified)",
         got["verification"] == V_NOT_TESTED, got["verification"]),
        ("injected endpoint listed in uncovered_endpoints",
         any(u["contract_id"] == victim["contract_id"] for u in probe["uncovered_endpoints"]),
         [u["contract_id"] for u in probe["uncovered_endpoints"]]),
        ("route_coverage unchanged by missing test case",
         probe["coverage"]["route_coverage"] == baseline["coverage"]["route_coverage"],
         f"{baseline['coverage']['route_coverage']} -> {probe['coverage']['route_coverage']}"),
        ("verification_coverage strictly below route_coverage",
         probe["coverage"]["verification_coverage"] != probe["coverage"]["route_coverage"],
         f"{probe['coverage']['route_coverage']} vs {probe['coverage']['verification_coverage']}"),
    ]
    print(f"selftest victim: #{victim['contract_id']} {victim['method']} {victim['python_route']}")
    for label, ok, detail in checks:
        print(f"[{'PASS' if ok else 'FAIL'}] {label}: {detail}")
    return 0 if all(ok for _, ok, _ in checks) else 1


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--json", action="store_true")
    parser.add_argument("--out", type=Path)
    parser.add_argument(
        "--execution-evidence", type=Path,
        help="JSON with per-case PASS/FAIL results; without it no endpoint is VERIFIED",
    )
    parser.add_argument(
        "--selftest", action="store_true",
        help="prove that an uncovered endpoint is not marked Verified (in-memory probe)",
    )
    args = parser.parse_args()

    if args.selftest:
        return selftest()

    data = build(evidence_path=args.execution_evidence)
    counts = data["counts"]

    if args.json:
        print(json.dumps(data, ensure_ascii=False, indent=2))
        return 0

    if args.out:
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(render_markdown(data), encoding="utf-8")
        print(f"written: {args.out}")
        print(f"contract={counts['contract_total']} implemented={counts['implemented']} "
              f"missing={counts['missing']} extra={counts['extra_routes']} "
              f"verified={counts['verified']} uncovered={len(data['uncovered_endpoints'])}")
        return 0

    print(render_markdown(data))
    return 0


if __name__ == "__main__":
    sys.exit(main())
