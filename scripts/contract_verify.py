#!/usr/bin/env python3
"""One-command contract comparison: seed, run both services, capture, diff.

The scenario in `contract_capture.py` writes, so each service gets its **own
copy** of the seeded database. The Python baseline at /mnt/o/ica is never
opened: the service runs from a throwaway copy in /tmp, and its database is a
copy of the seed too.

Usage:
    scripts/contract_verify.py --help
    scripts/contract_verify.py --dry-run
    scripts/contract_verify.py --force        # authorises scratch deletion

Exit code 0 means `contract_diff.py` reported 0 failures. The captures are
written to docs/contract-baseline/ so a reviewer can read the raw evidence.

This is the Python successor of ``scripts/contract_verify.sh``; the shell file
is kept as a one-line wrapper so existing invocations keep working.

Environment overrides (same names as the shell version):
    WORK             scratch directory (default /tmp/ica-contract); must resolve
                     under /tmp/ica-contract or <repo>/.work/ica-contract, or
                     under ICA_CONTRACT_WORK_ROOT
    ICA_CONTRACT_WORK_ROOT  extra absolute deletion-whitelist root
    ICA_COMMAND_TIMEOUT  per-command timeout in seconds (default 600)
    PY_BASELINE_REPO     baseline git checkout for the untouched check
                     (default /mnt/o/ica)
    PY_BASELINE      the Python baseline tree (default /mnt/o/ica/backend)
    PY_VENV_PYTHON   the baseline interpreter (default <PY_BASELINE>/.venv/bin/python)
    PY_PORT          port for the Python baseline (default 8010)
    RUST_PORT        port for the Rust port (default 8011)
    OUT_DIR          where the captures land (default docs/contract-baseline)
"""

from __future__ import annotations

import argparse
import logging
import os
import shutil
import signal
import subprocess
import sys
import tempfile
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
COMMAND_TIMEOUT = int(os.environ.get("ICA_COMMAND_TIMEOUT", "600"))
PY_BASELINE_REPO = Path(os.environ.get("PY_BASELINE_REPO", "/mnt/o/ica"))
# SPE-BAS-004 §8.1/§5.6: destructive cleanup is only permitted inside these
# roots. Extend via ICA_CONTRACT_WORK_ROOT (a real path, not a glob).
EXTRA_WORK_ROOT = os.environ.get("ICA_CONTRACT_WORK_ROOT", "")
FORCE = False
DRY_RUN = False
LOG = logging.getLogger("contract_verify")


def allowed_work_roots() -> list[Path]:
    """Resolved roots in which this harness may delete scratch state."""
    roots = [
        Path("/tmp/ica-contract").resolve(),
        Path("/tmp/ica-py-baseline").resolve(),
        (Path(tempfile.gettempdir()) / "ica-contract").resolve(),
        (Path(tempfile.gettempdir()) / "ica-py-baseline").resolve(),
        (REPO_ROOT / ".work" / "ica-contract").resolve(),
    ]
    if EXTRA_WORK_ROOT:
        roots.append(Path(EXTRA_WORK_ROOT).expanduser().resolve())
    return roots


def assert_deletable(path: Path, label: str) -> Path:
    """Fail closed unless `path` is inside a whitelisted scratch root (exit 7)."""
    target = path.expanduser().resolve()
    if not any(target.is_relative_to(root) for root in allowed_work_roots()):
        LOG.error(
            "%s %s is outside the deletion whitelist %s",
            label,
            target,
            [str(root) for root in allowed_work_roots()],
        )
        raise SystemExit(7)
    if target == Path(target.anchor):
        LOG.error("refusing to delete a filesystem root: %s", target)
        raise SystemExit(7)
    return target


def remove_tree(path: Path, label: str) -> None:
    """Print, validate and (with --force) delete a scratch tree; --dry-run previews."""
    if not path.exists():
        LOG.info("%s does not exist, nothing to delete: %s", label, path)
        return
    target = assert_deletable(path, label)
    LOG.info("DELETE TARGET [%s]: %s", label, target)
    if DRY_RUN:
        LOG.info("[DRY-RUN] no files were modified")
        return
    if not FORCE:
        LOG.error("refusing to delete %s without --force", target)
        raise SystemExit(6)
    shutil.rmtree(target)
    if target.exists():
        LOG.error("deletion failed: %s still exists", target)
        raise SystemExit(5)


def run_command(argv: list[str], *, cwd: Path | None = None, env=None,
                capture: bool = False) -> subprocess.CompletedProcess:
    """subprocess.run with a mandatory timeout; timeout maps to exit 9."""
    try:
        return subprocess.run(
            argv, cwd=cwd, env=env, check=False, shell=False,
            capture_output=capture, text=capture, timeout=COMMAND_TIMEOUT,
        )
    except subprocess.TimeoutExpired as exc:
        LOG.error("command timed out after %ss: %s", COMMAND_TIMEOUT, argv[0])
        if exc.stdout:
            sys.stderr.write(str(exc.stdout))
        raise SystemExit(9) from exc


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
    result = run_command(
        ["rsync", "-a", *excludes, f"{PY_BASELINE}/", f"{destination}/"]
    )
    if result.returncode != 0:
        LOG.error("rsync failed with exit %s", result.returncode)
        raise SystemExit(result.returncode or 1)


def reset_shared_secrets() -> None:
    """Wipe the shared credential-slot directory before a side boots.

    Both services resolve credential slots against the SAME data root (the
    OCR staging contract pins `ICA_DATA_DIR` to one directory), so a key the
    Python side stores mid-run would be loaded by the Rust side's startup
    overlay when it boots second — and `generate_no_key` (case order 160)
    would then make a REAL provider call on one side only. Credential slots
    are per-side state that must start equal; the file fallback (the Linux
    dev store) made that stale-state window observable.
    """
    remove_tree(WORK / "python-data" / "secrets", "shared credential slots")


def start_python_baseline() -> subprocess.Popen:
    """Start the Python baseline on its own copy of the seeded database."""
    if port_in_use(PY_PORT):
        sys.stderr.write(
            f"   port {PY_PORT} is already serving something; stop it first\n"
        )
        raise SystemExit(1)

    tree = assert_deletable(Path("/tmp/ica-py-baseline"), "baseline copy")
    remove_tree(tree, "baseline copy")
    tree.mkdir(parents=True, exist_ok=True)
    copy_baseline(tree)

    reset_shared_secrets()
    env = clean_env()
    env["PYTHONDONTWRITEBYTECODE"] = "1"
    env["DATABASE_URL"] = f"sqlite:///{WORK / 'python.db'}"
    env["ICA_DATA_DIR"] = str(WORK / "python-data")
    # W6: the token flow needs a real HMAC secret on both sides (the local
    # default "" makes PyJWT refuse to sign, which 500s the login).
    env["JWT_SECRET"] = "contract-verify-hmac-secret"
    # Backups are per side: the create/restore flows write under this
    # directory, and a shared one would let one side's files appear in the
    # other side's listing (the OCR staging stays shared — that contract
    # detail is the allowed_prefix itself).
    env["BACKUP_DIR"] = str(WORK / "python-data" / "backups-py")
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
    result = run_command(
        ["cargo", "metadata", "--format-version", "1", "--no-deps"],
        cwd=REPO_ROOT,
        env=clean_env(),
        capture=True,
    )
    if result.returncode != 0:
        sys.stderr.write(result.stderr)
        raise SystemExit(1)
    import json

    return json.loads(result.stdout)["target_directory"]


def start_rust_port() -> subprocess.Popen:
    """Build and start the Rust port on its own copy of the seeded database."""
    build = run_command(
        ["cargo", "build", "-p", "ica-server", "--quiet"],
        cwd=REPO_ROOT,
        env=clean_env(),
    )
    if build.returncode != 0:
        raise SystemExit(1)

    reset_shared_secrets()
    env = clean_env()
    env["DATABASE_URL"] = f"sqlite:///{WORK / 'rust.db'}"
    env["PORT"] = RUST_PORT
    env["HOST"] = "127.0.0.1"
    # The OCR staging directory is part of the contract: `ocr_path_not_allowed`
    # reports `details.allowed_prefix`, so both sides must resolve against the
    # SAME root or the envelopes differ on a path rather than on behaviour.
    env["ICA_DATA_DIR"] = str(WORK / "python-data")
    env["BACKUP_DIR"] = str(WORK / "python-data" / "backups-rs")
    # W6: the same HMAC secret the Python side gets (see start_python).
    env["JWT_SECRET"] = "contract-verify-hmac-secret"
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
    command = [sys.executable, str(SCRIPTS / name), *arguments]
    if name == "seed_contract_db.py" and FORCE:
        command.append("--force")
    if name == "seed_contract_db.py" and DRY_RUN:
        command.append("--dry-run")
    return run_command(command, cwd=REPO_ROOT, env=env).returncode


def report_baseline_untouched() -> int:
    """Check that the read-only baseline was not modified."""
    say("")
    say("== baseline untouched? ==")
    status = run_command(
        ["git", "-C", str(PY_BASELINE_REPO), "status", "--porcelain", "-uall"],
        capture=True,
    )
    if status.stdout.strip():
        say("git status: DIRTY")
        return 1
    say("git status: clean")
    head = run_command(
        ["git", "-C", str(PY_BASELINE_REPO), "rev-parse", "--short", "HEAD"],
        capture=True,
    )
    say(f"HEAD: {head.stdout.strip()}")
    digest = run_command(
        ["sha256sum", str(PY_BASELINE / "ica.db")], capture=True
    )
    say(f"ica.db sha256: {digest.stdout.split()[0]}")
    return 0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Seed, boot both services, capture and diff the contract."
    )
    parser.add_argument(
        "--force", action="store_true",
        help="authorise deletion of whitelisted scratch directories (L4)",
    )
    parser.add_argument(
        "--dry-run", action="store_true",
        help="validate paths and preview deletions without changing anything",
    )
    parser.add_argument(
        "--work", type=Path, default=WORK,
        help="scratch directory (must be inside the deletion whitelist)",
    )
    parser.add_argument(
        "--baseline", type=Path, default=PY_BASELINE,
        help="Python baseline tree (default PY_BASELINE or /mnt/o/ica/backend)",
    )
    parser.add_argument(
        "--out-dir", type=Path, default=OUT_DIR,
        help="where capture JSON is written",
    )
    parser.add_argument(
        "--timeout", type=int, default=COMMAND_TIMEOUT,
        help="timeout in seconds for each external command (default 600)",
    )
    return parser.parse_args()


def main() -> int:
    global WORK, PY_BASELINE, PY_VENV_PYTHON, OUT_DIR, COMMAND_TIMEOUT
    global FORCE, DRY_RUN
    logging.basicConfig(level=logging.INFO, format="%(levelname)s %(message)s")
    args = parse_args()
    if args.timeout <= 0:
        LOG.error("--timeout must be positive")
        return 2
    COMMAND_TIMEOUT = args.timeout
    FORCE, DRY_RUN = args.force, args.dry_run
    WORK = args.work.expanduser()
    PY_BASELINE = args.baseline.expanduser()
    PY_VENV_PYTHON = os.environ.get(
        "PY_VENV_PYTHON", str(PY_BASELINE / ".venv" / "bin" / "python")
    )
    OUT_DIR = args.out_dir.expanduser()
    assert_deletable(WORK, "work directory")
    say("== 1/5 seed two independent copies of the shared database ==")
    remove_tree(WORK, "work directory")
    if DRY_RUN:
        remove_tree(WORK / "python-data" / "secrets", "credential slots")
        LOG.info("[DRY-RUN] no services started, no files written")
        return 0
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
        # The Python capture ran `ai_key_put`, which STORED a credential slot
        # in the shared data root; the Rust capture boots against the same
        # root and its startup overlay would load that key, turning its own
        # `generate_no_key` (case 160, before its own `ai_key_put`) into a
        # real provider call. Each side's capture must start credential-less.
        reset_shared_secrets()
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
