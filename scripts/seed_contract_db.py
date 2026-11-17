#!/usr/bin/env python3
"""Build the shared contract seed database.

Both sides of the contract comparison read *this* file:

  * the Python baseline (SQLAlchemy, read-write),
  * the Rust port (opened `?mode=ro`).

So a value difference between the two captures can only come from the code,
never from the data. The seed is deliberately awkward: NULL and empty-string
locations, an archived position, an archived candidate, timestamps with and
without microseconds, and every `PositionStatus` value.

Usage:
    python3 scripts/seed_contract_db.py --out /tmp/ica-contract/seed.db

The schema is replayed from `crates/ica-infra/schema/ica-baseline.sql`, which is
the exported DDL of the real baseline database — not a hand-written schema.
"""

from __future__ import annotations

import argparse
import sqlite3
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
SCHEMA_PATH = REPO_ROOT / "crates" / "ica-infra" / "schema" / "ica-baseline.sql"

# (id, name, description, location, status, archived, created_at, updated_at,
#  salary_min, salary_max, skill_requirements, notes)
POSITIONS: list[tuple] = [
    (
        "11111111-1111-1111-1111-111111111111",
        "Senior Rust Engineer",
        "Own the Rust backend rewrite.",
        "Beijing",
        "Open",
        0,
        "2026-11-10 09:00:00.000000",
        "2026-11-10 09:30:00.000000",
        40000,
        60000,
        "Rust, SQLx, Axum",
        "priority role",
    ),
    (
        "22222222-2222-2222-2222-222222222222",
        "Frontend Developer",
        "Keep the React app in step with the API.",
        "Shanghai",
        "OnHold",
        0,
        "2026-11-11 10:30:00.123456",
        "2026-11-11 10:30:00.123456",
        25000,
        35000,
        "TypeScript, React",
        None,
    ),
    (
        "33333333-3333-3333-3333-333333333333",
        "Data Analyst",
        None,
        None,
        "Draft",
        0,
        "2026-11-12 11:45:00.000000",
        "2026-11-12 11:45:00.000000",
        None,
        None,
        None,
        "location unknown yet",
    ),
    (
        "44444444-4444-4444-4444-444444444444",
        "Archived Recruiter",
        "Filled months ago.",
        "beijing",
        "Closed",
        1,
        "2026-11-13 12:00:00.500000",
        "2026-11-13 12:00:00.500000",
        30000,
        45000,
        "Sourcing",
        None,
    ),
    (
        "55555555-5555-5555-5555-555555555555",
        "Backend Engineer (Rust)",
        "Second Rust role, different team.",
        "Shenzhen",
        "Open",
        0,
        "2026-11-14 08:15:30.250000",
        "2026-11-14 08:15:30.250000",
        None,
        55000,
        "Rust, gRPC",
        None,
    ),
    (
        "66666666-6666-6666-6666-666666666666",
        "Product Manager",
        "Empty-string location, not NULL.",
        "",
        "Open",
        0,
        "2026-11-15 09:00:00.000000",
        "2026-11-15 10:00:00.000000",
        20000,
        40000,
        None,
        None,
    ),
    (
        "77777777-7777-7777-7777-777777777777",
        "ML Engineer",
        None,
        "Beijing",
        "Open",
        0,
        "2026-11-16 07:05:00.000000",
        "2026-11-16 07:05:00.000000",
        None,
        None,
        "PyTorch",
        "new headcount",
    ),
]

# (id, name, position_id, archived)
CANDIDATES: list[tuple] = [
    ("c0000000-0000-0000-0000-000000000001", "Alice", "11111111-1111-1111-1111-111111111111", 0),
    ("c0000000-0000-0000-0000-000000000002", "Bob", "11111111-1111-1111-1111-111111111111", 0),
    # Archived candidate: it still counts toward candidate_count in Python.
    ("c0000000-0000-0000-0000-000000000003", "Carol", "11111111-1111-1111-1111-111111111111", 1),
    ("c0000000-0000-0000-0000-000000000004", "Dave", "44444444-4444-4444-4444-444444444444", 0),
    ("c0000000-0000-0000-0000-000000000005", "Eve", "44444444-4444-4444-4444-444444444444", 0),
    ("c0000000-0000-0000-0000-000000000006", "Frank", "77777777-7777-7777-7777-777777777777", 0),
]


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", default="/tmp/ica-contract/seed.db")
    parser.add_argument("--schema", default=str(SCHEMA_PATH))
    args = parser.parse_args(argv)

    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    if out.exists():
        out.unlink()
    for sidecar in (f"{out}-wal", f"{out}-shm"):
        Path(sidecar).unlink(missing_ok=True)

    connection = sqlite3.connect(str(out))
    try:
        connection.executescript(Path(args.schema).read_text(encoding="utf-8"))
        connection.executemany(
            "INSERT INTO positions (id, name, description, location, status, archived,"
            " created_at, updated_at, salary_min, salary_max, skill_requirements, notes)"
            " VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            POSITIONS,
        )
        connection.executemany(
            "INSERT INTO candidates (id, name, source, position_id, status, archived,"
            " created_at, updated_at) VALUES (?, ?, 'boss', ?, 'New', ?,"
            " '2026-11-10 09:05:00.000000', '2026-11-10 09:05:00.000000')",
            [(row[0], row[1], row[2], row[3]) for row in CANDIDATES],
        )
        connection.commit()

        tables = connection.execute(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table'"
            " AND name NOT LIKE 'sqlite_%'"
        ).fetchone()[0]
        positions = connection.execute("SELECT COUNT(*) FROM positions").fetchone()[0]
        candidates = connection.execute("SELECT COUNT(*) FROM candidates").fetchone()[0]
    finally:
        connection.close()

    print(
        f"seeded {out}: {tables} tables, {positions} positions, "
        f"{candidates} candidates"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
