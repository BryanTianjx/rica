#!/usr/bin/env bash
# One-command contract comparison: seed, run both services, capture, diff.
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
REPO_ROOT="$(pwd)"

unset PYTHONPATH PYTHONHOME AI_API_KEY || true

WORK="${WORK:-/tmp/ica-contract}"
PY_BASELINE="${PY_BASELINE:-/mnt/o/ica/backend}"
PY_VENV_PYTHON="${PY_VENV_PYTHON:-/mnt/o/ica/backend/.venv/bin/python}"
PY_PORT="${PY_PORT:-8010}"
RUST_PORT="${RUST_PORT:-8011}"
OUT_DIR="${OUT_DIR:-docs/contract-baseline}"

PY_PID=""
RUST_PID=""
cleanup() {
  for pid in "$PY_PID" "$RUST_PID"; do
    if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
      kill "$pid" 2>/dev/null || true
      wait "$pid" 2>/dev/null || true
    fi
  done
}
trap cleanup EXIT

wait_for_health() {
  local url="$1" label="$2" attempts=60
  for _ in $(seq 1 "$attempts"); do
    if curl -fsS "$url/health" >/dev/null 2>&1; then
      echo "   $label is up ($url)"
      return 0
    fi
    sleep 0.5
  done
  echo "   $label did not answer on $url/health" >&2
  return 1
}

echo "== 1/5 seed two independent copies of the shared database =="
rm -rf "$WORK" /tmp/ica-py-baseline
mkdir -p "$WORK" "$OUT_DIR"
python3 scripts/seed_contract_db.py --out "$WORK/seed.db"
cp "$WORK/seed.db" "$WORK/python.db"
cp "$WORK/seed.db" "$WORK/rust.db"

echo
echo "== 2/5 start the Python baseline on a copy (writes allowed there) =="
# A port already answering /health means a server from an earlier run leaked and
# would silently serve the wrong database; refuse instead of guessing.
if curl -fsS "http://127.0.0.1:$PY_PORT/health" >/dev/null 2>&1; then
  echo "   port $PY_PORT is already serving something; stop it first" >&2
  exit 1
fi
# A copy without .env/.venv/ica.db: the service must not read the real baseline
# tree, and it must not touch the baseline database.
rsync -a --exclude '.venv' --exclude '.env' --exclude '__pycache__' \
  --exclude 'ica.db*' "$PY_BASELINE/" /tmp/ica-py-baseline/
(
  cd /tmp/ica-py-baseline
  # `exec` so the recorded PID is uvicorn itself: killing a subshell would leave
  # the server holding the (now deleted) database file.
  exec env -u PYTHONPATH -u PYTHONHOME PYTHONDONTWRITEBYTECODE=1 \
    DATABASE_URL="sqlite:///$WORK/python.db" \
    "$PY_VENV_PYTHON" -m uvicorn app.main:app --host 127.0.0.1 --port "$PY_PORT" \
    >"$WORK/python.log" 2>&1
) &
PY_PID=$!
wait_for_health "http://127.0.0.1:$PY_PORT" "python"

echo
echo "== 3/5 build and start the Rust port on its own copy =="
cargo build -p ica-server --quiet
# The workspace pins `target-dir` to ext4, so ask cargo where the binary is
# instead of assuming `./target`.
TARGET_DIR="$(cargo metadata --format-version 1 --no-deps \
  | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')"
DATABASE_URL="sqlite:///$WORK/rust.db" PORT="$RUST_PORT" HOST=127.0.0.1 \
  "$TARGET_DIR/debug/ica-server" >"$WORK/rust.log" 2>&1 &
RUST_PID=$!
if ! kill -0 "$RUST_PID" 2>/dev/null; then
  echo "   rust server failed to start; see $WORK/rust.log" >&2
  exit 1
fi
wait_for_health "http://127.0.0.1:$RUST_PORT" "rust"

echo
echo "== 4/5 capture both sides =="
python3 scripts/contract_capture.py --base-url "http://127.0.0.1:$PY_PORT" \
  --db "$WORK/python.db" --out "$OUT_DIR/w1-python.json"
python3 scripts/contract_capture.py --base-url "http://127.0.0.1:$RUST_PORT" \
  --db "$WORK/rust.db" --out "$OUT_DIR/w1-rust.json"

echo
echo "== 5/5 diff =="
python3 scripts/contract_diff.py \
  --python "$OUT_DIR/w1-python.json" --rust "$OUT_DIR/w1-rust.json"

echo
echo "== baseline untouched? =="
echo -n "git status: "
if [ -z "$(git -C /mnt/o/ica status --porcelain -uall)" ]; then echo "clean"; else echo "DIRTY"; exit 1; fi
echo -n "HEAD: "; git -C /mnt/o/ica rev-parse --short HEAD
echo -n "ica.db sha256: "; sha256sum /mnt/o/ica/backend/ica.db | cut -d' ' -f1
