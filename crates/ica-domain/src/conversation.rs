//! Conversation and message entities.

use chrono::{DateTime, Utc};

/// Who authored a message (`SenderType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SenderType {
    Candidate,
    Recruiter,
    System,
}

impl SenderType {
    pub fn as_str(self) -> &'static str {
        match self {
            SenderType::Candidate => "Candidate",
            SenderType::Recruiter => "Recruiter",
            SenderType::System => "System",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "Candidate" => Some(SenderType::Candidate),
            "Recruiter" => Some(SenderType::Recruiter),
            "System" => Some(SenderType::System),
            _ => None,
        }
    }
}

/// How a message entered ICA (`MessageSource`).
///
/// `Manual` and `PasteImport` are the V1.0 paths; `ScreenshotOcr` is the V2.0
/// local input bridge; `BrowserExtract` is the browser pipeline's draft source
/// (added to the frozen baseline `9ed173c` with BROWSER-04 — its sessions are
/// confirmed through `POST /api/imports/{session_id}/confirm-candidate`, never
/// through the message-shaped confirm).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MessageSource {
    Manual,
    PasteImport,
    ScreenshotOcr,
    BrowserExtract,
}

impl MessageSource {
    pub fn as_str(self) -> &'static str {
        match self {
            MessageSource::Manual => "manual",
            MessageSource::PasteImport => "paste_import",
            MessageSource::ScreenshotOcr => "screenshot_ocr",
            MessageSource::BrowserExtract => "browser_extract",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "manual" => Some(MessageSource::Manual),
            "paste_import" => Some(MessageSource::PasteImport),
            "screenshot_ocr" => Some(MessageSource::ScreenshotOcr),
            "browser_extract" => Some(MessageSource::BrowserExtract),
            _ => None,
        }
    }
}

/// A conversation thread with one candidate.
///
/// `message_count`, `position_id` and `position_name` are resolved at read
/// time: the row itself carries no position snapshot, so every branch that
/// returns a conversation must fill them from the candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conversation {
    pub id: String,
    pub candidate_id: String,
    pub channel: String,
    pub title: Option<String>,
    pub is_active: bool,
    pub last_message_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub message_count: i64,
    pub position_id: Option<String>,
    pub position_name: Option<String>,
}

/// A single message in a conversation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub id: String,
    pub candidate_id: String,
    pub conversation_id: String,
    pub sender_type: Option<String>,
    pub content: String,
    pub message_time: Option<DateTime<Utc>>,
    pub channel: String,
    pub source: String,
    pub created_at: DateTime<Utc>,
}

/// One row of the candidate timeline.
///
/// `kind` exists so a draft can never be mistaken for a real message; only
/// messages appear today, but the field is part of the wire contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelineEntry {
    pub id: String,
    pub kind: &'static str,
    pub sender_type: Option<String>,
    pub content: String,
    pub message_time: Option<DateTime<Utc>>,
    pub source: Option<String>,
    pub status: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
}
