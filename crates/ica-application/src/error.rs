//! Errors raised by use cases.
//!
//! Kept free of HTTP concepts: the mapping to status codes and to the JSON
//! error envelope belongs to ``ica-api``. The message text is part of the
//! contract though — the Python baseline renders it verbatim in the
//! ``error.message`` field, and ``details`` is compared key-for-key.
//!
//! SPE-BAS-002 §10.1: both core error types derive `thiserror::Error`.
//! Every variant's `Display` is the **same message string** the hand-written
//! impls produced; `code()`/`http_status()` remain the only wire mapping.

use std::collections::BTreeMap;

use serde_json::Value;
use thiserror::Error;

use crate::ports::RepositoryError;

/// Why a use case could not produce a result.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum ApplicationError {
    /// The requested entity does not exist (HTTP 404).
    #[error("{message}")]
    NotFound { message: String },
    /// The request is well-formed but describes an impossible relation, e.g. a
    /// conversation that belongs to another candidate (HTTP 422
    /// ``validation_error``, carrying ``details``).
    #[error("{message}")]
    Validation {
        message: String,
        details: BTreeMap<String, String>,
    },
    /// A domain rule forbids the requested action (HTTP 409
    /// ``business_rule_violation``, carrying ``details``).
    #[error("{message}")]
    Conflict {
        message: String,
        details: BTreeMap<String, String>,
    },
    /// A 409 whose ``details`` carry non-string values — the follow-up state
    /// machine ships the settable-status list as a JSON array, exactly like
    /// the baseline's ``BusinessRuleError``.
    #[error("{message}")]
    ConflictValue { message: String, details: Value },
    /// A **422** with a caller-chosen code.
    ///
    /// The baseline's `ValidationError` accepts `code=`: an unusable export key
    /// is `invalid_encryption_key` on a 422, not the default
    /// `validation_error`. The code and status are both contract text.
    #[error("{message}")]
    ValidationWithCode {
        code: &'static str,
        message: String,
        details: Value,
    },
    /// Any status with a caller-chosen code.
    ///
    /// The baseline's `ICAError` takes `status_code=` and `code=`, and OCR uses
    /// both: a missing engine is a **503** `ocr_unavailable`, a missing file a
    /// 409 `ocr_image_missing`, a failed read a 409 `ocr_error`. Modelling the
    /// status alongside the code keeps those three distinct instead of folding
    /// them into one generic envelope.
    #[error("{message}")]
    StatusWithCode {
        status: u16,
        code: &'static str,
        message: String,
        details: Value,
    },
    /// A 409 with a **caller-chosen** error code.
    ///
    /// The baseline's ``BusinessRuleError`` takes ``code=`` and uses something
    /// other than ``business_rule_violation`` in places: the OCR path guard
    /// raises ``ocr_path_not_allowed`` and an undecodable image
    /// ``ocr_image_undecodable``. The code is contract text, so it has to
    /// travel with the error rather than being assumed.
    #[error("{message}")]
    ConflictWithCode {
        /// A `&'static str` because every such code in the baseline is a
        /// literal at the `raise` site; keeping it static lets the wire layer
        /// pass it through without allocating.
        code: &'static str,
        message: String,
        details: Value,
    },
    /// The upstream AI provider failed or is unavailable (HTTP 503
    /// ``ai_provider_error``) — the baseline's ``AIProviderError``. The
    /// message is contract text; the details stay JSON for the same reason as
    /// ``ConflictValue``.
    #[error("{message}")]
    ProviderUnavailable { message: String, details: Value },
    /// The repository could not be read or written (HTTP 500).
    #[error("{message}")]
    Unavailable { message: String },
    /// A repository failure carried with its source chain intact.
    ///
    /// `#[from]` also registers the repository error as `source()`, so `?`
    /// can lift a persistence failure without losing the original cause.
    /// Rendering delegates to the existing mapping: wire code, status and
    /// message stay byte-identical to the pre-thiserror implementation.
    #[error("{0}")]
    Persistence(#[from] RepositoryError),
}

impl ApplicationError {
    /// Build the baseline's 404 message: ``{label} '{id}' was not found``.
    pub fn not_found(label: &str, id: &str) -> Self {
        ApplicationError::NotFound {
            message: format!("{label} '{id}' was not found"),
        }
    }

    /// A 422 whose body carries the identifiers the client needs.
    pub fn validation(message: &str, details: BTreeMap<String, String>) -> Self {
        ApplicationError::Validation {
            message: message.to_string(),
            details,
        }
    }

    /// A 409 whose body carries the identifiers the client needs.
    pub fn conflict(message: &str, details: BTreeMap<String, String>) -> Self {
        ApplicationError::Conflict {
            message: message.to_string(),
            details,
        }
    }

    /// A 409 with free-form JSON details (arrays included).
    pub fn conflict_value(message: impl Into<String>, details: Value) -> Self {
        ApplicationError::ConflictValue {
            message: message.into(),
            details,
        }
    }

    /// The shared business error code (§9.2). `ica-contracts` renders this
    /// same vocabulary on every transport; a unit test in that crate asserts
    /// the two mappings stay identical.
    pub fn code(&self) -> &str {
        match self {
            ApplicationError::NotFound { .. } => "not_found",
            ApplicationError::Validation { .. } => "validation_error",
            ApplicationError::Conflict { .. } | ApplicationError::ConflictValue { .. } => {
                "business_rule_violation"
            }
            ApplicationError::ValidationWithCode { code, .. }
            | ApplicationError::StatusWithCode { code, .. }
            | ApplicationError::ConflictWithCode { code, .. } => code,
            ApplicationError::ProviderUnavailable { .. } => "ai_provider_error",
            ApplicationError::Persistence(source) => source.application_code(),
            ApplicationError::Unavailable { .. } => "internal_error",
        }
    }

    /// The HTTP status the contract assigns this business error. IPC has no
    /// status line, but uses the same code/details envelope; the sidecar uses
    /// the same code vocabulary. Kept here so no adapter may invent a code.
    pub fn http_status(&self) -> u16 {
        match self {
            ApplicationError::NotFound { .. } => 404,
            ApplicationError::Validation { .. } | ApplicationError::ValidationWithCode { .. } => {
                422
            }
            ApplicationError::Conflict { .. }
            | ApplicationError::ConflictValue { .. }
            | ApplicationError::ConflictWithCode { .. } => 409,
            ApplicationError::StatusWithCode { status, .. } => *status,
            ApplicationError::ProviderUnavailable { .. } => 503,
            ApplicationError::Persistence(source) => source.application_status(),
            ApplicationError::Unavailable { .. } => 500,
        }
    }

    /// The stable authorization denial reason carried in `details`, when the
    /// error was produced by an authorization gate (P1 plan §10).
    pub fn details_reason(&self) -> &str {
        let details = match self {
            ApplicationError::StatusWithCode { details, .. }
            | ApplicationError::ConflictWithCode { details, .. }
            | ApplicationError::ValidationWithCode { details, .. }
            | ApplicationError::ConflictValue { details, .. }
            | ApplicationError::ProviderUnavailable { details, .. } => details,
            _ => return "",
        };
        details
            .get("reason_code")
            .and_then(Value::as_str)
            .unwrap_or("")
    }

    /// Human-readable message, unchanged from the source that produced it.
    pub fn message(&self) -> &str {
        match self {
            ApplicationError::NotFound { message }
            | ApplicationError::Validation { message, .. }
            | ApplicationError::Conflict { message, .. }
            | ApplicationError::ConflictValue { message, .. }
            | ApplicationError::ConflictWithCode { message, .. }
            | ApplicationError::ValidationWithCode { message, .. }
            | ApplicationError::StatusWithCode { message, .. }
            | ApplicationError::ProviderUnavailable { message, .. }
            | ApplicationError::Unavailable { message } => message,
            ApplicationError::Persistence(source) => source.message(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The thiserror derive must not change a single byte of the contract:
    /// `Display` is `message`, and code/status stay where they always were.
    #[test]
    fn bas002_thiserror_keeps_display_text_and_wire_codes() {
        let cases = vec![
            (
                ApplicationError::not_found("Position", "p-1"),
                "Position 'p-1' was not found",
                "not_found",
                404,
            ),
            (
                ApplicationError::Validation {
                    message: "bad".into(),
                    details: BTreeMap::new(),
                },
                "bad",
                "validation_error",
                422,
            ),
            (
                ApplicationError::Conflict {
                    message: "nope".into(),
                    details: BTreeMap::new(),
                },
                "nope",
                "business_rule_violation",
                409,
            ),
            (
                ApplicationError::ConflictValue {
                    message: "nope".into(),
                    details: Value::Null,
                },
                "nope",
                "business_rule_violation",
                409,
            ),
            (
                ApplicationError::ValidationWithCode {
                    code: "invalid_encryption_key",
                    message: "key".into(),
                    details: Value::Null,
                },
                "key",
                "invalid_encryption_key",
                422,
            ),
            (
                ApplicationError::StatusWithCode {
                    status: 503,
                    code: "ocr_unavailable",
                    message: "no ocr".into(),
                    details: Value::Null,
                },
                "no ocr",
                "ocr_unavailable",
                503,
            ),
            (
                ApplicationError::ConflictWithCode {
                    code: "ocr_path_not_allowed",
                    message: "denied".into(),
                    details: Value::Null,
                },
                "denied",
                "ocr_path_not_allowed",
                409,
            ),
            (
                ApplicationError::ProviderUnavailable {
                    message: "provider down".into(),
                    details: Value::Null,
                },
                "provider down",
                "ai_provider_error",
                503,
            ),
            (
                ApplicationError::Unavailable {
                    message: "db down".into(),
                },
                "db down",
                "internal_error",
                500,
            ),
        ];
        for (error, text, code, status) in cases {
            assert_eq!(error.to_string(), text);
            assert_eq!(error.code(), code);
            assert_eq!(error.http_status(), status);
        }
    }

    /// `#[from]` must keep the original error as `source()` while rendering
    /// exactly like the old explicit `application_error()` mapping.
    #[test]
    fn bas002_from_repository_error_keeps_wire_behavior_and_source() {
        let repo = RepositoryError::conflict("state refused", BTreeMap::new());
        let expected = (
            repo.message().to_string(),
            repo.application_code(),
            repo.application_status(),
        );
        let app: ApplicationError = repo.into();
        assert!(matches!(app, ApplicationError::Persistence(_)));
        assert_eq!(
            (app.message().to_string(), app.code(), app.http_status()),
            expected
        );
        assert_eq!(app.to_string(), "state refused");
        assert!(
            std::error::Error::source(&app).is_some(),
            "#[from] registers the repository error as source"
        );
        assert_eq!(RepositoryError::not_found("gone").application_status(), 404);
        assert_eq!(
            RepositoryError::invalid("bad", Value::Null).application_status(),
            422
        );
        assert_eq!(RepositoryError::unavailable("x").application_status(), 500);
    }
}
