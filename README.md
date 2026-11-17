# ICA-RUST — Rust backend for ICA Personal

Rust port of the ICA Personal backend (axum + sqlx + SQLite).

**Status: R0 + W1 — 18 endpoints.** R0 shipped the skeleton and three read-only
routes; W1 adds the 15 remaining positions/candidate/conversation/message/
send-record routes, including the write paths (`POST`/`PATCH`/`DELETE`). All 18
are contract-verified against the Python baseline — responses **and** the rows
they leave behind — and 231 tests pass. The remaining 50 `/api` routes are
mapped to waves W2–W6 in
[`docs/rust-migration-contract.md`](docs/rust-migration-contract.md) and are
**not** implemented yet. No auth, no AI calls, no frontend change.

| Method | Path | Notes |
| --- | --- | --- |
| `GET` | `/health` | `{"status":"ok","app":…,"ai_enabled":…}` |
| `GET` | `/api/positions` | `Page<PositionRead>`; `include_archived`, `search`, `offset`, `limit` |
| `POST` | `/api/positions` | create → `201 PositionRead`; `name` required, `status` defaults to `Open` |
| `GET` | `/api/positions/{id}` | single `PositionRead`; 404 with the project envelope |
| `PATCH` | `/api/positions/{id}` | partial update; an explicit `null` on `name`/`archived` is **422** (IMP-2) |
| `DELETE` | `/api/positions/{id}` | `204`; linked candidates keep their row and lose the link |
| `GET` | `/api/candidates` | `search`/`status`/`position_id`/`include_archived`; `updated_at DESC` |
| `POST` | `/api/candidates` | create → `201`; name trimmed, blank rejected, position must exist |
| `GET` | `/api/candidates/{id}` | detail with the nested position and the two counters |
| `PATCH` | `/api/candidates/{id}` | partial update; `position_id: null` detaches |
| `DELETE` | `/api/candidates/{id}` | `204`; conversations/messages/send-records cascade |
| `GET` | `/api/candidates/{id}/timeline` | a bare array of `TimelineEntry` (not a `Page`) |
| `GET` | `/api/conversations` | `last_message_at DESC NULLS LAST`, with `message_count` |
| `POST` | `/api/conversations` | open a thread → `201`, audit `conversation.created` |
| `GET` | `/api/messages` | time order; `total` is the real count (IMP-1/IMP-3) |
| `POST` | `/api/messages` | reuse/open the candidate's active conversation; `content_hash` |
| `POST` | `/api/send-records` | append one manual send → `201`, audit `send.recorded` |
| `GET` | `/api/send-records` | `candidate_id`/`task_id`/`draft_id`/`status`; newest send first |

The Python service (`/mnt/o/ica`, @ `d39e321`) is the **contract authority**:
where this repository and the Python code disagree, Python wins until a wave
changes it deliberately. See [`docs/adr/ADR-0002`](docs/adr/ADR-0002-rust-backend-migration-reinstated.md)
for why the migration was reinstated despite ADM-001.

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
state of all eight business tables. It finishes by proving the baseline is
untouched (`git status` empty, HEAD `d39e321`, `ica.db` sha256 unchanged).

Last run: **`113/113 cases matched, 0 failure(s)`**, with `PASS db.<table>` for
`positions`, `candidates`, `conversations`, `messages`, `send_records`,
`follow_up_tasks`, `generated_drafts` and `audit_events` (17 audit events), and
the `improvements:` section asserting IMP-1 (`python total=2 / rust total=5`),
IMP-2 (`python 500 / rust 422` for both NULL cases), IMP-3 (five lists, one
`total` meaning) and IMP-4 (`offset` past `u64::MAX` → 422). The individual
steps are in
[`docs/rust-migration-contract.md`](docs/rust-migration-contract.md) §8.

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

## Known limitations (R0 + W1)

- Routes for W2–W6 are not served: follow-ups, dashboard, drafts/AI,
  imports/OCR/export, settings/audit-events/backups, auth and shares.
- **Tenant/owner scoping is not modelled.** It is a no-op in Python's default
  `local` mode, so both services agree today and every written row carries the
  V1.0 local owner/tenant; do not run the Rust service in `multiuser` mode
  before W6 ships auth.
- `GET /` (the welcome string) is not implemented — it is not part of the API
  surface.
- `422` bodies carry a structurally similar but not identical `details.errors`
  list to pydantic's; `contract_diff.py` compares status, `error.code`,
  `error.message` and the `details` key set (see contract doc §7).
- The three declared improvements in contract doc §7.1 are the **only**
  intentional differences from Python (IMP-1/IMP-2/IMP-3); the comparison fails
  on anything else.
- No packaging, Docker or desktop-shell changes.
