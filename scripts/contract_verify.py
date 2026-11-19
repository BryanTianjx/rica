#!/usr/bin/env python3
"""One-command contract comparison: seed, run both services, capture, diff.

The scenario in `contract_capture.py` writes, so each service gets its **own
copy** of the seeded database. The Python baseline at /mnt/o/ica is never
opened: the service runs from a throwaway copy in /tmp, and its database is a
copy of the seed too.

Usage:
    scripts/contract_verify.py

Exit code 0 means `contract_diff.py` reported 0 failures. The captures are
written to docs/contract-baseline/ so a reviewer can read the raw evidence.

This is the Python successor of ``scripts/contract_verify.sh``; the shell file
is kept as a one-line wrapper so existing invocations keep working.

Environment overrides (same names as the shell version):
    WORK             scratch directory (default /tmp/ica-contract)
    PY_BASELINE      the Python baseline tree (default /mnt/o/ica/backend)
    PY_VENV_PYTHON   the baseline interpreter (default <PY_BASELINE>/.venv/bin/python)
    PY_PORT          port for the Python baseline (default 8010)
    RUST_PORT        port for the Rust port (default 8011)
    OUT_DIR          where the captures land (default docs/contract-baseline)
"""

from __future__ import annotations

import os
import shutil
import signal
import subprocess
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
SCRIPTS = REPO_ROOT / "scripts"

WORK = Path(os.environ.get("WORK", "/tmp/ica-contract"))
PY_BASELINE = Path(os.environ.get("PY_BASELINE", "/mnt/o/ica/backend"))
PY_VENV_PYTHON = os.environ.get(
    "PY_VENV_PYTHON", str(PY_BASELINE / ".venv" / "bin" / "python")
)
PY_PORT = os.environ.get("PY_PORT", "8010")
RUST_PORT = os.environ.get("RUST_PORT", "8011")
OUT_DIR = Path(os.environ.get("OUT_DIR", str(REPO_ROOT / "docs" / "contract-baseline")))

# The baseline tree is copied without these: the service must not read the real
# baseline tree, and it must not touch the baseline database.
EXCLUDED_FROM_BASELINE = (".venv", ".env", "__pycache__")
# Glob patterns for the baseline database, which must never be copied.
BASELINE_DB_PATTERNS = ("ica.db", "ica.db-wal", "ica.db-shm")

HEALTH_ATTEMPTS = 60
HEALTH_INTERVAL = 0.5


def clean_env() -> dict[str, str]:
    """The environment with the host's Python paths and AI key neutralised, plus
    the build-artefact location on a fast local disk (Tauri-audit finding
    G-5: the repo volume is small and the committed config no longer pins an
    absolute path; an explicit `CARGO_TARGET_DIR` always wins).

    `AI_API_KEY` is set to the **empty string**, not deleted. Deleting it lets
    the Python baseline fall back to its own `.env` (pydantic-settings'
    `env_file` is an absolute path derived from `__file__`), so
    `/health.ai_enabled` would mirror whoever ran the harness instead of the
    no-key scenario the baseline facts were measured in. An environment
    variable wins over `.env`, so `""` pins `ai_enabled=false` on both sides —
    and the AI path still answers the same 503, because the provider reads the
    per-model env var rather than `settings`.

    That alone is not enough: `app/config.py` finishes with
    `apply_stored_secrets(settings)`, which overlays the **OS credential
    store** (`<data dir>/secrets/ai_api_key.secret`) onto the settings
    singleton *after* the env/`.env` sources are resolved — so a stored key
    beats `AI_API_KEY=""`. `start_python_baseline` therefore also points
    `ICA_DATA_DIR` at a scratch directory, which is what actually isolates the
    harness from the operator's stored credential.
    """
    env = dict(os.environ)
    for name in ("PYTHONPATH", "PYTHONHOME"):
        env.pop(name, None)
    env["AI_API_KEY"] = ""
    env.setdefault(
        "CARGO_TARGET_DIR", str(Path.home() / ".cache" / "ica-rust-target")
    )
    return env


def say(message: str) -> None:
    print(message)


def is_up(url: str) -> bool:
    """Whether a service answers its health probe."""
    try:
        with urllib.request.urlopen(url, timeout=1) as response:
            return response.status == 200
    except (urllib.error.URLError, OSError):
        return False


def wait_for_health(base_url: str, label: str) -> None:
    """Block until a service answers its health probe, or exit 1.

    The probe is `<base_url>/health` — the **baseline** answers 200 on `/` but
    the Rust port answers 404 there, so probing the bare base URL would make
    the baseline look healthy and the port look dead.
    """
    url = f"{base_url.rstrip('/')}/health"
    for _ in range(HEALTH_ATTEMPTS):
        if is_up(url):
            say(f"   {label} is up ({url})")
            return
        time.sleep(HEALTH_INTERVAL)
    sys.stderr.write(f"   {label} did not answer on {url}\n")
    raise SystemExit(1)


def port_in_use(port: str) -> bool:
    """Whether something already answers on the port.

    A server from an earlier run would silently serve the wrong database, so
    the comparison refuses instead of guessing.
    """
    return is_up(f"http://127.0.0.1:{port}/health")


def copy_baseline(destination: Path) -> None:
    """Copy the baseline tree without credentials, the venv or the database."""
    if shutil.which("rsync") is None:
        raise SystemExit("rsync is required to copy the baseline tree")
    excludes = [f"--exclude={name}" for name in EXCLUDED_FROM_BASELINE]
    excludes += [f"--exclude={pattern}" for pattern in BASELINE_DB_PATTERNS]
    subprocess.run(
        ["rsync", "-a", *excludes, f"{PY_BASELINE}/", f"{destination}/"],
        check=True,
    )


def start_python_baseline() -> subprocess.Popen:
    """Start the Python baseline on its own copy of the seeded database."""
    if port_in_use(PY_PORT):
        sys.stderr.write(
            f"   port {PY_PORT} is already serving something; stop it first\n"
        )
        raise SystemExit(1)

    tree = Path("/tmp/ica-py-baseline")
    if tree.exists():
        shutil.rmtree(tree)
    tree.mkdir(parents=True)
    copy_baseline(tree)

    env = clean_env()
    env["PYTHONDONTWRITEBYTECODE"] = "1"
    env["DATABASE_URL"] = f"sqlite:///{WORK / 'python.db'}"
    env["ICA_DATA_DIR"] = str(WORK / "python-data")
    log = (WORK / "python.log").open("w")

    # `exec`-like behaviour: the process group is started directly so killing
    # it actually stops uvicorn, instead of leaving a server holding the
    # (now deleted) database file.
    process = subprocess.Popen(
        [
            PY_VENV_PYTHON,
            "-m",
            "uvicorn",
            "app.main:app",
            "--host",
            "127.0.0.1",
            "--port",
            PY_PORT,
        ],
        cwd=tree,
        env=env,
        stdout=log,
        stderr=subprocess.STDOUT,
        start_new_session=True,
    )
    process._log_handle = log  # type: ignore[attr-defined]  # keep the file open
    return process


def cargo_target_dir() -> str:
    """Where cargo puts the binaries.

    The workspace pins `target-dir` to ext4, so ask cargo instead of assuming
    `./target`.
    """
    result = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--no-deps"],
        cwd=REPO_ROOT,
        env=clean_env(),
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        sys.stderr.write(result.stderr)
        raise SystemExit(1)
    import json

    return json.loads(result.stdout)["target_directory"]


def start_rust_port() -> subprocess.Popen:
    """Build and start the Rust port on its own copy of the seeded database."""
    build = subprocess.run(
        ["cargo", "build", "-p", "ica-server", "--quiet"],
        cwd=REPO_ROOT,
        env=clean_env(),
    )
    if build.returncode != 0:
        raise SystemExit(1)

    env = clean_env()
    env["DATABASE_URL"] = f"sqlite:///{WORK / 'rust.db'}"
    env["PORT"] = RUST_PORT
    env["HOST"] = "127.0.0.1"
    # The OCR staging directory is part of the contract: `ocr_path_not_allowed`
    # reports `details.allowed_prefix`, so both sides must resolve against the
    # SAME root or the envelopes differ on a path rather than on behaviour.
    env["ICA_DATA_DIR"] = str(WORK / "python-data")
    # The OCR models live in the baseline's venv (the port runs the SAME
    # model set, so the recognised text matches). Point the engine at them.
    models = Path(PY_BASELINE) / ".venv/lib/python3.12/site-packages/rapidocr/models"
    if models.is_dir():
        env["RAPIDOCR_MODEL_DIR"] = str(models)
    log = (WORK / "rust.log").open("w")
    process = subprocess.Popen(
        [str(Path(cargo_target_dir()) / "debug" / "ica-server")],
        cwd=REPO_ROOT,
        env=env,
        stdout=log,
        stderr=subprocess.STDOUT,
        start_new_session=True,
    )
    process._log_handle = log  # type: ignore[attr-defined]  # keep the file open
    if process.poll() is not None:
        sys.stderr.write(f"   rust server failed to start; see {WORK / 'rust.log'}\n")
        raise SystemExit(1)
    return process


def stop(process: subprocess.Popen | None) -> None:
    """Stop a service and its process group, then close its log."""
    if process is None:
        return
    if process.poll() is None:
        try:
            os.killpg(os.getpgid(process.pid), signal.SIGTERM)
        except (ProcessLookupError, PermissionError):
            process.terminate()
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            try:
                os.killpg(os.getpgid(process.pid), signal.SIGKILL)
            except (ProcessLookupError, PermissionError):
                process.kill()
            process.wait()
    handle = getattr(process, "_log_handle", None)
    if handle is not None:
        handle.close()


def run_script(name: str, *arguments: str, extra_env: dict[str, str] | None = None) -> int:
    """Run one of the repository's own Python scripts."""
    env = clean_env()
    env.update(extra_env or {})
    return subprocess.run(
        [sys.executable, str(SCRIPTS / name), *arguments],
        cwd=REPO_ROOT,
        env=env,
    ).returncode


def report_baseline_untouched() -> int:
    """Check that the read-only baseline was not modified."""
    say("")
    say("== baseline untouched? ==")
    status = subprocess.run(
        ["git", "-C", "/mnt/o/ica", "status", "--porcelain", "-uall"],
        capture_output=True,
        text=True,
    )
    if status.stdout.strip():
        say("git status: DIRTY")
        return 1
    say("git status: clean")
    head = subprocess.run(
        ["git", "-C", "/mnt/o/ica", "rev-parse", "--short", "HEAD"],
        capture_output=True,
        text=True,
    )
    say(f"HEAD: {head.stdout.strip()}")
    digest = subprocess.run(
        ["sha256sum", "/mnt/o/ica/backend/ica.db"], capture_output=True, text=True
    )
    say(f"ica.db sha256: {digest.stdout.split()[0]}")
    return 0


def main() -> int:
    say("== 1/5 seed two independent copies of the shared database ==")
    shutil.rmtree(WORK, ignore_errors=True)
    WORK.mkdir(parents=True, exist_ok=True)
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    if run_script("seed_contract_db.py", "--out", str(WORK / "seed.db")) != 0:
        return 1
    shutil.copy2(WORK / "seed.db", WORK / "python.db")
    shutil.copy2(WORK / "seed.db", WORK / "rust.db")

    python_process: subprocess.Popen | None = None
    rust_process: subprocess.Popen | None = None
    try:
        say("")
        say("== 2/5 start the Python baseline on a copy (writes allowed there) ==")
        python_process = start_python_baseline()
        wait_for_health(f"http://127.0.0.1:{PY_PORT}", "python")

        say("")
        say("== 3/5 build and start the Rust port on its own copy ==")
        rust_process = start_rust_port()
        wait_for_health(f"http://127.0.0.1:{RUST_PORT}", "rust")

        say("")
        say("== 4/5 capture both sides ==")
        if (
            run_script(
                "contract_capture.py",
                "--base-url",
                f"http://127.0.0.1:{PY_PORT}",
                "--db",
                str(WORK / "python.db"),
                "--out",
                str(OUT_DIR / "w1-python.json"),
                extra_env={
                    "ICA_OCR_STAGING_DIR": str(WORK / "python-data" / "ocr"),
                    "ICA_OCR_SIDE": "py",
                },
            )
            != 0
        ):
            return 1
        if (
            run_script(
                "contract_capture.py",
                "--base-url",
                f"http://127.0.0.1:{RUST_PORT}",
                "--db",
                str(WORK / "rust.db"),
                "--out",
                str(OUT_DIR / "w1-rust.json"),
                extra_env={
                    "ICA_OCR_STAGING_DIR": str(WORK / "python-data" / "ocr"),
                    "ICA_OCR_SIDE": "rs",
                },
            )
            != 0
        ):
            return 1

        say("")
        say("== 5/5 diff ==")
        code = run_script(
            "contract_diff.py",
            "--python",
            str(OUT_DIR / "w1-python.json"),
            "--rust",
            str(OUT_DIR / "w1-rust.json"),
        )
        if code != 0:
            return code
    finally:
        stop(rust_process)
        stop(python_process)

    return report_baseline_untouched()


if __name__ == "__main__":
    raise SystemExit(main())
