//! Errors raised by use cases.
//!
//! Kept free of HTTP concepts: the mapping to status codes and to the JSON
//! error envelope belongs to ``ica-api``. The message text is part of the
//! contract though — the Python baseline renders it verbatim in the
//! ``error.message`` field.

/// Why a use case could not produce a result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplicationError {
    /// The requested entity does not exist (HTTP 404).
    NotFound { message: String },
    /// The repository could not be read (HTTP 500).
    Unavailable { message: String },
}

impl ApplicationError {
    /// Build the baseline's 404 message: ``{label} '{id}' was not found``.
    pub fn not_found(label: &str, id: &str) -> Self {
        ApplicationError::NotFound {
            message: format!("{label} '{id}' was not found"),
        }
    }

    /// Human-readable message, unchanged from the source that produced it.
    pub fn message(&self) -> &str {
        match self {
            ApplicationError::NotFound { message } => message,
            ApplicationError::Unavailable { message } => message,
        }
    }
}

impl std::fmt::Display for ApplicationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.message())
    }
}

impl std::error::Error for ApplicationError {}
