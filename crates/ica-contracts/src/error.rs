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
///
/// `loc` and `input` keep pydantic's JSON types, not strings: an array index
/// is a NUMBER in `loc` and the offending value keeps its JSON type (a string
/// stays `"oops"`, a number stays `1`, an object stays an object). Rendering
/// both as strings used to double-encode the value (`input: "\"oops\""`) and
/// turn every index into text — measured by the independent audit as a
/// contract-visible difference on every body-validation 422.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ValidationIssue {
    #[serde(rename = "type")]
    pub kind: String,
    /// Where the bad value came from, e.g. `["query", "limit"]` or
    /// `["body", "data", "candidates", 0]` — strings for names, numbers for
    /// array indices.
    pub loc: Vec<Value>,
    pub msg: String,
    /// The offending value as it arrived, JSON type included.
    pub input: Value,
}

impl ValidationIssue {
    pub fn new(kind: &str, loc: Vec<Value>, msg: &str, input: Value) -> Self {
        Self {
            kind: kind.to_string(),
            loc,
            msg: msg.to_string(),
            input,
        }
    }

    /// The URL/query-side shape: every `loc` entry and the input are plain
    /// strings there (pydantic renders query inputs as strings too).
    pub fn query(kind: &str, loc: Vec<&str>, msg: &str, input: &str) -> Self {
        Self {
            kind: kind.to_string(),
            loc: loc
                .into_iter()
                .map(|part| Value::String(part.into()))
                .collect(),
            msg: msg.to_string(),
            input: Value::String(input.to_string()),
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

/// The wire vocabulary every transport renders (§9.2). Keep the codes in one
/// place so HTTP, IPC and the sidecar cannot drift apart; adapters must use
/// [`ContractError::envelope`] rather than rebuilding the object.
pub mod code {
    pub const NOT_FOUND: &str = "not_found";
    pub const VALIDATION: &str = "validation_error";
    pub const BUSINESS_RULE: &str = "business_rule_violation";
    pub const HTTP: &str = "http_error";
    pub const INTERNAL: &str = "internal_error";
    pub const PROVIDER: &str = "ai_provider_error";
    pub const UNAUTHORIZED: &str = "unauthorized";
}

/// Sidecar replies use a line protocol, but their `code` comes from the same
/// §9.2 vocabulary as HTTP/IPC. The worker stays alive after an error and
/// keeps its legacy `error` text field; `code` is additive.
pub mod sidecar {
    use super::code;

    /// Classify a worker error message into the shared business-error code.
    pub fn code_for_error_message(message: &str) -> &'static str {
        if message.starts_with("bad command json") {
            code::VALIDATION
        } else {
            code::INTERNAL
        }
    }
}

/// The `error` object both adapters put on the wire, byte-for-byte identical.
/// HTTP wraps it in the JSON response; IPC puts it in `IpcError.error`; the
/// sidecar reuses the same code constants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ErrorEnvelope {
    pub code: String,
    pub message: String,
    pub details: Value,
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
    /// Any status with a caller-chosen code (`ocr_unavailable` is a 503,
    /// `ocr_image_missing` / `ocr_error` are 409s).
    StatusWithCode {
        status: u16,
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
            Self::NotFound { .. } => code::NOT_FOUND,
            Self::ValidationIssues { .. } | Self::Validation { .. } => code::VALIDATION,
            Self::ValidationWithCode { code, .. } => code,
            Self::StatusWithCode { code, .. } => code,
            Self::Conflict { .. } => code::BUSINESS_RULE,
            // The caller chose this code (`ocr_path_not_allowed`, …), so it is
            // not a `&'static str` literal.
            Self::ConflictWithCode { code, .. } => code,
            Self::ProviderUnavailable { .. } => code::PROVIDER,
            Self::Http { .. } => code::HTTP,
            Self::Internal { .. } => code::INTERNAL,
        }
    }

    /// The canonical HTTP status for this business error. HTTP uses it
    /// directly; IPC has no status line but the adapter contract test asserts
    /// both transports render the same code/details/message for every input.
    pub fn http_status(&self) -> u16 {
        match self {
            Self::NotFound { .. } => 404,
            Self::ValidationIssues { .. } | Self::Validation { .. } => 422,
            Self::ValidationWithCode { .. } => 422,
            Self::Conflict { .. } | Self::ConflictWithCode { .. } => 409,
            Self::StatusWithCode { status, .. } => *status,
            Self::ProviderUnavailable { .. } => 503,
            Self::Http { status, .. } => *status,
            Self::Internal { .. } => 500,
        }
    }

    /// The single shared envelope builder. Both adapters must call this so a
    /// fix to one code or message cannot leave the other transport stale.
    pub fn envelope(&self) -> ErrorEnvelope {
        ErrorEnvelope {
            code: self.code().to_string(),
            message: self.wire_message().to_string(),
            details: self.wire_details(),
        }
    }

    /// The details object this error carries on the wire.
    pub fn wire_details(&self) -> Value {
        match self {
            Self::ValidationIssues { issues } => serde_json::json!({ "errors": issues }),
            Self::Validation { details, .. } | Self::Conflict { details, .. } => details.clone(),
            Self::ConflictWithCode { details, .. } => details.clone(),
            Self::ValidationWithCode { details, .. } => details.clone(),
            Self::StatusWithCode { details, .. } => details.clone(),
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
            | Self::StatusWithCode { message, .. }
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
            ApplicationError::StatusWithCode {
                status,
                code,
                message,
                details,
            } => ContractError::StatusWithCode {
                status,
                code,
                message,
                details,
            },
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
            // `#[from]` bridge introduced by the thiserror migration: render
            // exactly as the old explicit `application_error()` mapping did.
            ApplicationError::Persistence(source) => Self::from(source.application_error()),
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
    fn application_and_contract_error_vocabularies_agree() {
        // AC4: one business error vocabulary, from the use case to every wire
        // adapter. A drift in either mapping turns this test red.
        let mut details = BTreeMap::new();
        details.insert("k".to_string(), "v".to_string());
        let cases = vec![
            ApplicationError::not_found("Position", "p-1"),
            ApplicationError::validation("bad", details.clone()),
            ApplicationError::conflict("nope", details.clone()),
            ApplicationError::conflict_value("nope", serde_json::json!({"settable": []})),
            ApplicationError::ValidationWithCode {
                code: "invalid_encryption_key",
                message: "bad key".to_string(),
                details: serde_json::json!({}),
            },
            ApplicationError::StatusWithCode {
                status: 503,
                code: "ocr_unavailable",
                message: "no engine".to_string(),
                details: serde_json::json!({}),
            },
            ApplicationError::ConflictWithCode {
                code: "ocr_path_not_allowed",
                message: "outside".to_string(),
                details: serde_json::json!({}),
            },
            ApplicationError::ProviderUnavailable {
                message: "provider down".to_string(),
                details: serde_json::json!({}),
            },
            ApplicationError::Unavailable {
                message: "db said no".to_string(),
            },
        ];
        for error in cases {
            let expected_code = error.code().to_string();
            let expected_status = error.http_status();
            let contract: ContractError = error.into();
            assert_eq!(contract.code(), expected_code);
            assert_eq!(contract.http_status(), expected_status);
            assert_eq!(contract.envelope().code, expected_code);
        }
    }

    #[test]
    fn the_sidecar_classifier_only_emits_shared_codes() {
        assert_eq!(
            sidecar::code_for_error_message("bad command json: oops"),
            code::VALIDATION
        );
        assert_eq!(
            sidecar::code_for_error_message("cdp send failed"),
            code::INTERNAL
        );
        for code_value in [
            sidecar::code_for_error_message("bad command json: x"),
            sidecar::code_for_error_message("anything else"),
        ] {
            assert!(matches!(
                code_value,
                code::VALIDATION
                    | code::INTERNAL
                    | code::NOT_FOUND
                    | code::BUSINESS_RULE
                    | code::HTTP
                    | code::PROVIDER
                    | code::UNAUTHORIZED
            ));
        }
    }

    #[test]
    fn the_envelope_has_exactly_the_shared_error_shape() {
        let envelope = ContractError::not_found("Position 'p' was not found").envelope();
        assert_eq!(
            serde_json::to_value(&envelope).expect("serialise"),
            serde_json::json!({
                "code": "not_found",
                "message": "Position 'p' was not found",
                "details": {}
            })
        );
        // A 500 never echoes the real cause on any transport.
        let envelope = ContractError::internal("sql: no such column").envelope();
        assert_eq!(envelope.code, code::INTERNAL);
        assert_eq!(envelope.message, "An unexpected error occurred");
        assert_eq!(envelope.details, serde_json::json!({}));
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
