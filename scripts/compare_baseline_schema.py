#!/usr/bin/env python3
"""Compare the exported baseline DDL against a real SQLite database.

Reads the database file **read-only** (``mode=ro`` URI) and lists its tables
and indexes, so ``crates/ica-infra/schema/ica-baseline.sql`` can be proven to
describe the same object set — the acceptance criterion for AC2.

**Scope.** ICA's browser subsystem adds ``browser_*`` tables (and their
indexes) on top of the frozen baseline. They are *out of scope*, not DDL
mistakes: ``--exclude-table`` (repeatable, default ``browser_*``) removes them
from the comparison and prints how many were removed, so a re-anchored
baseline cannot disguise out-of-scope tables as schema drift.
``--expect-excluded-tables`` pins that count so the pattern list cannot
silently widen. An index is excluded when its own name or its **owner table**
matches a pattern — dropping a table must not leave its indexes behind as
noise.

Usage:
    python3 compare_baseline_schema.py <baseline.db> <baseline.sql>

``--self-test`` builds throwaway databases in a temp directory and exercises
the comparison; it needs no real database.
"""

from __future__ import annotations

import argparse
import fnmatch
import re
import sqlite3
import sys
import tempfile
from pathlib import Path

DEFAULT_DB = "/mnt/o/ica/backend/ica.db"
DEFAULT_SQL = Path(__file__).resolve().parent.parent / "crates/ica-infra/schema/ica-baseline.sql"
#: Browser-subsystem tables (browser_bridge sessions, tasks, …) are excluded
#: from this migration; see docs/rust-migration-scope-20261118.md.
DEFAULT_EXCLUDE_TABLES = ("browser_*",)

TABLE_RE = re.compile(r'CREATE TABLE\s+(?:IF NOT EXISTS\s+)?"?([A-Za-z_][A-Za-z0-9_]*)"?')
INDEX_RE = re.compile(
    r'CREATE\s+(?:UNIQUE\s+)?INDEX\s+(?:IF NOT EXISTS\s+)?"?([A-Za-z_][A-Za-z0-9_]*)"?'
)


def excluded(name: str, owner: str, patterns: list[str]) -> bool:
    """A table (owner == name) or an index (owner == its table) is out of
    scope when either name matches one of the patterns."""
    return any(fnmatch.fnmatch(name, p) or fnmatch.fnmatch(owner, p) for p in patterns)


def db_objects(
    path: Path, patterns: list[str]
) -> tuple[set[str], set[str], set[str], set[str]]:
    """(tables, indexes, excluded tables, excluded indexes) from the database."""
    uri = f"file:{path}?mode=ro"
    connection = sqlite3.connect(uri, uri=True)
    try:
        rows = connection.execute(
            "SELECT type, name, tbl_name FROM sqlite_master"
            " WHERE name NOT LIKE 'sqlite_%' ORDER BY type, name"
        ).fetchall()
    finally:
        connection.close()
    excluded_tables = {
        name for kind, name, owner in rows if kind == "table" and excluded(name, owner, patterns)
    }
    excluded_indexes = {
        name for kind, name, owner in rows if kind == "index" and excluded(name, owner, patterns)
    }
    tables = {name for kind, name, _ in rows if kind == "table"} - excluded_tables
    indexes = {name for kind, name, _ in rows if kind == "index"} - excluded_indexes
    return tables, indexes, excluded_tables, excluded_indexes


def sql_objects(path: Path, patterns: list[str]) -> tuple[set[str], set[str]]:
    """(tables, indexes) the DDL declares, minus excluded names."""
    text = path.read_text(encoding="utf-8")
    tables = {
        name
        for name in TABLE_RE.findall(text)
        if not excluded(name, name, patterns)
    }
    indexes = {
        name
        for name in INDEX_RE.findall(text)
        if not excluded(name, name, patterns)
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


def compare_databases(
    db: Path, sql: Path, patterns: list[str], expect_excluded: int | None
) -> int:
    db_tables, db_indexes, excluded_tables, excluded_indexes = db_objects(db, patterns)
    sql_tables, sql_indexes = sql_objects(sql, patterns)

    print(f"excluded tables:  {len(excluded_tables)}   pattern: {' '.join(patterns)}")
    for name in sorted(excluded_tables):
        print(f"    out of scope: {name}")
    print(f"excluded indexes: {len(excluded_indexes)}")

    tables_ok = report("TABLES", db_tables, sql_tables)
    indexes_ok = report("INDEXES", db_indexes, sql_indexes)
    print()
    print("db tables:  " + ", ".join(sorted(db_tables)))
    print()

    if expect_excluded is not None and len(excluded_tables) != expect_excluded:
        print(
            f"RESULT: DIFFERS — expected exactly {expect_excluded} excluded tables,"
            f" measured {len(excluded_tables)}: the pattern list drifted"
        )
        return 1

    identical = tables_ok and indexes_ok
    print("RESULT:", "IDENTICAL" if identical else "DIFFERS")
    return 0 if identical else 1


def self_test() -> int:
    """Build throwaway databases and exercise the comparison:

    1. a database replaying the baseline DDL matches the DDL exactly;
    2. adding two ``browser_*`` tables (with indexes) makes it DIFFERS without
       exclusion and IDENTICAL with the default exclusion;
    3. adding one non-``browser_*`` table still DIFFERS with exclusion —
       the pattern list must not be able to hide real drift.
    """
    baseline_sql = DEFAULT_SQL
    if not baseline_sql.exists():
        print(f"SELF-TEST FAIL: baseline DDL not found at {baseline_sql}")
        return 1

    def build(path: Path, extra_tables: list[str]) -> None:
        connection = sqlite3.connect(path)
        try:
            connection.executescript(baseline_sql.read_text(encoding="utf-8"))
            for table in extra_tables:
                connection.execute(
                    f'CREATE TABLE "{table}" (id TEXT PRIMARY KEY, payload TEXT)'
                )
                connection.execute(f'CREATE INDEX "ix_{table}_payload" ON "{table}" (payload)')
            connection.commit()
        finally:
            connection.close()

    failures: list[str] = []
    with tempfile.TemporaryDirectory(prefix="ica-schema-selftest-") as tmp:
        tmp_dir = Path(tmp)

        plain = tmp_dir / "plain.db"
        build(plain, [])
        if compare_databases(plain, baseline_sql, [], None) != 0:
            failures.append("baseline db vs DDL, no exclusion: expected IDENTICAL")

        drifted = tmp_dir / "drifted.db"
        build(drifted, ["browser_sessions", "browser_tasks"])
        if compare_databases(drifted, baseline_sql, [], None) == 0:
            failures.append("drifted db, no exclusion: expected DIFFERS")
        if compare_databases(drifted, baseline_sql, list(DEFAULT_EXCLUDE_TABLES), None) != 0:
            failures.append("drifted db, browser_* excluded: expected IDENTICAL")

        sneaky = tmp_dir / "sneaky.db"
        build(sneaky, ["browser_sessions", "browser_tasks", "sneaky_extra"])
        if compare_databases(sneaky, baseline_sql, list(DEFAULT_EXCLUDE_TABLES), None) == 0:
            failures.append("non-browser extra table, browser_* excluded: expected DIFFERS")

    if failures:
        for failure in failures:
            print(f"SELF-TEST FAIL: {failure}")
        return 1
    print(
        "self-test: passed (baseline identical; drifted differs without exclusion;"
        " identical with browser_* excluded; non-browser drift still differs)"
    )
    return 0


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument("db", nargs="?", default=DEFAULT_DB, help="database to inspect (ro)")
    parser.add_argument("sql", nargs="?", default=str(DEFAULT_SQL), help="baseline DDL file")
    parser.add_argument(
        "--exclude-table",
        action="append",
        default=None,
        metavar="PATTERN",
        help="tables (and their indexes) matching this fnmatch pattern are out of"
        " scope (repeatable; default browser_*)",
    )
    parser.add_argument(
        "--expect-excluded-tables",
        type=int,
        default=None,
        metavar="N",
        help="fail unless exactly N tables were excluded",
    )
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="exercise the comparison on throwaway databases, no real db needed",
    )
    args = parser.parse_args(argv)

    if args.self_test:
        return self_test()

    patterns = list(DEFAULT_EXCLUDE_TABLES) if args.exclude_table is None else args.exclude_table
    return compare_databases(
        Path(args.db), Path(args.sql), patterns, args.expect_excluded_tables
    )


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
