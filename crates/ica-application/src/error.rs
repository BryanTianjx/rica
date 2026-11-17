//! Errors raised by use cases.
//!
//! Kept free of HTTP concepts: the mapping to status codes and to the JSON
//! error envelope belongs to ``ica-api``. The message text is part of the
//! contract though — the Python baseline renders it verbatim in the
//! ``error.message`` field, and ``details`` is compared key-for-key.

use std::collections::BTreeMap;

/// Why a use case could not produce a result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplicationError {
    /// The requested entity does not exist (HTTP 404).
    NotFound { message: String },
    /// The request is well-formed but describes an impossible relation, e.g. a
    /// conversation that belongs to another candidate (HTTP 422
    /// ``validation_error``, carrying ``details``).
    Validation {
        message: String,
        details: BTreeMap<String, String>,
    },
    /// A domain rule forbids the requested action (HTTP 409
    /// ``business_rule_violation``, carrying ``details``).
    Conflict {
        message: String,
        details: BTreeMap<String, String>,
    },
    /// The repository could not be read or written (HTTP 500).
    Unavailable { message: String },
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

    /// Human-readable message, unchanged from the source that produced it.
    pub fn message(&self) -> &str {
        match self {
            ApplicationError::NotFound { message }
            | ApplicationError::Validation { message, .. }
            | ApplicationError::Conflict { message, .. }
            | ApplicationError::Unavailable { message } => message,
        }
    }

    /// Structured details rendered into ``error.details`` (empty for the
    /// variants the baseline leaves empty).
    pub fn details(&self) -> BTreeMap<String, String> {
        match self {
            ApplicationError::Validation { details, .. }
            | ApplicationError::Conflict { details, .. } => details.clone(),
            ApplicationError::NotFound { .. } | ApplicationError::Unavailable { .. } => {
                BTreeMap::new()
            }
        }
    }
}

impl std::fmt::Display for ApplicationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.message())
    }
}

impl std::error::Error for ApplicationError {}
