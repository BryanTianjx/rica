#!/usr/bin/env bash
# One-command contract comparison: seed, run both services, capture, diff.
#
# The implementation lives in `scripts/contract_verify.py`; this file stays as
# a thin wrapper so every existing invocation keeps working
# (`./scripts/contract_verify.sh`, `bash scripts/contract_verify.sh`).
#
# The scenario in `contract_capture.py` writes, so each service gets its **own
# copy** of the seeded database. The Python baseline at /mnt/o/ica is never
# opened: the service runs from a throwaway copy in /tmp, and its database is a
# copy of the seed too.
#
# Usage:
#   scripts/contract_verify.sh
#
# Exit code 0 means `contract_diff.py` reported 0 failures. The captures are
# written to docs/contract-baseline/ so a reviewer can read the raw evidence.

set -euo pipefail

cd "$(dirname "$0")/.."

unset PYTHONPATH PYTHONHOME AI_API_KEY || true

exec python3 scripts/contract_verify.py "$@"
