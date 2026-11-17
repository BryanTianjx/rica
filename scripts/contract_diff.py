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


def _uuid_map(capture: dict) -> dict[str, str]:
    """Assign a placeholder to every UUID, in a side-independent order."""
    mapping: dict[str, str] = {}

    def register(value: Any) -> None:
        if isinstance(value, dict):
            for key in sorted(value):
                register(value[key])
        elif isinstance(value, list):
            for item in value:
                register(item)
        elif isinstance(value, str):
            for match in UUID_PATTERN.finditer(value):
                token = match.group(0).lower()
                if token not in mapping:
                    mapping[token] = f"<uuid:{len(mapping) + 1}>"

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


def _normaliser(capture: dict):
    mapping = _uuid_map(capture)
    started_at = _parse_timestamp(capture.get("started_at", "")) or datetime.now(UTC)
    threshold = started_at - timedelta(seconds=60)

    def replace_uuids(text: str) -> str:
        def substitute(match: re.Match[str]) -> str:
            token = match.group(0).lower()
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
            return replace_uuids(value)
        return value

    return norm


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

    left_body = normalise_envelope(norm(left.get("body")), left["status"])
    right_body = normalise_envelope(norm(right.get("body")), right["status"])

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


def diff_db_state(py_state: dict, rs_state: dict, norm) -> list[str]:
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


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--python", required=True, help="capture from the Python baseline")
    parser.add_argument("--rust", required=True, help="capture from the Rust port")
    args = parser.parse_args(argv)

    with open(args.python, encoding="utf-8") as handle:
        py = json.load(handle)
    with open(args.rust, encoding="utf-8") as handle:
        rs = json.load(handle)

    py_cases, rs_cases = py["cases"], rs["cases"]
    py_norm = _normaliser(py)
    rs_norm = _normaliser(rs)

    only_py = sorted(set(py_cases) - set(rs_cases))
    only_rs = sorted(set(rs_cases) - set(py_cases))
    failures = 0
    if only_py or only_rs:
        print(f"case set differs: only_python={only_py} only_rust={only_rs}")
        failures += len(only_py) + len(only_rs)

    shared = sorted(set(py_cases) & set(rs_cases))
    for name in shared:
        if name in STATUS_DECLARED_CASES:
            print(f"DECL {name} (asserted in the improvements section)")
            continue
        left_body = py_norm(py_cases[name].get("body"))
        right_body = rs_norm(rs_cases[name].get("body"))
        if name in TOTAL_DECLARED_CASES:
            left_body = _without_counts(left_body)
            right_body = _without_counts(right_body)
        left = {**py_cases[name], "body": left_body}
        right = {**rs_cases[name], "body": right_body}
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

    print("\ndatabase state:")
    if py.get("db_state") and rs.get("db_state"):
        # Each side is normalised with its *own* UUID map: the same row has
        # different random ids on the two servers, and the placeholder numbering
        # is per capture.
        py_rows = {
            table: [py_norm(row) for row in rows]
            for table, rows in py["db_state"].items()
        }
        rs_rows = {
            table: [rs_norm(row) for row in rows]
            for table, rows in rs["db_state"].items()
        }
        for problem in diff_db_state(py_rows, rs_rows, lambda value: value):
            print(f"FAIL {problem}")
            failures += 1
    else:
        print("SKIP no row dump in one of the captures")

    total = len(shared)
    matched = total if failures == 0 else max(total - failures, 0)
    print(f"\n{matched}/{total} cases matched, {failures} failure(s)")
    return 0 if failures == 0 else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
