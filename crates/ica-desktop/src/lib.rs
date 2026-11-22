//! ICA desktop adapter: the Tauri IPC command layer over the same contract
//! and use cases as `ica-api`.
//!
//! Nothing here knows about HTTP, and nothing here knows about the Tauri
//! runtime: a command is a plain async function taking [`crate::state::DesktopState`]
//! plus typed/JSON arguments and returning a DTO or an [`crate::error::IpcError`].
//! The shell host wraps each function with `#[tauri::command]` and registers
//! it on the builder — the adapter logic (contract validation → use case →
//! error envelope) lives here, once, shared by every surface.
//!
//! Layering: this crate may depend on `ica-application` (use cases) and
//! `ica-infra` (the SQLite adapter it connects), plus `ica-contracts` for the
//! wire shapes — never on `ica-api`, and the inner layers never depend on it.

pub mod browser;
pub mod commands;
pub mod diagnostics;
pub mod error;
pub mod ipc_allowlist;
pub mod processes;
pub mod proxy;
pub mod routing;
pub mod runtime_policy;
pub mod sidecar;
pub mod state;

pub use error::{IpcError, IpcErrorBody};
// SEC-01: the desktop host obtains its JWT signing key exclusively through
// this re-exported infrastructure component (CSPRNG + secure persistence).
pub use ica_infra::jwt_key;
pub use state::{DesktopConfig, DesktopState};
