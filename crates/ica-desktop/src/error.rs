//! The IPC error object: the contract error rendered for Tauri.
//!
//! IPC has no status codes, so the adapter renders the contract error as the
//! same envelope shape the HTTP side emits — `{"error": {"code", "message",
//! "details"}}` — letting the frontend keep one error-parsing path across
//! transports. The 500 body stays deliberately generic; the real cause is
//! logged, never echoed.

use serde::Serialize;

use ica_contracts::error::ContractError;

/// The `Result` error type of every command in this crate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IpcError {
    pub error: IpcErrorBody,
}

/// The envelope body, field-for-field the HTTP side's `error` object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IpcErrorBody {
    pub code: String,
    pub message: String,
    pub details: serde_json::Value,
}

impl IpcError {
    pub fn from_contract(error: ContractError) -> Self {
        if let ContractError::Internal { message } = &error {
            // Log the cause; answer with the generic baseline text.
            tracing::error!("command failed: {message}");
        }
        Self {
            error: IpcErrorBody {
                code: error.code().to_string(),
                message: error.wire_message().to_string(),
                details: error.wire_details(),
            },
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
