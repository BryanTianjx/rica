#!/usr/bin/env bash
# One-command quality gate for the Rust backend.
#
# Runs the same checks a reviewer would run by hand, in the order that fails
# fastest and cheapest:
#
#   1. dependency layering (domain/application must stay framework-free)
#   2. formatting
#   3. clippy with warnings denied
#   4. the full test suite
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

echo "== 1/4 layering: ica-domain and ica-application must not depend on a web framework, a DB driver or a runtime =="
if cargo metadata --no-deps --format-version 1 \
    | python3 -c '
import json, sys
forbidden = {"axum", "sqlx", "tokio", "hyper"}
# Adapter crates: the inner layers must never depend on them, directly or via a
# new wrapper — `ica-application -> ica-infra` would drag sqlx in without this
# gate ever seeing the framework name (audit finding F-08). The reverse edge,
# ica-infra depending on ica-application, is the intended direction and is not
# checked here.
outer_crates = {"ica-infra", "ica-api", "ica-server"}
# Crates whose dependency list must stay clean.
guarded = {"ica-domain", "ica-application"}
meta = json.load(sys.stdin)
failures = []
for package in meta["packages"]:
    if package["name"] not in guarded:
        continue
    names = {dependency["name"] for dependency in package["dependencies"]}
    bad = sorted((names & forbidden) | (names & outer_crates))
    if bad:
        failures.append((package["name"], bad))
for name, bad in failures:
    print(f"  LAYERING VIOLATION: {name} depends on {bad}")
sys.exit(1 if failures else 0)
'; then
  echo "   ok: no framework, database driver, async runtime or adapter crate in the inner layers"
else
  echo "   FAILED: see the layering violation above"
  exit 1
fi

echo
echo "== 2/4 cargo fmt --check =="
cargo fmt --all -- --check

echo
echo "== 3/4 cargo clippy --all-targets -- -D warnings =="
cargo clippy --all-targets --workspace -- -D warnings

echo
echo "== 4/4 cargo test =="
cargo test --workspace

echo
echo "All gates passed."
