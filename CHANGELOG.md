# Changelog

All notable changes to this project are documented here. The format follows
Keep a Changelog, and the project adheres to Semantic Versioning. The license
for every release is MIT (see `LICENSE`), matching `license = "MIT"` in
`Cargo.toml`.

## [Unreleased]

No unreleased changes recorded in this file yet.

## [0.1.0] - 2026-11-23

First tagged release line of the Rust backend rewrite. Version, edition,
MSRV, license and repository are centralised in `[workspace.package]`;
`Cargo.lock` is committed.

### Added

- Toolchain pin: `rust-toolchain.toml` fixes `channel = "1.98.0"` with the
  `rustfmt` and `clippy` components and the `minimal` profile, so every gate
  runs on the compiler the baseline was measured with.
- Workspace of seven crates: `ica-domain`, `ica-application`, `ica-contracts`,
  `ica-infra`, `ica-api`, `ica-desktop`, `ica-server`, with a shared
  `[workspace.dependencies]` and `[workspace.lints]` inherited through
  `[lints] workspace = true` (SPE-BAS-002 §5.2/§8.5).
- Dependency and supply-chain policy: `deny.toml` (licenses / advisories /
  bans) plus a `cargo deny check` stage and a `security_scan` PASS merge
  blocker in `.github/workflows/quality-gate.yml` (§13.4/§15.3).
- Error handling: the core `ApplicationError` and `RepositoryError` types
  derive `thiserror::Error`; the `#[from]` bridge preserves their source chain
  while Display text and wire codes stay byte-identical (§10.1).
- Project metadata: MIT `LICENSE` and this changelog; `license` and
  `repository` inherited by all members (§5.2).
- Startup guard: `ica-server` refuses a non-loopback bind host at startup,
  with a unit test covering both the accepted and rejected cases (§13.1).

### Changed

- Wave R0 established the contract-verified positions read slice.
- Wave W1 added the message-list improvements (real totals, explicit-null
  422, consistent pagination) and the range/boundary gate cases.
- Wave W1.5 aligned route and query semantics with the frozen baseline;
  Wave W1.6 made the gate scope-sensitive (off-scope routes excluded by exact
  `(method, path)` plus drift guards).
- Wave W2 extended endpoint coverage; Wave W2.5 moved the transport-neutral
  contract source into `ica-contracts` and added the `ica-desktop` command
  layer (contract increment 0 — a pure refactor).
- Wave W3 delivered the drafts 8 + draft-groups 2 endpoints with cross-language
  capture; Wave W4 delivered the seven import/export/OCR endpoints with
  cross-language parity, including the single-transaction write ordering.
- Later slices added settings/AI-provider, auth, browser-extraction confirm,
  backup, OCR (RapidOCR PP-OCRv6 through `rapidocr-core`) and the Tauri IPC
  command layer; the desktop Tauri host remains a separate workspace and is
  not type-checked on Linux hosts without glib/WebKit.
- Rust commands in `scripts/check.py` now use `--locked` so CI and local runs
  resolve exactly the committed lockfile (§13.4).

[Unreleased]: https://github.com/BryanTianjx/rica/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/BryanTianjx/rica/releases/tag/v0.1.0
