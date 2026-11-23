#!/usr/bin/env python3
"""One-command quality gate for the Rust backend.

Runs the same checks a reviewer would run by hand, in the order that fails
fastest and cheapest:

  1. dependency layering (domain/application must stay framework-free)
  2. formatting
  3. clippy with warnings denied
  4. the full test suite

Usage:
    scripts/check.py

Exit code 0 means every gate passed. The test count printed at the end is the
number of tests that actually ran — quote that number, not an older one.

This is the Python successor of ``scripts/check.sh``. The shell file is kept as
a one-line wrapper so every existing invocation (``./scripts/check.sh``,
``bash scripts/check.sh``) keeps working.

Why Python and not bash: the layering gate was a 25-line Python program quoted
inside a shell string, where it could not be linted, tested or reused. It is a
function here, and the gate's failure message is produced by the same code that
parses the metadata.
"""

from __future__ import annotations

import json
import os
import re
import shlex
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent

# A framework, a database driver or an async runtime in an inner layer would
# let the inner crates depend on the outside world directly.
FORBIDDEN_IN_INNER_LAYERS = {"axum", "sqlx", "tokio", "hyper"}
# Adapter crates: the inner layers must never depend on them, directly or via a
# new wrapper — `ica-application -> ica-infra` would drag sqlx in without this
# gate ever seeing the framework name (audit finding F-08). The reverse edge,
# ica-infra depending on ica-application, is the intended direction and is not
# checked here. `ica-desktop` joins them with the Tauri route (ADR-0002 §5).
ADAPTER_CRATES = {"ica-infra", "ica-api", "ica-server", "ica-desktop"}
# Crates whose dependency list must stay clean — now including the wire
# contract, so an IPC adapter can trust it to be transport-neutral
# (Tauri-audit R0.1 §4.3, option C).
GUARDED_CRATES = {"ica-domain", "ica-application", "ica-contracts"}

# `cargo test` prints one line per test target:
#   test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
TEST_RESULT = re.compile(r"test result:\s+\w+\.\s+(\d+)\s+passed;\s+(\d+)\s+failed")


def clean_env() -> dict[str, str]:
    """The environment with the host's Python paths removed, plus the
    build-artefact location pinned to a fast local disk.

    Hermes (and other hosts) may export PYTHONPATH/PYTHONHOME pointing at
    another interpreter's site-packages. Cargo does not care, but a polluted
    environment makes the numbers printed here unreproducible elsewhere.

    The target-dir fallback exists because the repo lives on a small Windows
    volume with 9p semantics (Tauri-audit finding G-5): a debug target dir
    would fill it, and the committed `.cargo/config.toml` therefore no longer
    pins an absolute path that only exists on one machine. An explicit
    `CARGO_TARGET_DIR` always wins.
    """
    env = dict(os.environ)
    env.pop("PYTHONPATH", None)
    env.pop("PYTHONHOME", None)
    env.setdefault(
        "CARGO_TARGET_DIR", str(Path.home() / ".cache" / "ica-rust-target")
    )
    return env


def cargo_metadata() -> dict:
    """`cargo metadata` as a dict, or exit 1 with cargo's own message."""
    result = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"],
        cwd=REPO_ROOT,
        env=clean_env(),
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        sys.stderr.write(result.stderr)
        raise SystemExit(1)
    return json.loads(result.stdout)


def layering_violations() -> list[tuple[str, list[str]]]:
    """Crates in an inner layer that depend on something they must not."""
    failures: list[tuple[str, list[str]]] = []
    for package in cargo_metadata()["packages"]:
        if package["name"] not in GUARDED_CRATES:
            continue
        names = {dependency["name"] for dependency in package["dependencies"]}
        bad = sorted((names & FORBIDDEN_IN_INNER_LAYERS) | (names & ADAPTER_CRATES))
        if bad:
            failures.append((package["name"], bad))
    return failures


def run(command: list[str], env: dict[str, str]) -> int:
    """Run a gate command with its output on the terminal."""
    return subprocess.run(command, cwd=REPO_ROOT, env=env).returncode


def run_capture(command: list[str], env: dict[str, str]) -> tuple[int, str]:
    """Run a command, echoing its output live and returning it as well."""
    process = subprocess.Popen(
        command,
        cwd=REPO_ROOT,
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        bufsize=1,
    )
    assert process.stdout is not None
    collected: list[str] = []
    for line in process.stdout:
        sys.stdout.write(line)
        collected.append(line)
    process.wait()
    return process.returncode, "".join(collected)


def main() -> int:
    env = clean_env()

    print(
        "== 1/4 layering: ica-domain and ica-application must not depend on "
        "a web framework, a DB driver or a runtime =="
    )
    violations = layering_violations()
    for name, bad in violations:
        print(f"  LAYERING VIOLATION: {name} depends on {bad}")
    if violations:
        print("   FAILED: see the layering violation above")
        return 1
    print(
        "   ok: no framework, database driver, async runtime or adapter crate "
        "in the inner layers"
    )

    print()
    print("== 2/4 cargo fmt --check ==")
    if run(["cargo", "fmt", "--all", "--", "--check"], env) != 0:
        return 1

    print()
    print("== 3/4 cargo clippy --locked --all-targets -- -D warnings ==")
    if run(["cargo", "clippy", "--locked", "--all-targets", "--workspace", "--", "-D", "warnings"], env) != 0:
        return 1

    print()
    print("== 4/4 cargo test ==")
    # An environment hook, not a new CLI surface: the unified ARCH-P6 gate
    # (scripts/gate.py) needs to exclude the three DEF-006 browser_poc tests,
    # which hang on hosts where Chromium is discovered but cannot launch.
    # Empty by default, so every existing invocation is byte-for-byte the same
    # command as before. Kept here so check.py stays the single source of
    # truth for "how the Rust test gate is run".
    extra = shlex.split(os.environ.get("CARGO_TEST_EXTRA_ARGS", ""))
    # `--` is mandatory: without it cargo parses `--skip` as its own flag and
    # exits with "unexpected argument '--skip' found" before any test runs.
    test_command = ["cargo", "test", "--locked", "--workspace", *(["--", *extra] if extra else [])]
    print(f"   command: {' '.join(test_command)}")
    code, output = run_capture(test_command, env)
    if code != 0:
        return code

    passed = failed = 0
    for match in TEST_RESULT.finditer(output):
        passed += int(match.group(1))
        failed += int(match.group(2))
    print()
    print(f"tests: {passed} passed / {failed} failed")

    print()
    print("== 5/5 endpoint reconciliation ==")
    # The endpoint counts quoted in the plans must come from ONE source, not
    # from separate greps. This gate fails when the router and the contract
    # table disagree, or when the generated matrix is stale.
    # Captured, NOT echoed: the JSON payload is machine data, and the gate
    # prints its own one-line summary.
    result = subprocess.run(
        [sys.executable, "scripts/endpoint_reconciliation.py", "--json"],
        cwd=REPO_ROOT,
        env=env,
        capture_output=True,
        text=True,
    )
    output = result.stdout
    if result.returncode != 0:
        print("   FAILED: the endpoint reconciliation script did not run")
        print((result.stderr or output)[-2000:])
        return result.returncode
    try:
        data = json.loads(output)
    except json.JSONDecodeError:
        print("   FAILED: the endpoint reconciliation script produced no JSON")
        print(output[-2000:])
        return 1
    counts = data["counts"]
    implemented = counts["implemented"]
    missing = counts["missing"]
    total = counts["contract_total"]
    extra = counts["extra_routes"]
    auxiliary = counts.get("auxiliary_routes", 0)
    print(
        f"   contract={total} implemented={implemented} missing={missing} "
        f"extra={extra} auxiliary={auxiliary}"
    )
    if implemented + missing != total:
        print(
            f"   FAILED: {implemented} + {missing} != {total} — the endpoint "
            "counts do not reconcile"
        )
        return 1
    if extra:
        print(
            f"   FAILED: {extra} route(s) exist in the router but not in the "
            "contract table"
        )
        return 1
    print(
        f"   ok: implemented + missing == contract total, no extra routes"
        + (f" ({auxiliary} auxiliary route(s) per ADR-0003 D-2)" if auxiliary else "")
    )

    print()
    print("All gates passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
