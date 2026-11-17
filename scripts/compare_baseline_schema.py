"""Compare the exported baseline DDL against the real Python database.

Reads the baseline SQLite file **read-only** (``mode=ro`` URI) and lists its
tables and indexes, so ``crates/ica-infra/schema/ica-baseline.sql`` can be
proven to describe the same object set — the acceptance criterion for AC2.

Usage:
    python3 compare_baseline_schema.py <baseline.db> <baseline.sql>
"""

from __future__ import annotations

import re
import sqlite3
import sys
from pathlib import Path


def db_objects(path: Path) -> tuple[set[str], set[str]]:
    uri = f"file:{path}?mode=ro"
    connection = sqlite3.connect(uri, uri=True)
    try:
        rows = connection.execute(
            "SELECT type, name FROM sqlite_master"
            " WHERE name NOT LIKE 'sqlite_%' ORDER BY type, name"
        ).fetchall()
    finally:
        connection.close()
    tables = {name for kind, name in rows if kind == "table"}
    indexes = {name for kind, name in rows if kind == "index"}
    return tables, indexes


def sql_objects(path: Path) -> tuple[set[str], set[str]]:
    text = path.read_text(encoding="utf-8")
    tables = {
        m.group(2)
        for m in re.finditer(
            r'CREATE TABLE\s+(IF NOT EXISTS\s+)?"?([A-Za-z_][A-Za-z0-9_]*)"?', text
        )
    }
    indexes = {
        m.group(2)
        for m in re.finditer(
            r'CREATE\s+(?:UNIQUE\s+)?INDEX\s+(IF NOT EXISTS\s+)?"?([A-Za-z_][A-Za-z0-9_]*)"?',
            text,
        )
    }
    return tables, indexes


def report(label: str, left: set[str], right: set[str]) -> bool:
    only_left = sorted(left - right)
    only_right = sorted(right - left)
    print(f"{label}: db={len(left)} sql={len(right)} matched={len(left & right)}")
    if only_left:
        print(f"  only in db:  {only_left}")
    if only_right:
        print(f"  only in sql: {only_right}")
    return not only_left and not only_right


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print(__doc__)
        return 2
    db_tables, db_indexes = db_objects(Path(argv[0]))
    sql_tables, sql_indexes = sql_objects(Path(argv[1]))
    tables_ok = report("TABLES", db_tables, sql_tables)
    indexes_ok = report("INDEXES", db_indexes, sql_indexes)
    print()
    print("db tables:  " + ", ".join(sorted(db_tables)))
    print()
    print("RESULT:", "IDENTICAL" if tables_ok and indexes_ok else "DIFFERS")
    return 0 if tables_ok and indexes_ok else 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
