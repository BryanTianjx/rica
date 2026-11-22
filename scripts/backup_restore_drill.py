#!/usr/bin/env python3
"""ARCH-P6 AC4 — real backup -> restore -> verification drill on a temp DB.

This is NOT a code-reading exercise. It builds a throwaway baseline database
from the repository's exported baseline DDL via scripts/seed_contract_db.py,
takes a live copy, creates a backup through the SQLite ONLINE BACKUP API
(sqlite3_backup — the same mechanism rusqlite::backup::Backup uses in
crates/ica-infra/src/backup.rs), mutates the live copy, restores from the
backup through the same API after writing a pre-restore safety copy, and then
verifies per-table row counts and a canonical DDL digest against the baseline.

Nothing touches /mnt/o/ica/backend/ica.db (the frozen contract baseline).

Usage:
    python3 scripts/backup_restore_drill.py [--work-dir DIR] [--out EVIDENCE.json] [--keep]

Exit 0 = every checkpoint assertion held. Non-zero = drill failed (real failure).
"""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import sqlite3
import subprocess
import sys
import tempfile
import time
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
SEED = REPO / "scripts" / "seed_contract_db.py"
PROBE_ID = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaa1"
PROBE_NAME = "ARCH-P6 DRILL PROBE"


def now() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="seconds")


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def connect(path: Path) -> sqlite3.Connection:
    connection = sqlite3.connect(path)
    connection.execute("PRAGMA busy_timeout=5000")
    return connection


def online_copy(src: Path, dst: Path) -> None:
    """Consistent copy through sqlite3_backup, never a bare file copy."""
    source = connect(src)
    target = connect(dst)
    try:
        source.backup(target)
        target.commit()
        target.execute("PRAGMA wal_checkpoint(TRUNCATE)")
    finally:
        target.close()
        source.close()


def snapshot(path: Path) -> dict:
    """Per-table row counts + canonical DDL digest of one database file."""
    connection = connect(path)
    try:
        ddl_rows = connection.execute(
            "SELECT type, name, tbl_name, sql FROM sqlite_master "
            "ORDER BY type, name, tbl_name"
        ).fetchall()
        ddl_blob = json.dumps(
            [[("" if item is None else str(item)) for item in row] for row in ddl_rows],
            ensure_ascii=False,
        )
        tables = {
            name: connection.execute(f'SELECT count(*) FROM "{name}"').fetchone()[0]
            for (name,) in connection.execute(
                "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name"
            ).fetchall()
        }
        probe = connection.execute(
            "SELECT count(*) FROM positions WHERE id = ?", (PROBE_ID,)
        ).fetchone()[0]
        return {
            "ddl_object_count": len(ddl_rows),
            "ddl_sha256": hashlib.sha256(ddl_blob.encode("utf-8")).hexdigest(),
            "tables": tables,
            "total_rows": sum(tables.values()),
            "probe_present": probe == 1,
        }
    finally:
        connection.close()


def add_probe(path: Path) -> None:
    connection = connect(path)
    try:
        connection.execute(
            "INSERT INTO positions (id, name, description, location, status, archived,"
            " created_at, updated_at, salary_min, salary_max, skill_requirements, notes)"
            " VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            (
                PROBE_ID,
                PROBE_NAME,
                "ARCH-P6 backup/restore drill probe",
                "Beijing",
                "Open",
                0,
                "2026-11-23 00:00:00.000000",
                "2026-11-23 00:00:00.000000",
                None,
                None,
                None,
                None,
            ),
        )
        connection.commit()
        connection.execute("PRAGMA wal_checkpoint(TRUNCATE)")
    finally:
        connection.close()


def mutate(path: Path) -> None:
    connection = connect(path)
    try:
        connection.execute("DELETE FROM positions WHERE id = ?", (PROBE_ID,))
        connection.commit()
        connection.execute("PRAGMA wal_checkpoint(TRUNCATE)")
    finally:
        connection.close()


def step(evidence: list, name: str, detail: str, **extra) -> None:
    record = {"step": name, "at": now(), "detail": detail}
    record.update(extra)
    evidence.append(record)
    print(f"[{name}] {detail}", flush=True)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--work-dir", type=Path, help="scratch dir (default: mkdtemp)")
    parser.add_argument("--out", type=Path, help="write JSON evidence here")
    parser.add_argument("--keep", action="store_true", help="do not delete the scratch dir")
    args = parser.parse_args()

    created_temp = args.work_dir is None
    work = args.work_dir or Path(tempfile.mkdtemp(prefix="arch-p6-drill-"))
    work.mkdir(parents=True, exist_ok=True)
    backups = work / "backups"
    backups.mkdir(parents=True, exist_ok=True)

    baseline = work / "baseline.db"
    live = work / "live.db"
    backup = backups / "ica-backup-arch-p6.db"
    safety = backups / "pre-restore-arch-p6.db"

    evidence: list[dict] = []
    assertions: list[dict] = []
    started = time.monotonic()

    def check(label: str, ok: bool, observed: str) -> None:
        assertions.append({"assertion": label, "passed": bool(ok), "observed": observed})
        print(f"  CHECK {'PASS' if ok else 'FAIL'}: {label} — {observed}", flush=True)

    try:
        step(
            evidence,
            "C0",
            "build throwaway baseline via scripts/seed_contract_db.py "
            "(replays crates/ica-infra/schema/ica-baseline.sql)",
            command=f"{sys.executable} scripts/seed_contract_db.py --out {baseline}",
        )
        seeded = subprocess.run(
            [sys.executable, str(SEED), "--out", str(baseline)],
            cwd=REPO,
            capture_output=True,
            text=True,
        )
        print(seeded.stdout.strip()[-800:], flush=True)
        check(
            "seed script produced a baseline database",
            seeded.returncode == 0 and baseline.is_file(),
            f"exit={seeded.returncode}, exists={baseline.is_file()}",
        )
        if seeded.returncode != 0 or not baseline.is_file():
            print(seeded.stderr[-2000:], flush=True)
            return 1

        add_probe(baseline)
        base_snapshot = snapshot(baseline)
        step(
            evidence,
            "C1",
            "baseline snapshot captured with a probe row inserted",
            baseline_sha256=sha256(baseline),
            snapshot=base_snapshot,
        )
        check(
            "baseline snapshot contains the probe row",
            base_snapshot["probe_present"],
            f"probe_present={base_snapshot['probe_present']}",
        )
        check(
            "baseline DDL digest is non-trivial",
            base_snapshot["ddl_object_count"] > 50,
            f"objects={base_snapshot['ddl_object_count']}, "
            f"ddl_sha256={base_snapshot['ddl_sha256'][:16]}",
        )

        shutil.copy2(baseline, live)
        online_copy(live, backup)
        backup_snapshot = snapshot(backup)
        step(
            evidence,
            "C2",
            "backup created through SQLite online-backup API (sqlite3_backup)",
            command=f"sqlite3.Connection.backup({live.name} -> {backup.name})",
            backup_sha256=sha256(backup),
            backup_size_bytes=backup.stat().st_size,
            snapshot=backup_snapshot,
        )
        check(
            "backup is byte-distinct and snapshot-equal to baseline",
            backup_snapshot == base_snapshot and sha256(backup) != sha256(baseline),
            f"snapshots_equal={backup_snapshot == base_snapshot}, "
            f"backup_sha256={sha256(backup)[:16]}",
        )

        mutate(live)
        mutated = snapshot(live)
        step(
            evidence,
            "C3",
            "live database mutated: probe row deleted (proves restore has work to do)",
            command=f"DELETE FROM positions WHERE id='{PROBE_ID}'",
            snapshot=mutated,
            live_sha256=sha256(live),
        )
        check(
            "mutation removed the probe and changed row counts",
            (not mutated["probe_present"])
            and mutated["total_rows"] == base_snapshot["total_rows"] - 1,
            f"probe_present={mutated['probe_present']}, "
            f"rows {base_snapshot['total_rows']} -> {mutated['total_rows']}",
        )
        check(
            "mutation did not alter the DDL digest",
            mutated["ddl_sha256"] == base_snapshot["ddl_sha256"],
            f"ddl_sha256={mutated['ddl_sha256'][:16]}",
        )

        online_copy(live, safety)
        step(
            evidence,
            "C4",
            "pre-restore safety copy taken through the same online-backup API",
            command=f"sqlite3.Connection.backup({live.name} -> {safety.name})",
            safety_sha256=sha256(safety),
        )
        check(
            "pre-restore safety copy preserves the mutated state",
            snapshot(safety) == mutated,
            f"safety_snapshot_equals_mutated={snapshot(safety) == mutated}",
        )

        online_copy(backup, live)
        restored = snapshot(live)
        step(
            evidence,
            "C5",
            "restore executed in place from backup through sqlite3_backup + "
            "PRAGMA wal_checkpoint(TRUNCATE)",
            command=f"sqlite3.Connection.backup({backup.name} -> {live.name})",
            snapshot=restored,
            live_sha256_after=sha256(live),
        )
        check(
            "restored DDL digest equals baseline DDL digest",
            restored["ddl_sha256"] == base_snapshot["ddl_sha256"],
            f"{restored['ddl_sha256'][:16]} == {base_snapshot['ddl_sha256'][:16]}",
        )
        check(
            "restored per-table row counts equal baseline row counts",
            restored["tables"] == base_snapshot["tables"],
            "all "
            + str(len(base_snapshot["tables"]))
            + " tables compared"
            + (
                ""
                if restored["tables"] == base_snapshot["tables"]
                else f"; diff={{{', '.join(k for k in base_snapshot['tables'] if base_snapshot['tables'][k] != restored['tables'].get(k))}}}"
            ),
        )
        check(
            "restored total rows and probe row equal baseline",
            restored["total_rows"] == base_snapshot["total_rows"]
            and restored["probe_present"],
            f"rows={restored['total_rows']}, probe_present={restored['probe_present']}",
        )
    finally:
        if created_temp and not args.keep:
            shutil.rmtree(work, ignore_errors=True)

    all_ok = bool(assertions) and all(item["passed"] for item in assertions)
    record = {
        "schema": "arch-p6-backup-restore-drill-v1",
        "document_id": "TEST-ICA-DRILL-001",
        "ac": "ARCH-P6-AC4",
        "mechanism": "SQLite online-backup API (sqlite3_backup), same as rusqlite backup adapter",
        "head": subprocess.run(
            ["git", "rev-parse", "HEAD"], cwd=REPO, capture_output=True, text=True
        ).stdout.strip(),
        "started_at": now(),
        "duration_seconds": round(time.monotonic() - started, 2),
        "work_dir": str(work),
        "baseline_db_untouched": True,
        "steps": evidence,
        "assertions": assertions,
        "drill_result": "PASS" if all_ok else "FAIL",
    }
    output = json.dumps(record, ensure_ascii=False, indent=2)
    if args.out:
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(output, encoding="utf-8")
        print(f"\nevidence: {args.out}")
    else:
        print(output)
    print(
        f"\nBACKUP/RESTORE DRILL: {record['drill_result']} "
        f"({sum(1 for a in assertions if a['passed'])}/{len(assertions)} checks)"
    )
    return 0 if all_ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
