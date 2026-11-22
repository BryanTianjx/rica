#!/usr/bin/env python3
"""FollowUps state-machine parity (F0-5): Rust domain ↔ frontend mirror.

The frontend FollowUpsPage keeps hand-written mirror tables of the backend
state machine. If one side drifts and the other does not, the UI offers an
action the server rejects (or hides one the server accepts). This script
parses BOTH source files (no compilation, no generated artifacts) and asserts
the mirror dimensions agree.

Dimensions compared:
  * statuses          — the 12 state names
  * terminal          — closed states (Rust is_closed ↔ TERMINAL_STATUSES)
  * result_completable— Rust is_result_completable ↔ RESULT_COMPLETABLE_STATUSES
  * cancellable       — every non-terminal state ↔ CANCELLABLE_STATUSES

Usage:
  python3 scripts/follow_up_state_parity.py            # compare, exit 0/1
  python3 scripts/follow_up_state_parity.py --json     # machine-readable
  python3 scripts/follow_up_state_parity.py --selftest # mutation guard
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
RUST = ROOT / "crates/ica-domain/src/follow_up.rs"
FRONT = ROOT / "frontend/src/pages/FollowUpsPage.tsx"

ALL_STATES = [
    "Pending", "Eligible", "Drafting", "Reviewing", "Rejected", "Approved",
    "Copied", "AwaitingResult", "Completed", "Cancelled", "Expired", "Blocked",
]


def _rust_matches_set(text: str, fn_name: str) -> set[str]:
    m = re.search(rf"pub fn {fn_name}\(self\) -> bool \{{(.*?)\n    \}}", text, re.S)
    if not m:
        raise SystemExit(f"Rust: {fn_name} not found")
    return set(re.findall(r"Self::(\w+)", m.group(1)))


def parse_rust(text: str) -> dict[str, set[str]]:
    # The state universe is the enum variants.
    enum = re.search(r"pub enum FollowUpTaskStatus \{(.*?)\n\}", text, re.S)
    if not enum:
        raise SystemExit("Rust: FollowUpTaskStatus enum not found")
    states = set(re.findall(r"^\s{4}(\w+),", enum.group(1), re.M))
    return {
        "statuses": states,
        "terminal": _rust_matches_set(text, "is_closed"),
        "result_completable": _rust_matches_set(text, "is_result_completable"),
        # Cancellation is open to every non-terminal state.
        "cancellable": states - _rust_matches_set(text, "is_closed"),
    }


def _ts_array(text: str, name: str) -> set[str]:
    m = re.search(rf"const {name}:[^=]*=\s*\[(.*?)\];", text, re.S)
    if not m:
        raise SystemExit(f"frontend: {name} not found")
    return set(re.findall(r"'(\w+)'", m.group(1)))


def parse_frontend(text: str) -> dict[str, set[str]]:
    statuses = _ts_array(text, "TASK_STATUSES")
    terminal = _ts_array(text, "TERMINAL_STATUSES")
    return {
        "statuses": statuses,
        "terminal": terminal,
        "result_completable": _ts_array(text, "RESULT_COMPLETABLE_STATUSES"),
        "cancellable": _ts_array(text, "CANCELLABLE_STATUSES"),
    }


def compare(rust: dict[str, set[str]], front: dict[str, set[str]]) -> dict[str, dict]:
    out = {}
    for dim in ("statuses", "terminal", "result_completable", "cancellable"):
        r, f = rust[dim], front[dim]
        out[dim] = {
            "ok": r == f,
            "rust_only": sorted(r - f),
            "front_only": sorted(f - r),
        }
    return out


def selftest() -> int:
    """Prove the comparison catches a one-line drift in the frontend mirror."""
    base_front = parse_frontend(FRONT.read_text(encoding="utf-8"))
    rust = parse_rust(RUST.read_text(encoding="utf-8"))
    assert all(d["ok"] for d in compare(rust, base_front).values()), "baseline must be equal"
    mutated = {k: set(v) for k, v in base_front.items()}
    mutated["terminal"] = mutated["terminal"] - {"Expired"}  # simulate drift
    result = compare(rust, mutated)
    assert not result["terminal"]["ok"], "selftest: mutation must be detected"
    assert result["terminal"]["front_only"] == [] and result["terminal"]["rust_only"] == ["Expired"]
    print("SELFTEST OK: a one-state terminal drift is detected (rust_only=['Expired'])")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args()
    if args.selftest:
        return selftest()
    result = compare(
        parse_rust(RUST.read_text(encoding="utf-8")),
        parse_frontend(FRONT.read_text(encoding="utf-8")),
    )
    ok = all(d["ok"] for d in result.values())
    if args.json:
        print(json.dumps({"ok": ok, "dimensions": result}, indent=2))
    else:
        for dim, d in result.items():
            status = "OK" if d["ok"] else "DRIFT"
            print(f"{dim:20} {status}")
            if not d["ok"]:
                print(f"  rust_only={d['rust_only']}  front_only={d['front_only']}")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
