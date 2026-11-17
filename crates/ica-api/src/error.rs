//! Error mapping: application errors → HTTP status + the unified envelope.
//!
//! Every error the API returns has one shape, so the frontend has a single
//! error path:
//!
//! ```json
//! {"error": {"code": "not_found", "message": "...", "details": {}}}
//! ```
//!
//! Three statuses are produced here, matching the Python handlers:
//!
//! | Status | `code` | Source |
//! | --- | --- | --- |
//! | 422 | `validation_error` | pydantic/query validation, and the business 422s (a conversation that belongs to another candidate) |
//! | 404 | `not_found` | `NotFoundError` |
//! | 409 | `business_rule_violation` | `BusinessRuleError` |
//!
//! The 500 body is deliberately generic; the real cause is logged, never sent
//! to the client (it can carry SQL or filesystem details).

use std::collections::BTreeMap;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use serde_json::{json, Map, Value};

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

/// Render an application `details` map as a JSON object.
fn details_value(details: &BTreeMap<String, String>) -> Value {
    let mut map = Map::new();
    for (key, value) in details {
        map.insert(key.clone(), Value::String(value.clone()));
    }
    Value::Object(map)
}

/// A request the API refuses to serve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiError {
    /// 404 with the baseline message (`Position 'x' was not found`).
    NotFound { message: String },
    /// 422 with a pydantic-shaped `details.errors` list.
    ValidationIssues { issues: Vec<ValidationIssue> },
    /// 422 raised by a business rule, with arbitrary `details`.
    Validation { message: String, details: Value },
    /// 409 `business_rule_violation`.
    Conflict { message: String, details: Value },
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

    /// One schema/query validation failure.
    pub fn validation(issue: ValidationIssue) -> Self {
        Self::ValidationIssues {
            issues: vec![issue],
        }
    }

    /// Every schema/query validation failure found, like pydantic.
    pub fn validation_issues(issues: Vec<ValidationIssue>) -> Self {
        Self::ValidationIssues { issues }
    }

    /// A 422 that carries identifiers rather than a field-error list.
    pub fn business_validation(
        message: impl Into<String>,
        details: BTreeMap<String, String>,
    ) -> Self {
        Self::Validation {
            message: message.into(),
            details: details_value(&details),
        }
    }

    /// A 409 with the identifiers the client needs to repair the request.
    pub fn conflict(message: impl Into<String>, details: BTreeMap<String, String>) -> Self {
        Self::Conflict {
            message: message.into(),
            details: details_value(&details),
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
            ApplicationError::Validation { message, details } => ApiError::Validation {
                message,
                details: details_value(&details),
            },
            ApplicationError::Conflict { message, details } => ApiError::Conflict {
                message,
                details: details_value(&details),
            },
            // The state machine's 409: `details` is already free-form JSON
            // (it carries the settable-status list as an array).
            ApplicationError::ConflictValue { message, details } => {
                ApiError::Conflict { message, details }
            }
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
            ApiError::ValidationIssues { issues } => (
                StatusCode::UNPROCESSABLE_ENTITY,
                json!({
                    "error": {
                        "code": "validation_error",
                        "message": "Request payload failed validation",
                        "details": {"errors": issues}
                    }
                }),
            ),
            ApiError::Validation { message, details } => (
                StatusCode::UNPROCESSABLE_ENTITY,
                json!({
                    "error": {"code": "validation_error", "message": message, "details": details}
                }),
            ),
            ApiError::Conflict { message, details } => (
                StatusCode::CONFLICT,
                json!({
                    "error": {
                        "code": "business_rule_violation",
                        "message": message,
                        "details": details
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_business_validation_renders_its_identifiers_not_an_error_list() {
        let mut details = BTreeMap::new();
        details.insert("conversation_id".to_string(), "c-1".to_string());
        let error = ApiError::business_validation("Conversation does not belong", details);
        let ApiError::Validation { details, .. } = &error else {
            panic!("expected a 422");
        };
        assert_eq!(details["conversation_id"], "c-1");
    }

    #[test]
    fn a_conflict_keeps_the_baseline_code() {
        let error = ApiError::conflict("nope", BTreeMap::new());
        let ApiError::Conflict { message, .. } = &error else {
            panic!("expected a 409");
        };
        assert_eq!(message, "nope");
    }
}
