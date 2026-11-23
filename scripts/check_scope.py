#!/usr/bin/env python3
"""One command for the two scope gates: the endpoint table (68 in-scope routes)
and the baseline schema object set, each against the frozen baseline mirror
plus the two self-tests. See docs/rust-migration-scope-20261118.md.

This is an audit tool, deliberately NOT part of scripts/check.py: it needs the
Python baseline venv, while check.py must stay runnable on a machine that has
no Python environment at all.

Usage:
    scripts/check_scope.py --help
    scripts/check_scope.py --baseline /path/to/frozen/backend \
        --main-backend /path/to/main/backend

Exit code 0 means: endpoint map in scope, schema objects in scope, and both
self-tests pass. The main checkout (which carries the browser subsystem) is
only touched read-only; set MAIN_BACKEND to point somewhere else.

`scripts/check_scope.sh` is a thin wrapper around this file.
"""

from __future__ import annotations

import argparse
import os
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
SCRIPTS = REPO_ROOT / "scripts"

PY_BASELINE = Path(os.environ.get("PY_BASELINE", "/mnt/o/ica/backend"))
PY_BASELINE_VENV = os.environ.get(
    "PY_VENV_PYTHON", str(PY_BASELINE / ".venv" / "bin" / "python")
)
# Legacy local default kept for compatibility only: MAIN_BACKEND / --main-backend
# is the documented primary source (SPE-BAS-004 §5.6/§9.1).
MAIN_BACKEND = Path(os.environ.get("MAIN_BACKEND", "/home/bryan/ica/backend"))
COMMAND_TIMEOUT = int(os.environ.get("ICA_COMMAND_TIMEOUT", "900"))
MAIN_VENV_PYTHON = os.environ.get(
    "MAIN_VENV_PYTHON", str(MAIN_BACKEND / ".venv" / "bin" / "python")
)


def clean_env() -> dict[str, str]:
    """The environment without the host's Python paths."""
    env = dict(os.environ)
    for name in ("PYTHONPATH", "PYTHONHOME"):
        env.pop(name, None)
    env["PYTHONDONTWRITEBYTECODE"] = "1"
    return env


def run_baseline(*arguments: str) -> int:
    """Run a script with the frozen baseline's interpreter (timeout -> 9)."""
    try:
        return subprocess.run(
            [PY_BASELINE_VENV, *arguments],
            cwd=REPO_ROOT,
            env=clean_env(),
            timeout=COMMAND_TIMEOUT,
        ).returncode
    except subprocess.TimeoutExpired:
        sys.stderr.write(f"baseline command timed out after {COMMAND_TIMEOUT}s\n")
        return 9


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--baseline", type=Path, default=PY_BASELINE,
        help="frozen Python baseline tree (env PY_BASELINE; default /mnt/o/ica/backend)",
    )
    parser.add_argument(
        "--main-backend", type=Path, default=MAIN_BACKEND,
        help="main checkout for the informational probe (env MAIN_BACKEND; "
             "legacy default /home/bryan/ica/backend)",
    )
    parser.add_argument(
        "--timeout", type=int, default=COMMAND_TIMEOUT,
        help="per-command timeout in seconds (default 900; 9 on expiry)",
    )
    return parser.parse_args()


def main() -> int:
    global PY_BASELINE, PY_BASELINE_VENV, MAIN_BACKEND, MAIN_VENV_PYTHON
    global COMMAND_TIMEOUT
    args = parse_args()
    if args.timeout <= 0:
        print("--timeout must be positive")
        return 2
    COMMAND_TIMEOUT = args.timeout
    PY_BASELINE = args.baseline.expanduser()
    PY_BASELINE_VENV = os.environ.get(
        "PY_VENV_PYTHON", str(PY_BASELINE / ".venv" / "bin" / "python")
    )
    MAIN_BACKEND = args.main_backend.expanduser()
    MAIN_VENV_PYTHON = os.environ.get(
        "MAIN_VENV_PYTHON", str(MAIN_BACKEND / ".venv" / "bin" / "python")
    )
    print(
        "== 1/4 endpoint map: frozen baseline "
        "(expect live 68 / excluded 0 / documented 68) =="
    )
    if run_baseline(str(SCRIPTS / "check_endpoint_map.py")) != 0:
        return 1

    print()
    print("== 2/4 endpoint map: main checkout incl. the browser subsystem (read-only probe) ==")
    # Informational, not a pass/fail gate: the main worktree moves with the
    # uncommitted BROWSER-04 work, so its numbers are only stable per commit
    # (d96bb05 = live 102 / excluded 34). The gate itself is step 1 + 3 + 4.
    # A MISMATCH here that names a non-/api/browser/ route is the expected signal
    # while BROWSER-04 (uncommitted) adds POST /api/imports/{session_id}/
    # confirm-candidate outside the exclusion prefix — see
    # docs/rust-migration-scope-20261118.md §5 before "fixing" it.
    if os.access(MAIN_VENV_PYTHON, os.X_OK):
        try:
            probe = subprocess.run(
                [
                    MAIN_VENV_PYTHON,
                    str(SCRIPTS / "check_endpoint_map.py"),
                    "--backend",
                    str(MAIN_BACKEND),
                ],
                cwd=REPO_ROOT,
                env=clean_env(),
                timeout=COMMAND_TIMEOUT,
            )
        except subprocess.TimeoutExpired:
            print(f"   main checkout probe timed out ({COMMAND_TIMEOUT}s)")
            return 9
        if probe.returncode == 0:
            print("   main checkout: in-scope comparison matched")
        else:
            print(
                "   main checkout: MISMATCH recorded above "
                "(see the note; step 1 is the gate)"
            )
    else:
        print(
            f"   main checkout not present ({MAIN_BACKEND}); "
            "skipped (self-test covers the logic)"
        )

    print()
    print(
        "== 3/4 baseline schema objects "
        "(expect TABLES 20/20 · INDEXES 66/66 → IDENTICAL) =="
    )
    if (
        run_baseline(
            str(SCRIPTS / "compare_baseline_schema.py"),
            str(PY_BASELINE / "ica.db"),
            str(REPO_ROOT / "crates" / "ica-infra" / "schema" / "ica-baseline.sql"),
        )
        != 0
    ):
        return 1

    print()
    print("== 4/4 self-tests of both gates ==")
    if run_baseline(str(SCRIPTS / "check_endpoint_map.py"), "--self-test") != 0:
        return 1
    if run_baseline(str(SCRIPTS / "compare_baseline_schema.py"), "--self-test") != 0:
        return 1

    print()
    print("All scope checks passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
