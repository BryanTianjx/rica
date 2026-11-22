#!/usr/bin/env python3
"""ARCH-P6 unified full-stack quality gate — one local and CI entry point.

This is an ORCHESTRATOR, not a new check implementation. Every stage delegates
to an asset that already exists in the repository:

  * scripts/check.py                    layering + fmt + clippy + cargo test + endpoint reconciliation
  * scripts/check_endpoint_map.py       contract table vs. live Python baseline routes
  * scripts/contract_verify.py          seed -> run both services -> capture -> diff (opt-in)
  * crates/ica-contracts/tests/sidecar_contract_test.rs   sidecar wire contract test
  * frontend/package.json               lint / build (tsc+vite) / test (vitest)
  * desktop/tauri-host                  Tauri host build (environment-gated)

Stage outcomes are PASS / FAIL / SKIP / BLOCKED. A SKIP or BLOCKED stage is an
honest environment/limitation marker; it is NEVER promoted to PASS. The gate
NEVER self-signs "Verified": independent security verification and Windows
release acceptance remain manual and are reported as PENDING.

Usage:
    python3 scripts/gate.py
    python3 scripts/gate.py --ci --out /tmp/gate-run.json
    python3 scripts/gate.py --only rust_gate,frontend_test
    ICA_GATE_RUN_CONTRACT=1 python3 scripts/gate.py     # opt into full contract comparison

Exit code 0 = no stage FAILED (PASS or PASS_WITH_LIMITATIONS).
Exit code 1 = at least one stage FAILED.
"""

from __future__ import annotations

import argparse
import glob
import json
import os
import shutil
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
FRONTEND = REPO / "frontend"
TAURI_HOST = REPO / "desktop" / "tauri-host"

# DEF-006: three browser_poc tests hang on hosts where Chromium is discovered
# but cannot launch. They are excluded by FUNCTION NAME (--skip matches test
# names, not test-target names). They are recorded as Skipped, never Passed.
DEF006_SKIPS = (
    "the_full_poc_happy_path_with_state_records",
    "the_worker_env_carries_no_database",
    "a_worker_crash_is_contained_and_the_runtime_restarts",
)
CARGO_TEST_EXTRA_ARGS = " ".join(f"--skip {name}" for name in DEF006_SKIPS)

DEFAULT_TIMEOUT = int(os.environ.get("ICA_GATE_TIMEOUT", "900"))
PY_BASELINE = Path(os.environ.get("PY_BASELINE", "/mnt/o/ica/backend"))


def now() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="seconds")


def git_head() -> str:
    result = subprocess.run(
        ["git", "rev-parse", "HEAD"], cwd=REPO, capture_output=True, text=True
    )
    return result.stdout.strip() if result.returncode == 0 else "<unknown>"


def base_env() -> dict[str, str]:
    env = dict(os.environ)
    env.pop("PYTHONPATH", None)
    env.pop("PYTHONHOME", None)
    # /mnt/o is a 9p volume; all cargo stages must land on ext4 unless the
    # caller explicitly pinned another target dir.
    env.setdefault("CARGO_TARGET_DIR", str(Path.home() / ".cache" / "ica-rust-target"))
    return env


def executable(name: str) -> bool:
    return shutil.which(name) is not None


def has_system_lib(pkg: str) -> bool:
    if not executable("pkg-config"):
        return False
    return subprocess.run(["pkg-config", "--exists", pkg]).returncode == 0


def run_stage(
    name: str,
    argv: list[str],
    *,
    cwd: Path = REPO,
    env: dict[str, str] | None = None,
    timeout: int = DEFAULT_TIMEOUT,
    detail: str = "",
) -> dict:
    """Run one stage, echo output, and record a machine-readable result."""
    print(f"\n{'=' * 72}\n== STAGE {name} ==\n$ {' '.join(argv)}", flush=True)
    started = now()
    t0 = time.monotonic()
    try:
        result = subprocess.run(
            argv,
            cwd=cwd,
            env=env or base_env(),
            capture_output=True,
            text=True,
            timeout=timeout,
        )
        code = result.returncode
        output = (result.stdout or "") + (result.stderr or "")
    except subprocess.TimeoutExpired as exc:
        code = 124
        out = exc.stdout or ""
        err = exc.stderr or ""
        output = (
            (out.decode(errors="replace") if isinstance(out, bytes) else out)
            + (err.decode(errors="replace") if isinstance(err, bytes) else err)
            + f"\n[TIMEOUT after {timeout}s]"
        )
    except OSError as exc:  # command missing
        code = 127
        output = f"OSError: {exc}"

    seconds = round(time.monotonic() - t0, 2)
    lines = output.splitlines()
    tail = "\n".join(lines[-60:])
    print(tail, flush=True)
    status = "PASS" if code == 0 else "FAIL"
    print(f"--- {name}: {status} (exit={code}, {seconds}s) ---", flush=True)
    return {
        "name": name,
        "status": status,
        "command": argv,
        "cwd": str(cwd),
        "exit_code": code,
        "seconds": seconds,
        "started_at": started,
        "detail": detail,
        "output_tail": tail,
    }


def skip_stage(name: str, reason: str, *, blocked: bool = False) -> dict:
    status = "BLOCKED" if blocked else "SKIP"
    print(f"\n{'=' * 72}\n== STAGE {name} ==\n{status}: {reason}", flush=True)
    return {
        "name": name,
        "status": status,
        "command": [],
        "exit_code": None,
        "seconds": 0,
        "detail": reason,
        "output_tail": "",
    }


def build_stages(args: argparse.Namespace, env: dict[str, str]) -> list[dict]:
    stages: list[dict] = []
    only = set(args.only.split(",")) if args.only else None
    skip = set(args.skip_stage.split(",")) if args.skip_stage else set()

    def wanted(name: str) -> bool:
        return (only is None or name in only) and name not in skip

    # ---- Rust: delegate wholesale to scripts/check.py -------------------
    if wanted("rust_gate"):
        rust_env = dict(env)
        rust_env["CARGO_TEST_EXTRA_ARGS"] = CARGO_TEST_EXTRA_ARGS
        stages.append(
            run_stage(
                "rust_gate",
                [sys.executable, "scripts/check.py"],
                env=rust_env,
                detail=(
                    "reuses scripts/check.py (layering, fmt, clippy -D warnings, "
                    "workspace tests, endpoint reconciliation). DEF-006 3 POC "
                    "tests excluded via CARGO_TEST_EXTRA_ARGS."
                ),
            )
        )

    # ---- Sidecar wire contract: existing Rust integration test ----------
    if wanted("sidecar_contract"):
        stages.append(
            run_stage(
                "sidecar_contract",
                [
                    "cargo",
                    "test",
                    "-p",
                    "ica-contracts",
                    "--test",
                    "sidecar_contract_test",
                ],
                env=env,
                detail="reuses crates/ica-contracts/tests/sidecar_contract_test.rs",
            )
        )

    # ---- Endpoint map: baseline-side, self-test always available --------
    if wanted("endpoint_map"):
        stages.append(
            run_stage(
                "endpoint_map",
                [sys.executable, "scripts/check_endpoint_map.py", "--self-test"],
                env=env,
                detail=(
                    "reuses scripts/check_endpoint_map.py --self-test "
                    "(5/5 synthetic assertions; no baseline checkout needed)."
                ),
            )
        )
    if wanted("endpoint_map_baseline"):
        if (PY_BASELINE / ".venv/bin/python").is_file():
            stages.append(
                run_stage(
                    "endpoint_map_baseline",
                    [
                        str(PY_BASELINE / ".venv/bin/python"),
                        "scripts/check_endpoint_map.py",
                        "--expect-excluded",
                        "34",
                    ],
                    env={**env, "PYTHONDONTWRITEBYTECODE": "1"},
                    detail=(
                        "baseline contract table vs. live FastAPI routes; "
                        "34 out-of-scope /api/browser/ routes expected."
                    ),
                )
            )
        else:
            stages.append(
                skip_stage(
                    "endpoint_map_baseline",
                    f"Python baseline venv not present at {PY_BASELINE}; "
                    "self-test stage still covers the comparison logic.",
                )
            )

    # ---- Full cross-language contract comparison (opt-in) ---------------
    if wanted("contract_verify"):
        if os.environ.get("ICA_GATE_RUN_CONTRACT") == "1" and (
            PY_BASELINE / ".venv/bin/python"
        ).is_file():
            stages.append(
                run_stage(
                    "contract_verify",
                    [sys.executable, "scripts/contract_verify.py"],
                    env=env,
                    timeout=1800,
                    detail=(
                        "reuses scripts/contract_verify.py with OUT_DIR pinned "
                        "outside the worktree to avoid dirty evidence rewrites."
                    ),
                )
            )
        else:
            stages.append(
                skip_stage(
                    "contract_verify",
                    "opt-in (ICA_GATE_RUN_CONTRACT=1) and requires the frozen "
                    "Python baseline at /mnt/o/ica/backend; not run by default "
                    "in CI because it boots two services and rewrites captures.",
                )
            )

    # ---- Frontend: reuse package.json scripts --------------------------
    if wanted("frontend_install"):
        need_install = args.ci or not (FRONTEND / "node_modules").is_dir()
        if need_install and executable("npm"):
            stages.append(
                run_stage(
                    "frontend_install",
                    ["npm", "ci", "--prefer-offline", "--no-audit", "--no-fund"],
                    cwd=FRONTEND,
                    env=env,
                    detail="npm ci from frontend/package-lock.json",
                )
            )
        elif not executable("npm"):
            stages.append(
                skip_stage("frontend_install", "npm not installed.", blocked=True)
            )
        else:
            stages.append(skip_stage("frontend_install", "node_modules already present."))

    if wanted("frontend_lint"):
        if executable("npm"):
            stages.append(
                run_stage(
                    "frontend_lint", ["npm", "run", "lint"], cwd=FRONTEND, env=env
                )
            )
        else:
            stages.append(skip_stage("frontend_lint", "npm not installed.", blocked=True))

    if wanted("frontend_build"):
        if executable("npm"):
            stages.append(
                run_stage(
                    "frontend_build",
                    ["npm", "run", "build"],
                    cwd=FRONTEND,
                    env=env,
                    detail="tsc -b (typecheck) + vite build",
                )
            )
        else:
            stages.append(skip_stage("frontend_build", "npm not installed.", blocked=True))

    if wanted("frontend_test"):
        if executable("npm"):
            stages.append(
                run_stage(
                    "frontend_test",
                    ["npm", "run", "test"],
                    cwd=FRONTEND,
                    env=env,
                    detail="vitest run; allow >=300s for jsdom environment setup",
                )
            )
        else:
            stages.append(skip_stage("frontend_test", "npm not installed.", blocked=True))

    # ---- Tauri host: syntax check always, build only when libs exist ----
    if wanted("tauri_syntax"):
        sources = sorted(glob.glob(str(TAURI_HOST / "src" / "*.rs")))
        if executable("rustfmt") and sources:
            stages.append(
                run_stage(
                    "tauri_syntax",
                    ["rustfmt", "--edition", "2021", "--check", *sources],
                    env=env,
                    detail=(
                        "rustfmt parses the host sources without needing "
                        "glib/WebKit; syntax-check only, NOT a typed build."
                    ),
                )
            )
        else:
            stages.append(
                skip_stage("tauri_syntax", "rustfmt unavailable or host sources missing.")
            )

    if wanted("tauri_build"):
        if has_system_lib("glib-2.0") and (
            has_system_lib("webkit2gtk-4.1") or has_system_lib("webkit2gtk-4.0")
        ):
            stages.append(
                run_stage(
                    "tauri_build",
                    ["cargo", "build", "--manifest-path", str(TAURI_HOST / "Cargo.toml")],
                    env=env,
                    timeout=1800,
                    detail="real Tauri host compile on a host with WebKit2GTK.",
                )
            )
        else:
            stages.append(
                skip_stage(
                    "tauri_build",
                    "HOST LIMITATION: glib-2.0 / webkit2gtk-4.1 not available "
                    "(WSL has no WebKit2GTK). Tauri compile is NOT verified here; "
                    "tauri_syntax only proves the host sources parse.",
                    blocked=True,
                )
            )

    # ---- Security: never self-sign -------------------------------------
    if wanted("security_scan"):
        if executable("cargo") and subprocess.run(
            ["cargo", "audit", "--version"], capture_output=True
        ).returncode == 0:
            stages.append(
                run_stage(
                    "security_scan",
                    ["cargo", "audit", "--locked"],
                    cwd=REPO,
                    env=env,
                    detail="cargo-audit dependency advisory scan.",
                )
            )
        else:
            stages.append(
                skip_stage(
                    "security_scan",
                    "TOOL LIMITATION: cargo-audit is not installed. "
                    "INDEPENDENT SECURITY VERIFICATION IS PENDING (manual) — "
                    "this gate does not self-sign Verified. Existing advisory "
                    "evidence: docs/testing/sec09-audit-r1.log (DEF-007/008).",
                    blocked=True,
                )
            )

    # ---- E2E: chain the entry point, do not fake a pass -----------------
    if wanted("e2e"):
        e2e_manifest = REPO / "frontend" / "e2e"
        e2e_script = None
        pkg = FRONTEND / "package.json"
        if pkg.is_file():
            scripts = json.loads(pkg.read_text(encoding="utf-8")).get("scripts", {})
            for candidate in ("test:e2e", "e2e"):
                if candidate in scripts:
                    e2e_script = candidate
                    break
        if e2e_script:
            stages.append(
                run_stage("e2e", ["npm", "run", e2e_script], cwd=FRONTEND, env=env)
            )
        else:
            stages.append(
                skip_stage(
                    "e2e",
                    "NO E2E HARNESS IN REPO: QA-02 core recruiting-loop E2E is "
                    "not implemented (DEF-003 Triaged). AC1 chains the entry "
                    "point only; no E2E pass is claimed.",
                    blocked=True,
                )
            )

    return stages


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, help="write the JSON execution record here")
    parser.add_argument("--only", help="comma-separated stage names to run")
    parser.add_argument("--skip-stage", help="comma-separated stage names to omit")
    parser.add_argument("--ci", action="store_true", help="CI mode: force npm ci")
    args = parser.parse_args()

    env = base_env()
    head = git_head()
    print(f"ARCH-P6 unified gate | HEAD={head} | started={now()}")
    stages = build_stages(args, env)

    counts = {
        status: sum(1 for s in stages if s["status"] == status)
        for status in ("PASS", "FAIL", "SKIP", "BLOCKED")
    }
    if counts["FAIL"]:
        result = "FAIL"
    elif counts["SKIP"] or counts["BLOCKED"]:
        result = "PASS_WITH_LIMITATIONS"
    else:
        result = "PASS"

    record = {
        "schema": "arch-p6-gate-run-v1",
        "head": head,
        "worktree": str(REPO),
        "mode": "ci" if args.ci else "local",
        "started_at": stages[0]["started_at"] if stages else now(),
        "finished_at": now(),
        "stages": stages,
        "summary": counts,
        "gate_result": result,
        "independent_security_verification": "PENDING_MANUAL",
        "windows_release_acceptance": "BLOCKED_NO_WINDOWS_HOST",
        "self_signed_verified": False,
    }

    print(f"\n{'=' * 72}\n== GATE SUMMARY ==")
    print(
        f"HEAD={head}  PASS={counts['PASS']}  FAIL={counts['FAIL']}  "
        f"SKIP={counts['SKIP']}  BLOCKED={counts['BLOCKED']}"
    )
    print(f"gate_result: {result}")
    print(
        "INDEPENDENT SECURITY VERIFICATION: PENDING (manual) — "
        "this gate does NOT self-sign Verified."
    )
    print(
        "WINDOWS DESKTOP ACCEPTANCE: BLOCKED (no Windows host) — "
        "see docs/testing/desktop-release-acceptance.md"
    )

    if args.out:
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(
            json.dumps(record, ensure_ascii=False, indent=2), encoding="utf-8"
        )
        print(f"execution record: {args.out}")

    return 1 if counts["FAIL"] else 0


if __name__ == "__main__":
    raise SystemExit(main())
