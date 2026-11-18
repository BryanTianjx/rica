//! Import sessions: the two-step paste/OCR → preview → confirm flow.
//!
//! Parsing pasted text produces a *preview*: the parser's reading of the text,
//! stored as records the user can correct or skip before anything reaches the
//! conversation timeline. Only `confirm` writes messages, and it does so in one
//! transaction.
//!
//! Two baseline behaviours are modelled exactly as measured
//! (`docs/contract-baseline/w4-python-facts.md`), not as they "should" be:
//!
//! * `preview` writes `import_sessions` / `import_records` rows and an
//!   `import.previewed` audit — it is *not* a read-only call. What it never
//!   touches is the business tables (`candidates`, `messages`);
//! * a `rejected` session can still be confirmed. `confirm` guards `confirmed`
//!   only, and the records are still there. The baseline's comment claims
//!   rejecting is final; the behaviour is not, and this port copies the
//!   behaviour.

use chrono::{DateTime, Utc};

/// Lifecycle of an import session (`ImportStatus`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImportStatus {
    Pending,
    Previewed,
    Confirmed,
    Rejected,
    Failed,
}

impl ImportStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ImportStatus::Pending => "pending",
            ImportStatus::Previewed => "previewed",
            ImportStatus::Confirmed => "confirmed",
            ImportStatus::Rejected => "rejected",
            ImportStatus::Failed => "failed",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "pending" => Some(ImportStatus::Pending),
            "previewed" => Some(ImportStatus::Previewed),
            "confirmed" => Some(ImportStatus::Confirmed),
            "rejected" => Some(ImportStatus::Rejected),
            "failed" => Some(ImportStatus::Failed),
            _ => None,
        }
    }
}

/// Per-record outcome of the parser's duplicate/validation pass
/// (`RecordStatus`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RecordStatus {
    Pending,
    Valid,
    Invalid,
    Duplicate,
    Unknown,
}

impl RecordStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            RecordStatus::Pending => "pending",
            RecordStatus::Valid => "valid",
            RecordStatus::Invalid => "invalid",
            RecordStatus::Duplicate => "duplicate",
            RecordStatus::Unknown => "unknown",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "pending" => Some(RecordStatus::Pending),
            "valid" => Some(RecordStatus::Valid),
            "invalid" => Some(RecordStatus::Invalid),
            "duplicate" => Some(RecordStatus::Duplicate),
            "unknown" => Some(RecordStatus::Unknown),
            _ => None,
        }
    }
}

/// One parsed message inside an import session, before it becomes a `Message`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportRecord {
    pub id: String,
    pub session_id: String,
    /// 1-based position in the parsed text; `confirm` writes them in this order.
    pub sequence: i64,
    pub sender_type: Option<String>,
    pub content: String,
    pub message_time: Option<DateTime<Utc>>,
    pub status: RecordStatus,
    /// Set when `status` is `duplicate`: the message this row duplicates.
    pub duplicate_of_message_id: Option<String>,
    /// Hash of the normalised content — the duplicate test is this hash
    /// against the candidate's existing messages.
    pub content_hash: Option<String>,
    pub validation_messages: Option<String>,
}

/// A preview session and the records parsed from one block of text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportSession {
    pub id: String,
    pub candidate_id: Option<String>,
    /// `MessageSource` as stored (`paste_import` / `screenshot_ocr` / `manual`).
    pub source: String,
    pub status: ImportStatus,
    pub detected_duplicate_count: i64,
    pub confirmed_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
    pub records: Vec<ImportRecord>,
}
