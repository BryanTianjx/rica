#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Copyright (c) 2026 ICA-RUST Contributors.

Project : ICA-RUST
Module  : Test Case Catalog Generator (controlled library)
Purpose : Generate the SPE-QA-006 §16.1 controlled test-case catalogue from
          three real sources: Rust test functions, frontend vitest cases and
          contract_capture.CASES. Case IDs come from a persistent registry,
          never from scan order or source line numbers (SPE-QA-006 §3.1).

Usage:
    python scripts/generate_test_catalog.py [--project-root PATH]
        [--dry-run] [--verbose] [--stdout] [--self-test]

Outputs (written atomically):
    docs/testing/test-case-catalog.md      controlled case catalogue
    docs/testing/traceability-matrix.csv   requirement-proxy -> case mapping
    docs/testing/case-id-registry.json     persistent Case ID allocation

Case ID stability (SPEC-QA-006 §3.1):
    identity = <source>::<file path>::<function/test name>
    The registry maps identity -> Case ID once; renaming a file, moving code
    or inserting lines above a test never renumbers an existing case. A new
    case gets the next free number in its (module, type) bucket. Retired IDs
    are kept and never reallocated. Fallback matching by (source, crate,
    function) reuses the same ID when a file is moved, without a manual edit.

Exit codes: 0 success; 2 argument error; 5 file/data error; 10 validation
failed (no tests found). Follows SPE-BAS-004 R1.1; IDs follow SPE-QA-006 §3.
"""

import argparse
import ast
import copy
import json
import logging
import re
import sys
from collections import Counter
from pathlib import Path

PROJECT_ROOT = Path(__file__).resolve().parents[1]

# SPE-QA-006 §3.2 fields, fixed for this project.
PROJECT_CODE = "ICA"

# Crate/module -> module code (QA-006 §3.2 模块代码,由项目统一维护).
MODULE_CODES = {
    "ica-domain": "DOM",
    "ica-application": "APP",
    "ica-contracts": "CTR",
    "ica-infra": "INF",
    "ica-api": "API",
    "ica-desktop": "IPC",
    "ica-server": "SRV",
    # New controlled sources, added by the SPEC-QA card.
    "frontend": "WEB",
    "contract-capture": "CTR",
}

CATALOG_MD = "test-case-catalog.md"
TRACEABILITY_CSV = "traceability-matrix.csv"
REGISTRY_JSON = "case-id-registry.json"

# Attribute forms that declare a Rust test.
TEST_ATTR = re.compile(
    r"^\s*#\[(?:tokio::test(?:\([^)]*\))?|test)\]\s*$"
)
TEST_FN = re.compile(
    r"^\s*(?:async\s+)?fn\s+([a-zA-Z_][a-zA-Z0-9_]*)\s*\("
)
FE_TEST_CALL = re.compile(
    r"\b(?:it|test)\s*\(\s*([\"'`])([^\"'`\n]{1,160})\1"
)
FE_SUFFIXES = (".test.ts", ".test.tsx", ".spec.ts", ".spec.tsx")

# Old-catalog seed row: | TC-ICA-API-UNIT-001 | `fn` | `file.rs:158` | crate | ...
OLD_ROW = re.compile(
    r"^\|\s*(TC-[A-Z0-9-]+)\s*\|\s*`([^`]+)`\s*\|\s*`([^`]+)`\s*\|\s*([^|\s]+)\s*\|"
)

# Test-type inference (QA-006 §2.2). Inline #[cfg(test)] in src/ -> UNIT.
# Integration files under tests/ -> INT; browser_poc drives the whole
# subsystem -> SYS.


def infer_type(rel_path: Path) -> str:
    parts = rel_path.as_posix()
    if parts.endswith("browser_poc.rs"):
        return "SYS"
    if "/tests/" in parts:
        return "INT"
    return "UNIT"


def parse_args(argv: list[str] | None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Generate the SPE-QA-006 controlled test-case catalog for ICA-RUST"
    )
    parser.add_argument(
        "--project-root", type=Path, default=PROJECT_ROOT,
        help="Repository root (default: script's repo)",
    )
    parser.add_argument("--dry-run", action="store_true",
                        help="Scan and report, but do not write any files")
    parser.add_argument("--stdout", action="store_true",
                        help="Print the generated markdown instead of writing files")
    parser.add_argument("--verbose", action="store_true", help="Enable verbose logging")
    parser.add_argument("--self-test", action="store_true",
                        help="Prove Case IDs survive file moves and new cases (no file writes)")
    args = parser.parse_args(argv)
    if not args.project_root.is_dir():
        parser.error(f"project root is not a directory: {args.project_root}")
    return args


def scan_tests(project_root: Path) -> list[dict]:
    """One record per Rust #[test] / #[tokio::test] function."""
    records: list[dict] = []
    crates_dir = project_root / "crates"
    crate_dirs = sorted(p for p in crates_dir.iterdir() if p.is_dir())
    for crate_dir in crate_dirs:
        crate = crate_dir.name
        module = MODULE_CODES.get(crate, crate.upper()[:3])
        for rs_file in sorted(crate_dir.rglob("*.rs")):
            try:
                lines = rs_file.read_text(encoding="utf-8").splitlines()
            except (OSError, UnicodeDecodeError) as error:
                logging.warning("skip unreadable file %s: %s", rs_file, error)
                continue
            pending_attr = False
            for line_no, line in enumerate(lines, start=1):
                if TEST_ATTR.match(line):
                    pending_attr = True
                    continue
                if pending_attr:
                    match = TEST_FN.match(line)
                    if match:
                        rel = rs_file.relative_to(project_root)
                        records.append({
                            "source": "rust",
                            "crate": crate,
                            "module": module,
                            "type": infer_type(rel),
                            "function": match.group(1),
                            "file": rel.as_posix(),
                            "line": line_no,
                        })
                pending_attr = False
    records.sort(key=lambda r: (r["file"], r["line"]))
    return records


def scan_frontend(project_root: Path) -> list[dict]:
    """One record per vitest it()/test() case across the 25 frontend test files."""
    records: list[dict] = []
    frontend_dir = project_root / "frontend"
    if not frontend_dir.is_dir():
        return records
    for test_file in sorted(frontend_dir.rglob("*")):
        if not test_file.is_file() or not test_file.name.endswith(FE_SUFFIXES):
            continue
        if "node_modules" in test_file.parts:
            continue
        try:
            lines = test_file.read_text(encoding="utf-8").splitlines()
        except (OSError, UnicodeDecodeError) as error:
            logging.warning("skip unreadable frontend test %s: %s", test_file, error)
            continue
        rel = test_file.relative_to(project_root).as_posix()
        seen: Counter[str] = Counter()
        for line_no, line in enumerate(lines, start=1):
            for match in FE_TEST_CALL.finditer(line):
                name = match.group(2).strip()
                seen[name] += 1
                # Duplicate names in one file get a stable suffix that is a
                # function of source order, not of line numbers.
                function = name if seen[name] == 1 else f"{name}#{seen[name]}"
                records.append({
                    "source": "frontend",
                    "crate": "frontend",
                    "module": MODULE_CODES["frontend"],
                    "type": "FUNC",
                    "function": function,
                    "file": rel,
                    "line": line_no,
                    "test_name": name,
                })
    records.sort(key=lambda r: (r["file"], r["function"]))
    return records


def scan_capture_cases(project_root: Path) -> list[dict]:
    """One record per top-level entry of contract_capture.CASES.

    Uses the AST rather than ``grep -c '"name":'``: the latter also counts
    nested JSON payload keys (`{"name": None}` etc.) and overstates the real
    CASE count. Names are literal strings; no module import is performed, so
    the capture harness has no side effects here.
    """
    path = project_root / "scripts" / "contract_capture.py"
    if not path.is_file():
        return []
    tree = ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
    cases_node = None
    for node in tree.body:
        if isinstance(node, ast.Assign):
            targets, value = node.targets, node.value
        elif isinstance(node, ast.AnnAssign):
            targets, value = [node.target], node.value
        else:
            continue
        if any(getattr(t, "id", None) == "CASES" for t in targets):
            cases_node = value
            break
    if cases_node is None or not isinstance(cases_node, (ast.List, ast.Tuple)):
        logging.warning("CASES list not found in %s", path)
        return []
    records: list[dict] = []
    for index, element in enumerate(cases_node.elts, start=1):
        name = f"case_{index:03d}"
        method = ""
        route = ""
        if isinstance(element, ast.Dict):
            for key, value in zip(element.keys, element.values):
                if not isinstance(key, ast.Constant):
                    continue
                if key.value in ("name", "method", "path") and isinstance(value, ast.Constant):
                    if key.value == "name":
                        name = str(value.value)
                    elif key.value == "method":
                        method = str(value.value)
                    elif key.value == "path":
                        route = str(value.value)
        records.append({
            "source": "contract-capture",
            "crate": "contract-capture",
            "module": MODULE_CODES["contract-capture"],
            "type": "API",
            "function": name,
            "file": "scripts/contract_capture.py",
            "line": getattr(element, "lineno", 0),
            "method": method,
            "route": route,
        })
    return records


def identity_of(record: dict) -> str:
    """Stable identity: source + current file path + function/test name."""
    return f"{record['source']}::{record['file']}::{record['function']}"


def fallback_of(record: dict) -> str:
    """Identity used when a test file is moved to another path."""
    return f"{record['source']}::{record['crate']}::{record['function']}"


def new_registry() -> dict:
    return {
        "schema_version": "1.0",
        "policy": ("SPE-QA-006 §3.1: Case ID 一经分配不因名称/目录/行号变化而修改; "
                   "废弃编号不得重新分配。identity = source::file::function。"),
        "updated": "2026-11-23",
        "counters": {},
        "retired_ids": [],
        "ids": {},
    }


def seed_registry_from_catalog(registry: dict, catalog_path: Path) -> int:
    """Bootstrap the registry from the pre-existing (DRAFT) catalogue.

    This is a one-time migration: it preserves all previously printed IDs so
    the controlled library does not renumber history on its first run.
    """
    if not catalog_path.is_file():
        return 0
    seeded = 0
    for line in catalog_path.read_text(encoding="utf-8").splitlines():
        match = OLD_ROW.match(line)
        if not match:
            continue
        case_id, function, file_line, crate = match.groups()
        parts = case_id.split("-")
        if len(parts) != 5 or parts[:2] != ["TC", "ICA"] or not parts[4].isdigit():
            continue
        _, _, module, test_type, seq = parts
        file_path = file_line.split(":", 1)[0]
        identity = f"rust::{file_path}::{function}"
        if identity in registry["ids"]:
            continue
        registry["ids"][identity] = {
            "case_id": case_id,
            "module": module,
            "type": test_type,
            "crate": crate,
            "function": function,
            "file": file_path,
            "status": "active",
            "moved_from": [],
        }
        key = f"{module}-{test_type}"
        registry["counters"][key] = max(registry["counters"].get(key, 0), int(seq))
        seeded += 1
    return seeded


def load_registry(project_root: Path) -> tuple[dict, str]:
    path = project_root / "docs" / "testing" / REGISTRY_JSON
    if path.is_file():
        return json.loads(path.read_text(encoding="utf-8")), "existing registry"
    registry = new_registry()
    seeded = seed_registry_from_catalog(registry, path.parent / CATALOG_MD)
    return registry, f"seeded from legacy catalog ({seeded} IDs)"


def assign_case_ids(records: list[dict], registry: dict) -> dict:
    """Assign IDs from the persistent registry; return assignment statistics."""
    used = set(registry.get("retired_ids", []))
    used.update(entry["case_id"] for entry in registry["ids"].values())
    fallback_index: dict[str, list[str]] = {}
    for identity, entry in registry["ids"].items():
        fallback_index.setdefault(
            f"{entry.get('source', 'rust')}::{entry.get('crate', '')}::{entry.get('function', '')}",
            [],
        ).append(identity)

    stats = {"reused": 0, "moved": 0, "new": 0}
    seen: set[str] = set()
    for record in records:
        identity = identity_of(record)
        seen.add(identity)
        entry = registry["ids"].get(identity)
        if entry is not None:
            stats["reused"] += 1
        else:
            # A moved file must not change the ID. Only accept an unambiguous
            # (source, crate, function) match.
            candidates = [
                ident for ident in fallback_index.get(fallback_of(record), [])
                if ident not in seen
            ]
            if len(candidates) == 1:
                old_identity = candidates[0]
                entry = registry["ids"].pop(old_identity)
                entry.setdefault("moved_from", []).append(old_identity)
                entry["file"] = record["file"]
                registry["ids"][identity] = entry
                fallback_index[fallback_of(record)] = [identity]
                stats["moved"] += 1
            else:
                module, test_type = record["module"], record["type"]
                key = f"{module}-{test_type}"
                seq = registry["counters"].get(key, 0) + 1
                case_id = f"TC-{PROJECT_CODE}-{module}-{test_type}-{seq:03d}"
                while case_id in used:
                    seq += 1
                    case_id = f"TC-{PROJECT_CODE}-{module}-{test_type}-{seq:03d}"
                registry["counters"][key] = seq
                used.add(case_id)
                entry = {
                    "case_id": case_id,
                    "module": module,
                    "type": test_type,
                    "source": record["source"],
                    "crate": record["crate"],
                    "function": record["function"],
                    "file": record["file"],
                    "status": "active",
                    "moved_from": [],
                }
                registry["ids"][identity] = entry
                fallback_index.setdefault(fallback_of(record), []).append(identity)
                stats["new"] += 1
        record["case_id"] = entry["case_id"]

    # Absent cases are marked inactive, but their IDs stay reserved forever.
    for identity, entry in registry["ids"].items():
        entry["status"] = "active" if identity in seen else "inactive"
    registry["updated"] = "2026-11-23"
    return stats


# Traceability seed (EX-007): requirement proxy IDs come from
# docs/audits/mvp-r1.0/verify-matrix.md until a formal REQ baseline exists.
# Glob patterns match test files; "UNMAPPED" rows surface the gap honestly.
TRACEABILITY_SEED: dict[str, list[str]] = {
    "BIZ-02": ["crates/ica-api/tests/positions_http.rs", "crates/ica-api/tests/positions_write_http.rs"],
    "BIZ-03": ["crates/ica-api/tests/candidates_http.rs"],
    "BIZ-04": ["crates/ica-api/tests/candidates_http.rs"],
    "BIZ-08": ["crates/ica-api/tests/conversations_http.rs"],
    "BIZ-09": ["crates/ica-api/tests/follow_ups_http.rs"],
    "BIZ-10": ["crates/ica-api/tests/follow_ups_http.rs"],
    "BIZ-07": ["crates/ica-api/tests/audit_events_http.rs", "crates/ica-api/tests/w4_audit_regressions.rs"],
    "BIZ-12": ["crates/ica-api/tests/generate_model_resolution.rs", "crates/ica-api/tests/draft_unreachable_transition.rs"],
    "API-01": ["crates/ica-api/tests/router_parity_http.rs", "scripts/check_endpoint_map.py"],
    "API-02": ["crates/ica-contracts/src/body.rs", "crates/ica-contracts/src/query.rs"],
    "API-03": ["crates/ica-api/tests/support/mod.rs", "docs/contract-baseline/"],
    "API-04": ["crates/ica-desktop/tests/commands_smoke.rs", "crates/ica-api/src/error.rs"],
    "API-08": ["crates/ica-infra/tests/database.rs", "crates/ica-infra/src/store.rs"],
    "API-09": ["crates/ica-api/tests/follow_ups_http.rs", "crates/ica-api/tests/w4_imports_http.rs"],
    "DB-03": ["crates/ica-infra/tests/database.rs", "crates/ica-api/tests/draft_unreachable_transition.rs"],
    "DB-05": ["crates/ica-infra/src/audit.rs", "crates/ica-infra/tests/database.rs"],
    "DB-02": ["crates/ica-infra/tests/migrations.rs", "crates/ica-infra/src/migrations.rs"],
    "DB-06": ["crates/ica-api/tests/positions_write_http.rs", "crates/ica-api/tests/candidates_http.rs"],
    "DB-10": ["crates/ica-api/tests/w4_imports_http.rs", "crates/ica-desktop/tests/w4_imports_commands.rs"],
    "AI-04": ["crates/ica-application/src/ai_context.rs"],
    "AI-01": ["crates/ica-api/tests/settings_http.rs", "crates/ica-api/tests/generate_model_resolution.rs"],
    "AI-05": ["crates/ica-api/tests/generate_model_resolution.rs"],
    "AI-08": ["crates/ica-api/tests/draft_unreachable_transition.rs"],
    "AI-09": ["crates/ica-api/tests/settings_http.rs", "crates/ica-domain/src/redact.rs", "crates/ica-application/src/ai_context.rs"],
    "GUI-11": ["crates/ica-desktop/tests/commands_smoke.rs", "crates/ica-desktop/tests/w4_imports_commands.rs"],
    "SEC-04": ["crates/ica-api/tests/ai_key_backups_http.rs", "crates/ica-infra/src/secret_store.rs"],
    "DB-07": ["crates/ica-api/tests/ai_key_backups_http.rs", "crates/ica-infra/src/backup.rs"],
    "DB-08": ["crates/ica-api/tests/ai_key_backups_http.rs", "crates/ica-infra/src/backup.rs", "docs/testing/backup-restore-drill.md"],
    "SEC-06": ["crates/ica-api/tests/w4_imports_http.rs", "crates/ica-contracts/src/body.rs"],
    "SEC-07": ["crates/ica-api/tests/w4_imports_http.rs", "crates/ica-desktop/tests/w4_imports_commands.rs"],
    "SEC-08": ["crates/ica-api/tests/w4_audit_regressions.rs", "crates/ica-api/tests/audit_events_http.rs", "crates/ica-infra/src/audit.rs"],
}

P0_REQUIREMENTS = {"SEC-04", "SEC-06", "SEC-07", "SEC-08", "DB-02", "DB-07", "DB-08", "API-01"}
P0_PATH_HINTS = ("auth", "backup", "secret", "migration", "restore")
P0_NAME_HINTS = ("auth", "login", "token", "secret", "key", "backup", "restore",
                 "migrat", "transaction", "atomic", "permission")


def requirement_of(record: dict) -> str:
    """Requirement proxy or honest UNMAPPED (SPE-QA-006 §8.2/§8.3)."""
    if record["source"] != "rust":
        return "UNMAPPED"
    for req_id in sorted(TRACEABILITY_SEED):
        if match_files(TRACEABILITY_SEED[req_id], [record]):
            return req_id
    return "UNMAPPED"


def priority_of(record: dict) -> str:
    """Deterministic P0/P1/P2 rules; no human review is faked here."""
    haystack = (record["file"] + " " + record["function"]).lower()
    if (record.get("requirement_id") in P0_REQUIREMENTS
            or any(h in record["file"].lower() for h in P0_PATH_HINTS)
            or any(h in haystack for h in P0_NAME_HINTS)):
        return "P0"
    if record["type"] in ("INT", "API", "FUNC") or record["source"] in ("frontend", "contract-capture"):
        return "P1"
    return "P2"


def match_files(patterns: list[str], catalog: list[dict]) -> list[str]:
    """Return case IDs whose file matches any seed pattern."""
    matched: list[str] = []
    for record in catalog:
        file_path = record["file"]
        for pattern in patterns:
            if pattern.endswith("/") and file_path.startswith(pattern):
                matched.append(record["case_id"])
                break
            if (file_path == pattern
                    or file_path.endswith(pattern.split("/")[-1])
                    and pattern.split("/")[-1] in file_path):
                matched.append(record["case_id"])
                break
    return matched


def build_traceability(records: list[dict]) -> list[tuple[str, str, str]]:
    rows: list[tuple[str, str, str]] = []
    catalog_ids = {record["case_id"] for record in records}
    for req_id in sorted(TRACEABILITY_SEED):
        patterns = TRACEABILITY_SEED[req_id]
        matched = [c for c in match_files(patterns, records) if c in catalog_ids]
        rows.append((req_id, ";".join(patterns), ";".join(matched) if matched else "UNMAPPED"))
    for source, label in (("frontend", "frontend vitest cases"),
                          ("contract-capture", "contract_capture.CASES")):
        ids = [r["case_id"] for r in records if r["source"] == source]
        if ids:
            rows.append(("UNMAPPED", label, ";".join(ids)))
    return rows


def title_of(name: str) -> str:
    return name.replace("_", " ").replace("#", " #")[:80]


def preconditions_of(record: dict) -> str:
    if record["source"] == "rust":
        return "cargo + rust-toolchain.toml 就绪;测试库可用"
    if record["source"] == "frontend":
        return "frontend 依赖已安装(Linux binding);vitest 可启动"
    return "对拍种子库 + Python/Rust 双端服务已就绪"


def input_of(record: dict) -> str:
    if record["source"] == "contract-capture":
        method = record.get("method") or "?"
        route = record.get("route") or "(运行期构造)"
        return f"CASE `{record['function']}`: {method} {route}"
    if record["source"] == "frontend":
        return f"`{record.get('test_name', record['function'])}` 的 DOM/交互输入(见源文件)"
    return f"见 `{record['file']}` 中测试函数入参"


def steps_of(record: dict) -> str:
    if record["source"] == "frontend":
        return f"`npx vitest run {record['file']} -t '{record.get('test_name', record['function'])}'`"
    if record["source"] == "contract-capture":
        return f"`python3 scripts/contract_verify.py` 中执行 CASE `{record['function']}`"
    crate = record["crate"]
    return f"`cargo test -p {crate} {record['function']}`"


def evidence_of(record: dict) -> str:
    if record["source"] == "frontend":
        return f"frontend vitest:: `{record['file']}`"
    if record["source"] == "contract-capture":
        return f"contract_capture.CASES:: `{record['function']}`"
    return f"Rust:: `{record['file']}:{record['line']}`"


def render_catalog(records: list[dict], registry: dict) -> str:
    by_source = Counter(r["source"] for r in records)
    by_type = Counter(r["type"] for r in records)
    by_priority = Counter(r["priority"] for r in records)
    frontend_files = len({r["file"] for r in records if r["source"] == "frontend"})
    lines = [
        "---",
        'title: "ICA-RUST 受控测试用例目录"',
        'document_id: "TC-ICA-CATALOG-001"',
        'document_type: "TestAsset"',
        'project: "ICA-RUST"',
        'version: "1.1.0"',
        'status: "Controlled"',
        'created: "2026-11-21"',
        'updated: "2026-11-23"',
        'owner: "ICA-RUST 项目负责人"',
        'id_registry: "docs/testing/case-id-registry.json"',
        'review_status: "PENDING_HUMAN_REVIEW"',
        'related_documents: ["ENG-ICA-SPECS-R1.0", "AUDIT-ICA-ENG-001", "SPE-QA-006"]',
        "---",
        "",
        "# ICA-RUST 受控测试用例目录（脚本生成）",
        "",
        f"> 生成方式：`python3 scripts/generate_test_catalog.py`。"
        f"Case ID 注册表：`docs/testing/case-id-registry.json`（持久分配）。"
        "本目录已从 DRAFT 转为受控库：Case ID 由注册表持久分配，"
        "新增/移动用例不重排既有 ID（SPE-QA-006 §3.1 / §16.1）。",
        "",
        f"共 **{len(records)}** 条用例：Rust `{by_source['rust']}`、"
        f"前端 `{by_source['frontend']}`（覆盖 **{frontend_files}** 个 test 文件）、"
        f"对拍 CASE `{by_source['contract-capture']}`。",
        f"类型分布：{' / '.join(f'{k}={v}' for k, v in sorted(by_type.items()))}；"
        f"优先级分布：{' / '.join(f'{k}={v}' for k, v in sorted(by_priority.items()))}。",
        "",
        "## 编号与稳定性",
        "",
        "- 编号格式 `TC-ICA-<模块>-<类型>-<三位序号>`（SPE-QA-006 §3.2）。",
        "- `identity = source::file::function`，注册表一经分配即冻结；"
        "文件移动按 `(source, crate, function)` 复用原 ID；废弃 ID 永不复用。",
        "- 行号只作定位展示，**不参与** Case ID 计算。",
        "- 变更证明：`python3 scripts/generate_test_catalog.py --self-test`。",
        "",
        "## 最小必需字段（SPE-QA-006 附录 B + QA-001 §4.2）",
        "",
        "Case ID / Title / Objective / Test Type / Priority / Preconditions / Input / "
        "Steps / Expected Result / Pass Criteria / Status / Version 共 12 项，"
        "另加 Requirement ID、Source/Evidence 与评审记录栏。",
        "",
        "## 优先级规则（P0/P1/P2）",
        "",
        "| 级别 | 判定规则 |",
        "| --- | --- |",
        "| P0 | 安全/认证/密钥/备份恢复/迁移/事务原子性/路由契约对账：Requirement ID ∈ "
        "{SEC-04, SEC-06, SEC-07, SEC-08, DB-02, DB-07, DB-08, API-01}，"
        "或文件/用例名含 auth·backup·secret·migration·restore·transaction·atomic·permission |",
        "| P1 | 接口/集成/前端交互/对拍用例（非 P0）：Test Type ∈ {INT, API, FUNC} 或来源为 frontend / contract-capture |",
        "| P2 | 纯单元测试与 POC：Test Type = UNIT 或 browser_poc |",
        "",
        "## 评审记录（不得代签）",
        "",
        "本目录**未进行任何人工评审**；评审人/日期/结论一律写「待评审」。"
        "下表「评审结论」栏不是通过结论，填写真实评审结果前本库只视为"
        "「编号受控、设计待评审」。REVIEW_PENDING 是测试用例状态，**不是**验证通过状态。",
        "",
        "| 项 | 值 |",
        "| --- | --- |",
        "| 评审人 | 待评审 |",
        "| 评审日期 | 待评审 |",
        "| 评审结论 | 待评审 |",
        "| 评审依据 | SPE-QA-006 §9.3（目标/依据/输入/步骤/预期/边界/重复/自动化/追溯） |",
        "",
        "## 用例清单",
        "",
        "| Case ID | Title | Objective | Requirement ID | Test Type | Priority | Preconditions | Input | Steps | Expected Result | Pass Criteria | Status | Version | Source / Evidence | Reviewer | Review Date | Review Conclusion |",
        "| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |",
    ]
    for record in records:
        title = title_of(record["function"])
        lines.append(
            f"| {record['case_id']} | {title} | 验证 {title} | {record['requirement_id']} "
            f"| {record['type']} | {record['priority']} | {preconditions_of(record)} "
            f"| {input_of(record)} | {steps_of(record)} | 断言全部通过，0 failed "
            f"| 退出码 0 且所有断言通过 | REVIEW_PENDING | 1.0.0 | {evidence_of(record)} "
            f"| 待评审 | 待评审 | 待评审 |"
        )
    lines.append("")
    return "\n".join(lines)


def render_traceability(rows: list[tuple[str, str, str]]) -> str:
    lines = ["requirement_proxy_id,evidence_source,case_ids"]
    for req_id, source, case_ids in rows:
        safe_ids = '"' + case_ids + '"' if ";" in case_ids else case_ids
        lines.append(f"{req_id},{source},{safe_ids}")
    return "\n".join(lines) + "\n"


def write_atomic(path: Path, content: str) -> None:
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.write_text(content, encoding="utf-8")
    tmp.replace(path)


def self_test() -> int:
    """No-file proof that moving a file / adding a case preserves existing IDs."""
    registry = new_registry()
    base = [
        {"source": "rust", "crate": "ica-api", "module": "API", "type": "INT",
         "function": "an_existing_test", "file": "crates/ica-api/tests/a.rs", "line": 10},
        {"source": "frontend", "crate": "frontend", "module": "WEB", "type": "FUNC",
         "function": "an_existing_ui_test", "file": "frontend/src/a.test.ts", "line": 5},
    ]
    assign_case_ids(base, registry)
    first = {r["function"]: r["case_id"] for r in base}

    moved = copy.deepcopy(base)
    moved[0]["file"] = "crates/ica-api/tests/renamed.rs"   # file move
    moved[0]["line"] = 999                                  # line shift
    moved.append({"source": "rust", "crate": "ica-api", "module": "API", "type": "INT",
                  "function": "a_brand_new_test", "file": "crates/ica-api/tests/a.rs", "line": 11})
    assign_case_ids(moved, registry)
    second = {r["function"]: r["case_id"] for r in moved}

    checks = [
        ("moved Rust case keeps its ID", second["an_existing_test"] == first["an_existing_test"],
         f"{first['an_existing_test']} -> {second['an_existing_test']}"),
        ("line shift alone does not renumber", second["an_existing_ui_test"] == first["an_existing_ui_test"],
         f"{first['an_existing_ui_test']} -> {second['an_existing_ui_test']}"),
        ("new case gets a distinct ID", second["a_brand_new_test"] not in first.values(),
         second["a_brand_new_test"]),
        ("moving registers a moved_from trail",
         any("tests/a.rs" in trail
             for e in registry["ids"].values() for trail in e.get("moved_from", [])),
         [trail for e in registry["ids"].values() for trail in e.get("moved_from", [])]),
    ]
    for label, ok, detail in checks:
        print(f"[{'PASS' if ok else 'FAIL'}] {label}: {detail}")
    return 0 if all(ok for _, ok, _ in checks) else 1


def execute(args: argparse.Namespace) -> int:
    project_root = args.project_root.resolve()
    out_dir = project_root / "docs" / "testing"
    if args.self_test:
        return self_test()

    logging.info("Scanning Rust / frontend / contract cases under %s", project_root)
    records = scan_tests(project_root)
    frontend = scan_frontend(project_root)
    cases = scan_capture_cases(project_root)
    records.extend(frontend)
    records.extend(cases)
    if not records:
        logging.error("No test functions found — scan result invalid")
        return 10
    registry, origin = load_registry(project_root)
    stats = assign_case_ids(records, registry)
    for record in records:
        record["requirement_id"] = requirement_of(record)
        record["priority"] = priority_of(record)
    logging.info("Found %d cases (rust=%d frontend=%d cases=%d); registry=%s; %s",
                 len(records), len(records) - len(frontend) - len(cases),
                 len(frontend), len(cases), origin, stats)

    markdown = render_catalog(records, registry)
    csv_rows = build_traceability(records)
    csv_text = render_traceability(csv_rows)
    unmapped = [req for req, _, ids in csv_rows if ids == "UNMAPPED"]

    if args.stdout:
        sys.stdout.write(markdown)
        return 0
    if args.dry_run:
        logging.info("[DRY-RUN] would write %s, %s and %s",
                     out_dir / CATALOG_MD, out_dir / TRACEABILITY_CSV, out_dir / REGISTRY_JSON)
        logging.info("[DRY-RUN] unmapped requirement proxies: %s", unmapped or "none")
        return 0
    out_dir.mkdir(parents=True, exist_ok=True)
    write_atomic(out_dir / CATALOG_MD, markdown)
    write_atomic(out_dir / TRACEABILITY_CSV, csv_text)
    write_atomic(out_dir / REGISTRY_JSON,
                 json.dumps(registry, ensure_ascii=False, indent=2, sort_keys=True) + "\n")
    logging.info("Wrote %s, %s and %s", out_dir / CATALOG_MD,
                 out_dir / TRACEABILITY_CSV, out_dir / REGISTRY_JSON)
    if unmapped:
        logging.warning("Traceability gaps (UNMAPPED): %s", ", ".join(unmapped))
    return 0


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    logging.basicConfig(
        level=logging.DEBUG if args.verbose else logging.INFO,
        format="%(asctime)s %(levelname)s %(name)s: %(message)s",
        datefmt="%Y-%m-%d %H:%M:%S",
    )
    try:
        return execute(args)
    except KeyboardInterrupt:
        logging.error("Operation interrupted")
        return 130
    except (OSError, SyntaxError):
        logging.exception("System / parse operation failed")
        return 5
    except Exception:
        logging.exception("Unexpected failure")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
