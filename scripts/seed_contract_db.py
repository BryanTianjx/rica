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
import hashlib
import json
import re
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
# ── W4: import sessions and their records ────────────────────────────────
# The export comparison needs import_sessions to carry a row whose
# `raw_text` must be EXCLUDED from the export — so the exclusion is proved by
# the data, not only by the code.
IMPORT_SESSIONS: list[tuple] = [
    # (id, candidate_id, source, raw_text, status, detected_duplicates,
    #  confirmed_at, error_message, created_at)
    ("ffffffff-0000-0000-0000-000000000001", ALICE, "paste_import",
     "候选人：种子数据里的原文\n我：回复", "previewed", 0, None, None,
     "2026-11-10 11:00:00.000000"),
    ("ffffffff-0000-0000-0000-000000000002", BOB, "paste_import",
     "", "rejected", 0, None, None, "2026-11-10 11:05:00.000000"),
    ("ffffffff-0000-0000-0000-000000000003", ALICE, "paste_import",
     "候选人：已确认\n我：好的", "confirmed", 1, "2026-11-10 11:10:00.000000",
     None, "2026-11-10 11:10:00.000000"),
    # A `failed` session: the parse produced nothing.
    ("ffffffff-0000-0000-0000-000000000004", ALICE, "paste_import",
     "", "failed", 0, None, "no messages parsed", "2026-11-10 11:15:00.000000"),
]

# (id, session_id, sequence, sender_type, content, message_time, status,
#  duplicate_of_message_id, content_hash, validation_messages)
IMPORT_RECORDS: list[tuple] = [
    ("ffffffff-1111-0000-0000-000000000001",
     "ffffffff-0000-0000-0000-000000000001", 0, "Candidate",
     "种子数据里的原文", None, "pending", None, "hash-seed-1", None),
    ("ffffffff-1111-0000-0000-000000000002",
     "ffffffff-0000-0000-0000-000000000001", 1, "Recruiter",
     "回复", None, "pending", None, "hash-seed-2", None),
    ("ffffffff-1111-0000-0000-000000000003",
     "ffffffff-0000-0000-0000-000000000003", 0, "Candidate",
     "已确认", None, "pending", None, "hash-seed-3", None),
    # A duplicate record: `duplicate_of_message_id` is set, which the export
    # must carry verbatim.
    ("ffffffff-1111-0000-0000-000000000004",
     "ffffffff-0000-0000-0000-000000000003", 1, "Candidate",
     "重复的消息", None, "duplicate",
     "bbbbbbbb-0000-0000-0000-000000000001", "hash-seed-4", "重复内容"),
]

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

#: One comparison group shared by two drafts, so `GET /api/draft-groups/{id}`
#: and the supersede-on-select side effect have something to act on.
DRAFT_GROUP = "dddddddd-0000-0000-0000-000000000001"

#: The routing blob on draft `…0004`. `actual_tier` must win over
#: `requested_tier` in `routing_tier`, and `reason` becomes `routing_reason`
#: (`app/api/mappers.py::draft_to_read`).
DRAFT_ROUTING = (
    '{"requested_tier": "standard", "actual_tier": "economy", "reason": "budget"}'
)

# (id, candidate_id, status, draft_group_id, routing_decision_json, created_at)
DRAFTS: list[tuple] = [
    # `approved` is sendable; `generated` is not.
    ("eeeeeeee-0000-0000-0000-000000000001", ALICE, "approved", None, None,
     "2026-11-10 09:00:00.000000"),
    ("eeeeeeee-0000-0000-0000-000000000002", ALICE, "generated", None, None,
     "2026-11-10 09:00:01.000000"),
    ("eeeeeeee-0000-0000-0000-000000000003", BOB, "approved", None, None,
     "2026-11-10 09:00:00.000000"),
    # W3 fixtures. Alice is deleted by the FK section, so anything W3 asserts
    # has to hang off Bob.
    ("eeeeeeee-0000-0000-0000-000000000004", BOB, "generated", None, DRAFT_ROUTING,
     "2026-11-10 09:00:02.000000"),
    ("eeeeeeee-0000-0000-0000-000000000005", BOB, "generated", DRAFT_GROUP, None,
     "2026-11-10 09:00:03.000000"),
    ("eeeeeeee-0000-0000-0000-000000000006", BOB, "generated", DRAFT_GROUP, None,
     "2026-11-10 09:00:04.000000"),
]


# ── W7: browser candidate-extraction drafts ────────────────────────────────
# The confirm-candidate flow needs browser_extract sessions with drafts in
# every guard state. The digests are computed with the SAME expressions as
# `app/domain/candidate_extraction.py` (plain hashlib + json, so the seed does
# not need the app importable) — the draft's `review_digest` MUST match what
# the approved fields render to, or the confirm path would refuse everything.

EXTRACTION_FIELD_NAMES = (
    "name", "job_intention", "expected_salary", "expected_location",
    "experience_summary", "skill_summary",
)

_PUNCT_RE = re.compile(
    r"[\s\u3000·•.,，。;；:：!！?？'\"“”‘’()（）\[\]【】<>《》_\-/\\|]+"
)
_NAME_SUFFIX_RE = re.compile(r"(的个人简历|的简历|个人简历|简历|的主页|主页)$")


def _review_digest(fields: dict) -> str:
    payload = {key: fields.get(key) for key in EXTRACTION_FIELD_NAMES}
    return hashlib.sha256(
        json.dumps(payload, ensure_ascii=False, sort_keys=True).encode("utf-8")
    ).hexdigest()


def _normalize_name(name: str) -> str:
    text = _NAME_SUFFIX_RE.sub("", _PUNCT_RE.sub("", name.strip()))
    return text.lower()[:200]


def _identity_key(name: str, domain: str) -> str:
    payload = f"{_normalize_name(name)}\x1f{domain.strip().lower()}"
    return hashlib.sha256(payload.encode("utf-8")).hexdigest()


WANGFANG = {"name": "王芳", "expected_location": "杭州", "skill_summary": "Go, Kubernetes"}
_WANGFANG_DIGEST = _review_digest(WANGFANG)
_WANGFANG_IDENTITY = _identity_key("王芳", "zhipin.com")
_WANGFANG_NAME_KEY = "王芳"
_WANGFANG_FIELDS_JSON = json.dumps(WANGFANG, ensure_ascii=False)
_WANGFANG_ORIGINS = '{"name": {"method": "label", "ref": "n7"}}'
_WANGFANG_MISSING = '["job_intention", "expected_salary", "experience_summary"]'

# (session_id, extraction_id, status, review_digest, duplicate_kind,
#  duplicate_of_candidate_id, duplicate_evidence_json, fields_json)
BROWSER_DRAFTS = [
    # draft: refuses with review_required; also the reject side-effect target
    ("b0000000-0000-0000-0000-000000000001", "e0000000-0000-0000-0000-000000000001",
     "draft", None, None, None, None, _WANGFANG_FIELDS_JSON),
    # approved, clean digest: the happy path
    ("b0000000-0000-0000-0000-000000000002", "e0000000-0000-0000-0000-000000000002",
     "approved", _WANGFANG_DIGEST, None, None, None, _WANGFANG_FIELDS_JSON),
    # approved, clean digest #2: status/note variants
    ("b0000000-0000-0000-0000-000000000003", "e0000000-0000-0000-0000-000000000003",
     "approved", _WANGFANG_DIGEST, None, None, None, _WANGFANG_FIELDS_JSON),
    # approved, clean digest #3: a missing position link
    ("b0000000-0000-0000-0000-000000000004", "e0000000-0000-0000-0000-000000000004",
     "approved", _WANGFANG_DIGEST, None, None, None, _WANGFANG_FIELDS_JSON),
    # approved but the digest no longer matches the fields: review_stale
    ("b0000000-0000-0000-0000-000000000005", "e0000000-0000-0000-0000-000000000005",
     "approved", "0" * 64, None, None, None, _WANGFANG_FIELDS_JSON),
    # approved + a stored name-duplicate verdict: blocked unless overridden
    ("b0000000-0000-0000-0000-000000000006", "e0000000-0000-0000-0000-000000000006",
     "approved", _WANGFANG_DIGEST, "name",
     "c0000000-0000-0000-0000-000000000001",
     json.dumps({"signal": "same_normalised_name",
                 "matched_candidate_ids": ["c0000000-0000-0000-0000-000000000001"],
                 "match_count": 1}, ensure_ascii=False),
     _WANGFANG_FIELDS_JSON),
    # already confirmed: refuses again
    ("b0000000-0000-0000-0000-000000000007", "e0000000-0000-0000-0000-000000000007",
     "confirmed", _WANGFANG_DIGEST, None, None, None, _WANGFANG_FIELDS_JSON),
]

BROWSER_IMPORT_SESSIONS = [
    # (id, candidate_id, source, raw_text, status, detected_duplicate_count,
    #  confirmed_at, error_message, updated_at)
    ("b0000000-0000-0000-0000-000000000001", None, "browser_extract",
     "姓名：王芳\n技能：Go, Kubernetes", "previewed", 1, None, None,
     "2026-11-18 12:00:00.000000"),
    ("b0000000-0000-0000-0000-000000000002", None, "browser_extract",
     "姓名：王芳\n技能：Go, Kubernetes", "previewed", 0, None, None,
     "2026-11-18 12:00:00.000000"),
    ("b0000000-0000-0000-0000-000000000003", None, "browser_extract",
     "姓名：王芳\n技能：Go, Kubernetes", "previewed", 0, None, None,
     "2026-11-18 12:00:00.000000"),
    ("b0000000-0000-0000-0000-000000000004", None, "browser_extract",
     "姓名：王芳\n技能：Go, Kubernetes", "previewed", 0, None, None,
     "2026-11-18 12:00:00.000000"),
    ("b0000000-0000-0000-0000-000000000005", None, "browser_extract",
     "姓名：王芳\n技能：Go, Kubernetes", "previewed", 0, None, None,
     "2026-11-18 12:00:00.000000"),
    ("b0000000-0000-0000-0000-000000000006", None, "browser_extract",
     "姓名：王芳\n技能：Go, Kubernetes", "previewed", 1, None, None,
     "2026-11-18 12:00:00.000000"),
    # confirmed session + confirmed draft: already_confirmed
    ("b0000000-0000-0000-0000-000000000007", "c0000000-0000-0000-0000-000000000003",
     "browser_extract", "", "confirmed", 0,
     "2026-11-18 12:05:00.000000", None, "2026-11-18 12:05:00.000000"),
    # a browser session WITHOUT a draft: extraction_not_found
    ("b0000000-0000-0000-0000-000000000008", None, "browser_extract",
     "姓名：无名氏", "previewed", 0, None, None, "2026-11-18 12:00:00.000000"),
]

BROWSER_EXTRACTION_ROWS = [
    (
        row[1],                      # id
        row[0],                      # session_id (the browser session)
        "a0000000-0000-0000-0000-00000000000" + row[0][-1],  # snapshot_action_id
        row[0],                      # import_session_id
        "https://www.zhipin.com/job_detail/1998.html",
        "zhipin.com",
        "f" * 64,                    # snapshot_sha256
        "browser/artifacts/x.snapshot.json",
        42, 3,                       # node_count, masked_fields
        row[7],                      # fields_json
        _WANGFANG_ORIGINS,
        _WANGFANG_MISSING,
        _WANGFANG_NAME_KEY, _WANGFANG_IDENTITY,
        row[4],                      # duplicate_kind
        row[5],                      # duplicate_of_candidate_id
        row[6],                      # duplicate_evidence_json
        row[2],                      # status
        row[3],                      # review_digest
        ("2026-11-18 11:00:00.000000" if row[2] in ("approved", "confirmed") else None),
        ("c0000000-0000-0000-0000-000000000003" if row[2] == "confirmed" else None),
        ("2026-11-18 12:05:00.000000" if row[2] == "confirmed" else None),
        "2026-11-18 10:00:00.000000", "2026-11-18 10:00:00.000000",
    )
    for row in BROWSER_DRAFTS
]


def seed_browser_fixtures(connection) -> None:
    """Insert the W7 browser_extract sessions and their extraction drafts."""
    connection.executemany(
        "INSERT INTO import_sessions (id, candidate_id, source, raw_text, status,"
        " detected_duplicate_count, confirmed_at, error_message, created_at, updated_at)"
        " VALUES (?, ?, ?, ?, ?, ?, ?, ?, '2026-11-18 12:00:00.000000', ?)",
        [(row[0], row[1], row[2], row[3], row[4], row[5], row[6], row[7], row[8])
         for row in BROWSER_IMPORT_SESSIONS],
    )
    connection.executemany(
        "INSERT INTO browser_candidate_extractions (id, session_id,"
        " snapshot_action_id, import_session_id, source_url, source_domain,"
        " snapshot_sha256, snapshot_path, snapshot_node_count,"
        " snapshot_masked_fields, fields_json, field_origin_json,"
        " missing_fields_json, name_key, identity_key, duplicate_kind,"
        " duplicate_of_candidate_id, duplicate_evidence_json, status,"
        " review_digest, reviewed_at, candidate_id, confirmed_at, mask_hits,"
        " created_at, updated_at)"
        " VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,"
        " ?, ?, ?, ?, ?, ?)",
        [row[:23] + (3,) + row[23:] for row in BROWSER_EXTRACTION_ROWS],
    )


def seed_local_identity(connection) -> None:
    """The deployed local-mode identity: default tenant + synthetic user.

    The sharing endpoints' owner FK needs a users row for the local actor
    (...0001), and a fresh database does not have one — the baseline's own
    empty local DB fails the same way. Registration then joins this tenant
    as a member: the bootstrap-admin path is exercised by the Rust unit
    tests instead of this comparison.
    """
    now = "2026-11-10 08:00:00.000000"
    connection.execute(
        "INSERT OR IGNORE INTO tenants (id, name, created_at, updated_at)"
        " VALUES ('00000000-0000-0000-0000-000000000000', 'Local', ?, ?)",
        (now, now),
    )
    connection.execute(
        "INSERT OR IGNORE INTO users (id, tenant_id, email, password_hash,"
        " display_name, role, is_active, created_at, updated_at)"
        " VALUES ('00000000-0000-0000-0000-000000000001',"
        " '00000000-0000-0000-0000-000000000000', '', '', 'Local User',"
        " 'admin', 1, ?, ?)",
        (now, now),
    )


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
        seed_local_identity(connection)
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
            "INSERT INTO import_sessions (id, candidate_id, source, raw_text, status,"
            " detected_duplicate_count, confirmed_at, error_message, created_at,"
            " updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            [
                (row[0], row[1], row[2], row[3], row[4], row[5], row[6], row[7],
                 row[8], row[8])
                for row in IMPORT_SESSIONS
            ],
        )
        connection.executemany(
            "INSERT INTO import_records (id, session_id, sequence, sender_type,"
            " content, message_time, status, duplicate_of_message_id, content_hash,"
            " validation_messages, created_at, updated_at)"
            " VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            [
                (row[0], row[1], row[2], row[3], row[4], row[5], row[6], row[7],
                 row[8], row[9], "2026-11-10 11:00:00.000000",
                 "2026-11-10 11:00:00.000000")
                for row in IMPORT_RECORDS
            ],
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
            " copied_at, rejected_at, draft_group_id, routing_decision_json,"
            " created_at, updated_at)"
            " VALUES (?, ?, NULL, 'reply', 'friendly', ?, NULL, 'draft body', NULL, NULL,"
            " NULL, NULL, 'Passed', NULL, NULL, NULL, NULL, NULL, 0, NULL, NULL, NULL,"
            " ?, ?, ?, ?)",
            [
                (draft_id, candidate_id, status, group_id, routing, created_at, created_at)
                for draft_id, candidate_id, status, group_id, routing, created_at in DRAFTS
            ],
        )
        connection.commit()

        seed_browser_fixtures(connection)
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
