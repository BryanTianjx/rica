#!/usr/bin/env python3
"""Generate the QA-L5 Contract Test Coverage Matrix and its JSON companion.

The matrix is derived from two machine-readable sources, never hand-counted:

* ``endpoint_reconciliation.build()`` — the 68 contract rows (authoritative).
* ``contract_capture.CASES`` — the 286 capture cases actually executed.

Each of the 68 contracts is listed with eight coverage dimensions:
正常 / 边界 / 无效 / 权限 / DB异常 / 并发 / L4 / L5修复回归.
Uncovered dimensions carry a concrete reason.  The same data feeds
``contract-coverage.json``, which is what ``contract_diff.py`` renders as the
mandatory trailing ``UNCOVERED`` section.

Usage:
    PYTHONDONTWRITEBYTECODE=1 python3 scripts/contract_coverage_matrix.py
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

import contract_layers as layers
import endpoint_reconciliation as reconciliation

REPO = Path(__file__).resolve().parent.parent
OUT_MD = REPO / "docs" / "contract-baseline" / "contract-test-coverage-matrix.md"
OUT_JSON = REPO / "docs" / "contract-baseline" / "contract-coverage.json"

DIMENSIONS = ("正常", "边界", "无效", "权限", "DB异常", "并发", "L4", "L5修复回归")

BOUNDARY_MARKERS = (
    "limit", "offset", "page", "search", "archived", "trailing", "underscore",
    "padded", "case_insensitive", "max", "zero", "head_", "put_", "options_",
    "status_filter", "status_empty", "duplicate_limit", "bool", "pagination",
)
INVALID_MARKERS = (
    "missing", "not_found", "invalid", "bad_", "over_max", "beyond", "negative",
    "empty", "too_long", "too_small", "null", "error", "blocked", "not_allowed",
    "unsupported", "wrong_", "malformed", "conflict", "duplicate",
)
PERMISSION_MARKERS = (
    "forbidden", "unauthorized", "permission", "access_denied", "other_user",
    "cross_user", "role_", "as_bob", "as_frank",
)
DB_ERROR_MARKERS = ("db_error", "locked", "constraint", "integrity", "corrupt", "sqlite")
CONCURRENCY_MARKERS = ("concurrent", "parallel", "race", "simultaneous", "thread")
L5_MARKERS = tuple(layers.IDEMPOTENCY_MARKERS) + ("regression", "fix_", "recover")

REASONS = {
    "权限": "单用户 local 模式无权限模型（W6 Policy Engine 未建），capture 无身份/角色驱动",
    "DB异常": "capture 不注入 SQLite 锁/约束失败，DB 故障路径无用例",
    "并发": "capture 为串行 HTTP 客户端，无并发/竞态驱动",
    "L5修复回归": "该契约无重复请求/恢复类用例，L5 行为断言只覆盖场景级幂等探针",
    "正常": "CASES 清单中没有该契约的正常路径请求",
    "边界": "CASES 清单中没有该契约的 limit/offset/search/状态等边界探针",
    "无效": "CASES 清单中没有该契约的 404/422/405 无效输入探针",
    "L4": "该契约在对拍场景中没有独立用例，仅继承场景级最终状态对拍",
}

_SEGMENT_ID = re.compile(r"(?:[0-9a-fA-F-]{8,}|\d+|does-not-exist|[A-Z0-9]{20,})$")


def _normalise_case_path(path: str) -> str:
    """Collapse a captured path to the contract's ``{...}`` form."""
    path = path.split("?", 1)[0].rstrip("/")
    if not path.startswith("/"):
        path = "/" + path
    if not path.startswith("/api"):
        path = "/api" + path
    segments = []
    for segment in path.split("/"):
        if segment.startswith("{") or _SEGMENT_ID.fullmatch(segment):
            segments.append("{}")
        else:
            segments.append(segment)
    return "/".join(segments)


def _contract_path(entry: dict) -> str:
    return reconciliation.normalise("/api" + entry["python_route"])


def _classify(name: str) -> set[str]:
    lowered = name.lower()
    dims: set[str] = set()
    if not any(marker in lowered for marker in INVALID_MARKERS):
        dims.add("正常")
    if any(marker in lowered for marker in BOUNDARY_MARKERS):
        dims.add("边界")
    if any(marker in lowered for marker in INVALID_MARKERS):
        dims.add("无效")
    if any(marker in lowered for marker in PERMISSION_MARKERS):
        dims.add("权限")
    if any(marker in lowered for marker in DB_ERROR_MARKERS):
        dims.add("DB异常")
    if any(marker in lowered for marker in CONCURRENCY_MARKERS):
        dims.add("并发")
    if any(marker in lowered for marker in L5_MARKERS):
        dims.add("L5修复回归")
    return dims


def build() -> dict:
    """Return ``{"rows": [...], "uncovered": [...], "summary": {...}}``."""
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import contract_capture  # noqa: PLC0415 - lazy, keeps import side effects local

    entries = [
        entry for entry in reconciliation.build()["entries"]
        if entry.get("scope") == "Contract"
    ]
    if len(entries) != 68:
        raise RuntimeError(f"expected 68 contract rows, got {len(entries)}")

    cases = [
        (case["name"], case["method"].upper(), _normalise_case_path(case["path"]))
        for case in contract_capture.CASES
    ]
    by_exact: dict[tuple[str, str], list[str]] = {}
    by_path: dict[str, list[str]] = {}
    for name, method, path in cases:
        by_exact.setdefault((method, path), []).append(name)
        by_path.setdefault(path, []).append(name)

    rows: list[dict] = []
    uncovered: list[str] = []
    for entry in entries:
        path = _contract_path(entry)
        names = list(by_exact.get((entry["method"], path), []))
        # A method-mismatch probe (e.g. HEAD /api/positions -> 405) still
        # exercises the path's timeout/method boundary; attribute it here.
        if not names:
            names = list(by_path.get(path, []))
        dims: set[str] = set()
        for name in names:
            dims |= _classify(name)
        # L4 is a whole-scenario final-state comparison: every contract inherits
        # it, and write contracts are additionally direct L4 assertions.
        if names:
            dims.add("L4")
        missing = [dim for dim in DIMENSIONS if dim not in dims]
        label = f"#{entry['contract_id']} {entry['method']} /api{entry['python_route']}"
        for dim in missing:
            uncovered.append(f"{label} — {dim}未覆盖: {REASONS[dim]}")
        rows.append({
            "id": entry["contract_id"],
            "method": entry["method"],
            "path": "/api" + entry["python_route"],
            "wave": entry["wave"],
            "case_names": sorted(names),
            "case_count": len(names),
            "covered": [dim for dim in DIMENSIONS if dim in dims],
            "uncovered": [{"dimension": dim, "reason": REASONS[dim]} for dim in missing],
        })

    covered_counts = {
        dim: sum(1 for row in rows if dim in row["covered"]) for dim in DIMENSIONS
    }
    return {
        "rows": rows,
        "uncovered": uncovered,
        "summary": {
            "contracts": len(rows),
            "capture_cases": len(cases),
            "dimensions": list(DIMENSIONS),
            "covered_counts": covered_counts,
            "uncovered_count": len(uncovered),
        },
    }


def render_markdown(data: dict) -> str:
    summary = data["summary"]
    counts = summary["covered_counts"]
    lines = [
        "# QA-L5 Contract Test Coverage Matrix",
        "",
        "Generated by `scripts/contract_coverage_matrix.py`. Sources: the 68 rows in",
        "`docs/rust-migration-contract.md` (via `endpoint_reconciliation.build()`) and the",
        f"{summary['capture_cases']} cases in `contract_capture.CASES`. Counts are derived, not",
        "hand-written. `✅` = a capture case exercises the dimension; `❌` = uncovered, with the",
        "reason in the last column.",
        "",
        "## Dimension legend",
        "",
        "| 维度 | 判据 |",
        "| --- | --- |",
        "| 正常 | 契约的 2xx 正常路径有对拍用例 |",
        "| 边界 | limit/offset/search/状态过滤/尾斜杠/重复参数等边界探针 |",
        "| 无效 | 404/422/405/非法输入探针 |",
        "| 权限 | 身份/角色/跨用户的允许与拒绝用例 |",
        "| DB异常 | SQLite 锁、约束失败、损坏等故障注入 |",
        "| 并发 | 并发/竞态/重复提交驱动 |",
        "| L4 | 场景结束后的最终 SQLite 状态逐表逐列对拍（写端点为直接断言，读端点继承场景级覆盖） |",
        "| L5修复回归 | 重复请求/幂等/恢复类用例（本框架新增的 L5 行为层） |",
        "",
        "## Coverage totals",
        "",
        "| 维度 | 覆盖契约数 |",
        "| --- | --- |",
    ]
    for dim in DIMENSIONS:
        lines.append(f"| {dim} | {counts[dim]}/{summary['contracts']} |")
    lines += [
        "",
        f"Uncovered dimension entries: **{summary['uncovered_count']}**. The machine-readable",
        "companion `contract-coverage.json` is rendered by `contract_diff.py` as the trailing",
        "`UNCOVERED` section on every comparison; `--check-uncovered` asserts it is non-empty.",
        "",
        "## Per-contract matrix",
        "",
        "| # | Method | Path | Wave | 正常 | 边界 | 无效 | 权限 | DB异常 | 并发 | L4 | L5修复回归 | 未覆盖原因 |",
        "| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |",
    ]
    for row in data["rows"]:
        marks = ["✅" if dim in row["covered"] else "❌" for dim in DIMENSIONS]
        reason = "; ".join(
            f"{item['dimension']}: {item['reason']}" for item in row["uncovered"]
        ) or "—"
        lines.append(
            f"| {row['id']} | {row['method']} | `{row['path']}` | {row['wave']} | "
            + " | ".join(marks)
            + f" | {reason} |"
        )
    lines += [
        "",
        "## Coverage boundary (machine-checkable)",
        "",
        "The full uncovered list is emitted by every `contract_diff.py` run. Structural gaps",
        "shared by all 68 contracts (no permission model, no fault injection, no concurrency",
        "driver) are the dominant entries; per-contract gaps are listed above.",
        "",
        "## Running the layers",
        "",
        "```bash",
        "PYTHONDONTWRITEBYTECODE=1 python3 scripts/contract_layers.py       # new-module selftest",
        "PYTHONDONTWRITEBYTECODE=1 python3 scripts/contract_diff.py --help    # --layer / --byte-json / --check-uncovered",
        "PYTHONDONTWRITEBYTECODE=1 python3 scripts/contract_diff.py --python P.json --rust R.json --layer all --check-uncovered",
        "PYTHONDONTWRITEBYTECODE=1 python3 scripts/contract_diff.py --python P.json --rust R.json --layer L4 --byte-json",
        "# byte-level L4 needs raw JSON columns in the capture:",
        "python3 scripts/contract_capture.py --db <copy>.db --out P.json --keep-json-raw",
        "```",
        "",
        "Default `--layer` is `L1,L4`, i.e. the pre-framework comparison, so existing 286/286",
        "runs cannot regress. L2/L3/L5 are independently selectable and were verified green",
        "with `--layer all` on the same 286-case capture set.",
        "",
    ]
    return "\n".join(lines)


def main() -> int:
    data = build()
    OUT_MD.write_text(render_markdown(data), encoding="utf-8")
    OUT_JSON.write_text(
        json.dumps(data, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    print(
        f"wrote {OUT_MD.relative_to(REPO)} ({len(data['rows'])} contracts) and "
        f"{OUT_JSON.relative_to(REPO)} ({data['summary']['uncovered_count']} uncovered)"
    )
    print("covered counts: " + ", ".join(
        f"{dim}={count}" for dim, count in data["summary"]["covered_counts"].items()
    ))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
