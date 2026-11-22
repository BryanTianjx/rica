#!/usr/bin/env python3
"""Check that the contract document's endpoint table matches the real API.

The migration contract claims a total of 68 routes. That claim is only useful if
it can be re-derived, so this script:

1. imports the *Python baseline* app and reads the router objects uvicorn would
   actually serve (not a regex over the source files),
2. parses the markdown table in ``docs/rust-migration-contract.md``,
3. reports any route present in one and missing from the other.

**Scope.** The migration covers the frozen baseline's 68 business routes only.
Routes ICA added after the freeze — the browser subsystem under
``/api/browser/`` — are *out of scope*, not forgotten. ``--exclude-prefix``
(default ``/api/browser/``) removes them from the comparison and prints how
many were removed, so a drifted baseline mirror cannot disguise out-of-scope
routes as missing work. ``--expect-excluded`` pins that count, so the prefix
list cannot silently widen and hide genuinely missing routes.

Run it with the baseline interpreter, from anywhere:

    cd /mnt/o/ICA-RUST
    PYTHONDONTWRITEBYTECODE=1 env -u PYTHONPATH \
      /mnt/o/ica/backend/.venv/bin/python scripts/check_endpoint_map.py

Exit code 0 means the table and the in-scope API agree. ``--self-test`` runs
the comparison logic against synthetic data and needs no checkout.
"""

from __future__ import annotations

import argparse
import os
import re
import sys
from pathlib import Path

DEFAULT_BACKEND = "/mnt/o/ica/backend"
DEFAULT_DOC = Path(__file__).resolve().parent.parent / "docs" / "rust-migration-contract.md"
#: The browser subsystem is a separate vertical slice, excluded from this
#: migration by ICA's own architecture documents (see
#: docs/rust-migration-scope-20261118.md). Repeatable on the command line.
DEFAULT_EXCLUDE_PREFIXES: tuple[str, ...] = ("/api/browser/",)
#: Post-freeze routes that ADR-0008 (signed 2026-11-22) rules OUT of the
#: 68-route contract.  Exact (method, path) pairs — NOT a prefix — so a new
#: route under these families that is not on this list still shows as missing.
#: 4 ai/models CRUD -> AI-EXT; confirm-candidate -> W7. 68 + 4 + 1 = 73 live.
NONCONTRACT_ROUTES: frozenset[tuple[str, str]] = frozenset({
    ("POST", "/api/ai/models"),
    ("PUT", "/api/ai/models/{model_id:path}"),
    ("PUT", "/api/ai/models/{model_id:path}/default"),
    ("DELETE", "/api/ai/models/{model_id:path}"),
    ("POST", "/api/imports/{session_id}/confirm-candidate"),
})
SKIP_PATHS = {"/openapi.json", "/docs", "/docs/oauth2-redirect", "/redoc"}

ROW = re.compile(
    r"^\|\s*(\d+)\s*\|\s*(GET|POST|PUT|PATCH|DELETE)\s*\|\s*`([^`]+)`",
    re.MULTILINE,
)


def api_routes(backend: Path) -> set[tuple[str, str]]:
    """(method, path) pairs under /api, from the live app object."""
    sys.path.insert(0, str(backend))
    os.chdir(backend)
    from app.main import app  # noqa: PLC0415 - import needs the path set up

    # Prove which tree was imported: a probe that resolves to a different
    # checkout reports on code nobody changed.
    module_file = sys.modules["app.main"].__file__ or "<unknown>"
    print(f"imported app.main from: {module_file}")
    expected = str(backend.resolve())
    if not str(Path(module_file).resolve()).startswith(expected):
        print(f"REFUSING: expected the app under {expected}")
        raise SystemExit(2)
    routes: set[tuple[str, str]] = set()
    for route in app.routes:
        methods = getattr(route, "methods", None)
        path = getattr(route, "path", None)
        if not methods or path is None or path in SKIP_PATHS:
            continue
        if not path.startswith("/api"):
            continue
        for method in methods:
            routes.add((method, path))
    return routes


def documented_routes(doc: Path) -> set[tuple[str, str]]:
    """(method, path) pairs listed in the document's endpoint table."""
    text = doc.read_text(encoding="utf-8")
    routes = set()
    for _number, method, path in ROW.findall(text):
        routes.add((method, f"/api{path}"))
    return routes


def split_scope(
    live: set[tuple[str, str]], prefixes: list[str]
) -> tuple[set[tuple[str, str]], set[tuple[str, str]]]:
    """Split live routes into (in scope, excluded).

    Exclusion is a strict ``startswith`` on the path — never a substring match,
    never a whitelist of implemented routes, so a route that is new but not
    implemented still shows up as missing.
    """
    excluded = {
        route for route in live if any(route[1].startswith(prefix) for prefix in prefixes)
    }
    return live - excluded, excluded


def compare(
    in_scope: set[tuple[str, str]], documented: set[tuple[str, str]]
) -> tuple[list[tuple[str, str]], list[tuple[str, str]]]:
    """(missing from the table, in the table but not in the API), sorted."""
    return sorted(in_scope - documented), sorted(documented - in_scope)


def self_test() -> int:
    """Three assertions on synthetic data, no checkout required:

    1. without exclusion, 34 out-of-scope routes read as *missing* work;
    2. with the default prefix, the same inputs match and exactly 34 are
       excluded;
    3. exclusion does not hide a genuinely new in-scope route — it still
       reports as missing.
    """
    methods = ("GET", "POST", "PUT", "PATCH", "DELETE")
    scope = {(methods[i % 5], f"/api/business/{i:02d}") for i in range(68)}
    browser = {(methods[i % 5], f"/api/browser/v1/route-{i:02d}") for i in range(34)}
    documented = set(scope)
    prefixes = list(DEFAULT_EXCLUDE_PREFIXES)
    failures: list[str] = []

    missing, _ = compare(scope | browser, documented)
    if len(missing) != 34:
        failures.append(f"no-exclusion run: expected 34 missing, got {len(missing)}")

    in_scope, excluded = split_scope(scope | browser, prefixes)
    missing, extra = compare(in_scope, documented)
    if missing or extra:
        failures.append(
            f"with exclusion: expected an exact match, got missing={missing} extra={extra}"
        )
    if len(excluded) != 34:
        failures.append(f"with exclusion: expected 34 excluded, got {len(excluded)}")

    sneaky = {("GET", "/api/never-planned")}
    in_scope, _ = split_scope(scope | browser | sneaky, prefixes)
    missing, _ = compare(in_scope, documented)
    if missing != [("GET", "/api/never-planned")]:
        failures.append(f"new in-scope route: expected it missing, got {missing}")

    # ADR-0008: 5 contract-external routes must be excluded by exact match,
    # and an UNLISTED new route under the same family must still be reported.
    extra_live = {
        ("POST", "/api/ai/models"),
        ("PUT", "/api/ai/models/{model_id:path}"),
        ("PUT", "/api/ai/models/{model_id:path}/default"),
        ("DELETE", "/api/ai/models/{model_id:path}"),
        ("POST", "/api/imports/{session_id}/confirm-candidate"),
    }
    in_scope, _ = split_scope(scope | browser | extra_live, prefixes)
    still_missing, _ = compare(in_scope - NONCONTRACT_ROUTES, documented)
    if still_missing:
        failures.append(f"ADR-0008 exclusion: expected exact match, got {still_missing}")
    unlisted = {("POST", "/api/ai/models/new-unlisted")}
    in_scope, _ = split_scope(scope | browser | extra_live | unlisted, prefixes)
    still_missing, _ = compare(in_scope - NONCONTRACT_ROUTES, documented)
    if still_missing != [("POST", "/api/ai/models/new-unlisted")]:
        failures.append(f"ADR-0008 unlisted route: expected it missing, got {still_missing}")

    if failures:
        for failure in failures:
            print(f"SELF-TEST FAIL: {failure}")
        return 1
    print("self-test: 5/5 assertions passed"
          " (no-exclusion mismatch, exclusion match, new-route mismatch,"
          " ADR-0008 exact exclusion, ADR-0008 unlisted-route mismatch)")
    return 0


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument("--doc", default=str(DEFAULT_DOC))
    parser.add_argument("--backend", default=DEFAULT_BACKEND)
    parser.add_argument(
        "--exclude-prefix",
        action="append",
        default=None,
        metavar="PREFIX",
        help="live routes under this path prefix are out of scope"
        " (repeatable; default /api/browser/)",
    )
    parser.add_argument(
        "--expect-excluded",
        type=int,
        default=None,
        metavar="N",
        help="fail unless exactly N live routes were excluded",
    )
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="exercise the comparison logic on synthetic data, no checkout needed",
    )
    args = parser.parse_args(argv)

    if args.self_test:
        return self_test()

    prefixes = (
        list(DEFAULT_EXCLUDE_PREFIXES) if args.exclude_prefix is None else args.exclude_prefix
    )
    live = api_routes(Path(args.backend))
    documented = documented_routes(Path(args.doc))
    in_scope, excluded = split_scope(live, prefixes)

    # ADR-0008 (signed 2026-11-22): 5 post-freeze routes are out of contract.
    # Remove by EXACT (method, path) so an unlisted new route still shows up.
    noncontract_present = in_scope & NONCONTRACT_ROUTES
    in_scope = in_scope - NONCONTRACT_ROUTES

    print(f"live /api routes:        {len(live)}")
    print(f"excluded (out of scope): {len(excluded)}   prefix: {' '.join(prefixes)}")
    print(f"excluded (ADR-0008):     {len(noncontract_present)}   contract-external exact routes")
    for route in sorted(noncontract_present):
        print(f"    out of contract: {route[0]:7s} {route[1]}")
    # Fail if the ADR-0008 list has drifted: every listed route must actually
    # exist live, otherwise a future removal would silently shrink the set.
    if noncontract_present != NONCONTRACT_ROUTES:
        print(
            f"\nRESULT: MISMATCH — ADR-0008 lists {sorted(NONCONTRACT_ROUTES)} "
            f"but live has {sorted(noncontract_present)}"
        )
        return 1
    for route in sorted(excluded):
        print(f"    out of scope: {route[0]:7s} {route[1]}")
    print(f"compared live routes:    {len(in_scope)}")
    print(f"documented rows:         {len(documented)}")

    if args.expect_excluded is not None and len(excluded) != args.expect_excluded:
        print(
            f"\nRESULT: MISMATCH — expected exactly {args.expect_excluded} excluded"
            f" routes, measured {len(excluded)}: the prefix list drifted"
        )
        return 1

    missing, extra = compare(in_scope, documented)
    if missing:
        print(f"\nMISSING from the table ({len(missing)}):")
        for method, path in missing:
            print(f"  {method:7s} {path}")
    if extra:
        print(f"\nIN THE TABLE but not in the API ({len(extra)}):")
        for method, path in extra:
            print(f"  {method:7s} {path}")

    if missing or extra:
        print("\nRESULT: MISMATCH")
        return 1

    print("\nRESULT: table matches the in-scope live route table exactly")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
