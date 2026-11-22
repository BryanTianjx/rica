//! Transport-neutral event contract (F1-3).
//!
//! A backend state change is announced to the UI as an event; the frontend
//! listens and refreshes the affected view instead of manually reloading
//! after each write (the "reload chain"). The event name and payload live
//! here, beside the DTOs, so IPC (Tauri Events) and any future transport
//! share one shape — the frontend does not invent a second contract.
//!
//! Payload fields are a subset of the corresponding HTTP DTO, so the event
//! carries exactly what a view needs to decide to refresh; no new semantics.

/// `position-changed`: emitted after a positions write (create/update/delete).
pub const POSITION_CHANGED: &str = "ica://position-changed";

/// The kind of mutation that produced a `position-changed` event.
///
/// Rendering is a `&'static str` (matching the crate's DTO convention: the
/// domain stays serde-free and DTOs project enums to string literals).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PositionChangeKind {
    Created,
    Updated,
    Deleted,
}

impl PositionChangeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Updated => "updated",
            Self::Deleted => "deleted",
        }
    }
}

/// Serializable payload for `POSITION_CHANGED` (Tauri Event / future HTTP SSE).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PositionChanged {
    pub kind: String,
    /// The affected position id. Empty only for a bulk/no-target change.
    pub id: String,
}

impl PositionChanged {
    pub fn new(kind: PositionChangeKind, id: impl Into<String>) -> Self {
        Self {
            kind: kind.as_str().to_string(),
            id: id.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_name_is_stable_and_namespaced() {
        assert_eq!(POSITION_CHANGED, "ica://position-changed");
    }

    #[test]
    fn payload_serialises_to_a_flat_string_shape() {
        let event = PositionChanged::new(PositionChangeKind::Updated, "pos-1");
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(json, r#"{"kind":"updated","id":"pos-1"}"#);
        let back: PositionChanged = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, event);
    }

    #[test]
    fn all_change_kinds_render_as_lowercase_literals() {
        assert_eq!(PositionChangeKind::Created.as_str(), "created");
        assert_eq!(PositionChangeKind::Updated.as_str(), "updated");
        assert_eq!(PositionChangeKind::Deleted.as_str(), "deleted");
    }
}
