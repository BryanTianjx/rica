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

# ── W1 fixtures ────────────────────────────────────────────────────────────
#
# Rows that give the conversation / message / send-record / follow-up paths
# something to read. Everything is deterministic, and the counts are load
# bearing: `GET /api/messages?limit=2` must see exactly five matching rows so
# IMP-1's `python total=2 / rust total=5` is a real measurement.

ALICE = "c0000000-0000-0000-0000-000000000001"
BOB = "c0000000-0000-0000-0000-000000000002"
FRANK = "c0000000-0000-0000-0000-000000000006"

# (id, candidate_id, channel, is_active, last_message_at)
CONVERSATIONS: list[tuple] = [
    ("aaaaaaaa-0000-0000-0000-000000000001", ALICE, "boss", 1, "2026-11-12 18:00:00.000000"),
    # Bob's only conversation is inactive, so `POST /api/messages` must open a
    # new one instead of reusing this row.
    ("aaaaaaaa-0000-0000-0000-000000000002", BOB, "wx", 0, "2026-11-11 08:00:00.000000"),
]

# (id, candidate_id, conversation_id, sender_type, content, message_time)
MESSAGES: list[tuple] = [
    ("bbbbbbbb-0000-0000-0000-000000000001", ALICE, "aaaaaaaa-0000-0000-0000-000000000001",
     "Candidate", "msg 0", "2026-11-10 10:00:00.000000"),
    ("bbbbbbbb-0000-0000-0000-000000000002", ALICE, "aaaaaaaa-0000-0000-0000-000000000001",
     "Recruiter", "msg 1", "2026-11-10 10:05:00.000000"),
    ("bbbbbbbb-0000-0000-0000-000000000003", ALICE, "aaaaaaaa-0000-0000-0000-000000000001",
     "Candidate", "msg 2", "2026-11-11 11:00:00.000000"),
    # A message with no parsed time: it sorts last, not first.
    ("bbbbbbbb-0000-0000-0000-000000000004", ALICE, "aaaaaaaa-0000-0000-0000-000000000001",
     None, "msg 3", None),
    ("bbbbbbbb-0000-0000-0000-000000000005", ALICE, "aaaaaaaa-0000-0000-0000-000000000001",
     "Recruiter", "msg 4", "2026-11-12 18:00:00.000000"),
]

# (id, candidate_id, status, sent_at)
SEND_RECORDS: list[tuple] = [
    ("cccccccc-0000-0000-0000-000000000001", ALICE, "sent", "2026-11-10 10:00:00.000000"),
    ("cccccccc-0000-0000-0000-000000000002", ALICE, "candidate_replied", "2026-11-11 10:00:00.000000"),
    ("cccccccc-0000-0000-0000-000000000003", ALICE, "not_sent", "2026-11-12 10:00:00.000000"),
    ("cccccccc-0000-0000-0000-000000000004", BOB, "sent", "2026-11-13 10:00:00.000000"),
]

# (id, candidate_id, status)
FOLLOW_UP_TASKS: list[tuple] = [
    # `Completed` counts as a contact; `Pending` does not.
    ("dddddddd-0000-0000-0000-000000000001", ALICE, "Completed"),
    ("dddddddd-0000-0000-0000-000000000002", ALICE, "Pending"),
    ("dddddddd-0000-0000-0000-000000000003", BOB, "Completed"),
]

# (id, candidate_id, status)
DRAFTS: list[tuple] = [
    # `approved` is sendable; `generated` is not.
    ("eeeeeeee-0000-0000-0000-000000000001", ALICE, "approved"),
    ("eeeeeeee-0000-0000-0000-000000000002", ALICE, "generated"),
    ("eeeeeeee-0000-0000-0000-000000000003", BOB, "approved"),
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
            " created_at, updated_at) VALUES (?, ?, 'boss', ?, 'New', ?, ?, ?)",
            # A distinct `updated_at` per candidate (one microsecond apart) so
            # `ORDER BY updated_at DESC` is a total order: a tie would leave the
            # listing order to SQLite's sort implementation, which is not
            # something either implementation should be compared on.
            [
                (
                    row[0],
                    row[1],
                    row[2],
                    row[3],
                    f"2026-11-10 09:05:00.{index:06d}",
                    f"2026-11-10 09:05:00.{index:06d}",
                )
                for index, row in enumerate(CANDIDATES)
            ],
        )
        connection.executemany(
            "INSERT INTO conversations (id, candidate_id, channel, title, is_active,"
            " last_message_at, created_at, updated_at)"
            " VALUES (?, ?, ?, NULL, ?, ?, '2026-11-10 09:10:00.000000',"
            " '2026-11-10 09:10:00.000000')",
            CONVERSATIONS,
        )
        connection.executemany(
            "INSERT INTO messages (id, candidate_id, conversation_id, sender_type, content,"
            " message_time, channel, source, content_hash, created_at, updated_at)"
            " VALUES (?, ?, ?, ?, ?, ?, 'boss', 'manual', NULL,"
            " '2026-11-10 10:00:00.000000', '2026-11-10 10:00:00.000000')",
            MESSAGES,
        )
        connection.executemany(
            "INSERT INTO send_records (id, candidate_id, task_id, draft_id, channel, target,"
            " content_summary, status, sent_at, note, actor, created_at, updated_at)"
            " VALUES (?, ?, NULL, NULL, 'boss', 'target', 'summary', ?, ?, NULL,"
            " 'local_user', ?, ?)",
            [(row[0], row[1], row[2], row[3], row[3], row[3]) for row in SEND_RECORDS],
        )
        connection.executemany(
            "INSERT INTO follow_up_tasks (id, candidate_id, position_id, status, objective,"
            " reason, block_reason, recommended_at, expires_at, last_follow_up_at,"
            " follow_up_count, completed_at, result_type, result_note, result_recorded_at,"
            " idempotency_key, created_at, updated_at) VALUES (?, ?, NULL, ?, NULL, NULL, NULL,"
            " NULL, NULL, NULL, 0, NULL, NULL, NULL, NULL, NULL,"
            " '2026-11-10 09:00:00.000000', '2026-11-10 09:00:00.000000')",
            FOLLOW_UP_TASKS,
        )
        connection.executemany(
            "INSERT INTO generated_drafts (id, candidate_id, task_id, message_type, tone,"
            " status, objective, message_text, alternatives_json, context_references_json,"
            " uncertainties_json, warnings_json, validation_status, validation_messages_json,"
            " quality_status, provider, model, prompt_version, is_edited, approved_at,"
            " copied_at, rejected_at, created_at, updated_at)"
            " VALUES (?, ?, NULL, 'reply', 'friendly', ?, NULL, 'draft body', NULL, NULL,"
            " NULL, NULL, 'Passed', NULL, NULL, NULL, NULL, NULL, 0, NULL, NULL, NULL,"
            " '2026-11-10 09:00:00.000000', '2026-11-10 09:00:00.000000')",
            DRAFTS,
        )
        connection.commit()

        tables = connection.execute(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table'"
            " AND name NOT LIKE 'sqlite_%'"
        ).fetchone()[0]
        positions = connection.execute("SELECT COUNT(*) FROM positions").fetchone()[0]
        candidates = connection.execute("SELECT COUNT(*) FROM candidates").fetchone()[0]
        conversations = connection.execute("SELECT COUNT(*) FROM conversations").fetchone()[0]
        messages = connection.execute("SELECT COUNT(*) FROM messages").fetchone()[0]
        send_records = connection.execute("SELECT COUNT(*) FROM send_records").fetchone()[0]
    finally:
        connection.close()

    print(
        f"seeded {out}: {tables} tables, {positions} positions, "
        f"{candidates} candidates, {conversations} conversations, "
        f"{messages} messages, {send_records} send records"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
