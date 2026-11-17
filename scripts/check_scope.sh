#!/usr/bin/env bash
# One command for the two scope gates: the endpoint table (68 in-scope routes)
# and the baseline schema object set, each against the frozen baseline mirror
# plus the two self-tests. See docs/rust-migration-scope-20261118.md.
#
# The implementation lives in `scripts/check_scope.py`; this file stays as a
# thin wrapper so every existing invocation keeps working.
#
# This is an audit tool, deliberately NOT part of scripts/check.sh: it needs
# the Python baseline venv, while check.sh must stay runnable on a machine
# that has no Python environment at all.
#
# Usage:
#   scripts/check_scope.sh
#
# Exit code 0 means: endpoint map in scope, schema objects in scope, and both
# self-tests pass. The main checkout (which carries the browser subsystem) is
# only touched read-only; set MAIN_BACKEND to point somewhere else.

set -euo pipefail

cd "$(dirname "$0")/.."

unset PYTHONPATH PYTHONHOME || true

exec python3 scripts/check_scope.py "$@"
