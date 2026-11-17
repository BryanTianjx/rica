//! Error mapping: application errors → HTTP status + the unified envelope.
//!
//! Every error the API returns has one shape, so the frontend has a single
//! error path:
//!
//! ```json
//! {"error": {"code": "not_found", "message": "...", "details": {}}}
//! ```
//!
//! The 500 body is deliberately generic; the real cause is logged, never sent
//! to the client (it can carry SQL or filesystem details).

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use serde_json::json;

use ica_application::ApplicationError;

/// One entry of `details.errors` in a 422 body.
///
/// The shape follows pydantic's error list closely enough for a client that
/// renders validation messages; the exact `type`/`msg` text is
/// implementation-specific and is not part of the compared contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ValidationIssue {
    #[serde(rename = "type")]
    pub kind: String,
    /// Where the bad value came from, e.g. `["query", "limit"]`.
    pub loc: Vec<String>,
    pub msg: String,
    /// The offending value, rendered as text (query values are strings).
    pub input: String,
}

impl ValidationIssue {
    pub fn new(kind: &str, loc: Vec<&str>, msg: &str, input: &str) -> Self {
        Self {
            kind: kind.to_string(),
            loc: loc.into_iter().map(str::to_string).collect(),
            msg: msg.to_string(),
            input: input.to_string(),
        }
    }
}

/// A request the API refuses to serve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiError {
    /// 404 with the baseline message (`Position 'x' was not found`).
    NotFound { message: String },
    /// 422 from query validation.
    Validation { issues: Vec<ValidationIssue> },
    /// A transport-level error such as an unknown path.
    Http { status: u16, message: String },
    /// 500: the cause is logged, never echoed.
    Internal { message: String },
}

impl ApiError {
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::NotFound {
            message: message.into(),
        }
    }

    pub fn validation(issue: ValidationIssue) -> Self {
        Self::Validation {
            issues: vec![issue],
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal {
            message: message.into(),
        }
    }

    /// The envelope an unknown path produces, matching Starlette's default
    /// `HTTPException(404, "Not Found")` through the project's handler.
    pub fn unknown_path() -> Self {
        Self::Http {
            status: 404,
            message: "Not Found".to_string(),
        }
    }
}

impl From<ApplicationError> for ApiError {
    fn from(error: ApplicationError) -> Self {
        match error {
            ApplicationError::NotFound { message } => ApiError::NotFound { message },
            ApplicationError::Unavailable { message } => ApiError::Internal { message },
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, body) = match self {
            ApiError::NotFound { message } => (
                StatusCode::NOT_FOUND,
                json!({
                    "error": {"code": "not_found", "message": message, "details": {}}
                }),
            ),
            ApiError::Validation { issues } => (
                StatusCode::UNPROCESSABLE_ENTITY,
                json!({
                    "error": {
                        "code": "validation_error",
                        "message": "Request payload failed validation",
                        "details": {"errors": issues}
                    }
                }),
            ),
            ApiError::Http { status, message } => (
                StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                json!({
                    "error": {"code": "http_error", "message": message, "details": {}}
                }),
            ),
            ApiError::Internal { message } => {
                // Log the cause; answer with the generic baseline text.
                tracing::error!("request failed: {message}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    json!({
                        "error": {
                            "code": "internal_error",
                            "message": "An unexpected error occurred",
                            "details": {}
                        }
                    }),
                )
            }
        };
        (status, Json(body)).into_response()
    }
}
