# ADR-0002 — Rust backend migration reinstated (ICA Personal)

- Status: Accepted (overrides three clauses of ADM-001 for this repository only)
- Date: 2026-11-17
- Supersedes: ADM-001 §1.7, §3.2 (item 6), §20.3 — for the backend HTTP/data plane only
- Approval: work item `wi_31c11ba4f37644bc`, approval `apr_6f65829a96524475`
  (bryan, 2026-11-17T00:39:18Z), actions WORKSPACE_WRITE / CREATE_TICKET /
  ASSIGN_AGENT / DISPATCH_EXECUTION / GIT_LOCAL_COMMIT
- Repository: `/mnt/o/ICA-RUST` (new, separate from the ICA main repo)
- Related: `docs/rust-migration-contract.md` (endpoint map, waves, coexistence)

## 1. Context

`ICA-ARCH-ADM-001` (`docs/architecture/local-admin-cloud-management-architecture-r1.md`
in the ICA main repo, status **Proposed**) forbids a Python → Rust backend
rewrite in three separate places. The exact wording, so this decision can be
audited against the source:

| Clause | File:line | Text (verbatim) |
| --- | --- | --- |
| §1.7 | `local-admin-cloud-management-architecture-r1.md:30` | 「当前继续使用 Tauri 2 + React/TypeScript + Python/FastAPI + SQLite 模块化单体；不实施 Python → Rust 后端迁移，不提前拆分微服务。」 |
| §3.2 | `local-admin-cloud-management-architecture-r1.md:99` | 「Python 后端迁移到 Rust。」（列为 R1.0 非目标之一） |
| §20.3 | `local-admin-cloud-management-architecture-r1.md:689` | 「继续采用 Tauri 2 + React/TypeScript + Python/FastAPI + SQLite 本地核心。云端管理服务是未来新增部署面，不要求替换桌面后端，也不构成 Python → Rust 迁移理由。」 |

Those clauses were written to stop an *unplanned* rewrite: they protect the
V1.0 freeze, they do not argue that the Python backend is a better technical
choice. Two facts have changed since:

1. The user explicitly reversed the direction, with a recorded approval
   (`apr_6f65829a96524475`) that names this repository and authorises local
   commits only.
2. The migration is now planned as a wave sequence with a verifiable contract
   (`docs/rust-migration-contract.md`): the Python service stays the authority
   for behaviour until each wave's endpoint set has been compared against it.

## 2. Decision

Build the ICA Personal backend HTTP/data plane in Rust, in a **separate
repository** (`/mnt/o/ICA-RUST`), and migrate endpoint by endpoint behind a
byte-comparable contract, instead of rewriting in place or not at all.

Scope of the override — the three clauses above are superseded **only** for the
backend HTTP/data plane of ICA Personal:

- **In scope**: FastAPI HTTP surface, application services, SQLite access layer,
  server process configuration.
- **Out of scope**: the Tauri 2 desktop shell, the React/TypeScript frontend
  (kept as-is; the Rust API must satisfy its existing contract), the V1.0 frozen
  requirements, the data model, and the cloud-management architecture that
  ADM-001 actually proposed.
- **Unaffected**: ADM-001's other decisions (ADM-002, ADM-003, IAM-001, IAM-002).

Nothing in this ADR changes the ICA main repository. The ADR lives here, not
there; landing it in the main repo is a separate work item that needs its own
approval.

## 3. Consequences

### Positive

- The contract is externalised: `docs/contract-baseline/positions-python.json`
  is captured from a running Python server, so "it works" is a diff, not an
  opinion.
- The layering rule is compile-enforced rather than review-enforced:
  `ica-domain` and `ica-application` cannot depend on axum/sqlx/tokio/hyper, so
  domain logic cannot drift into the transport.
- SQLite schema reuse means no data migration: the Rust service opens the
  existing database (read-only in R0).

### Negative / accepted costs

- Two backends exist during the transition. Every changed endpoint has to be
  verified twice until W7.
- The Rust service must reproduce Python quirks that a clean-room design would
  not choose — for example pydantic's timestamp rendering (`...T09:00:00Z`, six
  fractional digits only when non-zero) and FastAPI's `422` envelope.
- ADM-001 is a *Proposed* document; this ADR is the record that parts of it no
  longer describe the intended architecture.

### Neutral

- R0 implements three read-only endpoints. The remaining 65 `/api` routes are
  mapped to waves W1–W7 with their own acceptance criteria.

## 4. Review conditions

Re-open this ADR if any of the following becomes true:

1. A wave's contract diff cannot be made to pass without changing the Python
   behaviour (that would mean the "contract" was wrong, not the port).
2. The dual-run overhead exceeds the migration benefit — e.g. two waves in a
   row produce no endpoint that can be cut over.
3. ADM-001 is revised to allow (or again forbid) the migration; this ADR then
   either becomes redundant or must be re-argued.
4. The frontend needs changes to talk to the Rust backend. D3 says the contract
   is compatible; needing frontend edits invalidates that assumption and must
   be re-approved.
5. The Rust service cannot preserve the read-only guarantee on the baseline
   database (see `docs/rust-migration-contract.md` §7).
