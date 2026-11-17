#!/usr/bin/env bash
# One-command quality gate for the Rust backend.
#
# The implementation lives in `scripts/check.py`; this file stays as a thin
# wrapper so every existing invocation keeps working (`./scripts/check.sh`,
# `bash scripts/check.sh`, and the audit commands in the task orders).
#
# Usage:
#   scripts/check.sh
#
# Exit code 0 means every gate passed. The test count printed at the end is the
# number of tests that actually ran — quote that number, not an older one.

set -euo pipefail

cd "$(dirname "$0")/.."

# Hermes (and other hosts) may export PYTHONPATH/PYTHONHOME pointing at another
# interpreter's site-packages. Cargo does not care, but the helper scripts this
# gate calls do, so clear them once here.
unset PYTHONPATH PYTHONHOME || true

exec python3 scripts/check.py "$@"
