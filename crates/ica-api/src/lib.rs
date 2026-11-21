//! ICA HTTP layer: axum routes, state and the error binding.
//!
//! This crate is the HTTP adapter over the wire contract: the contract itself
//! (request validation, response DTOs, the error vocabulary) lives in
//! `ica-contracts`, and the use cases live in `ica-application` — both are
//! shared with the desktop adapter, so no contract logic is duplicated per
//! transport (Tauri-audit R0.1 §4.3, option C).

pub mod error;
pub mod extract;
pub mod http_parity;
pub mod routes;
pub mod state;

// The contract modules keep their established paths (`crate::dto`, …) inside
// this crate and stay importable as `ica_api::dto` from the outside.
pub use ica_contracts::{body, dto, query};

pub use error::ApiError;
pub use ica_application::AiBudgetSettings;
pub use ica_application::AI_KEY_SLOT;
pub use ica_contracts::error::ValidationIssue;
pub use routes::router;
pub use state::{AppConfig, AppState};
