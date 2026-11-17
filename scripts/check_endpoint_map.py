#!/usr/bin/env python3
"""Check that the contract document's endpoint table matches the real API.

The migration contract claims a total of 68 routes. That claim is only useful if
it can be re-derived, so this script:

1. imports the *Python baseline* app and reads the router objects uvicorn would
   actually serve (not a regex over the source files),
2. parses the markdown table in ``docs/rust-migration-contract.md``,
3. reports any route present in one and missing from the other.

Run it with the baseline interpreter, from anywhere:

    cd /mnt/o/ICA-RUST
    PYTHONDONTWRITEBYTECODE=1 env -u PYTHONPATH \
      /mnt/o/ica/backend/.venv/bin/python scripts/check_endpoint_map.py

Exit code 0 means the table and the API agree.
"""

from __future__ import annotations

import argparse
import os
import re
import sys
from pathlib import Path

DEFAULT_BACKEND = "/mnt/o/ica/backend"
DEFAULT_DOC = Path(__file__).resolve().parent.parent / "docs" / "rust-migration-contract.md"
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


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--doc", default=str(DEFAULT_DOC))
    parser.add_argument("--backend", default=DEFAULT_BACKEND)
    args = parser.parse_args(argv)

    live = api_routes(Path(args.backend))
    documented = documented_routes(Path(args.doc))

    print(f"live /api routes:       {len(live)}")
    print(f"documented rows:        {len(documented)}")

    missing = sorted(live - documented)
    extra = sorted(documented - live)

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

    print("\nRESULT: table matches the live route table exactly")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
