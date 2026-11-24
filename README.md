# ICA-RUST — Rust backend for ICA Personal

Rust port of the ICA Personal backend (axum + sqlx + SQLite).

**Status: all 68 contract endpoints are implemented in the Rust router, plus the
auxiliary `POST /api/imports/{session_id}/confirm-candidate` (ADR-0003 D-2).**
Measured at HEAD `40575e1` by `python3 scripts/endpoint_reconciliation.py --json`:
`contract=68 implemented=68 missing=0 extra_routes=0 auxiliary_routes=1`.
The earlier "45 of 68 / W5–W6 not implemented / no frontend change" text in this
README was stale (audit finding DOC-01) and has been replaced with the value the
gate actually measures — re-run the script rather than trusting this sentence.
Feature-level caveats that remain (local-mode identity, independent security
verification, Windows desktop acceptance) are recorded in
[`docs/audits/arch-p6-delivery-record.md`](docs/audits/arch-p6-delivery-record.md)
and [`docs/testing/desktop-release-acceptance.md`](docs/testing/desktop-release-acceptance.md);
they are **not** claimed as verified here.

The per-wave route table lives in the migration contract §2. The measured
status (test counts, comparison numbers, audit trail) lives in
[`docs/ICA-RUST 当前设计总方案.md`](docs/ICA-RUST%20当前设计总方案.md) §1 and in
the formal test report [`docs/testing/test-report-r1.0.md`](docs/testing/test-report-r1.0.md)
— this README does not restate volatile numbers that the gates and those
documents record better.

The Python service (`/mnt/o/ica`) is the **contract authority**: where this
repository and the Python code disagree, Python wins until a wave changes it
deliberately. The authoritative anchor SHA is re-measured by
`scripts/contract_verify.py` on every run (currently `9ed173c`, "Python 基线终态")
— see [`docs/adr/ADR-0006`](docs/adr/ADR-0006-baseline-layers-and-change-handling.md)
for the four different things called "baseline" here, and
[`docs/adr/ADR-0002`](docs/adr/ADR-0002-rust-backend-migration-reinstated.md)
for why the migration was reinstated despite ADM-001.

Documentation map (per-domain index, authoritative sources, lifecycle states):
[`docs/index.md`](docs/index.md). Project engineering-spec configuration
(SPE-GOV-001 §5.3): [`docs/engineering/ENG-ICA-SPECS-R1.0.md`](docs/engineering/ENG-ICA-SPECS-R1.0.md).

## Layout

```
crates/ica-domain/        pure model: entities, enums, pagination maths, search rules, content hash
crates/ica-application/   use cases + repository ports   (no axum/sqlx/tokio/hyper)
crates/ica-infra/         sqlx + SQLite: connect, pragmas, one store implementing every port
crates/ica-api/           axum routes, DTOs, body/query validation, error mapping
crates/ica-server/        binary: env config, tracing, startup self-check, shutdown
crates/ica-infra/schema/ica-baseline.sql   exported DDL of the real baseline database
scripts/check.py           fmt + clippy + tests (+ layering check) in one command
scripts/contract_verify.py                      seed → run both services → capture → diff
scripts/contract_capture.py / contract_diff.py   contract capture and comparison
scripts/seed_contract_db.py                      the seed both sides are compared on
scripts/check_endpoint_map.py                    doc table vs. live FastAPI routes
```

The two `*.sh` files are thin wrappers around `check.py` and
`contract_verify.py`, kept so every existing invocation keeps working:

```bash
scripts/check.sh          # same as: python3 scripts/check.py
scripts/contract_verify.sh   # same as: python3 scripts/contract_verify.py
```

Within `ica-infra`, `store.rs` holds the one adapter object (one connection
pool) and each aggregate has its own file — `repository.rs` (positions),
`candidates.rs`, `conversations.rs` (conversations + messages),
`send_records.rs` — plus `audit.rs`, the single audit writer every write path
calls inside its own transaction.

Dependency direction: `ica-server → ica-api + ica-infra → ica-application →
ica-domain`. It is compile-enforced: the inner crates simply do not list a web
framework, a database driver or an async runtime, so `use sqlx::…` there does
not build. `scripts/check.py` re-asserts it with a readable error.

## Prerequisites

- `cargo` / `rustc` **1.98.0** (pinned by `rust-toolchain.toml`; rustup will
  refuse a different toolchain rather than silently using one).
- For the contract comparison only: the Python baseline virtualenv at
  `/mnt/o/ica/backend/.venv` (Python 3.12).

## Build and test

```bash
cd /mnt/o/ICA-RUST
scripts/check.sh          # layering + cargo fmt --check + clippy -D warnings + cargo test
```

The gate itself is `scripts/check.py`; `scripts/check.sh` is a wrapper around it.
It prints the real test count (`tests: N passed / M failed`) — quote that number,
never an older one.

Individual gates:

```bash
cargo build --workspace
cargo fmt --all -- --check
cargo clippy --all-targets --workspace -- -D warnings
cargo test --workspace
```

### Unified full-stack gate and CI (ARCH-P6)

One entry point for local and CI — it delegates to the existing scripts and
does not re-implement them:

```bash
python3 scripts/gate.py --out /tmp/gate-run.json
```

It chains Rust (`scripts/check.py`), the sidecar wire contract test
(`ica-contracts/sidecar_contract_test`), endpoint reconciliation, frontend
`lint` / `build` (tsc) / `test` (vitest), the Tauri host (syntax check always;
real build only where glib/WebKit exist), `cargo audit` when installed, and the
E2E entry point when one exists. Stages that cannot run here are reported as
`SKIP`/`BLOCKED` with reasons — never silently promoted to PASS, and the gate
never self-signs `Verified`.

CI: `.github/workflows/quality-gate.yml` runs the same entry point on PRs to
`main`; it is a check only and contains no release/publish/push step.
Main-branch protection rules and the manual release process:
[`.github/BRANCH_PROTECTION.md`](.github/BRANCH_PROTECTION.md).
Current HEAD-bound execution report:
[`docs/testing/test-report-arch-p6.md`](docs/testing/test-report-arch-p6.md).
Windows desktop acceptance is explicitly `BLOCKED` on this WSL host:
[`docs/testing/desktop-release-acceptance.md`](docs/testing/desktop-release-acceptance.md).

### Where build artefacts go (decision D2)

`.cargo/config.toml` sets

```toml
[build]
target-dir = "/home/bryan/.cache/ica-rust-target"
```

because this repository lives on `/mnt/o` — a 20 GB Windows volume with ~7 GB
free and 9p filesystem semantics. A Rust debug target directory is 4–8 GB and
cargo's incremental compilation is slow and unreliable over 9p, so artefacts go
to ext4 and the repository stays small. Nothing else depends on the location; a
build with the redirect removed still works, it is just slower and may fill the
volume.

## Run

```bash
# Serve the existing baseline database, read-only (recommended for inspection).
DATABASE_URL="sqlite:////mnt/o/ica/backend/ica.db?mode=ro&immutable=true" \
  PORT=8011 cargo run -p ica-server

curl -s http://127.0.0.1:8011/health
curl -s "http://127.0.0.1:8011/api/positions?limit=2&include_archived=true"
```

Environment variables (all optional):

| Variable | Default | Meaning |
| --- | --- | --- |
| `DATABASE_URL` | `sqlite:///./ica.db` | `sqlite:` URL; `?mode=ro` read-only, `&immutable=true` no locking at all |
| `PORT` | `8000` | listen port (loopback only) |
| `HOST` | `127.0.0.1` | listen address — never expose candidate data to the LAN |
| `ICA_APP_NAME` | `ICA Personal` | value reported by `/health` as `app` |
| `AI_API_KEY` | *(unset)* | only its presence matters: `/health` reports `ai_enabled` |
| `RUST_LOG` | `info` | tracing filter |

> **URL slashes matter.** Three slashes make the path *relative* to the working
> directory (`sqlite:///./ica.db` → `./ica.db`), four make it *absolute*
> (`sqlite:////mnt/o/ica/backend/ica.db`). This is SQLAlchemy's rule, kept on
> purpose so a Python `.env` can be reused unchanged.

Startup fails fast with a named missing table when the database is not the one
the endpoints need, instead of 500-ing on the first request. `SIGTERM`/`Ctrl-C`
drain in-flight requests before exit.

## Contract comparison (re-run)

The Rust service must answer exactly like Python — same responses **and** the
same rows afterwards. `scripts/contract_verify.py` does the whole thing
(`scripts/contract_verify.sh` is a wrapper around it):

```bash
cd /mnt/o/ICA-RUST
scripts/contract_verify.sh
```

It seeds the database twice (the scenario writes, so each service needs its own
copy), starts the Python baseline from a `/tmp` copy of its tree, builds and
starts the Rust port, captures both sides, and diffs responses plus the final
state of all eleven business tables. It finishes by proving the baseline is
untouched (`git status` empty, ica.db sha256 unchanged).

Last recorded result: **zero-failure comparison over the 286-case set (W5/W6 included)**
(recorded in [`docs/ICA-RUST 当前设计总方案.md`](docs/ICA-RUST%20当前设计总方案.md) §1;
the W2 dashboard `calendar` baseline-drift item was closed by the G0 governance).
Re-run the comparison yourself rather than trusting any number here — the
`improvements:` section asserts IMP-1 (`python total=2 / rust total=5`),
IMP-2 (`python 500 / rust 422` for both NULL cases), IMP-3 (five lists, one
`total` meaning) and IMP-4 (`offset` past `u64::MAX` → 422). The individual
steps are in
[`docs/rust-migration-contract.md`](docs/rust-migration-contract.md) §8.

> The OCR staging fixture in `contract_capture.py` renders a 760 px-wide
> image — deliberately at the width where the detector's input clamping used
> to flip the line breaks (independent audit, OCR F1). The engine now runs
> with the baseline's unrestricted policy; the config is pinned by
> `the_detector_runs_unrestricted_like_the_baseline` in `ica-infra`.

## Proving the baseline database was not modified

```bash
sha256sum /mnt/o/ica/backend/ica.db          # before
DATABASE_URL="sqlite:////mnt/o/ica/backend/ica.db?mode=ro&immutable=true" \
  PORT=8014 timeout 3 cargo run -p ica-server
sha256sum /mnt/o/ica/backend/ica.db          # identical
git -C /mnt/o/ica status --porcelain -uall   # empty
```

Use `immutable=true` for a strict proof: plain `?mode=ro` on a WAL database
still updates the `-shm` sidecar's mtime, even though `ica.db` itself is
untouched. `immutable=true` leaves `ica.db`, `-wal` and `-shm` byte- and
mtime-identical.

## Endpoint map

`docs/rust-migration-contract.md` lists all 68 `/api` routes and which wave owns
each. The table is checked against the live FastAPI route table:

```bash
PYTHONDONTWRITEBYTECODE=1 env -u PYTHONPATH \
  /mnt/o/ica/backend/.venv/bin/python scripts/check_endpoint_map.py
# live /api routes: 68 / documented rows: 68 / RESULT: table matches
```

## Known limitations (current state)

- **MultiUser authorization is partial.** The HTTP authentication middleware is
  real (anonymous/forged bearer requests get 401) and resource-level scope is
  enforced for **Positions** only, with partial ownership checks for Shares.
  Candidates, conversations, messages, drafts, follow-ups, imports, settings,
  AI and backup data are not yet tenant/owner filtered in the repository, and
  admin-only operations have no HTTP/IPC role gate. Do not expose the service
  on a non-loopback interface: `NETWORK_AUTHENTICATION_READY` in
  `crates/ica-server/src/main.rs` remains `false` on purpose. See
  [`docs/architecture/api-authentication-matrix.md`](docs/architecture/api-authentication-matrix.md)
  and [`docs/audits/sec-04-bc-delivery-record.md`](docs/audits/sec-04-bc-delivery-record.md).
- **Verification evidence is version-bound.** The last full-stack gate report
  is [`docs/testing/test-report-arch-p6.md`](docs/testing/test-report-arch-p6.md)
  at HEAD `40575e1`; later UI phases recorded frontend-only numbers. Full
  Python/Rust contract parity at the current HEAD has not been executed, and
  the frozen Python baseline commit is still ambiguous (`d39e321` vs
  `9ed173c`). Three Browser POC tests remain excluded, and there is no E2E
  harness for the recruiting loop.
- **Desktop acceptance is partial.** Windows 11 evidence at
  [`docs/testing/desktop-release-acceptance.md`](docs/testing/desktop-release-acceptance.md)
  is 7/9 items (W4/W5 not executed, W9 partial, installer unsigned, no release
  tag) and bound to an older commit. Tauri host compilation is not verified on
  Linux hosts without WebKit2GTK.
- `422` bodies carry a structurally similar but not identical `details.errors`
  list to pydantic's; `contract_diff.py` compares status, `error.code`,
  `error.message` and the `details` key set (see contract doc §7).
- The three declared improvements in contract doc §7.1 are the **only**
  intentional differences from Python (IMP-1/IMP-2/IMP-3); the comparison fails
  on anything else.
- UI reuse metrics C3/C5/C8 are still red in
  `frontend/ui-reuse-baseline-r4-20261124.json`; the "regression=0" gate only
  means no new regression against that frozen baseline.
