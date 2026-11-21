#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Copyright (c) 2026 ICA-RUST Contributors.

Project : ICA-RUST
Module  : Test Case Catalog Generator
Purpose : Scan Rust test functions and generate the SPE-QA-006 test-case
          catalog and traceability-matrix seed under docs/testing/.

Usage:
    python scripts/generate_test_catalog.py [--project-root PATH]
        [--dry-run] [--verbose] [--stdout]

Outputs (overwritten atomically on each run):
    docs/testing/test-case-catalog.md      human-readable case catalog draft
    docs/testing/traceability-matrix.csv   requirement-proxy -> test mapping

Exit codes: 0 success; 2 argument error; 5 file/data error; 10 validation failed
(no tests found). Follows SPE-BAS-004 R1.1; case IDs follow SPE-QA-006 R1.1 §3.
"""

import argparse
import logging
import re
import sys
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
}

# Attribute forms that declare a Rust test.
TEST_ATTR = re.compile(
    r"^\s*#\[(?:tokio::test(?:\([^)]*\))?|test)\]\s*$"
)
TEST_FN = re.compile(
    r"^\s*(?:async\s+)?fn\s+([a-zA-Z_][a-zA-Z0-9_]*)\s*\("
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
        description="Generate the SPE-QA-006 test-case catalog for ICA-RUST"
    )
    parser.add_argument(
        "--project-root",
        type=Path,
        default=PROJECT_ROOT,
        help="Repository root (default: script's repo)",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Scan and report, but do not write any files",
    )
    parser.add_argument(
        "--stdout",
        action="store_true",
        help="Print the generated markdown instead of writing files",
    )
    parser.add_argument(
        "--verbose",
        action="store_true",
        help="Enable verbose logging",
    )
    args = parser.parse_args(argv)
    if not args.project_root.is_dir():
        parser.error(f"project root is not a directory: {args.project_root}")
    return args


def scan_tests(project_root: Path) -> list[dict]:
    """Return one record per test function, sorted deterministically."""
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
                        records.append(
                            {
                                "crate": crate,
                                "module": module,
                                "type": infer_type(rel),
                                "function": match.group(1),
                                "file": rel.as_posix(),
                                "line": line_no,
                            }
                        )
                pending_attr = False
    records.sort(key=lambda r: (r["file"], r["line"]))
    return records


def assign_case_ids(records: list[dict]) -> None:
    """TC-ICA-<MODULE>-<TYPE>-<seq>, seq per (module, type) in scan order."""
    counters: dict[tuple[str, str], int] = {}
    for record in records:
        key = (record["module"], record["type"])
        counters[key] = counters.get(key, 0) + 1
        record["case_id"] = (
            f"TC-{PROJECT_CODE}-{record['module']}-{record['type']}-"
            f"{counters[key]:03d}"
        )


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


def match_files(patterns: list[str], catalog: list[dict]) -> list[str]:
    """Return case IDs whose file matches any seed pattern."""
    matched: list[str] = []
    for record in catalog:
        file_path = record["file"]
        for pattern in patterns:
            if pattern.endswith("/") and file_path.startswith(pattern):
                matched.append(record["case_id"])
                break
            if file_path == pattern or file_path.endswith(pattern.split("/")[-1]) and pattern.split("/")[-1] in file_path:
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
    return rows


def render_catalog(records: list[dict], total: int) -> str:
    lines: list[str] = []
    lines.append("---")
    lines.append('title: "ICA-RUST 测试用例目录(草稿)"')
    lines.append('document_id: "TC-ICA-CATALOG-001"')
    lines.append('document_type: "TestAsset"')
    lines.append('project: "ICA-RUST"')
    lines.append('version: "0.1.0"')
    lines.append('status: "Draft"')
    lines.append('created: "2026-11-21"')
    lines.append('updated: "2026-11-21"')
    lines.append('owner: "ICA-RUST 项目负责人"')
    lines.append('related_documents: ["ENG-ICA-SPECS-R1.0", "AUDIT-ICA-ENG-001"]')
    lines.append("---")
    lines.append("")
    lines.append("# 测试用例目录(草稿)— 脚本生成")
    lines.append("")
    lines.append(f"由 `scripts/generate_test_catalog.py` 生成,共扫描到 **{total}** 个 Rust 测试函数。")
    lines.append("编号规则:SPE-QA-006 §3.2(`TC-ICA-<模块>-<类型>-<三位序号>`);")
    lines.append("扫描顺序为文件路径+行号排序,重跑可复现。**本目录为 DRAFT 状态:**")
    lines.append("Case ID 尚未登记确认(EX-006),正式启用需项目负责人批准;批准后")
    lines.append("ID 冻结,不再随文件移动变化。Priority 统一 P2(待评审定级)。")
    lines.append("")
    lines.append("| Case ID | 测试函数 | 文件 | Crate | 类型 | 优先级 | 自动化 | 状态 |")
    lines.append("|---|---|---|---|---|---|---|---|")
    for record in records:
        automation = "AUTOMATED"
        if record["file"].endswith("browser_poc.rs"):
            automation = "AUTOMATED(条件:本机有 Chromium)"
        lines.append(
            f"| {record['case_id']} | `{record['function']}` "
            f"| `{record['file']}:{record['line']}` | {record['crate']} "
            f"| {record['type']} | P2(待定) | {automation} | DRAFT |"
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


def execute(args: argparse.Namespace) -> int:
    project_root = args.project_root.resolve()
    logging.info("Scanning Rust tests under %s", project_root / "crates")
    records = scan_tests(project_root)
    if not records:
        logging.error("No test functions found — scan result invalid")
        return 10
    assign_case_ids(records)
    logging.info("Found %d test functions", len(records))

    markdown = render_catalog(records, len(records))
    csv_rows = build_traceability(records)
    csv_text = render_traceability(csv_rows)
    unmapped = [req for req, _, ids in csv_rows if ids == "UNMAPPED"]

    out_dir = project_root / "docs" / "testing"
    if args.stdout:
        sys.stdout.write(markdown)
        return 0
    if args.dry_run:
        logging.info("[DRY-RUN] would write %s (catalog) and %s (matrix)",
                     out_dir / "test-case-catalog.md",
                     out_dir / "traceability-matrix.csv")
        logging.info("[DRY-RUN] unmapped requirements: %s", unmapped or "none")
        return 0
    out_dir.mkdir(parents=True, exist_ok=True)
    write_atomic(out_dir / "test-case-catalog.md", markdown)
    write_atomic(out_dir / "traceability-matrix.csv", csv_text)
    logging.info("Wrote %s and %s", out_dir / "test-case-catalog.md",
                 out_dir / "traceability-matrix.csv")
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
    except OSError:
        logging.exception("System operation failed")
        return 5
    except Exception:
        logging.exception("Unexpected failure")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
