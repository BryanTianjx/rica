//! The AI context builder — a value-for-value port of `app/ai/context.py`
//! (`build_context`, V1.0 path: no token truncation, `message_limit = 30`).
//!
//! Isolation (candidate-scoped queries) and minimisation (`redact_for_ai` on
//! every free-text field, the name withheld by default) are the two rules; the
//! builder also separates *unknown* from *conflicting* information, because
//! the requirement forbids inventing missing facts.

use serde_json::{json, Map, Value};

use ica_domain::redact::NAME_PLACEHOLDER;
use ica_domain::{redact_for_ai, CandidateStatus, PositionStatus};

use crate::error::ApplicationError;
use crate::ports::{CandidateRepository, MessageRepository, PositionRepository, RepositoryError};

fn unavailable(error: RepositoryError) -> ApplicationError {
    error.application_error()
}

/// Render a parsed instant the way Python's naive `datetime.isoformat()` does
/// (no `Z` suffix, six fractional digits only when non-zero) — the `time`
/// field of the context messages.
fn format_message_naive(instant: &chrono::DateTime<chrono::Utc>) -> String {
    if instant.timestamp_subsec_micros() == 0 {
        instant.format("%Y-%m-%dT%H:%M:%S").to_string()
    } else {
        instant.format("%Y-%m-%dT%H:%M:%S%.6f").to_string()
    }
}

/// The scoped context handed to the model (and to the `/context` preview).
#[derive(Debug, Clone, PartialEq)]
pub struct AiContext {
    pub candidate_id: String,
    pub candidate_summary: Value,
    pub position_summary: Option<Value>,
    /// Oldest-first, capped at `message_limit` (30), each entry
    /// `{id, sender, time, content}` with the content redacted.
    pub messages: Vec<Value>,
    pub notes: Vec<String>,
    pub unknowns: Vec<String>,
    pub conflicts: Vec<String>,
    pub redaction_applied: Vec<String>,
}

/// The generate request after body validation, as the AI port receives it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerateDraftRequest {
    pub candidate_id: String,
    pub task_id: Option<String>,
    pub message_type: ica_domain::MessageType,
    pub tone: ica_domain::Tone,
    pub user_instruction: Option<String>,
    pub include_alternatives: bool,
    pub max_length: Option<i64>,
    /// The model the client asked for, if any. The generator resolves it the
    /// way the baseline's registry does — an unknown id is a 503, not a
    /// silent fall back to the default.
    pub model_id: Option<String>,
    /// A/B comparison models. When non-empty this wins over `model_id`, and
    /// every id is resolved in order (`app/ai/service.py`).
    pub comparison_models: Vec<String>,
}

/// Build the AI context for exactly one candidate.
///
/// The candidate's real name is withheld by default: the `name` field becomes
/// `[NAME]` and every other occurrence in free text is masked too. The
/// `redaction_applied` list always states which of the two happened.
pub async fn build_context(
    candidates: &dyn CandidateRepository,
    positions: &dyn PositionRepository,
    messages: &dyn MessageRepository,
    candidate_id: &str,
) -> Result<AiContext, ApplicationError> {
    let candidate = candidates
        .find(candidate_id)
        .await
        .map_err(unavailable)?
        .ok_or_else(|| ApplicationError::not_found("Candidate", candidate_id))?;

    let mut unknowns: Vec<String> = Vec::new();
    let mut conflicts: Vec<String> = Vec::new();
    let mut redactions: Vec<String> = Vec::new();

    // Names only travel when explicitly opted in; the same decision drives
    // masking of the name inside free text.
    let names: Vec<&str> = vec![candidate.name.as_str()];

    // ── Candidate summary ──────────────────────────────────────────────────
    let mut summary = Map::new();
    summary.insert("name".to_string(), json!(NAME_PLACEHOLDER));
    redactions.push("name".to_string());
    summary.insert("status".to_string(), json!(candidate.status.as_str()));
    summary.insert("source".to_string(), json!(candidate.source));
    summary.insert("position_id".to_string(), json!(candidate.position_id));
    for (key, value) in [
        ("experience_summary", &candidate.experience_summary),
        ("skill_summary", &candidate.skill_summary),
        ("expected_salary", &candidate.expected_salary),
        ("expected_location", &candidate.expected_location),
        ("job_intention", &candidate.job_intention),
    ] {
        if let Some(value) = value {
            let (masked, categories) = redact_for_ai(value, &names);
            redactions.extend(categories);
            summary.insert(key.to_string(), json!(masked));
        }
    }
    for missing in ["experience_summary", "skill_summary", "job_intention"] {
        let present = match missing {
            "experience_summary" => candidate.experience_summary.is_some(),
            "skill_summary" => candidate.skill_summary.is_some(),
            _ => candidate.job_intention.is_some(),
        };
        if !present {
            unknowns.push(format!(
                "Candidate {} is unknown.",
                missing.replace('_', " ")
            ));
        }
    }

    // ── Position ───────────────────────────────────────────────────────────
    let position = match &candidate.position_id {
        Some(position_id) => positions.find(position_id).await.map_err(unavailable)?,
        None => None,
    };
    let position_summary = position.as_ref().map(|position| {
        let mut summary = Map::new();
        summary.insert("name".to_string(), json!(position.name));
        summary.insert("status".to_string(), json!(position.status.as_str()));
        summary.insert("location".to_string(), json!(position.location));
        for (key, value) in [
            ("description", &position.description),
            ("skill_requirements", &position.skill_requirements),
            ("experience_requirements", &position.experience_requirements),
            ("education_requirements", &position.education_requirements),
        ] {
            if let Some(value) = value {
                let (masked, categories) = redact_for_ai(value, &names);
                redactions.extend(categories);
                summary.insert(key.to_string(), json!(masked));
            }
        }
        Value::Object(summary)
    });
    match &position {
        Some(position) => {
            if position.skill_requirements.is_none() {
                unknowns.push("Position skill requirements are unknown.".to_string());
            }
        }
        None => unknowns.push("The candidate is not linked to a position.".to_string()),
    }

    // ── Messages (strictly candidate-scoped, oldest first) ─────────────────
    let context_messages = messages
        .context_messages(&candidate.id, 30)
        .await
        .map_err(unavailable)?;
    let mut rendered_messages: Vec<Value> = Vec::new();
    for message in &context_messages {
        let (masked, categories) = redact_for_ai(&message.content, &names);
        redactions.extend(categories);
        rendered_messages.push(json!({
            "id": message.id,
            "sender": message.sender_type.as_deref().unwrap_or("Unknown"),
            "time": message
                .message_time
                .as_ref()
                .map(format_message_naive),
            "content": masked,
        }));
    }
    if context_messages.is_empty() {
        unknowns.push("No communication history is recorded yet.".to_string());
    }

    // ── Notes ──────────────────────────────────────────────────────────────
    let mut notes: Vec<String> = Vec::new();
    if let Some(note) = &candidate.notes {
        let (masked, categories) = redact_for_ai(note, &names);
        redactions.extend(categories);
        notes.push(masked);
    }

    // ── Conflicts ──────────────────────────────────────────────────────────
    if let Some(position) = &position {
        if position.status == PositionStatus::Closed && !candidate_blocks(candidate.status) {
            conflicts.push(
                "The linked position is closed while the candidate is still active.".to_string(),
            );
        }
    }
    let candidate_messages: Vec<&str> = context_messages
        .iter()
        .filter(|message| {
            message.sender_type.as_deref() == Some("Candidate") && !message.content.is_empty()
        })
        .map(|message| message.content.as_str())
        .collect();
    if candidate.expected_salary.is_some() && candidate_messages.len() > 1 {
        conflicts.push(
            "The candidate mentioned expectations in several messages; the recorded \
             expected salary may not match all of them."
                .to_string(),
        );
    }

    redactions.sort();
    redactions.dedup();

    Ok(AiContext {
        candidate_id: candidate.id.clone(),
        candidate_summary: Value::Object(summary),
        position_summary,
        messages: rendered_messages,
        notes,
        unknowns,
        conflicts,
        redaction_applied: redactions,
    })
}

fn candidate_blocks(status: CandidateStatus) -> bool {
    matches!(
        status,
        CandidateStatus::Rejected | CandidateStatus::Withdrawn | CandidateStatus::Closed
    )
}
