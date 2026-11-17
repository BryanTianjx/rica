#!/usr/bin/env bash
# One command for the two scope gates: the endpoint table (68 in-scope routes)
# and the baseline schema object set, each against the frozen baseline mirror
# plus the two self-tests. See docs/rust-migration-scope-20261118.md.
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
export PYTHONDONTWRITEBYTECODE=1

PY_BASELINE="${PY_BASELINE:-/mnt/o/ica/backend}"
PY_BASELINE_VENV="${PY_VENV_PYTHON:-$PY_BASELINE/.venv/bin/python}"
MAIN_BACKEND="${MAIN_BACKEND:-/home/bryan/ica/backend}"
MAIN_VENV_PYTHON="${MAIN_VENV_PYTHON:-/home/bryan/ica/backend/.venv/bin/python}"

run_baseline() {
  env -u PYTHONPATH -u PYTHONHOME "$PY_BASELINE_VENV" "$@"
}

echo "== 1/4 endpoint map: frozen baseline (expect live 68 / excluded 0 / documented 68) =="
run_baseline scripts/check_endpoint_map.py

echo
echo "== 2/4 endpoint map: main checkout incl. the browser subsystem (read-only probe) =="
# Informational, not a pass/fail gate: the main worktree moves with the
# uncommitted BROWSER-04 work, so its numbers are only stable per commit
# (d96bb05 = live 102 / excluded 34). The gate itself is step 1 + 3 + 4.
# A MISMATCH here that names a non-/api/browser/ route is the expected signal
# while BROWSER-04 (uncommitted) adds POST /api/imports/{session_id}/
# confirm-candidate outside the exclusion prefix — see
# docs/rust-migration-scope-20261118.md §5 before "fixing" it.
if [ -x "$MAIN_VENV_PYTHON" ]; then
  if env -u PYTHONPATH -u PYTHONHOME "$MAIN_VENV_PYTHON" \
      scripts/check_endpoint_map.py --backend "$MAIN_BACKEND"; then
    echo "   main checkout: in-scope comparison matched"
  else
    echo "   main checkout: MISMATCH recorded above (see the note; step 1 is the gate)"
  fi
else
  echo "   main checkout not present ($MAIN_BACKEND); skipped (self-test covers the logic)"
fi

echo
echo "== 3/4 baseline schema objects (expect TABLES 20/20 · INDEXES 66/66 → IDENTICAL) =="
run_baseline scripts/compare_baseline_schema.py "$PY_BASELINE/ica.db" \
  crates/ica-infra/schema/ica-baseline.sql

echo
echo "== 4/4 self-tests of both gates =="
run_baseline scripts/check_endpoint_map.py --self-test
run_baseline scripts/compare_baseline_schema.py --self-test

echo
echo "All scope checks passed."
