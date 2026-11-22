#!/usr/bin/env python3
"""L1-L5 comparison layers for the contract diff, and the coverage boundary.

This module is *additive*.  ``contract_diff.py`` keeps its existing comparison
semantics; this module only offers extra, independently selectable views of the
same two captures:

* **L1 — HTTP**: ``contract_diff.diff_case`` (status / content-type / location /
  body shape / body values) and the declared-improvements assertions.  Owned by
  ``contract_diff.py``; listed here so the layer map is complete.
* **L2 — Schema**: response field shapes and the database table/column sets.
* **L3 — Validation**: 422 envelopes (code / message / details key set) and the
  status of every error-path case.
* **L4 — Persistence**: per-table, per-column final SQLite state; JSON columns
  are compared as **raw bytes**, never through ``json.loads``.
* **L5 — Behaviour**: idempotency (repeat requests must not create duplicate
  side effects), state transitions and exception recovery.

It also renders the mandatory ``UNCOVERED`` section: the coverage boundary is
output, not a footnote in a document.  ``selftest()`` exercises each new layer
without needing a running server; run it with
``PYTHONDONTWRITEBYTECODE=1 python3 scripts/contract_layers.py``.
"""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any, Iterable

LAYERS: tuple[str, ...] = ("L1", "L2", "L3", "L4", "L5")
LAYER_TITLES: dict[str, str] = {
    "L1": "HTTP responses (status / headers / body) + declared improvements",
    "L2": "Schema (response field shapes, DB table and column sets)",
    "L3": "Validation (422 envelopes and error-path statuses)",
    "L4": "Persistence (final SQLite state, JSON columns byte-for-byte)",
    "L5": "Behaviour (idempotency, state transitions, exception recovery)",
}
#: Legacy behaviour of ``contract_diff.py`` is exactly L1 + L4.  Keeping that
#: as the default is what makes the existing 286/286 comparison a true
#: no-regression check; L2/L3/L5 are opt-in with ``--layer``.
DEFAULT_LAYERS: tuple[str, ...] = ("L1", "L4")

REPO_ROOT = Path(__file__).resolve().parent.parent
COVERAGE_JSON = REPO_ROOT / "docs" / "contract-baseline" / "contract-coverage.json"

#: Case-name markers that denote a repeat / idempotency probe.
IDEMPOTENCY_MARKERS: tuple[str, ...] = (
    "idempotent",
    "twice",
    "_again",
    "repeat",
    "retry",
    "duplicate",
)

#: Coverage boundary when the generated matrix data is unavailable.  These are
#: real, structural gaps in the capture harness, not placeholders: the harness
#: is a serial HTTP client with no permission model and no fault injection.
UNCOVERED_FALLBACK: tuple[str, ...] = (
    "权限维度: 68/68 契约未覆盖 — local 单用户模式无 Policy Engine (W6 建权限模型后补)",
    "并发维度: 68/68 契约未覆盖 — capture 为串行 HTTP 客户端，无并发驱动",
    "DB异常维度: 68/68 契约未覆盖 — capture 不注入 SQLite 锁/约束失败",
    "L5修复回归: 仅幂等/重试类用例可用，状态迁移与异常恢复未有独立断言层",
)


# ── Layer selection ────────────────────────────────────────────────────────


def parse_layers(spec: str | Iterable[str] | None) -> list[str]:
    """Parse ``--layer`` input into an ordered subset of :data:`LAYERS`.

    Accepted forms: ``all``, ``L1``, ``L1,L3,L5``, ``L1-L3`` (inclusive).
    ``None``/empty selects :data:`DEFAULT_LAYERS`.  A bad token raises
    ``ValueError`` with the offending token in the message.
    """
    if spec is None or spec == "":
        return list(DEFAULT_LAYERS)
    if isinstance(spec, str):
        tokens = [token.strip().upper() for token in spec.split(",") if token.strip()]
    else:
        tokens = [str(token).strip().upper() for token in spec if str(token).strip()]
    if not tokens:
        return list(DEFAULT_LAYERS)
    if "ALL" in tokens:
        return list(LAYERS)

    selected: set[str] = set()
    for token in tokens:
        if "-" in token:
            start, end = token.split("-", 1)
            if start not in LAYERS or end not in LAYERS:
                raise ValueError(f"unknown layer range: {token}")
            lo, hi = LAYERS.index(start), LAYERS.index(end)
            if lo > hi:
                raise ValueError(f"reversed layer range: {token}")
            selected.update(LAYERS[lo : hi + 1])
        elif token in LAYERS:
            selected.add(token)
        else:
            raise ValueError(f"unknown layer: {token} (expected one of {', '.join(LAYERS)}, all)")
    return [layer for layer in LAYERS if layer in selected]


def describe_layers(selected: Iterable[str]) -> list[str]:
    """Human-readable list of the active layers, in L1..L5 order."""
    return [f"  {layer}: {LAYER_TITLES[layer]}" for layer in LAYERS if layer in set(selected)]


# ── Byte-level helpers (L4) ────────────────────────────────────────────────


def byte_repr(value: Any) -> bytes:
    """Bytes used for the L4 comparison.

    Strings are encoded verbatim — a JSON column's spacing, key order and
    escaping are part of what is compared.  Non-string SQLite values are
    rendered through a stable, type-tagged JSON form so that integer ``1`` and
    string ``"1"`` never collapse into the same bytes.
    """
    # The tag is part of the compared bytes: SQLite's integer 1 and the string
    # "1" must not collapse into the same value.
    if isinstance(value, str):
        return b"s:" + value.encode("utf-8")
    if isinstance(value, (bytes, bytearray)):
        return b"b:" + bytes(value)
    return b"v:" + json.dumps(
        value, sort_keys=True, ensure_ascii=False, separators=(",", ":")
    ).encode("utf-8")


def diff_db_state_bytes(py_state: dict, rs_state: dict,
                        json_columns: dict[str, list[str]] | None = None) -> list[str]:
    """Compare two final DB dumps table-by-table, column-by-column, in bytes.

    Unlike ``contract_diff.diff_db_state`` this NEVER calls ``json.loads`` on a
    JSON column: ``metadata_json`` with ``{\"a\": 1}`` and ``{\"a\":1}`` differ.
    Requires a capture made with ``contract_capture.py --keep-json-raw``; a
    parsed JSON value is compared through its canonical repr and the caller is
    told that byte fidelity was not available.
    """
    json_columns = json_columns or {}
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
        left_cols = sorted({key for row in left for key in row})
        right_cols = sorted({key for row in right for key in row})
        if left_cols != right_cols:
            problems.append(
                f"db.{table}: columns differ python={left_cols} rust={right_cols}"
            )
            continue
        for col in sorted(json_columns.get(table, [])):
            if any(isinstance(row.get(col), (dict, list)) for row in left + right):
                problems.append(
                    f"db.{table}.{col}: capture holds parsed JSON — byte-level L4 "
                    "needs contract_capture.py --keep-json-raw"
                )
        for index, (a_row, b_row) in enumerate(zip(left, right)):
            for col in sorted(set(a_row) | set(b_row)):
                if byte_repr(a_row.get(col)) != byte_repr(b_row.get(col)):
                    problems.append(
                        f"db.{table}[{index}].{col}: bytes differ "
                        f"python={byte_repr(a_row.get(col))!r} rust={byte_repr(b_row.get(col))!r}"
                    )
    return problems


# ── L2: schema ─────────────────────────────────────────────────────────────


def _shape(value: Any) -> Any:
    if isinstance(value, dict):
        return {key: _shape(item) for key, item in value.items()}
    if isinstance(value, list):
        return [_shape(item) for item in value]
    return True


def check_l2_schema(py_bodies: dict, rs_bodies: dict,
                    py_state: dict | None = None,
                    rs_state: dict | None = None,
                    exempt: Iterable[str] = ()) -> tuple[list[str], list[str]]:
    """Compare response field shapes and DB table/column sets.

    ``py_bodies``/``rs_bodies`` are the *already normalised* case bodies from
    the L1 pass, so the known 422 ``details.errors`` normalisation is honoured
    instead of being re-litigated here.
    """
    lines: list[str] = []
    problems: list[str] = []
    exempt = set(exempt)
    shared = sorted(name for name in (set(py_bodies) & set(rs_bodies)) if name not in exempt)
    bad = [name for name in shared if _shape(py_bodies[name]) != _shape(rs_bodies[name])]
    for name in bad:
        problems.append(f"L2 schema: {name} response shape differs")
    lines.append(
        f"  L2 response schema: {len(shared) - len(bad)}/{len(shared)} cases share a field shape"
    )

    py_state = py_state or {}
    rs_state = rs_state or {}
    if py_state or rs_state:
        for table in sorted(set(py_state) | set(rs_state)):
            left, right = py_state.get(table), rs_state.get(table)
            if left is None or right is None:
                problems.append(f"L2 schema: db.{table} present on only one side")
                continue
            left_cols = sorted({key for row in left for key in row})
            right_cols = sorted({key for row in right for key in row})
            if left_cols != right_cols:
                problems.append(
                    f"L2 schema: db.{table} columns python={left_cols} rust={right_cols}"
                )
        lines.append(
            f"  L2 db schema: {len(set(py_state) | set(rs_state))} table(s) checked "
            "for table/column-set parity"
        )
    else:
        lines.append("  L2 db schema: SKIP no row dump in the captures")
    return lines, problems


# ── L3: validation ─────────────────────────────────────────────────────────


def check_l3_validation(py_cases: dict, rs_cases: dict,
                        py_envelopes: dict, rs_envelopes: dict,
                        exempt: Iterable[str] = ()) -> tuple[list[str], list[str]]:
    """Compare the contract-visible part of every validation error envelope."""
    lines: list[str] = []
    problems: list[str] = []
    exempt = set(exempt)
    shared = sorted(set(py_cases) & set(rs_cases))
    error_cases = [
        name for name in shared
        if name not in exempt and (
            py_cases[name].get("status") == 422 or rs_cases[name].get("status") == 422
            or (isinstance(py_cases[name].get("body"), dict)
                and "error" in py_cases[name]["body"])
        )
    ]
    for name in error_cases:
        py_status = py_cases[name].get("status")
        rs_status = rs_cases[name].get("status")
        if py_status != rs_status:
            problems.append(f"L3 validation: {name} status python={py_status} rust={rs_status}")
        if py_envelopes.get(name) != rs_envelopes.get(name):
            problems.append(
                f"L3 validation: {name} envelope python={py_envelopes.get(name)!r} "
                f"rust={rs_envelopes.get(name)!r}"
            )
    lines.append(
        f"  L3 validation: {len(error_cases)} error-path case(s) checked "
        "(status + code + message + details key set)"
    )
    lines.append(
        "  L3 note: 422 details.errors VALUES stay outside the contract "
        "(pydantic-only); L3 pins status/code/message/key set"
    )
    return lines, problems


# ── L5: behaviour ──────────────────────────────────────────────────────────


def idempotency_cases(names: Iterable[str]) -> list[str]:
    """Case names that probe repeat-request behaviour."""
    return sorted(
        name for name in names
        if any(marker in name for marker in IDEMPOTENCY_MARKERS)
    )


def check_l5_behavior(py_cases: dict, rs_cases: dict,
                      py_state: dict | None = None,
                      rs_state: dict | None = None,
                      exempt: Iterable[str] = ()) -> tuple[list[str], list[str]]:
    """Compare behaviour: idempotency, state transitions, exception recovery.

    Idempotency is observable as "a repeated request leaves the same final
    state as a single one".  The capture only retains the final dump, so L5
    compares the per-table row-count vector between the two sides (an extra
    row on one side is a duplicate side effect) and the status of each probe
    request.  Content parity is L4's job; L5 owns the behavioural claim.
    """
    lines: list[str] = []
    problems: list[str] = []
    exempt = set(exempt)
    shared = sorted(set(py_cases) & set(rs_cases))
    probes = [name for name in idempotency_cases(shared) if name not in exempt]
    if not probes:
        problems.append("L5 behaviour: no idempotency probe found in the capture")
    for name in probes:
        py_status = py_cases[name].get("status")
        rs_status = rs_cases[name].get("status")
        if py_status != rs_status:
            problems.append(
                f"L5 idempotency: {name} status python={py_status} rust={rs_status}"
            )
        lines.append(f"  L5 idempotency: {name} python={py_status} rust={rs_status}")

    py_state, rs_state = py_state or {}, rs_state or {}
    for table in sorted(set(py_state) | set(rs_state)):
        left, right = py_state.get(table), rs_state.get(table)
        if left is None or right is None:
            problems.append(f"L5 state: db.{table} present on only one side")
            continue
        if len(left) != len(right):
            problems.append(
                f"L5 idempotency: db.{table} row count python={len(left)} "
                f"rust={len(right)} — duplicate side effect suspected"
            )
    lines.append(
        f"  L5 state: {len(set(py_state) | set(rs_state))} table(s) checked for "
        "duplicate-side-effect row counts"
    )
    recovery = [
        name for name in shared
        if name not in exempt
        and any(marker in name for marker in ("missing", "not_found", "invalid", "blocked",
                                             "bad_", "error", "reject", "cancel"))
    ]
    for name in recovery:
        if py_cases[name].get("status") != rs_cases[name].get("status"):
            problems.append(
                f"L5 recovery: {name} status python={py_cases[name].get('status')} "
                f"rust={rs_cases[name].get('status')}"
            )
    lines.append(f"  L5 recovery: {len(recovery)} error/recovery case(s) checked")
    return lines, problems


# ── Coverage boundary ──────────────────────────────────────────────────────


def uncovered_paths(path: str | Path | None = None) -> list[str]:
    """Machine-checkable list of uncovered contract paths / dimensions.

    Reads the JSON companion of the coverage matrix when present; otherwise
    falls back to the structural gaps listed in :data:`UNCOVERED_FALLBACK`.
    The list is always non-empty by construction — an empty coverage boundary
    would mean the generator had silently failed.
    """
    target = Path(path) if path else COVERAGE_JSON
    items: list[str] = []
    if target.is_file():
        try:
            data = json.loads(target.read_text(encoding="utf-8"))
            items = [str(item) for item in data.get("uncovered", []) if str(item)]
        except (OSError, ValueError, AttributeError):
            items = []
    return items or list(UNCOVERED_FALLBACK)


def render_uncovered(paths: Iterable[str] | None = None, limit: int = 12) -> str:
    """Render the mandatory trailing UNCOVERED section as a single string.

    The first entry of every dimension is shown so the section always carries
    the whole coverage boundary at a glance; the complete list lives in the
    JSON companion and its count is printed.  ``limit`` bounds console noise.
    """
    paths = list(paths) if paths is not None else uncovered_paths()
    header = f"UNCOVERED {len(paths)} path(s) — coverage boundary (machine-checkable):"
    representatives: list[str] = []
    for path in paths:
        dimension = path.split(" — ", 1)[-1].split("未覆盖", 1)[0]
        if dimension not in representatives:
            representatives.append(dimension)
    ordered: list[str] = []
    for dimension in representatives:
        ordered.extend(path for path in paths if f" — {dimension}未覆盖:" in path and path not in ordered)
    for path in paths:
        if len(ordered) >= limit:
            break
        if path not in ordered:
            ordered.append(path)
    shown = ordered[:limit]
    lines = [header] + [f"  UNCOVERED {item}" for item in shown]
    if len(paths) > len(shown):
        lines.append(
            f"  UNCOVERED … and {len(paths) - len(shown)} more path(s); full machine-readable "
            "list: docs/contract-baseline/contract-coverage.json"
        )
    return "\n".join(lines)


# ── Selftest ───────────────────────────────────────────────────────────────


def selftest() -> int:
    """Self-contained checks for the new layer code.  Returns 0 on success."""
    checks: list[tuple[str, bool]] = []

    checks.append(("parse_layers(None) == legacy", parse_layers(None) == ["L1", "L4"]))
    checks.append(("parse_layers(all) == L1..L5", parse_layers("all") == list(LAYERS)))
    checks.append(("parse_layers range", parse_layers("L2-L4") == ["L2", "L3", "L4"]))
    checks.append(("parse_layers list", parse_layers("L5,L1") == ["L1", "L5"]))
    try:
        parse_layers("L9")
        checks.append(("parse_layers rejects bad token", False))
    except ValueError:
        checks.append(("parse_layers rejects bad token", True))

    # L4 byte comparison: whitespace-only JSON difference MUST be a problem.
    py_state = {"audit_events": [{"id": "1", "metadata_json": '{"note": "x"}'}]}
    rs_state = {"audit_events": [{"id": "1", "metadata_json": '{"note":"x"}'}]}
    byte_problems = diff_db_state_bytes(py_state, rs_state)
    checks.append((
        "L4 JSON byte comparison catches spacing",
        any("metadata_json" in problem for problem in byte_problems),
    ))
    checks.append((
        "L4 identical bytes produce no problem",
        not diff_db_state_bytes(py_state, py_state),
    ))
    parsed_problems = diff_db_state_bytes(
        {"audit_events": [{"metadata_json": {"note": "x"}}]},
        {"audit_events": [{"metadata_json": {"note": "x"}}]},
        {"audit_events": ["metadata_json"]},
    )
    checks.append((
        "L4 reports parsed JSON explicitly",
        any("--keep-json-raw" in problem for problem in parsed_problems),
    ))
    checks.append(("L4 byte_repr keeps type tag", byte_repr(1) != byte_repr("1")))

    # L2 schema.
    lines, problems = check_l2_schema(
        {"a": {"x": 1}}, {"a": {"x": 2}},
        {"t": [{"c": 1}]}, {"t": [{"c": 2}]},
    )
    checks.append(("L2 shape parity", not problems and len(lines) == 2))
    _, problems = check_l2_schema({"a": {"x": 1}}, {"a": {"y": 2}})
    checks.append(("L2 catches shape difference", bool(problems)))

    # L3 validation.
    cases_a = {"bad": {"status": 422, "body": {}}}
    cases_b = {"bad": {"status": 422, "body": {}}}
    envelope = {"error": {"code": "validation_error", "message": "m", "details_keys": []}}
    _, problems = check_l3_validation(cases_a, cases_b, {"bad": envelope}, {"bad": envelope})
    checks.append(("L3 envelope parity", not problems))
    _, problems = check_l3_validation(
        cases_a, {"bad": {"status": 400, "body": {}}}, {"bad": envelope}, {"bad": envelope}
    )
    checks.append(("L3 catches status divergence", bool(problems)))

    # L5 behaviour.
    l5_cases = {
        "vote_create": {"status": 201}, "vote_create_again": {"status": 409},
        "vote_missing": {"status": 404},
    }
    _, problems = check_l5_behavior(l5_cases, l5_cases, {"t": [{"id": 1}]}, {"t": [{"id": 1}]})
    checks.append(("L5 idempotency probe parity", not problems))
    checks.append(("L5 detects probes", "vote_create_again" in idempotency_cases(l5_cases)))
    _, problems = check_l5_behavior(
        l5_cases, l5_cases, {"t": [{"id": 1}]}, {"t": [{"id": 1}, {"id": 2}]}
    )
    checks.append(("L5 catches duplicate side effect", bool(problems)))

    # Coverage boundary is always a non-empty string.
    rendered = render_uncovered(["example uncovered path"])
    checks.append(("UNCOVERED section is non-empty", bool(rendered) and "UNCOVERED" in rendered))
    checks.append(("UNCOVERED fallback non-empty", bool(uncovered_paths("/nonexistent/coverage.json"))))

    failed = [name for name, ok in checks if not ok]
    for name, ok in checks:
        print(f"{'PASS' if ok else 'FAIL'} selftest: {name}")
    print(f"\n{len(checks) - len(failed)}/{len(checks)} selftests passed")
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(selftest())
