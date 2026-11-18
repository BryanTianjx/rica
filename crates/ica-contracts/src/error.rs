//! The error vocabulary every adapter renders.
//!
//! The data is transport-neutral: variants carry exactly what the wire needs
//! (message + details), never a status code. Binding to a transport happens
//! in the adapter — `ica-api` maps these to HTTP statuses and the unified
//! JSON envelope; `ica-desktop` maps them to the IPC error object. The 500
//! body is deliberately generic in every transport; the real cause is logged,
//! never sent to the client (it can carry SQL or filesystem details).
//!
//! ```json
//! {"error": {"code": "not_found", "message": "...", "details": {}}}
//! ```
//!
//! Codes: `not_found` / `validation_error` / `business_rule_violation` /
//! `http_error` / `internal_error`.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::{Map, Value};

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

/// A request the adapter refuses to serve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractError {
    /// 404 with the baseline message (`Position 'x' was not found`).
    NotFound { message: String },
    /// 422 with a pydantic-shaped `details.errors` list.
    ValidationIssues { issues: Vec<ValidationIssue> },
    /// 422 raised by a business rule, with arbitrary `details`.
    Validation { message: String, details: Value },
    /// 409 `business_rule_violation`.
    Conflict { message: String, details: Value },
    /// 422 with a caller-chosen code (`invalid_encryption_key`, …). The
    /// baseline's `ValidationError` accepts `code=`.
    ValidationWithCode {
        code: &'static str,
        message: String,
        details: Value,
    },
    /// 409 with a caller-chosen code (`ocr_path_not_allowed`,
    /// `ocr_image_undecodable`, …). The baseline's `BusinessRuleError` accepts
    /// `code=`, so the code is part of the contract.
    ConflictWithCode {
        code: &'static str,
        message: String,
        details: Value,
    },
    /// 503 `ai_provider_error` — the upstream AI provider failed or is not
    /// configured (the baseline's `AIProviderError`).
    ProviderUnavailable { message: String, details: Value },
    /// A transport-level error such as an unknown path (HTTP) or an unknown
    /// command target (IPC).
    Http { status: u16, message: String },
    /// 500: the cause is logged, never echoed.
    Internal { message: String },
}

impl ContractError {
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

    /// The wire vocabulary for this error — the `error.code` field every
    /// transport renders. The HTTP status codes are an adapter concern.
    pub fn code(&self) -> &str {
        match self {
            Self::NotFound { .. } => "not_found",
            Self::ValidationIssues { .. } | Self::Validation { .. } => "validation_error",
            Self::ValidationWithCode { code, .. } => code,
            Self::Conflict { .. } => "business_rule_violation",
            // The caller chose this code (`ocr_path_not_allowed`, …), so it is
            // not a `&'static str` literal.
            Self::ConflictWithCode { code, .. } => code,
            Self::ProviderUnavailable { .. } => "ai_provider_error",
            Self::Http { .. } => "http_error",
            Self::Internal { .. } => "internal_error",
        }
    }

    /// The details object this error carries on the wire.
    pub fn wire_details(&self) -> Value {
        match self {
            Self::ValidationIssues { issues } => serde_json::json!({ "errors": issues }),
            Self::Validation { details, .. } | Self::Conflict { details, .. } => details.clone(),
            Self::ConflictWithCode { details, .. } => details.clone(),
            Self::ValidationWithCode { details, .. } => details.clone(),
            Self::ProviderUnavailable { details, .. } => details.clone(),
            Self::NotFound { .. } | Self::Http { .. } | Self::Internal { .. } => {
                serde_json::json!({})
            }
        }
    }

    /// The wire message. Note the 500 is deliberately generic; the real cause
    /// is logged by the adapter, never echoed (it can carry SQL or filesystem
    /// details).
    pub fn wire_message(&self) -> &str {
        match self {
            Self::NotFound { message }
            | Self::Validation { message, .. }
            | Self::Conflict { message, .. }
            | Self::ConflictWithCode { message, .. }
            | Self::ValidationWithCode { message, .. }
            | Self::ProviderUnavailable { message, .. }
            | Self::Http { message, .. } => message,
            Self::ValidationIssues { .. } => "Request payload failed validation",
            Self::Internal { .. } => "An unexpected error occurred",
        }
    }
}

impl From<ApplicationError> for ContractError {
    fn from(error: ApplicationError) -> Self {
        match error {
            ApplicationError::NotFound { message } => ContractError::NotFound { message },
            ApplicationError::Validation { message, details } => ContractError::Validation {
                message,
                details: details_value(&details),
            },
            ApplicationError::Conflict { message, details } => ContractError::Conflict {
                message,
                details: details_value(&details),
            },
            // The state machine's 409: `details` is already free-form JSON
            // (it carries the settable-status list as an array).
            ApplicationError::ConflictValue { message, details } => {
                ContractError::Conflict { message, details }
            }
            ApplicationError::ValidationWithCode {
                code,
                message,
                details,
            } => ContractError::ValidationWithCode {
                code,
                message,
                details,
            },
            ApplicationError::ConflictWithCode {
                code,
                message,
                details,
            } => ContractError::ConflictWithCode {
                code,
                message,
                details,
            },
            // The AI provider's 503: same free-form details shape.
            ApplicationError::ProviderUnavailable { message, details } => {
                ContractError::ProviderUnavailable { message, details }
            }
            ApplicationError::Unavailable { message } => ContractError::Internal { message },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_business_validation_renders_its_identifiers_not_an_error_list() {
        let mut details = BTreeMap::new();
        details.insert("conversation_id".to_string(), "c-1".to_string());
        let error = ContractError::business_validation("Conversation does not belong", details);
        let ContractError::Validation { details, .. } = &error else {
            panic!("expected a 422");
        };
        assert_eq!(details["conversation_id"], "c-1");
    }

    #[test]
    fn a_conflict_keeps_the_baseline_code() {
        let error = ContractError::conflict("nope", BTreeMap::new());
        let ContractError::Conflict { message, .. } = &error else {
            panic!("expected a 409");
        };
        assert_eq!(message, "nope");
    }

    #[test]
    fn the_wire_vocabulary_matches_the_baseline_codes() {
        assert_eq!(ContractError::not_found("x").code(), "not_found");
        assert_eq!(
            ContractError::validation_issues(vec![]).code(),
            "validation_error"
        );
        assert_eq!(
            ContractError::conflict("x", BTreeMap::new()).code(),
            "business_rule_violation"
        );
        assert_eq!(ContractError::unknown_path().code(), "http_error");
        assert_eq!(ContractError::internal("x").code(), "internal_error");
        assert_eq!(
            ContractError::internal("real cause").wire_message(),
            "An unexpected error occurred"
        );
    }
}
