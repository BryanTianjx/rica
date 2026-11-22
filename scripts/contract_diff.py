#!/usr/bin/env python3
"""Diff two contract captures produced by ``contract_capture.py``.

The comparison is structural rather than textual, and it is deliberately
explicit about the two things that *must* be normalised and the three things
that are *declared* to differ.

Normalised on both sides (symmetrically, never to paper over a defect):

* **UUIDs.** Every generated primary key is a random UUID4, so the same row has a
  different id on each side. Ids are replaced by a stable placeholder assigned in
  order of first appearance (cases in sorted-name order, then the row dump). The
  map is per capture, so a row that appears in a response *and* in the final
  database gets the same placeholder in both — which is what makes
  ``audit_events.entity_id`` a meaningful assertion rather than noise.
* **Timestamps produced during the run.** Anything at or after the capture's
  own start time (minus a minute) is rendered ``<now>``: both servers stamp
  "now" with their own microsecond value. Seeded timestamps are in the past and
  stay literal, so they are compared exactly.
* ``details.errors`` inside a 422 envelope (pydantic's error list vs the Rust
  port's own stably shaped list). Only ``code``, ``message`` and the *set* of
  ``details`` keys are compared there.

Declared differences (``DECLARED_CASES``) are not compared generically: the
``improvements`` section asserts each side's concrete value against the table in
``docs/rust-migration-contract.md`` §7. A difference outside that list fails.

Exit code 0 means every case matched and every declared improvement held.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from datetime import UTC, datetime, timedelta
from typing import Any

import contract_layers as layers

#: Keys whose values are allowed to differ between implementations because
#: they describe *how* validation failed, not *whether* it failed.
VOLATILE_422_KEYS = ("errors",)

#: Cases whose `total`/`has_more` are *declared* to differ (IMP-1/IMP-3).
#: Everything else in those bodies — the items, their order, offset and limit —
#: is still compared strictly; only the envelope's count is exempt.
TOTAL_DECLARED_CASES = {
    "messages_limit2",
    "messages_limit2_offset1",
}

#: Cases whose whole response is declared to differ. IMP-2: the Python baseline
#: answers 500 where the port answers 422. IMP-4: `offset` beyond `u64::MAX` is
#: echoed by Python with an empty page, but the port's envelope cannot carry a
#: wider JSON number and answers 422. Asserted in `check_improvements`.
STATUS_DECLARED_CASES = {
    "position_patch_null_name",
    "position_patch_null_archived",
    "positions_offset_beyond_u64",
}

UUID_PATTERN = re.compile(
    r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}"
)

#: The declared values of IMP-1 and IMP-3, measured on the shared seed.
IMPROVEMENT_TOTALS = {
    # case: (python_total, rust_total)
    "positions_limit2": (6, 6),
    "candidates_limit2": (5, 5),
    "conversations_limit2": (2, 2),
    "send_records_limit2": (4, 4),
    "messages_limit2": (2, 5),
}


# ── Normalisation ──────────────────────────────────────────────────────────


def _collect_uuids(capture: dict) -> set[str]:
    """Every UUID that appears anywhere in one capture."""
    found: set[str] = set()

    def register(value: Any) -> None:
        if isinstance(value, dict):
            for key in sorted(value):
                register(value[key])
        elif isinstance(value, list):
            for item in value:
                register(item)
        elif isinstance(value, str):
            for match in UUID_PATTERN.finditer(value):
                found.add(match.group(0).lower())

    for name in sorted(capture.get("cases", {})):
        register(capture["cases"][name].get("body"))
    for table in sorted(capture.get("db_state", {})):
        for row in capture["db_state"][table]:
            register(row)
    return found


def _uuid_map(left: dict, right: dict) -> dict[str, str]:
    """Assign a placeholder to every UUID, using a SHARED order.

    The mapping used to be built per capture, so the same logical entity could
    get `<uuid:84>` on the Python side and `<uuid:85>` on the Rust side, and
    every row carrying an `owner_id` looked like a difference. The mapping is
    now built from BOTH captures at once: a UUID that appears in only one side
    is still mapped (to a placeholder of its own), and one that appears in both
    gets the SAME placeholder.

    The order is the sorted union of both sides' UUIDs, which is
    side-independent by construction.
    """
    every = _collect_uuids(left) | _collect_uuids(right)
    return {token: f"<uuid:{index}>" for index, token in enumerate(sorted(every), 1)}



def _side_local_uuid_map(capture: dict, shared: dict[str, str],
                          other: dict) -> dict[str, str]:
    """Placeholders for the UUIDs that exist in only ONE capture.

    Those are the ids the run itself generated (a created candidate, a
    confirmed message, a new draft) — the two sides legitimately produce
    different random ids, so comparing the values is meaningless. Each such
    UUID is replaced by `<new-uuid:N>`, numbered by the order in which it
    FIRST APPEARS in that side's own capture. The N-th new id on the left and
    the N-th new id on the right are the same entity, because the scenario
    creates them in the same order.

    A UUID present in BOTH captures is NOT side-local: it is a seeded id, and
    the shared mapping already handles it.
    """
    other_uuids = _collect_uuids(other)
    mapping: dict[str, str] = {}
    counter = 0

    def register(value: Any) -> None:
        nonlocal counter
        if isinstance(value, dict):
            for key in sorted(value):
                register(value[key])
        elif isinstance(value, list):
            for item in value:
                register(item)
        elif isinstance(value, str):
            for match in UUID_PATTERN.finditer(value):
                token = match.group(0).lower()
                if token in other_uuids or token in mapping:
                    continue
                counter += 1
                mapping[token] = f"<new-uuid:{counter}>"

    for name in sorted(capture.get("cases", {})):
        register(capture["cases"][name].get("body"))
    for table in sorted(capture.get("db_state", {})):
        for row in capture["db_state"][table]:
            register(row)
    return mapping

def _parse_timestamp(value: str) -> datetime | None:
    candidate = value.strip()
    if not candidate or len(candidate) < 10:
        return None
    if candidate.endswith("Z"):
        candidate = candidate[:-1] + "+00:00"
    # SQLite hands back `YYYY-MM-DD HH:MM:SS.ffffff`; fromisoformat accepts the
    # `T` separator, so give it one.
    if len(candidate) > 10 and candidate[10] == " ":
        candidate = candidate[:10] + "T" + candidate[11:]
    try:
        parsed = datetime.fromisoformat(candidate)
    except ValueError:
        return None
    if parsed.tzinfo is None:
        return parsed.replace(tzinfo=UTC)
    return parsed.astimezone(UTC)


def _normaliser(capture: dict, mapping: dict[str, str] | None = None,
                side_local: dict[str, str] | None = None):
    if mapping is None:
        mapping = _uuid_map(capture, capture)
    side_local = side_local or {}
    started_at = _parse_timestamp(capture.get("started_at", "")) or datetime.now(UTC)
    threshold = started_at - timedelta(seconds=60)

    def replace_uuids(text: str) -> str:
        def substitute(match: re.Match[str]) -> str:
            token = match.group(0).lower()
            # A side-local (run-generated) id first: it must win over the
            # shared mapping, because the shared map would give the same
            # placeholder to a DIFFERENT seeded id on the other side.
            if token in side_local:
                return side_local[token]
            return mapping.get(token, "<uuid>")

        return UUID_PATTERN.sub(substitute, text)

    def norm(value: Any) -> Any:
        if isinstance(value, dict):
            return {key: norm(item) for key, item in value.items()}
        if isinstance(value, list):
            return [norm(item) for item in value]
        if isinstance(value, str):
            parsed = _parse_timestamp(value)
            if parsed is not None and parsed >= threshold:
                return "<now>"
            # W6: a refresh token's expiry sits days ahead of the capture —
            # the near-now window never catches it, and the two sides' literal
            # values can never agree.
            if parsed is not None and parsed > started_at:
                return "<future>"
            return replace_uuids(value)
        return value

    return norm


#: The sealed-export envelope's fields. `iv` and `ciphertext` are fresh on every
#: call — a reused GCM nonce under one key is a real break, so comparing their
#: VALUES would fail every run for a reason that is not a contract difference.
#: Their LENGTHS are contract, though: a 96-bit nonce and a 128-bit tag.
SEALED_VOLATILE_FIELDS = ("iv", "ciphertext")
SEALED_LENGTH_FIELDS = ("iv", "tag")


def normalise_sealed_envelope(body: Any) -> Any:
    """Replace the volatile parts of a sealed export with their shape.

    The envelope is `{alg, iv, tag, ciphertext}`. The algorithm name and the
    tag's length are contract; the nonce and the ciphertext bytes are not, and
    cannot be — they differ on every call by design.
    """
    if not isinstance(body, dict):
        return body
    if not {"alg", "iv", "tag", "ciphertext"} <= set(body):
        return body

    normalised = dict(body)
    for field in SEALED_VOLATILE_FIELDS:
        value = normalised.get(field)
        if isinstance(value, str):
            normalised[field] = f"<{field}:{len(value)}>"
    for field in SEALED_LENGTH_FIELDS:
        value = normalised.get(field)
        if isinstance(value, str):
            normalised[field] = f"<{field}:{len(value)}>"
    return normalised


def normalise_envelope(body: Any, status: int) -> Any:
    """Collapse a 422 to the parts that are actually part of the contract."""
    if status != 422 or not isinstance(body, dict):
        return body
    error = body.get("error")
    if not isinstance(error, dict):
        return body
    details = error.get("details")
    if isinstance(details, dict):
        details = {k: v for k, v in details.items() if k not in VOLATILE_422_KEYS}
    return {
        "error": {
            "code": error.get("code"),
            "message": error.get("message"),
            "details_keys": sorted(details) if isinstance(details, dict) else None,
        }
    }


def normalise_case_body(name: str, body: Any) -> Any:
    """Collapse the W5/W6 responses' genuinely per-side values to shapes.

    * token pairs: the JWT signature and the refresh token are random on
      every issue; the token_type/expires_in/user are contract.
    * backups: the file NAME embeds a timestamp, the PATH embeds each side's
      own directories, and the SIZE reflects each implementation's page
      layout — the name shape and the kind are contract.
    * settings: database_url and backup_dir name each side's own throwaway
      directories; the baseline runs from /tmp/ica-py-baseline and this repo
      from /tmp/ica-rs-port, so the values can never literally agree.
    """
    if not isinstance(body, (dict, list)):
        return body

    def walk(value):
        if isinstance(value, dict):
            return {key: walk(item) for key, item in value.items()}
        if isinstance(value, list):
            return [walk(item) for item in value]
        if isinstance(value, str):
            import re as _re
            if _re.search(r"(ica-backup|pre-restore)-[0-9A-Za-z-]+\.db", value):
                return _re.sub(
                    r"(ica-backup|pre-restore)-[0-9A-Za-z-]+\.db",
                    "<backup-file>",
                    value,
                )
            return value
        return value

    def strip_backup_volatile(record):
        if not isinstance(record, dict):
            return record
        record = dict(record)
        for field in ("database_url", "backup_dir", "path"):
            if isinstance(record.get(field), str):
                record[field] = "<" + field + ">"
        if "size_bytes" in record:
            record["size_bytes"] = "<size>"
        return record

    if name.startswith("auth_login") or name.startswith("auth_refresh"):
        if not isinstance(body, dict):
            return body
        body = walk(dict(body))
        if "access_token" in body:
            body["access_token"] = "<jwt>"
        if "refresh_token" in body:
            body["refresh_token"] = "<refresh>"
        return body
    if "backup" in name or name.startswith("settings"):
        body = walk(body)
        if isinstance(body, list):
            return [strip_backup_volatile(item) for item in body]
        return strip_backup_volatile(body)
    if name.startswith("imports_ocr"):
        # The undecodable-image reason echoes the staged FILE PATH, whose
        # name embeds the side (each side reads its own copy — the request
        # deletes it). The basename with the side marker stripped is the
        # contract; the staging layout is not.
        def walk_ocr(value):
            if isinstance(value, dict):
                return {key: walk_ocr(item) for key, item in value.items()}
            if isinstance(value, list):
                return [walk_ocr(item) for item in value]
            if isinstance(value, str):
                import re as _re
                return _re.sub(
                    r"contract-ocr-[a-z-]*(py|rs)\.png",
                    "contract-ocr.png",
                    value,
                )
            return value

        return walk_ocr(body)
    return body


def field_set(value: Any) -> Any:
    """Structurally describe a JSON value, for field-set comparisons."""
    if isinstance(value, dict):
        return {k: field_set(v) for k, v in value.items()}
    if isinstance(value, list):
        return [field_set(v) for v in value]
    return True


def _without_counts(body: Any) -> Any:
    """Drop the two declared-different envelope fields, keeping the rest."""
    if isinstance(body, dict):
        return {key: value for key, value in body.items() if key not in ("total", "has_more")}
    return body


def diff_case(name: str, left: dict, right: dict, norm) -> list[str]:
    problems: list[str] = []
    if left["status"] != right["status"]:
        problems.append(f"status: python={left['status']} rust={right['status']}")
    # The envelope's media type is part of the frontend's single error path: a
    # plain-text rejection is exactly the F-02 failure mode, so pin it.
    if left.get("content_type") != right.get("content_type"):
        problems.append(
            f"content_type: python={left.get('content_type')!r} "
            f"rust={right.get('content_type')!r}"
        )
    # Redirect targets, already reduced to `path?query` at capture time (the
    # absolute URL embeds each side's own port).
    if left.get("location") != right.get("location"):
        problems.append(
            f"location: python={left.get('location')!r} rust={right.get('location')!r}"
        )

    left_body = normalise_sealed_envelope(
        normalise_envelope(norm(left.get("body")), left["status"])
    )
    right_body = normalise_sealed_envelope(
        normalise_envelope(norm(right.get("body")), right["status"])
    )

    left_shape = field_set(left_body)
    right_shape = field_set(right_body)
    if left_shape != right_shape:
        problems.append(
            f"shape differs:\n  python={json.dumps(left_shape, sort_keys=True)}"
            f"\n  rust  ={json.dumps(right_shape, sort_keys=True)}"
        )

    if left_body != right_body:
        problems.append(
            "values differ:\n  python="
            + json.dumps(left_body, sort_keys=True, ensure_ascii=False)
            + "\n  rust  ="
            + json.dumps(right_body, sort_keys=True, ensure_ascii=False)
        )
    return problems


# ── Declared improvements ──────────────────────────────────────────────────


def _total(case: dict) -> Any:
    body = case.get("body")
    return body.get("total") if isinstance(body, dict) else None


def _items(case: dict) -> Any:
    body = case.get("body")
    return body.get("items") if isinstance(body, dict) else None


def check_improvements(py_cases: dict, rs_cases: dict) -> tuple[list[str], list[str]]:
    """Assert each declared improvement's concrete values on both sides.

    Returns ``(report_lines, problems)``.
    """
    lines: list[str] = []
    problems: list[str] = []

    # IMP-1: GET /api/messages reports the real total.
    case = "messages_limit2"
    py, rs = py_cases.get(case), rs_cases.get(case)
    if py is None or rs is None:
        problems.append(f"IMP-1: case {case} missing from a capture")
    else:
        py_total, rs_total = _total(py), _total(rs)
        declared = IMPROVEMENT_TOTALS[case]
        lines.append(
            f"  IMP-1 messages.total is the real count: python total={py_total} "
            f"(truncated to limit+offset), rust total={rs_total} (5 matching rows)"
        )
        if (py_total, rs_total) != declared:
            problems.append(
                f"IMP-1: expected python total={declared[0]} / rust total={declared[1]}, "
                f"measured {py_total}/{rs_total}"
            )
        py_more = py.get("body", {}).get("has_more")
        rs_more = rs.get("body", {}).get("has_more")
        lines.append(
            f"  IMP-1 has_more: python={py_more} (unreachable), rust={rs_more}"
        )
        if py_more is not False or rs_more is not True:
            problems.append(
                f"IMP-1: expected python has_more=False / rust has_more=True, "
                f"measured {py_more}/{rs_more}"
            )

    # IMP-2: explicit null on a NOT NULL column.
    for case in ("position_patch_null_name", "position_patch_null_archived"):
        py, rs = py_cases.get(case), rs_cases.get(case)
        if py is None or rs is None:
            problems.append(f"IMP-2: case {case} missing from a capture")
            continue
        py_code = (py.get("body") or {}).get("error", {}).get("code")
        rs_code = (rs.get("body") or {}).get("error", {}).get("code")
        lines.append(
            f"  IMP-2 {case}: python status={py['status']} code={py_code}, "
            f"rust status={rs['status']} code={rs_code}"
        )
        if (py["status"], py_code) != (500, "internal_error"):
            problems.append(
                f"IMP-2: {case} expected python 500/internal_error, "
                f"measured {py['status']}/{py_code}"
            )
        if (rs["status"], rs_code) != (422, "validation_error"):
            problems.append(
                f"IMP-2: {case} expected rust 422/validation_error, "
                f"measured {rs['status']}/{rs_code}"
            )

    # IMP-3: one `total` meaning across the five lists, with item order pinned.
    for case, (py_expected, rs_expected) in IMPROVEMENT_TOTALS.items():
        py, rs = py_cases.get(case), rs_cases.get(case)
        if py is None or rs is None:
            problems.append(f"IMP-3: case {case} missing from a capture")
            continue
        py_total, rs_total = _total(py), _total(rs)
        lines.append(
            f"  IMP-3 {case}: python total={py_total}, rust total={rs_total}"
        )
        if (py_total, rs_total) != (py_expected, rs_expected):
            problems.append(
                f"IMP-3: {case} expected python={py_expected} / rust={rs_expected}, "
                f"measured {py_total}/{rs_total}"
            )
        # OPT-2 depends on this: pagination pushed into SQL must return the same
        # rows in the same order as the in-memory slice.
        py_ids = [row.get("id") for row in (_items(py) or [])]
        rs_ids = [row.get("id") for row in (_items(rs) or [])]
        if len(py_ids) != len(rs_ids):
            problems.append(
                f"IMP-3: {case} item count differs: python={len(py_ids)} rust={len(rs_ids)}"
            )
        elif py_ids != rs_ids:
            problems.append(f"IMP-3: {case} item order differs: {py_ids} vs {rs_ids}")

    # IMP-4: `offset` beyond `u64::MAX`. Python parses unbounded integers and
    # echoes the value back with an empty page; the port's envelope types the
    # offset as `usize` (a JSON number up to u64::MAX), so the echo is
    # impossible and it answers 422 `int_parsing` instead. Everything up to
    # u64::MAX is matched by `positions_offset_beyond_i64` above.
    case = "positions_offset_beyond_u64"
    py, rs = py_cases.get(case), rs_cases.get(case)
    if py is None or rs is None:
        problems.append(f"IMP-4: case {case} missing from a capture")
    else:
        py_offset = (py.get("body") or {}).get("offset")
        rs_code = (rs.get("body") or {}).get("error", {}).get("code")
        lines.append(
            f"  IMP-4 {case}: python status={py['status']} offset={py_offset}, "
            f"rust status={rs['status']} code={rs_code}"
        )
        if py["status"] != 200 or py_offset != 18446744073709551616:
            problems.append(
                f"IMP-4: expected python 200 with offset=18446744073709551616, "
                f"measured status={py['status']} offset={py_offset}"
            )
        if (rs["status"], rs_code) != (422, "validation_error"):
            problems.append(
                f"IMP-4: expected rust 422/validation_error, "
                f"measured {rs['status']}/{rs_code}"
            )

    return lines, problems


# ── Database state ─────────────────────────────────────────────────────────


def diff_db_state(py_state: dict, rs_state: dict, norm,
                  byte_json: bool = False,
                  json_columns: dict[str, list[str]] | None = None) -> list[str]:
    """Compare final DB state.

    Default keeps the existing, parsed-value semantics (L4 legacy).  With
    ``byte_json=True`` the comparison is delegated to the byte-level L4 in
    ``contract_layers``: every column — JSON columns in particular — is
    compared as raw bytes, with no ``json.loads`` anywhere on the path.
    """
    if byte_json:
        return layers.diff_db_state_bytes(py_state, rs_state, json_columns)
    problems: list[str] = []
    for table in sorted(set(py_state) | set(rs_state)):
        left = py_state.get(table)
        right = rs_state.get(table)
        if left is None or right is None:
            problems.append(f"db.{table}: present on only one side")
            continue
        if len(left) != len(right):
            problems.append(f"db.{table}: row count python={len(left)} rust={len(right)}")
            continue
        left_rows = [json.dumps(norm(row), sort_keys=True) for row in left]
        right_rows = [json.dumps(norm(row), sort_keys=True) for row in right]
        if left_rows == right_rows:
            print(f"PASS db.{table} ({len(left)} rows)")
            continue
        problems.append(f"db.{table}: rows differ")
        for index, (a, b) in enumerate(zip(left_rows, right_rows)):
            if a != b:
                problems.append(f"  first difference at row {index}:\n    python={a}\n    rust  ={b}")
                break
        # Show which rows are unmatched, which is more useful than a positional
        # diff once the lists have been sorted for comparison.
        only_py = sorted(set(left_rows) - set(right_rows))
        only_rs = sorted(set(right_rows) - set(left_rows))
        for row in only_py[:3]:
            problems.append(f"  only in python: {row}")
        for row in only_rs[:3]:
            problems.append(f"  only in rust:   {row}")
    return problems


# ── Entry point ────────────────────────────────────────────────────────────


#: JSON columns that L4's byte-level mode keeps opaque (no ``json.loads``).
L4_JSON_COLUMNS: dict[str, list[str]] = {"audit_events": ["metadata_json"]}


def _l4_rows(state: dict, norm, byte_json: bool) -> dict:
    """Final-state rows; JSON columns stay raw when L4 byte mode is on."""
    def mask_secrets(table, rows):
        if table == "users":
            return [{**row, "password_hash": "<bcrypt>"} for row in rows]
        if table == "refresh_tokens":
            return [{**row, "token_hash": "<sha256>"} for row in rows]
        return rows

    result: dict = {}
    for table, rows in state.items():
        masked = mask_secrets(table, rows)
        if not byte_json:
            result[table] = [norm(row) for row in masked]
            continue
        json_cols = set(L4_JSON_COLUMNS.get(table, []))
        result[table] = [
            {
                key: (value if key in json_cols else norm(value))
                for key, value in row.items()
            }
            for row in masked
        ]
    return result


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(
        description="Diff two contract captures through the L1-L5 layer framework."
    )
    parser.add_argument("--python", required=True, help="capture from the Python baseline")
    parser.add_argument("--rust", required=True, help="capture from the Rust port")
    parser.add_argument(
        "--layer", default=None,
        help="layers to run: L1 (HTTP), L2 (schema), L3 (validation), L4 (persistence), "
             "L5 (behaviour); 'all', comma list or range (e.g. L2-L4). "
             f"default: {','.join(layers.DEFAULT_LAYERS)} (the legacy L1+L4 comparison)",
    )
    parser.add_argument(
        "--byte-json", action="store_true",
        help="L4 compares JSON columns as raw bytes instead of parsed values "
             "(capture with contract_capture.py --keep-json-raw)",
    )
    parser.add_argument(
        "--check-uncovered", action="store_true",
        help="assert that the trailing UNCOVERED boundary section is a non-empty, "
             "machine-checkable string (exit 0 when it is and no diff failed)",
    )
    args = parser.parse_args(argv)

    try:
        enabled = layers.parse_layers(args.layer)
    except ValueError as exc:
        parser.error(str(exc))

    print("layers:")
    for line in layers.describe_layers(enabled):
        print(line)

    with open(args.python, encoding="utf-8") as handle:
        py = json.load(handle)
    with open(args.rust, encoding="utf-8") as handle:
        rs = json.load(handle)

    py_cases, rs_cases = py["cases"], rs["cases"]
    # ONE mapping for both sides: a UUID that means the same thing on both
    # sides must normalise to the same placeholder, or every row carrying it
    # reads as a difference.
    shared_uuids = _uuid_map(py, rs)
    py_local = _side_local_uuid_map(py, shared_uuids, rs)
    rs_local = _side_local_uuid_map(rs, shared_uuids, py)
    py_norm = _normaliser(py, shared_uuids, py_local)
    rs_norm = _normaliser(rs, shared_uuids, rs_local)

    only_py = sorted(set(py_cases) - set(rs_cases))
    only_rs = sorted(set(rs_cases) - set(py_cases))
    failures = 0
    if only_py or only_rs:
        print(f"case set differs: only_python={only_py} only_rust={only_rs}")
        failures += len(only_py) + len(only_rs)

    shared = sorted(set(py_cases) & set(rs_cases))
    # Normalised bodies and envelopes are shared by L1/L2/L3 so no layer can
    # re-litigate the existing 422/sealed-envelope normalisation rules.
    py_bodies: dict[str, Any] = {}
    rs_bodies: dict[str, Any] = {}
    py_l1_bodies: dict[str, Any] = {}
    rs_l1_bodies: dict[str, Any] = {}
    py_envelopes: dict[str, Any] = {}
    rs_envelopes: dict[str, Any] = {}
    for name in shared:
        left_body = normalise_case_body(name, py_norm(py_cases[name].get("body")))
        right_body = normalise_case_body(name, rs_norm(rs_cases[name].get("body")))
        if name in TOTAL_DECLARED_CASES:
            left_body = _without_counts(left_body)
            right_body = _without_counts(right_body)
        py_l1_bodies[name] = left_body
        rs_l1_bodies[name] = right_body
        py_envelopes[name] = normalise_envelope(left_body, py_cases[name]["status"])
        rs_envelopes[name] = normalise_envelope(right_body, rs_cases[name]["status"])
        # L2 sees exactly the bodies L1 compares (same envelope + sealed
        # normalisation), so a layer can never disagree with L1 on shape.
        py_bodies[name] = normalise_sealed_envelope(py_envelopes[name])
        rs_bodies[name] = normalise_sealed_envelope(rs_envelopes[name])

    if "L1" in enabled:
        print("\nlayer L1 (HTTP):")
        for name in shared:
            if name in STATUS_DECLARED_CASES:
                print(f"DECL {name} (asserted in the improvements section)")
                continue
            left = {**py_cases[name], "body": py_l1_bodies[name]}
            right = {**rs_cases[name], "body": rs_l1_bodies[name]}
            problems = diff_case(name, left, right, lambda value: value)
            if problems:
                failures += 1
                print(f"FAIL {name}")
                for problem in problems:
                    print("  " + problem.replace("\n", "\n  "))
            else:
                suffix = " (items only: total is declared)" if name in TOTAL_DECLARED_CASES else ""
                print(f"PASS {name}{suffix}")

        print("\nimprovements:")
        lines, improvement_problems = check_improvements(py_cases, rs_cases)
        for line in lines:
            print(line)
        for problem in improvement_problems:
            print(f"  FAIL {problem}")
        failures += len(improvement_problems)

    if "L2" in enabled:
        print("\nlayer L2 (schema):")
        l2_lines, l2_problems = layers.check_l2_schema(
            py_bodies, rs_bodies, py.get("db_state"), rs.get("db_state"),
            STATUS_DECLARED_CASES,
        )
        for line in l2_lines:
            print(line)
        for problem in l2_problems:
            print(f"  FAIL {problem}")
        failures += len(l2_problems)

    if "L3" in enabled:
        print("\nlayer L3 (validation):")
        l3_lines, l3_problems = layers.check_l3_validation(
            py_cases, rs_cases, py_envelopes, rs_envelopes, STATUS_DECLARED_CASES
        )
        for line in l3_lines:
            print(line)
        for problem in l3_problems:
            print(f"  FAIL {problem}")
        failures += len(l3_problems)

    if "L5" in enabled:
        print("\nlayer L5 (behaviour):")
        l5_lines, l5_problems = layers.check_l5_behavior(
            py_cases, rs_cases, py.get("db_state"), rs.get("db_state"),
            STATUS_DECLARED_CASES,
        )
        for line in l5_lines:
            print(line)
        for problem in l5_problems:
            print(f"  FAIL {problem}")
        failures += len(l5_problems)

    if "L4" in enabled:
        print("\ndatabase state:")
        if py.get("db_state") and rs.get("db_state"):
            # Each side is normalised with its *own* UUID map: the same row has
            # different random ids on the two servers. Byte mode keeps JSON
            # columns raw so their spacing/key order is compared verbatim.
            py_rows = _l4_rows(py["db_state"], py_norm, args.byte_json)
            rs_rows = _l4_rows(rs["db_state"], rs_norm, args.byte_json)
            db_problems = diff_db_state(
                py_rows, rs_rows, lambda value: value,
                byte_json=args.byte_json, json_columns=L4_JSON_COLUMNS,
            )
            for problem in db_problems:
                print(f"FAIL {problem}")
                failures += 1
        else:
            print("SKIP no row dump in one of the captures")
    else:
        print("\ndatabase state: SKIP (L4 not enabled)")

    total = len(shared)
    matched = total if failures == 0 else max(total - failures, 0)
    print(f"\n{matched}/{total} cases matched, {failures} failure(s)")

    # Coverage boundary: always the LAST section of every comparison, and
    # always a non-empty machine-checkable string.
    uncovered_text = layers.render_uncovered()
    print("\n" + uncovered_text)
    check_ok = True
    if args.check_uncovered:
        check_ok = bool(uncovered_text.strip())
        print(
            f"UNCOVERED_CHECK {'ok' if check_ok else 'FAIL'}: "
            f"{'non-empty' if check_ok else 'empty'} boundary string"
        )
    return 0 if failures == 0 and check_ok else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
