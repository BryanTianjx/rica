//! The IPC error object: the contract error rendered for Tauri.
//!
//! IPC has no status codes, so the adapter renders the contract error as the
//! same envelope shape the HTTP side emits — `{"error": {"code", "message",
//! "details"}}` — letting the frontend keep one error-parsing path across
//! transports. The 500 body stays deliberately generic; the real cause is
//! logged, never echoed.

use serde::Serialize;

use ica_contracts::error::ContractError;

/// The `Result` error type of every command in this crate. Its body is the
/// exact `ErrorEnvelope` the HTTP adapter renders, so one error-parsing path
/// serves both transports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IpcError {
    pub error: IpcErrorBody,
}

/// The envelope body, field-for-field the HTTP side's `error` object.
pub type IpcErrorBody = ica_contracts::error::ErrorEnvelope;

impl IpcError {
    pub fn from_contract(error: ContractError) -> Self {
        if let ContractError::Internal { message } = &error {
            // Log the cause; answer with the generic baseline text.
            tracing::error!("command failed: {message}");
        }
        Self {
            error: error.envelope(),
        }
    }

    /// The `unknown path` analogue: an unknown command target.
    pub fn unknown_command() -> Self {
        Self::from_contract(ContractError::unknown_path())
    }
}

impl From<ContractError> for IpcError {
    fn from(error: ContractError) -> Self {
        Self::from_contract(error)
    }
}

impl From<ica_application::ApplicationError> for IpcError {
    fn from(error: ica_application::ApplicationError) -> Self {
        Self::from_contract(error.into())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_sidecar_uses_the_same_error_code_vocabulary() {
        // The worker is a separate binary; a source-level check keeps it on
        // the shared classifier even though no Chromium runs on this host.
        const WORKER: &str = include_str!("bin/ica-browser-worker.rs");
        assert!(WORKER.contains("ica_contracts::error::sidecar::code_for_error_message"));
        assert!(WORKER.contains("\"code\": code"));
    }
}
