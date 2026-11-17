# ICA-RUST — Rust backend for ICA Personal

Rust port of the ICA Personal backend (axum + sqlx + SQLite).

**Status: R0 — skeleton.** Three read-only endpoints are implemented and
contract-verified against the Python baseline; the remaining 66 `/api` routes
are mapped to waves W1–W7 in [`docs/rust-migration-contract.md`](docs/rust-migration-contract.md)
and are **not** implemented yet. There are no write paths, no auth and no AI
calls.

| Implemented | Path | Notes |
| --- | --- | --- |
| `GET` | `/health` | `{"status":"ok","app":…,"ai_enabled":…}` |
| `GET` | `/api/positions` | `Page<PositionRead>`; `include_archived`, `search`, `offset`, `limit` |
| `GET` | `/api/positions/{id}` | single `PositionRead`; 404 with the project envelope |

The Python service (`/mnt/o/ica`, @ `d39e321`) is the **contract authority**:
where this repository and the Python code disagree, Python wins until a wave
changes it deliberately. See [`docs/adr/ADR-0002`](docs/adr/ADR-0002-rust-backend-migration-reinstated.md)
for why the migration was reinstated despite ADM-001.

## Layout

```
crates/ica-domain/        pure model: entities, enums, pagination maths, search rules
crates/ica-application/   use cases + repository ports   (no axum/sqlx/tokio/hyper)
crates/ica-infra/         sqlx + SQLite: connect, pragmas, repository, schema snapshot
crates/ica-api/           axum routes, DTOs, error mapping
crates/ica-server/        binary: env config, tracing, startup self-check, shutdown
crates/ica-infra/schema/ica-baseline.sql   exported DDL of the real baseline database
scripts/check.sh          fmt + clippy + tests (+ layering check) in one command
scripts/contract_capture.py / contract_diff.py   contract capture and comparison
scripts/seed_contract_db.py                      the seed both sides are compared on
scripts/check_endpoint_map.py                    doc table vs. live FastAPI routes
```

Dependency direction: `ica-server → ica-api + ica-infra → ica-application →
ica-domain`. It is compile-enforced: the inner crates simply do not list a web
framework, a database driver or an async runtime, so `use sqlx::…` there does
not build. `scripts/check.sh` re-asserts it with a readable error.

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

Individual gates:

```bash
cargo build --workspace
cargo fmt --all -- --check
cargo clippy --all-targets --workspace -- -D warnings
cargo test --workspace
```

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

The Rust service must answer exactly like Python. Both sides read the *same*
seeded database, so a difference can only come from code.

```bash
cd /mnt/o/ICA-RUST

# 1. Schema (from the exported baseline DDL) + deterministic seed
python3 scripts/seed_contract_db.py --out /tmp/ica-contract/seed.db
python3 scripts/peek_seed.py /tmp/ica-contract/seed.db      # optional: what is in it

# 2. Python baseline: a copy WITHOUT .env / .venv (no credentials leave the baseline)
mkdir -p /tmp/ica-py-baseline
rsync -a --exclude '.venv' --exclude '.env' --exclude '__pycache__' \
  --exclude 'ica.db*' /mnt/o/ica/backend/ /tmp/ica-py-baseline/
cd /tmp/ica-py-baseline
env -u PYTHONPATH -u AI_API_KEY PYTHONDONTWRITEBYTECODE=1 \
  DATABASE_URL="sqlite:////tmp/ica-contract/seed.db" \
  /mnt/o/ica/backend/.venv/bin/python -m uvicorn app.main:app --port 8010 &

# 3. Rust service on the same file, read-only
cd /mnt/o/ICA-RUST
DATABASE_URL="sqlite:////tmp/ica-contract/seed.db?mode=ro" PORT=8011 cargo run -p ica-server &

# 4. Capture both and diff
python3 scripts/contract_capture.py --base-url http://127.0.0.1:8010 \
  --out docs/contract-baseline/positions-python.json
python3 scripts/contract_capture.py --base-url http://127.0.0.1:8011 \
  --out docs/contract-baseline/positions-rust.json
python3 scripts/contract_diff.py \
  --python docs/contract-baseline/positions-python.json \
  --rust docs/contract-baseline/positions-rust.json
```

Last run: **20/20 cases matched, 0 failure(s)** (20 cases covering default list,
pagination boundaries, `offset` past the end, `include_archived`, search
hit/miss/case/whitespace/NULL-location, detail, archived detail, 404, and the
422s for `limit=0`, `limit=501`, `offset=-1`, `search` over 200 characters).

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

## Known limitations (R0)

- Read-only endpoints only; every write endpoint is scheduled (W1+).
- **Tenant/owner scoping is not modelled.** It is a no-op in Python's default
  `local` mode, so both services agree today; do not run the Rust service in
  `multiuser` mode before W6 ships auth.
- `GET /` (the welcome string) is not implemented — it is not part of the API
  surface.
- `422` bodies carry a structurally similar but not identical `details.errors`
  list to pydantic's; `contract_diff.py` compares status, `error.code`,
  `error.message` and the `details` key set (see contract doc §7).
- No packaging, Docker or desktop-shell changes.
