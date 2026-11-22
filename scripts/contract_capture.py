#!/usr/bin/env python3
"""Capture the ICA contract from a running server: responses *and* final rows.

The script plays one fixed scenario against any base URL — the Python baseline
or the Rust port — and writes a single JSON document with three parts:

``cases``
    The ordered request/response pairs. Names are the diff keys, so they must be
    identical on both sides.

``db_state``
    Every row of the business tables *after* the scenario, so a response that
    looks right while the write was wrong cannot pass. Read-only: the capture
    never writes to the database itself.

``started_at``
    When the scenario began. ``contract_diff.py`` uses it to render timestamps
    produced *during* the run (the ``created_at`` of a new row, a defaulted
    ``sent_at``) as one placeholder on both sides, while seeded timestamps stay
    literal and are compared exactly.

Each side must run against its **own copy** of the seed database, because the
scenario writes: `contract_verify.sh` makes two copies and points one server at
each.

Usage:
    python3 scripts/contract_capture.py --base-url http://127.0.0.1:8010 \
        --db /tmp/ica-contract/python.db --out docs/contract-baseline/w1-python.json

Standard library only: no third-party dependency, so it runs under any python3.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sqlite3
import sys
import urllib.error
import urllib.parse
import urllib.request
from datetime import UTC, datetime
from pathlib import Path

ALICE = "c0000000-0000-0000-0000-000000000001"
BOB = "c0000000-0000-0000-0000-000000000002"
FRANK = "c0000000-0000-0000-0000-000000000006"
ALICE_CONVERSATION = "aaaaaaaa-0000-0000-0000-000000000001"
BOB_INACTIVE_CONVERSATION = "aaaaaaaa-0000-0000-0000-000000000002"
POSITION_WITH_CANDIDATES = "11111111-1111-1111-1111-111111111111"
POSITION_ARCHIVED = "44444444-4444-4444-4444-444444444444"
POSITION_DRAFT = "33333333-3333-3333-3333-333333333333"
#: Only Frank points at this position, so deleting it touches exactly one
#: candidate (see the FK section of the scenario).
POSITION_SINGLE_CANDIDATE = "77777777-7777-7777-7777-777777777777"

#: W3 draft fixtures (see `seed_contract_db.py`). Alice is cascade-deleted by
#: the FK section, so every draft assertion below hangs off Bob.
BOB_DRAFT_APPROVED = "eeeeeeee-0000-0000-0000-000000000003"
BOB_DRAFT_ROUTED = "eeeeeeee-0000-0000-0000-000000000004"
BOB_DRAFT_GROUP = "dddddddd-0000-0000-0000-000000000001"
BOB_DRAFT_GROUP_A = "eeeeeeee-0000-0000-0000-000000000005"
BOB_DRAFT_GROUP_B = "eeeeeeee-0000-0000-0000-000000000006"
MISSING_DRAFT = "does-not-exist"

#: The scenario. Order matters: every read-only case runs before the first write,
#: so the declared improvements are measured on the untouched seed (five
#: messages, seven positions, …). ``capture`` pulls a generated id out of the
#: response into the substitution context used by later paths.
CASES: list[dict] = [
    # ── R0: health and positions (unchanged, must not regress) ─────────────
    {"name": "health", "method": "GET", "path": "/health"},
    {"name": "positions_default", "method": "GET", "path": "/api/positions"},
    {"name": "positions_limit2", "method": "GET", "path": "/api/positions?limit=2"},
    {"name": "positions_page2", "method": "GET", "path": "/api/positions?limit=2&offset=2"},
    {"name": "positions_offset_beyond_total", "method": "GET", "path": "/api/positions?offset=100"},
    {"name": "positions_include_archived", "method": "GET",
     "path": "/api/positions?include_archived=true"},
    {"name": "positions_include_archived_bool1", "method": "GET",
     "path": "/api/positions?include_archived=1"},
    {"name": "positions_search_hit", "method": "GET", "path": "/api/positions?search=beijing"},
    {"name": "positions_search_hit_with_archived", "method": "GET",
     "path": "/api/positions?search=beijing&include_archived=true"},
    {"name": "positions_search_case_insensitive", "method": "GET",
     "path": "/api/positions?search=RUST"},
    {"name": "positions_search_miss", "method": "GET",
     "path": "/api/positions?search=zzz-nothing"},
    {"name": "positions_search_padded", "method": "GET",
     "path": "/api/positions?search=%20beijing%20"},
    {"name": "positions_limit_max", "method": "GET", "path": "/api/positions?limit=500"},
    {"name": "positions_limit_zero", "method": "GET", "path": "/api/positions?limit=0"},
    {"name": "positions_limit_over_max", "method": "GET", "path": "/api/positions?limit=501"},
    {"name": "positions_offset_negative", "method": "GET", "path": "/api/positions?offset=-1"},
    {"name": "positions_search_too_long", "method": "GET",
     "path": "/api/positions?search=" + "a" * 201},
    {"name": "position_detail", "method": "GET",
     "path": f"/api/positions/{POSITION_WITH_CANDIDATES}"},
    {"name": "position_detail_archived", "method": "GET",
     "path": f"/api/positions/{POSITION_ARCHIVED}"},
    {"name": "position_detail_missing", "method": "GET", "path": "/api/positions/does-not-exist"},

    # ── Router and parser boundaries (R0 audit F-02..F-05, F-12) ───────────
    # A repeated key must not break the envelope: Starlette keeps the LAST
    # occurrence (`?limit=1&limit=2` means limit=2, answered 200).
    {"name": "positions_duplicate_limit", "method": "GET",
     "path": "/api/positions?limit=1&limit=2"},
    # Integers past i64::MAX are still integers to pydantic; the envelope can
    # echo anything up to u64::MAX (`has_more=false`, empty page).
    {"name": "positions_offset_beyond_i64", "method": "GET",
     "path": "/api/positions?offset=9223372036854775808"},
    # Python's int grammar accepts underscores between digits (`1_0` == 10).
    {"name": "positions_offset_underscore", "method": "GET",
     "path": "/api/positions?offset=1_0"},
    # Past u64::MAX the port cannot echo the offset — declared IMP-4 below.
    {"name": "positions_offset_beyond_u64", "method": "GET",
     "path": "/api/positions?offset=18446744073709551616"},
    # pydantic strips whitespace for ints but NOT for bools: `" true "` is a
    # 422 `bool_parsing` on both sides.
    {"name": "positions_include_archived_padded", "method": "GET",
     "path": "/api/positions?include_archived=%20true%20"},
    # Trailing slashes are 307-redirected with method and query preserved —
    # they must not be a hard 404 (a browser or hand-built link would break).
    # Three shapes: bare path, query string, parameterised path — the `Location`
    # differs in each, so one case cannot cover the others.
    {"name": "positions_trailing_slash", "method": "GET", "path": "/api/positions/"},
    {"name": "positions_trailing_slash_query", "method": "GET",
     "path": "/api/positions/?limit=2&search=abc"},
    {"name": "position_detail_trailing_slash", "method": "GET",
     "path": f"/api/positions/{POSITION_WITH_CANDIDATES}/"},
    # No endpoint declares HEAD, so Starlette answers 405 (envelope headers,
    # body suppressed) where axum would serve a body-stripped 200.
    {"name": "positions_head_not_allowed", "method": "HEAD", "path": "/api/positions"},
    # Same for any other unserved method on a served path: 405 envelope.
    {"name": "positions_put_not_allowed", "method": "PUT", "path": "/api/positions",
     "json": {"name": "wrong collection"}},
    {"name": "positions_options_not_allowed", "method": "OPTIONS", "path": "/api/positions"},

    # ── Candidates (reads) ─────────────────────────────────────────────────
    {"name": "candidates_default", "method": "GET", "path": "/api/candidates"},
    {"name": "candidates_limit2", "method": "GET", "path": "/api/candidates?limit=2"},
    {"name": "candidates_page2", "method": "GET", "path": "/api/candidates?limit=2&offset=2"},
    {"name": "candidates_include_archived", "method": "GET",
     "path": "/api/candidates?include_archived=true"},
    {"name": "candidates_search_name", "method": "GET", "path": "/api/candidates?search=alice"},
    {"name": "candidates_search_case", "method": "GET", "path": "/api/candidates?search=ALICE"},
    {"name": "candidates_search_miss", "method": "GET",
     "path": "/api/candidates?search=zzz-nothing"},
    {"name": "candidates_status_filter", "method": "GET", "path": "/api/candidates?status=New"},
    {"name": "candidates_status_empty", "method": "GET",
     "path": "/api/candidates?status=Contacted"},
    {"name": "candidates_position_filter", "method": "GET",
     "path": f"/api/candidates?position_id={POSITION_WITH_CANDIDATES}"},
    {"name": "candidates_bad_status", "method": "GET", "path": "/api/candidates?status=Nope"},
    {"name": "candidates_limit_over_max", "method": "GET", "path": "/api/candidates?limit=501"},
    {"name": "candidate_detail", "method": "GET", "path": f"/api/candidates/{ALICE}"},
    {"name": "candidate_detail_second", "method": "GET", "path": f"/api/candidates/{BOB}"},
    {"name": "candidate_detail_missing", "method": "GET", "path": "/api/candidates/does-not-exist"},

    # ── Conversations (reads) ──────────────────────────────────────────────
    {"name": "conversations_default", "method": "GET", "path": "/api/conversations"},
    {"name": "conversations_limit2", "method": "GET", "path": "/api/conversations?limit=2"},
    {"name": "conversations_by_candidate", "method": "GET",
     "path": f"/api/conversations?candidate_id={BOB}"},
    {"name": "conversations_unknown_candidate", "method": "GET",
     "path": "/api/conversations?candidate_id=does-not-exist"},

    # ── Messages and timeline (reads) — IMP-1 is measured here ─────────────
    {"name": "messages_default", "method": "GET", "path": "/api/messages"},
    {"name": "messages_limit2", "method": "GET", "path": "/api/messages?limit=2"},
    {"name": "messages_limit2_offset1", "method": "GET", "path": "/api/messages?limit=2&offset=1"},
    {"name": "messages_offset_beyond_total", "method": "GET", "path": "/api/messages?offset=100"},
    {"name": "messages_by_candidate", "method": "GET",
     "path": f"/api/messages?candidate_id={ALICE}"},
    {"name": "messages_by_empty_candidate", "method": "GET",
     "path": f"/api/messages?candidate_id={FRANK}"},
    {"name": "messages_by_conversation", "method": "GET",
     "path": f"/api/messages?conversation_id={ALICE_CONVERSATION}"},
    {"name": "messages_limit_over_max", "method": "GET", "path": "/api/messages?limit=1001"},
    {"name": "timeline_default", "method": "GET", "path": f"/api/candidates/{ALICE}/timeline"},
    {"name": "timeline_limit2", "method": "GET",
     "path": f"/api/candidates/{ALICE}/timeline?limit=2"},
    {"name": "timeline_missing_candidate", "method": "GET",
     "path": "/api/candidates/does-not-exist/timeline"},

    # ── Send records (reads) ───────────────────────────────────────────────
    {"name": "send_records_default", "method": "GET", "path": "/api/send-records"},
    {"name": "send_records_limit2", "method": "GET", "path": "/api/send-records?limit=2"},
    {"name": "send_records_by_candidate", "method": "GET",
     "path": f"/api/send-records?candidate_id={ALICE}"},
    {"name": "send_records_by_status", "method": "GET", "path": "/api/send-records?status=sent"},
    {"name": "send_records_bad_status", "method": "GET", "path": "/api/send-records?status=maybe"},

    # ── IMP-2: explicit null on a NOT NULL column (declared difference) ────
    {"name": "position_patch_null_name", "method": "PATCH",
     "path": f"/api/positions/{POSITION_DRAFT}", "json": {"name": None}},
    {"name": "position_patch_null_archived", "method": "PATCH",
     "path": f"/api/positions/{POSITION_DRAFT}", "json": {"archived": None}},

    # ── Positions (writes) ─────────────────────────────────────────────────
    {"name": "position_create", "method": "POST", "path": "/api/positions",
     "json": {"name": "Contract Created", "location": "Wuhan", "salary_min": 10000,
              "status": "Draft"},
     "capture": {"created_position_id": "id"}},
    {"name": "position_detail_created", "method": "GET",
     "path": "/api/positions/{created_position_id}"},
    {"name": "position_patch", "method": "PATCH",
     "path": "/api/positions/{created_position_id}",
     "json": {"name": "Contract Renamed", "location": "Wuhan", "salary_max": 20000}},
    {"name": "position_patch_status_null", "method": "PATCH",
     "path": "/api/positions/{created_position_id}", "json": {"status": None}},
    {"name": "position_patch_clear_nullable", "method": "PATCH",
     "path": "/api/positions/{created_position_id}", "json": {"location": None}},
    {"name": "position_patch_archived", "method": "PATCH",
     "path": "/api/positions/{created_position_id}", "json": {"archived": True}},
    {"name": "position_patch_missing", "method": "PATCH", "path": "/api/positions/does-not-exist",
     "json": {"name": "x"}},
    {"name": "position_delete", "method": "DELETE", "path": "/api/positions/{created_position_id}"},
    {"name": "position_detail_after_delete", "method": "GET",
     "path": "/api/positions/{created_position_id}"},
    {"name": "position_delete_missing", "method": "DELETE",
     "path": "/api/positions/does-not-exist"},

    # ── Candidates (writes) ────────────────────────────────────────────────
    {"name": "candidate_create", "method": "POST", "path": "/api/candidates",
     "json": {"name": "  Contract Candidate  ", "position_id": POSITION_DRAFT,
              "status": "Contacted", "expected_location": "Wuhan"},
     "capture": {"created_candidate_id": "id"}},
    {"name": "candidate_detail_created", "method": "GET",
     "path": "/api/candidates/{created_candidate_id}"},
    {"name": "candidate_patch", "method": "PATCH",
     "path": "/api/candidates/{created_candidate_id}",
     "json": {"status": "Replied", "notes": "patched"}},
    {"name": "candidate_patch_null_position", "method": "PATCH",
     "path": "/api/candidates/{created_candidate_id}", "json": {"position_id": None}},
    {"name": "candidate_patch_null_name", "method": "PATCH",
     "path": "/api/candidates/{created_candidate_id}", "json": {"name": None}},
    {"name": "candidate_patch_missing", "method": "PATCH",
     "path": "/api/candidates/does-not-exist", "json": {"name": "x"}},
    {"name": "candidate_delete", "method": "DELETE",
     "path": "/api/candidates/{created_candidate_id}"},
    {"name": "candidate_detail_after_delete", "method": "GET",
     "path": "/api/candidates/{created_candidate_id}"},

    # ── Conversations and messages (writes) ────────────────────────────────
    {"name": "conversation_create", "method": "POST", "path": "/api/conversations",
     "json": {"candidate_id": FRANK, "channel": "boss", "title": "Contract thread"},
     "capture": {"created_conversation_id": "id"}},
    {"name": "conversation_create_missing_candidate", "method": "POST",
     "path": "/api/conversations", "json": {"candidate_id": "does-not-exist"}},
    {"name": "message_create_explicit_conversation", "method": "POST", "path": "/api/messages",
     "json": {"candidate_id": ALICE, "conversation_id": ALICE_CONVERSATION,
              "content": "  Contract   Message  ", "sender_type": "Recruiter",
              "message_time": "2026-11-14T09:30:00Z"}},
    # No conversation_id and the candidate's only conversation is inactive:
    # both sides must open a fresh `boss` thread.
    {"name": "message_create_auto_conversation", "method": "POST", "path": "/api/messages",
     "json": {"candidate_id": BOB, "content": "auto thread"},
     "capture": {"auto_conversation_id": "conversation_id"}},
    {"name": "message_create_wrong_conversation", "method": "POST", "path": "/api/messages",
     "json": {"candidate_id": ALICE, "conversation_id": BOB_INACTIVE_CONVERSATION,
              "content": "wrong thread"}},
    {"name": "message_create_missing_conversation", "method": "POST", "path": "/api/messages",
     "json": {"candidate_id": ALICE, "conversation_id": "does-not-exist", "content": "x"}},
    {"name": "message_create_missing_candidate", "method": "POST", "path": "/api/messages",
     "json": {"candidate_id": "does-not-exist", "content": "x"}},
    {"name": "message_create_empty_content", "method": "POST", "path": "/api/messages",
     "json": {"candidate_id": ALICE, "content": ""}},
    {"name": "messages_after_writes", "method": "GET", "path": "/api/messages"},
    {"name": "timeline_after_writes", "method": "GET", "path": f"/api/candidates/{ALICE}/timeline"},
    {"name": "conversations_after_writes", "method": "GET", "path": "/api/conversations"},

    # ── Send records (writes) ──────────────────────────────────────────────
    {"name": "send_record_create", "method": "POST", "path": "/api/send-records",
     "json": {"candidate_id": ALICE, "status": "sent", "target": "boss-42",
              "content_summary": "contract send"}},
    {"name": "send_record_create_explicit_time", "method": "POST", "path": "/api/send-records",
     "json": {"candidate_id": BOB, "status": "candidate_replied",
              "sent_at": "2026-11-14T08:15:00Z", "channel": "wx"}},
    {"name": "send_record_task_conflict", "method": "POST", "path": "/api/send-records",
     "json": {"candidate_id": ALICE, "status": "sent",
              "task_id": "dddddddd-0000-0000-0000-000000000003"}},
    {"name": "send_record_draft_not_sendable", "method": "POST", "path": "/api/send-records",
     "json": {"candidate_id": ALICE, "status": "sent",
              "draft_id": "eeeeeeee-0000-0000-0000-000000000002"}},
    {"name": "send_record_missing_task", "method": "POST", "path": "/api/send-records",
     "json": {"candidate_id": ALICE, "status": "sent", "task_id": "does-not-exist"}},
    {"name": "send_record_bad_status", "method": "POST", "path": "/api/send-records",
     "json": {"candidate_id": ALICE, "status": "delivered"}},
    {"name": "send_records_after_writes", "method": "GET", "path": "/api/send-records"},

    # ── Foreign keys: SET NULL, then CASCADE ───────────────────────────────
    # The position has exactly one candidate on purpose: Python's ORM stamps the
    # affected children with one `utcnow()` per row, and two rows updated in the
    # same microsecond would leave the later `updated_at DESC` order to chance.
    # With a single child the touch is deterministic.
    {"name": "position_delete_set_null", "method": "DELETE",
     "path": f"/api/positions/{POSITION_SINGLE_CANDIDATE}"},
    {"name": "candidate_detail_after_position_delete", "method": "GET",
     "path": f"/api/candidates/{FRANK}"},
    {"name": "candidate_delete_with_children", "method": "DELETE",
     "path": f"/api/candidates/{ALICE}"},
    {"name": "messages_after_cascade", "method": "GET", "path": "/api/messages"},
    {"name": "send_records_after_cascade", "method": "GET", "path": "/api/send-records"},
    {"name": "conversations_after_cascade", "method": "GET", "path": "/api/conversations"},
    {"name": "timeline_after_cascade", "method": "GET", "path": f"/api/candidates/{BOB}/timeline"},

    # ── W2: follow-ups (state machine) + policy + dashboard ────────────────
    # Runs after the FK section: Alice is gone (cascade removed her tasks),
    # Frank has no position, Bob keeps his Completed task and message.
    {"name": "follow_ups_list_default", "method": "GET", "path": "/api/follow-ups"},
    {"name": "follow_up_get_missing", "method": "GET", "path": "/api/follow-ups/does-not-exist"},
    # First use creates the default policy row; the diff sees it in db_state.
    {"name": "follow_up_policy_get", "method": "GET", "path": "/api/follow-ups/policy/current"},
    {"name": "follow_up_policy_patch", "method": "PATCH",
     "path": "/api/follow-ups/policy/current", "json": {"max_active_follow_ups": 3}},
    {"name": "follow_up_policy_patch_invalid", "method": "PATCH",
     "path": "/api/follow-ups/policy/current", "json": {"max_active_follow_ups": 0}},
    {"name": "follow_up_policy_patch_null", "method": "PATCH",
     "path": "/api/follow-ups/policy/current", "json": {"task_validity_days": None}},
    {"name": "follow_up_policy_restore", "method": "PATCH",
     "path": "/api/follow-ups/policy/current", "json": {"max_active_follow_ups": 2}},
    # The decision engine: every case lands on a branch whose reason string is
    # stable (fixed seed timestamps); the humanized-duration branches are
    # pinned in w2-python-facts.md instead, they would flip across an hour
    # boundary between the two captures.
    {"name": "follow_up_evaluate_frank", "method": "POST", "path": "/api/follow-ups/evaluate",
     "json": {"candidate_id": FRANK}},
    {"name": "follow_up_evaluate_bob", "method": "POST", "path": "/api/follow-ups/evaluate",
     "json": {"candidate_id": BOB}},
    {"name": "follow_up_evaluate_unknown", "method": "POST", "path": "/api/follow-ups/evaluate",
     "json": {"candidate_id": "does-not-exist"}},
    {"name": "follow_up_evaluate_closed_position", "method": "POST", "path": "/api/follow-ups/evaluate",
     "json": {"candidate_id": BOB, "position_id": POSITION_ARCHIVED}},
    # Task creation: Bob's only task is terminal, so a new open one is created;
    # the second call reuses it (same id on both sides).
    {"name": "follow_up_create_bob", "method": "POST", "path": "/api/follow-ups",
     "json": {"candidate_id": BOB}, "capture": {"created_task_id": "id"}},
    {"name": "follow_up_create_bob_again", "method": "POST", "path": "/api/follow-ups",
     "json": {"candidate_id": BOB}},
    {"name": "follow_up_create_frank", "method": "POST", "path": "/api/follow-ups",
     "json": {"candidate_id": FRANK}, "capture": {"frank_task_id": "id"}},
    # The state machine, walked over real PATCHes with the measured 409s.
    {"name": "follow_up_patch_drafting", "method": "PATCH",
     "path": "/api/follow-ups/{frank_task_id}", "json": {"status": "Drafting"}},
    {"name": "follow_up_patch_reviewing", "method": "PATCH",
     "path": "/api/follow-ups/{frank_task_id}", "json": {"status": "Reviewing"}},
    {"name": "follow_up_patch_reject", "method": "PATCH",
     "path": "/api/follow-ups/{frank_task_id}", "json": {"status": "Rejected"}},
    {"name": "follow_up_patch_illegal", "method": "PATCH",
     "path": "/api/follow-ups/{frank_task_id}", "json": {"status": "Drafting"}},
    {"name": "follow_up_patch_approved_direct", "method": "PATCH",
     "path": "/api/follow-ups/{frank_task_id}", "json": {"status": "Approved"}},
    {"name": "follow_up_patch_invalid_status", "method": "PATCH",
     "path": "/api/follow-ups/{frank_task_id}", "json": {"status": "Nonsense"}},
    # Cancel from Rejected is legal; re-cancelling is the legal self-transition.
    {"name": "follow_up_cancel", "method": "POST",
     "path": "/api/follow-ups/{frank_task_id}/cancel"},
    {"name": "follow_up_cancel_again", "method": "POST",
     "path": "/api/follow-ups/{frank_task_id}/cancel"},
    {"name": "follow_up_complete_wrong", "method": "POST",
     "path": "/api/follow-ups/{frank_task_id}/complete"},
    # record-result never rejects; a cancelled task keeps its status but gains
    # the result, the count and one send-record row. The replay adds nothing.
    {"name": "follow_up_record_result", "method": "POST",
     "path": "/api/follow-ups/{frank_task_id}/record-result",
     "json": {"result_type": "sent", "note": "contract"}},
    {"name": "follow_up_record_result_again", "method": "POST",
     "path": "/api/follow-ups/{frank_task_id}/record-result",
     "json": {"result_type": "sent", "note": "contract"}},
    # Bob's open task completes through result recording (Eligible is
    # result-completable; a bare complete from Eligible would 409).
    {"name": "follow_up_record_result_bob", "method": "POST",
     "path": "/api/follow-ups/{created_task_id}/record-result",
     "json": {"result_type": "candidate_replied"}},
    {"name": "follow_ups_list_final", "method": "GET", "path": "/api/follow-ups"},
    {"name": "dashboard_default", "method": "GET", "path": "/api/dashboard"},

    # ── W3: drafts, draft groups, AI context and the generate guards ───────
    # Runs after the FK section, so only Bob's drafts exist: `approved`
    # (…0003), a `generated` one carrying a routing blob (…0004, which also
    # proves `actual_tier` wins over `requested_tier`), and two members of one
    # comparison group (…0005 / …0006).
    {"name": "drafts_default", "method": "GET", "path": "/api/drafts"},
    {"name": "drafts_limit2", "method": "GET", "path": "/api/drafts?limit=2"},
    {"name": "drafts_page2", "method": "GET", "path": "/api/drafts?limit=2&offset=2"},
    {"name": "drafts_offset_beyond_total", "method": "GET", "path": "/api/drafts?offset=100"},
    {"name": "drafts_filter_candidate", "method": "GET",
     "path": f"/api/drafts?candidate_id={BOB}"},
    {"name": "drafts_filter_candidate_deleted", "method": "GET",
     "path": f"/api/drafts?candidate_id={ALICE}"},
    {"name": "drafts_filter_status_generated", "method": "GET", "path": "/api/drafts?status=generated"},
    {"name": "drafts_filter_status_approved", "method": "GET", "path": "/api/drafts?status=approved"},
    # pydantic validates the `status` alias as `DraftStatus`: an unknown value
    # is a 422 enum error, not an empty page.
    {"name": "drafts_filter_status_invalid", "method": "GET", "path": "/api/drafts?status=nope"},
    {"name": "drafts_limit_zero", "method": "GET", "path": "/api/drafts?limit=0"},
    {"name": "drafts_limit_over_max", "method": "GET", "path": "/api/drafts?limit=501"},
    {"name": "drafts_offset_negative", "method": "GET", "path": "/api/drafts?offset=-1"},
    {"name": "drafts_trailing_slash", "method": "GET", "path": "/api/drafts/"},
    {"name": "drafts_head_not_allowed", "method": "HEAD", "path": "/api/drafts"},
    # The routed draft: `routing_tier` is projected out of the blob and the raw
    # column is echoed — both are part of Python's `DraftRead`.
    {"name": "draft_detail_routed", "method": "GET", "path": f"/api/drafts/{BOB_DRAFT_ROUTED}"},
    {"name": "draft_detail_approved", "method": "GET", "path": f"/api/drafts/{BOB_DRAFT_APPROVED}"},
    {"name": "draft_detail_missing", "method": "GET", "path": f"/api/drafts/{MISSING_DRAFT}"},

    # The transparency surface: masking, unknowns and the message cap.
    {"name": "context_preview", "method": "GET", "path": f"/api/candidates/{BOB}/context"},
    {"name": "context_preview_missing", "method": "GET",
     "path": f"/api/candidates/{MISSING_DRAFT}/context"},

    # `generate` with no provider configured: the guard chain runs first, then
    # the AI port answers 503 `ai_provider_error` (measured baseline behaviour).
    {"name": "generate_no_key", "method": "POST", "path": "/api/drafts/generate",
     "json": {"candidate_id": BOB}},
    {"name": "generate_unknown_candidate", "method": "POST", "path": "/api/drafts/generate",
     "json": {"candidate_id": MISSING_DRAFT}},
    {"name": "generate_missing_candidate_id", "method": "POST", "path": "/api/drafts/generate",
     "json": {}},
    {"name": "generate_bad_message_type", "method": "POST", "path": "/api/drafts/generate",
     "json": {"candidate_id": BOB, "message_type": "smalltalk"}},
    {"name": "generate_max_length_too_small", "method": "POST", "path": "/api/drafts/generate",
     "json": {"candidate_id": BOB, "max_length": 49}},
    {"name": "generate_max_length_too_large", "method": "POST", "path": "/api/drafts/generate",
     "json": {"candidate_id": BOB, "max_length": 2001}},

    # Draft groups: the read, the 404, and the three `select` refusals.
    {"name": "draft_group_get", "method": "GET", "path": f"/api/draft-groups/{BOB_DRAFT_GROUP}"},
    {"name": "draft_group_missing", "method": "GET",
     "path": f"/api/draft-groups/{MISSING_DRAFT}"},
    {"name": "draft_group_select_not_member", "method": "POST",
     "path": f"/api/draft-groups/{BOB_DRAFT_GROUP}/select",
     "json": {"draft_id": BOB_DRAFT_ROUTED}},

    # ── W3 writes: the draft state machine and the group supersede ─────────
    # A text edit marks `is_edited` and advances `generated → edited`; absent
    # and explicit null are both ignored (`exclude_unset + exclude_none`).
    {"name": "draft_patch_text", "method": "PATCH", "path": f"/api/drafts/{BOB_DRAFT_ROUTED}",
     "json": {"message_text": "edited by the contract scenario"}},
    {"name": "draft_patch_empty_text", "method": "PATCH",
     "path": f"/api/drafts/{BOB_DRAFT_ROUTED}", "json": {"message_text": ""}},
    {"name": "draft_patch_null_objective", "method": "PATCH",
     "path": f"/api/drafts/{BOB_DRAFT_ROUTED}", "json": {"objective": None}},
    # `approved` can no longer be edited: 409 `business_rule_violation`.
    {"name": "draft_patch_approved", "method": "PATCH",
     "path": f"/api/drafts/{BOB_DRAFT_APPROVED}", "json": {"message_text": "too late"}},
    # reject has no guard (every state answers 200); approve and copy refuse a
    # rejected draft.
    {"name": "draft_reject", "method": "POST", "path": f"/api/drafts/{BOB_DRAFT_GROUP_A}"},
    {"name": "draft_approve_rejected", "method": "POST",
     "path": f"/api/drafts/{BOB_DRAFT_GROUP_A}/approve", "json": {}},
    {"name": "draft_copy_rejected", "method": "POST",
     "path": f"/api/drafts/{BOB_DRAFT_GROUP_A}/copy"},
    {"name": "draft_group_select_rejected_member", "method": "POST",
     "path": f"/api/draft-groups/{BOB_DRAFT_GROUP}/select",
     "json": {"draft_id": BOB_DRAFT_GROUP_A}},
    {"name": "draft_approve", "method": "POST", "path": f"/api/drafts/{BOB_DRAFT_ROUTED}/approve",
     "json": {"note": "contract"}},
    {"name": "draft_patch_after_approve", "method": "PATCH",
     "path": f"/api/drafts/{BOB_DRAFT_ROUTED}", "json": {"message_text": "too late"}},
    # copy is not a send: the response says so in `note`, and the follow-up
    # task only moves to `AwaitingResult`.
    {"name": "draft_copy", "method": "POST", "path": f"/api/drafts/{BOB_DRAFT_APPROVED}/copy"},
    {"name": "draft_reject_copied", "method": "POST",
     "path": f"/api/drafts/{BOB_DRAFT_APPROVED}"},
    # Selecting a live member rejects the other ones (and re-rejects the
    # already-rejected one, exactly like the baseline's loop).
    {"name": "draft_group_select", "method": "POST",
     "path": f"/api/draft-groups/{BOB_DRAFT_GROUP}/select",
     "json": {"draft_id": BOB_DRAFT_GROUP_B}},
    {"name": "draft_group_get_after_select", "method": "GET",
     "path": f"/api/draft-groups/{BOB_DRAFT_GROUP}"},
    {"name": "drafts_final", "method": "GET", "path": "/api/drafts"},
    {"name": "dashboard_after_drafts", "method": "GET", "path": "/api/dashboard"},

    # ── W4: imports, export, import, OCR ───────────────────────────────────
    # NOTE: these use BOB, not ALICE: the W1 scenario deletes ALICE at
    # `candidate_delete_with_children` (cascade), so a later case naming her
    # would 404 on one side only.
    # The pasted text is the parser's simplest non-empty shape: two senders,
    # no timestamps. A preview PERSISTS the session and its records — it does
    # NOT write to the timeline; only confirm does. (The earlier task draft
    # claimed preview was "zero-write", which the baseline contradicts.)
    {"name": "imports_preview", "method": "POST", "path": "/api/imports/preview",
     "json": {"candidate_id": BOB, "raw_text": "候选人：你好，我对这个岗位感兴趣\n我：您好，请问方便聊聊吗"},
     "capture": {"session_id": "id"}},
    {"name": "imports_preview_unknown_candidate", "method": "POST",
     "path": "/api/imports/preview",
     "json": {"candidate_id": "00000000-0000-0000-0000-0000000000ff",
              "raw_text": "候选人：你好"}},
    # An empty page is a `failed` session, not an error response.
    {"name": "imports_preview_empty", "method": "POST", "path": "/api/imports/preview",
     "json": {"candidate_id": BOB, "raw_text": ""}},
    {"name": "imports_preview_whitespace_only", "method": "POST",
     "path": "/api/imports/preview",
     "json": {"candidate_id": BOB, "raw_text": "   \n  \n "}},
    # No candidate_id: the session is not filed against anyone yet.
    {"name": "imports_preview_no_candidate", "method": "POST",
     "path": "/api/imports/preview",
     "json": {"raw_text": "候选人：没有候选人也可以解析"}},
    {"name": "imports_get", "method": "GET", "path": "/api/imports/{session_id}"},
    {"name": "imports_get_missing", "method": "GET", "path": "/api/imports/does-not-exist"},
    # Confirm writes the timeline; a second confirm is a 409.
    {"name": "imports_confirm", "method": "POST", "path": "/api/imports/{session_id}/confirm",
     "json": {"candidate_id": BOB}},
    {"name": "imports_confirm_twice", "method": "POST", "path": "/api/imports/{session_id}/confirm",
     "json": {"candidate_id": BOB}},
    # A session for a different candidate: confirm needs a candidate, and one
    # that does not exist is a 404.
    {"name": "imports_preview_other_candidate", "method": "POST",
     "path": "/api/imports/preview",
     "json": {"candidate_id": FRANK, "raw_text": "候选人：弗兰克的��息\n我：收到"},
     "capture": {"other_session_id": "id"}},
    {"name": "imports_confirm_unknown_candidate", "method": "POST",
     "path": "/api/imports/{other_session_id}/confirm",
     "json": {"candidate_id": "00000000-0000-0000-0000-0000000000ff"}},
    {"name": "imports_confirm_missing_candidate_field", "method": "POST",
     "path": "/api/imports/{other_session_id}/confirm", "json": {}},
    # Reject clears the raw text; a rejected session can still be confirmed.
    {"name": "imports_preview_to_reject", "method": "POST", "path": "/api/imports/preview",
     "json": {"candidate_id": BOB, "raw_text": "候选人：将被拒绝\n我：好的"},
     "capture": {"reject_session_id": "id"}},
    {"name": "imports_reject", "method": "POST", "path": "/api/imports/{reject_session_id}/reject"},
    {"name": "imports_get_after_reject", "method": "GET",
     "path": "/api/imports/{reject_session_id}"},
    {"name": "imports_confirm_after_reject", "method": "POST",
     "path": "/api/imports/{reject_session_id}/confirm", "json": {"candidate_id": BOB}},
    {"name": "imports_reject_missing", "method": "POST", "path": "/api/imports/does-not-exist/reject"},
    # The OCR path guard runs before any I/O: an out-of-directory path is
    # refused without disclosing whether the file exists.
    {"name": "imports_ocr_outside_directory", "method": "POST", "path": "/api/imports/ocr",
     "json": {"candidate_id": BOB, "image_path": "/etc/passwd"}},
    {"name": "imports_ocr_missing_file", "method": "POST", "path": "/api/imports/ocr",
     "json": {"candidate_id": BOB, "image_path": "/tmp/ica-contract-ocr/missing.png"}},
    {"name": "imports_ocr_no_path", "method": "POST", "path": "/api/imports/ocr",
     "json": {"candidate_id": BOB}},
    # ── W4 export / import (last: they touch every table) ──────────────────
    # The plaintext snapshot: `version` is pinned, `exported_at` is a STRING
    # built with `isoformat()` (so `+00:00`, not the wire `Z`), and
    # `import_sessions.raw_text` is excluded from the export.
    {"name": "export_plaintext", "method": "POST", "path": "/api/export"},
    # An unusable key is a 422 `invalid_encryption_key` — never a silent
    # fallback to plaintext.
    {"name": "export_bad_key", "method": "POST", "path": "/api/export",
     "headers": {"X-Encrypt-Aes-Key": "not-a-valid-key"}},
    {"name": "export_short_key", "method": "POST", "path": "/api/export",
     "headers": {"X-Encrypt-Aes-Key": "c2hvcnQ="}},
    # A sealed export returns the envelope; the plaintext must NOT be present.
    {"name": "export_sealed", "method": "POST", "path": "/api/export",
     "headers": {"X-Encrypt-Aes-Key": "BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc="}},
    # Importing the snapshot back is an upsert, so re-importing changes
    # nothing.
    {"name": "import_snapshot", "method": "POST", "path": "/api/import",
     "json": {"data": {}}},
    {"name": "import_snapshot_empty", "method": "POST", "path": "/api/import",
     "json": {"data": {}}},
    {"name": "import_snapshot_unknown_table", "method": "POST", "path": "/api/import",
     "json": {"data": {"no_such_table": [{"id": "x"}]}}},
    {"name": "import_snapshot_no_data", "method": "POST", "path": "/api/import", "json": {}},
    {"name": "import_snapshot_null_data", "method": "POST", "path": "/api/import",
     "json": {"data": None}},
    # ── W4 import LWW semantics ────────────────────────────────────────────
    # The earlier W4 cases all sent empty `data`, so the merge rules were never
    # exercised: a port that only implemented "newer wins, else skip" passed
    # every one of them. These six pin the real rules, including the
    # `conflicts` counter and the append-only table exception.
    #
    # Dave's seeded `updated_at` is ...000003, Eve's is ...000004.
    {"name": "import_lww_older_differs", "method": "POST", "path": "/api/import",
     "json": {"data": {"candidates": [{
         "id": "c0000000-0000-0000-0000-000000000004",
         "name": "Dave STALE", "source": "boss", "status": "New", "archived": False,
         "created_at": "2026-11-10T09:05:00.000003+00:00",
         "updated_at": "2020-01-01T00:00:00+00:00"}]}}},
    {"name": "import_lww_equal_differs", "method": "POST", "path": "/api/import",
     "json": {"data": {"candidates": [{
         "id": "c0000000-0000-0000-0000-000000000004",
         "name": "Dave EQUAL", "source": "boss", "status": "New", "archived": False,
         "created_at": "2026-11-10T09:05:00.000003+00:00",
         "updated_at": "2026-11-10T09:05:00.000003+00:00"}]}}},
    {"name": "import_lww_no_timestamp_differs", "method": "POST", "path": "/api/import",
     "json": {"data": {"candidates": [{
         "id": "c0000000-0000-0000-0000-000000000004",
         "name": "Dave NO-TS", "source": "boss", "status": "New", "archived": False,
         "created_at": "2026-11-10T09:05:00.000003+00:00"}]}}},
    # Identical payload, no timestamp: skipped but NOT a conflict.
    {"name": "import_lww_no_timestamp_identical", "method": "POST", "path": "/api/import",
     "json": {"data": {"candidates": [{
         "id": "c0000000-0000-0000-0000-000000000004",
         "name": "Dave", "source": "boss", "status": "New", "archived": False,
         "created_at": "2026-11-10T09:05:00.000003+00:00"}]}}},
    # Newer wins — and only the columns the payload mentions are written, so
    # `status` / `source` must keep their stored values.
    {"name": "import_lww_newer_wins", "method": "POST", "path": "/api/import",
     "json": {"data": {"candidates": [{
         "id": "c0000000-0000-0000-0000-000000000005",
         "name": "Eve RENAMED",
         "updated_at": "2099-01-01T00:00:00+00:00"}]}}},
    # Append-only: a stored send record is never rewritten, even by a snapshot
    # carrying a newer stamp. The disagreement is still reported as a conflict.
    {"name": "import_lww_append_only_send_record", "method": "POST", "path": "/api/import",
     "json": {"data": {"send_records": [{
         "id": "cccccccc-0000-0000-0000-000000000004", "candidate_id": "c0000000-0000-0000-0000-000000000002",
         "channel": "boss", "content_summary": "REWRITTEN BY IMPORT",
         "status": "sent", "actor": "local_user",
         "created_at": "2026-11-13T10:00:00.000000+00:00",
         "updated_at": "2099-01-01T00:00:00+00:00"}]}}},

    # ── W4 OCR with the real engine ────────────────────────────────────────
    # The port runs the SAME model set as the baseline (RapidOCR PP-OCRv6
    # small through ONNX Runtime), so the recognised text must be identical —
    # it is fed to the import parser, so a difference changes what is parsed.
    # These need a staged image, which the capture writes before the request.
    {"name": "imports_ocr_real_image", "method": "POST", "path": "/api/imports/ocr",
     "json": {"candidate_id": BOB, "image_path": "{ocr_image}"}},
    # A file that is not an image at all: 4xx, never a 500.
    {"name": "imports_ocr_not_an_image", "method": "POST", "path": "/api/imports/ocr",
     "json": {"candidate_id": BOB, "image_path": "{ocr_bad_image}"}},
    # 1000x136 — the audit's own divergent image. The detector used to clamp its
    # long side (4096 / 2048² / Downscale), which rescaled the picture and moved
    # where the detector split a line: at 1000 px the baseline emitted
    # "Candidate: hi\nthere" and the port emitted "Candidate: hi there". The
    # 760x130 case above did not show it, which is why the first delivery looked
    # green. This case fails if the clamp ever comes back.
    {"name": "imports_ocr_wide_image", "method": "POST", "path": "/api/imports/ocr",
     "json": {"candidate_id": BOB, "image_path": "{ocr_wide_image}"}},

    # ── W7: browser extraction confirm-candidate ───────────────────────────
    # The frozen baseline's BROWSER-04 flow: a browser_extract session confirms
    # through `POST /api/imports/{session_id}/confirm-candidate` — the ONLY
    # path from a page into `candidates`. Every guard is a negative path, and
    # each case must actually REACH its branch (the W7 card's acceptance bar).
    # The seed carries drafts in every guard state, with the review digest the
    # baseline's own algorithm computes over the approved fields.

    # A manual-source session refuses: wrong_import_type (409). The captured
    # {other_session_id} is a manual paste-import preview from the W4 block.
    {"name": "confirm_candidate_wrong_import_type", "method": "POST",
     "path": "/api/imports/{other_session_id}/confirm-candidate",
     "json": {}},
    # An unknown session is the import 404.
    {"name": "confirm_candidate_unknown_session", "method": "POST",
     "path": "/api/imports/no-such-session/confirm-candidate",
     "json": {}},
    # A browser session WITHOUT a draft: extraction_not_found (404).
    {"name": "confirm_candidate_extraction_not_found", "method": "POST",
     "path": "/api/imports/b0000000-0000-0000-0000-000000000008/confirm-candidate",
     "json": {}},
    # A draft in `draft` state: review_required (409).
    {"name": "confirm_candidate_review_required", "method": "POST",
     "path": "/api/imports/b0000000-0000-0000-0000-000000000001/confirm-candidate",
     "json": {}},
    # A confirmed draft refuses again (409).
    {"name": "confirm_candidate_already_confirmed", "method": "POST",
     "path": "/api/imports/b0000000-0000-0000-0000-000000000007/confirm-candidate",
     "json": {}},
    # An approval whose digest no longer matches the fields (409).
    {"name": "confirm_candidate_review_stale", "method": "POST",
     "path": "/api/imports/b0000000-0000-0000-0000-000000000005/confirm-candidate",
     "json": {}},
    # A stored name-duplicate verdict blocks WITHOUT an override — and the
    # block itself is audited (the final state diff sees the audit row).
    {"name": "confirm_candidate_duplicate_blocked", "method": "POST",
     "path": "/api/imports/b0000000-0000-0000-0000-000000000006/confirm-candidate",
     "json": {}},
    # The override goes through and writes the candidate.
    {"name": "confirm_candidate_duplicate_override", "method": "POST",
     "path": "/api/imports/b0000000-0000-0000-0000-000000000006/confirm-candidate",
     "json": {"confirm_duplicate": True}},
    # The happy path: an approved, non-duplicate draft becomes a candidate
    # (source `browser:zhipin.com`, provenance note, session confirmed).
    {"name": "confirm_candidate_happy", "method": "POST",
     "path": "/api/imports/b0000000-0000-0000-0000-000000000002/confirm-candidate",
     "json": {}},
    # Status and note flow into the candidate row.
    {"name": "confirm_candidate_status_and_note", "method": "POST",
     "path": "/api/imports/b0000000-0000-0000-0000-000000000003/confirm-candidate",
     "json": {"status": "Interview", "note": "面评请写完整"}},
    # A missing position link is the candidates API's own 404.
    {"name": "confirm_candidate_position_missing", "method": "POST",
     "path": "/api/imports/b0000000-0000-0000-0000-000000000004/confirm-candidate",
     "json": {"position_id": "p-nonexistent"}},
    # A bad enum value is a route-level 422 (pydantic's own message text).
    {"name": "confirm_candidate_bad_status", "method": "POST",
     "path": "/api/imports/b0000000-0000-0000-0000-000000000002/confirm-candidate",
     "json": {"status": "bogus"}},
    # The MESSAGE-shaped confirm refuses a browser session — the same session
    # confirms through confirm-candidate instead (frozen baseline's guard).
    {"name": "confirm_message_guard_browser_session", "method": "POST",
     "path": "/api/imports/b0000000-0000-0000-0000-000000000001/confirm",
     "json": {"candidate_id": "c0000000-0000-0000-0000-000000000001"}},
    # Rejecting a browser session drags its draft out of `draft`
    # (mark_rejected_for_session), with the reviewed audit event.
    {"name": "reject_browser_session_marks_draft", "method": "POST",
     "path": "/api/imports/b0000000-0000-0000-0000-000000000001/reject",
     "json": {}},

    # ── W4-R2/R3: the import VALIDATION error paths, now IN the comparison ──
    # The second audit's lesson: 220/222 was green while
    # `import_validation_error` had ZERO coverage — the envelope was wrong and
    # three validator rules were missing, invisibly. These cases compare the
    # full 422 envelope family on both sides (status + code + message + the
    # details key set), and the state diff proves the refusal wrote NOTHING.

    # A row whose `status` is not a PositionStatus: 422, nothing written.
    {"name": "import_positions_bad_status", "method": "POST", "path": "/api/import",
     "json": {"data": {"positions": [{"name": "Rule Probe", "status": "bogus"}]}}},
    # A non-string `location` and a non-boolean `archived`: 422 each.
    {"name": "import_positions_bad_location", "method": "POST", "path": "/api/import",
     "json": {"data": {"positions": [{"name": "Rule Probe", "location": 123}]}}},
    {"name": "import_positions_bad_archived", "method": "POST", "path": "/api/import",
     "json": {"data": {"positions": [{"name": "Rule Probe", "archived": "yes"}]}}},
    # A 201-character name has its own message.
    {"name": "import_positions_name_too_long", "method": "POST", "path": "/api/import",
     "json": {"data": {"positions": [{"name": "111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111"}]}}},
    # A negative salary has its own message.
    {"name": "import_positions_negative_salary", "method": "POST", "path": "/api/import",
     "json": {"data": {"positions": [{"name": "Rule Probe", "salary_min": -5}]}}},
    # A SQLite-dump boolean (0/1) is READABLE — 200, imported.
    {"name": "import_positions_dump_archived_ok", "method": "POST", "path": "/api/import",
     "json": {"data": {"positions": [{"name": "Dump Bool", "archived": 0}]}}},
    # A table whose value is not a LIST: pydantic answers before any import
    # logic, and the audit measured the envelope double-encoding here.
    {"name": "import_table_not_a_list", "method": "POST", "path": "/api/import",
     "json": {"data": {"candidates": "oops"}}},
    # A list ELEMENT that is not an object: the loc index is a NUMBER.
    {"name": "import_row_not_an_object", "method": "POST", "path": "/api/import",
     "json": {"data": {"candidates": [1, 2]}}},
    # `"id": null` mints a UUID (audit N-1: was a 500).
    {"name": "import_row_null_id_mints_uuid", "method": "POST", "path": "/api/import",
     "json": {"data": {"candidates": [{"id": None, "name": "Null Id Row"}]}}},
    # `"id": ""` is also "no id" (audit N-2 / LWW P2-4).
    {"name": "import_row_empty_id_mints_uuid", "method": "POST", "path": "/api/import",
     "json": {"data": {"candidates": [{"id": "", "name": "Empty Id"}]}}},
    # A bare-date `updated_at` WINS the LWW race (audit LWW P2-1).
    {"name": "import_lww_bare_date_wins", "method": "POST", "path": "/api/import",
     "json": {"data": {"candidates": [{
         "id": "c0000000-0000-0000-0000-000000000003", "name": "Carol BARE DATE",
         "updated_at": "2033-01-01"}]}}},
    # A numeric boolean is not a phantom conflict (audit LWW P2-2).
    {"name": "import_lww_numeric_bool_no_conflict", "method": "POST", "path": "/api/import",
     "json": {"data": {"candidates": [{
         "id": "c0000000-0000-0000-0000-000000000001", "archived": 0,
         "updated_at": "2033-01-01"}]}}},

    # ── Final observations ─────────────────────────────────────────────────
    {"name": "positions_final", "method": "GET", "path": "/api/positions?include_archived=true"},
    {"name": "candidates_final", "method": "GET", "path": "/api/candidates?include_archived=true"},

    # ── W5: settings + AI (local mode; the ai-key side effect lives in the
    # OS credential store on Rust and in the baseline's secret_store file on
    # Python — only the RESPONSES and rows are compared) ───────────────────
    {"name": "settings_read", "method": "GET", "path": "/api/settings"},
    {"name": "ai_egress_read", "method": "GET", "path": "/api/settings/ai-egress"},
    {"name": "ai_key_put", "method": "PUT", "path": "/api/settings/ai-key",
     "json": {"ai_api_key": "contract-key-1234"}},
    {"name": "settings_after_key", "method": "GET", "path": "/api/settings"},
    {"name": "ai_egress_consent_revoke", "method": "POST",
     "path": "/api/settings/ai-egress/consent", "json": {"consent_granted": False}},
    {"name": "ai_egress_consent_grant", "method": "POST",
     "path": "/api/settings/ai-egress/consent", "json": {"consent_granted": True}},
    {"name": "ai_egress_consent_missing", "method": "POST",
     "path": "/api/settings/ai-egress/consent", "json": {}},
    {"name": "ai_models_list", "method": "GET", "path": "/api/ai/models"},
    {"name": "ai_routing_patch", "method": "PATCH", "path": "/api/ai/routing",
     "json": {"routing": {"salary_talk": "standard", "greeting": "premium"}}},
    {"name": "ai_routing_patch_unknown_type", "method": "PATCH", "path": "/api/ai/routing",
     "json": {"routing": {"not_a_type": "standard"}}},
    {"name": "ai_usage_get", "method": "GET", "path": "/api/ai/usage"},
    {"name": "ai_budget_patch", "method": "PATCH", "path": "/api/ai/budget",
     "json": {"daily_budget_cents": 1000, "warning_threshold": 50}},
    {"name": "ai_budget_get", "method": "GET", "path": "/api/ai/budget"},
    {"name": "ai_budget_patch_bad_threshold", "method": "PATCH", "path": "/api/ai/budget",
     "json": {"daily_budget_cents": 10, "warning_threshold": 101}},

    # Backups: the file mechanics are per side; names carry a timestamp and
    # sizes reflect each implementation's page layout, so the diff normalises
    # these bodies to their shapes.
    {"name": "backups_create", "method": "POST", "path": "/api/backups", "json": {}},
    {"name": "backups_list", "method": "GET", "path": "/api/backups"},

    # ── W6: auth (registration order matters — the seed carries the default
    # tenant, so every registration joins it as member) ───────────────────
    {"name": "auth_register_owner", "method": "POST", "path": "/api/auth/register",
     "json": {"email": "Owner@Example.com", "password": "contract-pass-1"},
     "capture": {"owner_user_id": "id"}},
    {"name": "auth_register_duplicate_email", "method": "POST", "path": "/api/auth/register",
     "json": {"email": "owner@example.com", "password": "contract-pass-1"}},
    {"name": "auth_register_bad_email", "method": "POST", "path": "/api/auth/register",
     "json": {"email": "no-at-sign", "password": "contract-pass-1"}},
    {"name": "auth_register_short_password", "method": "POST", "path": "/api/auth/register",
     "json": {"email": "ok@example.com", "password": "short"}},
    {"name": "auth_register_unknown_tenant", "method": "POST", "path": "/api/auth/register",
     "json": {"email": "x@example.com", "password": "contract-pass-1",
              "tenant_id": "t-missing"}},
    {"name": "auth_login_wrong_password", "method": "POST", "path": "/api/auth/login",
     "json": {"email": "owner@example.com", "password": "wrong-pass"}},
    {"name": "auth_login", "method": "POST", "path": "/api/auth/login",
     "json": {"email": "owner@example.com", "password": "contract-pass-1"},
     "capture": {"refresh_token_1": "refresh_token"}},
    {"name": "auth_refresh_rotates", "method": "POST", "path": "/api/auth/refresh",
     "json": {"refresh_token": "{refresh_token_1}"},
     "capture": {"refresh_token_2": "refresh_token"}},
    {"name": "auth_refresh_replay_is_single_use", "method": "POST", "path": "/api/auth/refresh",
     "json": {"refresh_token": "{refresh_token_1}"}},
    {"name": "auth_logout_revokes", "method": "POST", "path": "/api/auth/logout",
     "json": {"refresh_token": "{refresh_token_2}"}},
    {"name": "auth_refresh_after_logout", "method": "POST", "path": "/api/auth/refresh",
     "json": {"refresh_token": "{refresh_token_2}"}},
    {"name": "auth_logout_idempotent", "method": "POST", "path": "/api/auth/logout",
     "json": {"refresh_token": "{refresh_token_2}"}},
    {"name": "auth_me_local", "method": "GET", "path": "/api/auth/me"},

    # ── W6: shares (the actor is the synthetic local user, seeded) ────────
    {"name": "shares_create", "method": "POST", "path": "/api/shares",
     "json": {"recipient_id": "{owner_user_id}",
              "resource_type": "position", "resource_id": "p-seed-1"}},
    {"name": "shares_upgrade", "method": "POST", "path": "/api/shares",
     "json": {"recipient_id": "{owner_user_id}",
              "resource_type": "position", "resource_id": "p-seed-1",
              "permission": "write"}},
    {"name": "shares_create_self", "method": "POST", "path": "/api/shares",
     "json": {"recipient_id": "00000000-0000-0000-0000-000000000001",
              "resource_type": "candidate", "resource_id": "c-seed-1"}},
    {"name": "shares_create_unknown_recipient", "method": "POST", "path": "/api/shares",
     "json": {"recipient_id": "u-missing",
              "resource_type": "candidate", "resource_id": "c-seed-1"}},
    {"name": "shares_list", "method": "GET", "path": "/api/shares"},
    {"name": "shares_delete_missing", "method": "DELETE",
     "path": "/api/shares/no-such-share"},

    # ── Final W5/W6 observations ───────────────────────────────────────────
    {"name": "audit_events_w6", "method": "GET",
     "path": "/api/audit-events?limit=100"},
    {"name": "backups_list_final", "method": "GET", "path": "/api/backups"},
]

#: Tables compared row-by-row after the scenario.
DB_TABLES = [
    "positions",
    "candidates",
    "conversations",
    "messages",
    "send_records",
    "follow_up_tasks",
    "follow_up_policies",
    "generated_drafts",
    "audit_events",
    # W4
    "import_sessions",
    "import_records",
    # W7 — the extraction drafts (confirm writes into candidates FROM here)
    "browser_candidate_extractions",
    # W5/W6 — the settings/AI/auth/shares state
    "tenants",
    "users",
    "refresh_tokens",
    "shares",
    "ai_model_routing",
    "app_settings",
]

#: Columns that hold a JSON document; the diff compares them parsed, so the
#: exact spacing `json.dumps` vs `serde_json` produce is not a false failure.
JSON_COLUMNS = {
    "audit_events": ["metadata_json"],
}

BASE_URL_DEFAULT = "http://127.0.0.1:8010"


class _NoRedirect(urllib.request.HTTPRedirectHandler):
    """Record a redirect instead of following it.

    A redirect *is* contract behaviour (the trailing-slash 307), so the capture
    must show what the server answered, not where a well-behaved client chose
    to go next.
    """

    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


#: One opener for the whole run, with redirect chasing disabled.
OPENER = urllib.request.build_opener(_NoRedirect)


def _location_path(location: str | None) -> str | None:
    """Reduce an absolute `Location` to `path?query`.

    The server renders its own scheme/host/port into the header, and the two
    sides run on different ports, so only the path and query are comparable.
    """
    if not location:
        return None
    parts = urllib.parse.urlsplit(location)
    return parts.path + (f"?{parts.query}" if parts.query else "")


def _one_request(opener, request, is_head: bool,
                 timeout: float) -> tuple[int, str, object]:
    """One request, returning `(status, body, headers)`.

    A `ConnectionResetError` raised anywhere in here — while opening the
    response *or* while reading the error body out of an `HTTPError` — is
    allowed to escape, because the caller retries on it.
    """
    try:
        with opener.open(request, timeout=timeout) as response:
            status = response.status
            raw = "" if is_head else response.read().decode("utf-8")
            return status, raw, response.headers
    except urllib.error.HTTPError as exc:  # 4xx/5xx — and, with redirect
        # chasing disabled, every 3xx too — still carries headers
        status = exc.code
        raw = "" if is_head else exc.read().decode("utf-8")
        return status, raw, exc.headers or {}


def fetch(base_url: str, method: str, path: str, payload: dict | None,
          timeout: float = 20.0, attempts: int = 4,
          extra_headers: dict[str, str] | None = None) -> dict[str, object]:
    """Issue one request and return status + decoded body (or raw text).

    A `HEAD` response carries no body by definition, so it is never read: the
    405 envelope advertises a `Content-Length` that the server then withholds,
    and reading it blocks until the connection is reset.

    A request that carries a **body** and gets a **405** is retried. The
    Python baseline answers 405 without draining the request body, so the
    socket still holds unread data and the server's close arrives as a TCP
    RST — which `urllib` surfaces as `ConnectionResetError` instead of the
    405 envelope. It is a race (measured: one run in three needed two
    retries), not a contract difference, so the capture must not fail on it.
    A fresh connection has no unread body, so a retry sees the real answer.
    """
    url = base_url.rstrip("/") + path
    data = None if payload is None else json.dumps(payload).encode("utf-8")
    headers = {"accept": "application/json"}
    if data is not None:
        headers["content-type"] = "application/json"
    # Extra request headers (e.g. the export's `X-Encrypt-Aes-Key`).
    for name, value in (extra_headers or {}).items():
        headers[name] = value
    is_head = method.upper() == "HEAD"
    request = urllib.request.Request(url, data=data, method=method, headers=headers)
    last_reset: ConnectionResetError | None = None
    for _attempt in range(attempts):
        try:
            status, raw, response_headers = _one_request(
                OPENER, request, is_head, timeout
            )
        except ConnectionResetError as exc:
            # The 405-with-unread-body race described above. It can surface in
            # two places: while opening the response, and while reading the
            # error body out of the `HTTPError` — both are inside
            # `_one_request`, so both land here.
            last_reset = exc
            continue
        break
    else:
        raise last_reset if last_reset else RuntimeError(
            f"{method} {path}: the server reset the connection {attempts} times"
        )
    content_type = response_headers.get("content-type", "") if response_headers else ""
    location = response_headers.get("location") if response_headers else None

    if not raw:
        body: object = None
    else:
        try:
            body = json.loads(raw)
        except json.JSONDecodeError:
            body = {"__raw__": raw}
    return {
        "method": method,
        "path": path,
        "status": status,
        "content_type": content_type.split(";")[0],
        "location": _location_path(location),
        "body": body,
    }


#: A UUID-shaped token. The SORT KEY below masks these, because a run-created
#: id sorts differently on each side (the two sides mint different bytes), and
#: a row whose content differs only in such an id would align with a DIFFERENT
#: row on the other side — turning identical content into a positional
#: "first difference at row 0" (measured on the W7 extraction rows, whose
#: confirmed drafts differ only in the created candidate's id).
_UUID_TOKEN_RE = re.compile(
    r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}"
)

#: Timestamp-like substrings, for the row-sort key: two sides never agree on
#: the wall-clock instant a near-now row was written, and an unstable sort
#: would present the same row-set in a different order.
_TS_IN_TEXT = re.compile(
    r"\d{4}-\d{2}-\d{2}[ T]\d{2}:\d{2}:\d{2}(\.\d+)?"
)


def dump_db(path: str) -> dict[str, object]:
    """Every row of the business tables, with JSON columns parsed."""
    connection = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    connection.row_factory = sqlite3.Row
    try:
        state: dict[str, object] = {}
        for table in DB_TABLES:
            rows = [dict(row) for row in connection.execute(f"SELECT * FROM {table}")]
            for row in rows:
                for column in JSON_COLUMNS.get(table, []):
                    raw = row.get(column)
                    if isinstance(raw, str):
                        try:
                            row[column] = json.loads(raw)
                        except json.JSONDecodeError:
                            pass
            # Sort by the JSON rendering with UUID tokens masked, so a random
            # primary key cannot reorder the list differently on the two sides;
            # the diff compares content.
            rows.sort(
                key=lambda row: _TS_IN_TEXT.sub(
                    "<ts>",
                    _UUID_TOKEN_RE.sub(
                        "<uuid>", json.dumps(row, sort_keys=True, ensure_ascii=False)
                    ),
                )
            )
            state[table] = rows
        return state
    finally:
        connection.close()


def _stage_ocr_images(staging_dir: str, side: str) -> dict[str, str]:
    """Write the OCR fixtures into the staging directory and return their paths.

    The image is a rendered PNG, not a fixture file checked into the repo, so
    both sides recognise the same characters. The file name carries the SIDE,
    because the first side's request DELETES its staging copy — reusing one
    name would make the second side answer "image not found", which is a probe
    artefact rather than a contract difference.
    """
    from PIL import Image, ImageDraw, ImageFont

    os.makedirs(staging_dir, exist_ok=True)
    image_path = os.path.join(staging_dir, f"contract-ocr-{side}.png")
    bad_path = os.path.join(staging_dir, f"contract-ocr-bad-{side}.png")
    wide_path = os.path.join(staging_dir, f"contract-ocr-wide-{side}.png")

    canvas = Image.new("RGB", (760, 130), "white")
    draw = ImageDraw.Draw(canvas)
    font = ImageFont.load_default(size=30)
    draw.text((20, 20), "Candidate: hi there", fill="black", font=font)
    draw.text((20, 70), "Recruiter: welcome", fill="black", font=font)
    canvas.save(image_path)

    # The audit's divergent layout, verbatim: 1000x136, two lines, 30 px.
    # The 760x130 fixture above sits just past the old 757 px clamp threshold,
    # so it catches a clamped detector but says nothing about how the divergence
    # grows with width. This one is 31% wider and is the exact image the second
    # audit used to break the earlier "character-for-character identical" claim
    # (py '\n' vs rs ' ' at char 23).
    wide = Image.new("RGB", (1000, 136), "white")
    wide_draw = ImageDraw.Draw(wide)
    wide_draw.text((24, 18), "Hello OCR", fill="black", font=font)
    wide_draw.text((24, 63), "Candidate: hi there", fill="black", font=font)
    wide.save(wide_path)

    with open(bad_path, "wb") as handle:
        handle.write(b"this is definitely not a png")

    return {"ocr_image": image_path, "ocr_bad_image": bad_path, "ocr_wide_image": wide_path}


def _substitute_context(value, context: dict[str, str]):
    """Replace `{key}` placeholders in every string leaf with captured values.

    Only keys that ARE in the context are touched, so existing bodies with
    literal braces (none today) keep working.
    """
    if isinstance(value, dict):
        return {key: _substitute_context(item, context) for key, item in value.items()}
    if isinstance(value, list):
        return [_substitute_context(item, context) for item in value]
    if isinstance(value, str):
        for key, captured in context.items():
            value = value.replace("{" + key + "}", captured)
        return value
    return value


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--base-url", default=BASE_URL_DEFAULT)
    parser.add_argument("--db", required=True, help="database the server writes to")
    parser.add_argument("--out", required=True)
    parser.add_argument("--skip-db", action="store_true",
                        help="capture responses only (no final row dump)")
    args = parser.parse_args(argv)

    started_at = datetime.now(UTC).isoformat()
    context: dict[str, str] = {}
    results: dict[str, dict] = {}
    failures = 0

    # The OCR staging directory is `<data dir>/ocr`; the server reads from
    # there and DELETES the file after every request, so each side gets its
    # own copy.
    staging_dir = os.environ.get("ICA_OCR_STAGING_DIR", "")
    if staging_dir:
        side = os.environ.get("ICA_OCR_SIDE", "x")
        context.update(_stage_ocr_images(staging_dir, side))

    for case in CASES:
        # A captured placeholder that was never filled (the capture step
        # failed) must not abort the whole scenario: the missing value is
        # itself a finding, and the remaining cases still have to run.
        try:
            path = case["path"].format(**context)
        except KeyError as missing:
            print(f"  {case['name']:42s} SKIPPED (no value for {missing})")
            failures += 1
            results[case["name"]] = {
                "status": None,
                "body": {"skipped": f"missing {missing}"},
                "headers": {},
            }
            continue
        json_body = case.get("json")
        if json_body is not None and context:
            json_body = _substitute_context(json_body, context)
        result = fetch(args.base_url, case["method"], path, json_body,
                       extra_headers=case.get("headers"))
        results[case["name"]] = result
        body = result["body"]
        for key, field in case.get("capture", {}).items():
            value = body
            for part in field.split("."):
                value = value.get(part) if isinstance(value, dict) else None
            if value is None:
                print(f"  cannot capture {field!r} from {case['name']}: {body!r}")
                failures += 1
                continue
            context[key] = str(value)
        print(f"{case['name']:42s} {result['status']} {path}")

    db_state = {} if args.skip_db else dump_db(args.db)
    payload = {
        "base_url": args.base_url,
        "started_at": started_at,
        "cases": results,
        "db_state": db_state,
    }
    Path(args.out).parent.mkdir(parents=True, exist_ok=True)
    with open(args.out, "w", encoding="utf-8") as handle:
        json.dump(payload, handle, indent=2, sort_keys=True, ensure_ascii=False)
        handle.write("\n")
    print(f"wrote {args.out} ({len(results)} cases, {len(db_state)} tables)")
    return 0 if failures == 0 else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
