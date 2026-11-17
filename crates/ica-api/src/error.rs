//! Error binding: `ica_contracts::error::ContractError` → axum's HTTP surface.
//!
//! The error **data** lives in `ica-contracts` (transport-neutral, shared with
//! `ica-desktop`); this module owns only the HTTP rendering — the status-code
//! mapping and the unified JSON envelope. Every error the API returns has one
//! shape, so the frontend has a single error path:
//!
//! ```json
//! {"error": {"code": "not_found", "message": "...", "details": {}}}
//! ```
//!
//! Status mapping, matching the Python handlers:
//!
//! | Status | `code` | Source |
//! | --- | --- | --- |
//! | 422 | `validation_error` | pydantic/query validation, and the business 422s (a conversation that belongs to another candidate) |
//! | 404 | `not_found` | `NotFoundError` |
//! | 409 | `business_rule_violation` | `BusinessRuleError` |
//!
//! The 500 body is deliberately generic; the real cause is logged, never sent
//! to the client (it can carry SQL or filesystem details).

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use std::ops::Deref;

use ica_application::ApplicationError;
use ica_contracts::error::ContractError;

/// The HTTP face of a [`ContractError`]: a newtype so the axum
/// `IntoResponse` implementation can live in this crate without the contract
/// crate knowing about HTTP.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError(pub ContractError);

impl ApiError {
    /// The envelope an unknown path produces, matching Starlette's default
    /// `HTTPException(404, "Not Found")` through the project's handler.
    pub fn unknown_path() -> Self {
        Self(ContractError::unknown_path())
    }
}

impl Deref for ApiError {
    type Target = ContractError;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<ApplicationError> for ApiError {
    fn from(error: ApplicationError) -> Self {
        Self(error.into())
    }
}

impl From<ContractError> for ApiError {
    fn from(error: ContractError) -> Self {
        Self(error)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message, details) = match self.0 {
            ContractError::NotFound { message } => {
                (StatusCode::NOT_FOUND, "not_found", message, json!({}))
            }
            ContractError::ValidationIssues { issues } => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "validation_error",
                "Request payload failed validation".to_string(),
                json!({ "errors": issues }),
            ),
            ContractError::Validation { message, details } => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "validation_error",
                message,
                details,
            ),
            ContractError::Conflict { message, details } => (
                StatusCode::CONFLICT,
                "business_rule_violation",
                message,
                details,
            ),
            ContractError::Http { status, message } => (
                StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                "http_error",
                message,
                json!({}),
            ),
            ContractError::Internal { message } => {
                // Log the cause; answer with the generic baseline text.
                tracing::error!("request failed: {message}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "An unexpected error occurred".to_string(),
                    json!({}),
                )
            }
        };
        (
            status,
            Json(json!({
                "error": {"code": code, "message": message, "details": details}
            })),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_newtype_binds_application_errors_to_http_statuses() {
        let response =
            ApiError::from(ApplicationError::not_found("Position", "p-1")).into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let response = ApiError::from(ApplicationError::Unavailable {
            message: "sql said no".to_string(),
        })
        .into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn a_state_machine_conflict_renders_its_json_details() {
        let error = ApplicationError::conflict_value(
            "cannot set directly",
            json!({ "settable": ["Cancelled", "Drafting"] }),
        );
        let response = ApiError::from(error).into_response();
        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    #[test]
    fn deref_exposes_the_contract_constructors() {
        let error = ApiError::from(ContractError::not_found("x"));
        assert_eq!(error.code(), "not_found");
    }
}
