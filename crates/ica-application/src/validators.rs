//! Content validator — a value-for-value port of `app/ai/validators.py`.
//!
//! Checks a generated draft against deterministic rules before it reaches the
//! review queue.
//!
//! Important framing: a `Passed` result means *no blocking rule was found*.
//! It is not a guarantee that the message is factually true — the requirement
//! keeps human review mandatory regardless of the validator outcome.

use regex::Regex;

use ica_domain::follow_up::candidate_blocks_follow_up;
use ica_domain::{CandidateStatus, ValidationStatus};

/// One finding from the validator (`ValidationIssue`). Only the message list
/// is persisted (`validation_messages_json`); the severity pair drives the
/// status.
struct ValidationIssue {
    message: String,
    severity: Severity,
}

#[derive(PartialEq, Clone, Copy)]
enum Severity {
    Warning,
    Blocking,
}

/// Phrases that promise something the recruiter has not committed to.
const PROMISE_PATTERNS: [(&str, &str); 4] = [
    (r"保证|包过|一定能|肯定会|承诺", "promise"),
    (r"薪资\s*(?:一定|确定|就是)?\s*\d{2,}", "salary_claim"),
    // `(?i)` replaces Python's re.IGNORECASE flag.
    (
        r"(?i)offer\s*(?:letter)?\s*(?:已发|马上发|已发送)",
        "offer_claim",
    ),
    (r"入职|入职时间|什么时候能到岗|offer", "start_date_claim"),
];

/// Facts that must never be asserted without context support.
const FABRICATION_HINTS: [(&str, &str); 3] = [
    (r"[你您]有?\d+\s*年(以上)?\s*经[验历]", "experience"),
    (r"精通|熟练掌握|擅长", "skill_assertion"),
    (r"期望薪资(?:是|为)?\s*\d", "salary_assertion"),
];

/// A message longer than this with no cited context is treated as making
/// substantive claims that need verification. Short messages (greetings,
/// simple questions) legitimately cite nothing.
const SUBSTANTIVE_LENGTH: usize = 40;

/// The mobile-phone pattern needs Python's lookarounds (`(?<!\d)…(?!\d)`),
/// which the `regex` crate does not support — the match is found without
/// them and the digit boundaries are checked by hand.
const PHONE_INNER: &str = r"1[3-9]\d{9}";

/// Direct identifiers must not leak into a message (email half).
const EMAIL: &str = r"[\w.+-]+@[\w-]+\.[\w.-]+";

fn first_match(pattern: &str, text: &str) -> Option<String> {
    Regex::new(pattern)
        .ok()
        .and_then(|re| re.find(text).map(|m| m.as_str().to_string()))
}

/// `(?<!\d)1[3-9]\d{9}(?!\d)` — the first match whose neighbouring characters
/// are not ASCII digits.
fn first_phone(text: &str) -> Option<String> {
    let re = Regex::new(PHONE_INNER).ok()?;
    for m in re.find_iter(text) {
        let before_free = text[..m.start()]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_ascii_digit());
        let after_free = text[m.end()..]
            .chars()
            .next()
            .is_none_or(|c| !c.is_ascii_digit());
        if before_free && after_free {
            return Some(m.as_str().to_string());
        }
    }
    None
}

fn warning(message: String) -> ValidationIssue {
    ValidationIssue {
        message,
        severity: Severity::Warning,
    }
}

fn blocking(message: String) -> ValidationIssue {
    ValidationIssue {
        message,
        severity: Severity::Blocking,
    }
}

/// Validate a generated draft (`validate_draft`), returning the verdict plus
/// the human-readable messages the draft stores.
pub fn validate_draft(
    message_text: &str,
    candidate_status: CandidateStatus,
    position_status: Option<&str>,
    uncertainties: &[String],
    warnings: &[String],
    quality_status: Option<&str>,
    context_references: &[String],
) -> (ValidationStatus, Vec<String>) {
    let mut issues: Vec<ValidationIssue> = Vec::new();

    // ── Blocking: candidate state ─────────────────────────────────────────
    if candidate_blocks_follow_up(candidate_status) {
        issues.push(blocking(format!(
            "Candidate status is '{}'. Contacting is not allowed in this state.",
            candidate_status.as_str()
        )));
    }

    // ── Blocking: position state ──────────────────────────────────────────
    if position_status == Some("Closed") {
        issues.push(blocking(
            "The linked position is closed; do not send a message about it.".to_string(),
        ));
    }

    // ── Blocking: the model's own assessment ─────────────────────────────
    if quality_status == Some("blocked") {
        issues.push(blocking(
            "The generator reported a blocking condition for this draft.".to_string(),
        ));
    }

    let text = message_text;

    // ── Warnings: promises ────────────────────────────────────────────────
    for (pattern, code) in PROMISE_PATTERNS {
        if let Some(found) = first_match(pattern, text) {
            issues.push(warning(format!(
                "The message contains '{found}', which may be an unverified promise or \
                 commitment. Verify before sending."
            )));
            let _ = code;
        }
    }

    // ── Warnings: fabricated facts ────────────────────────────────────────
    for (pattern, _code) in FABRICATION_HINTS {
        if let Some(found) = first_match(pattern, text) {
            issues.push(warning(format!(
                "The message asserts '{found}'. Confirm it is supported by the candidate's \
                 recorded profile."
            )));
        }
    }

    // ── Warnings: leaked identifiers ──────────────────────────────────────
    if first_phone(text).is_some() || first_match(EMAIL, text).is_some() {
        issues.push(warning(
            "The message contains a personal identifier (phone or email). Consider removing \
             it before sending."
                .to_string(),
        ));
    }

    // ── Warnings: unresolved uncertainty ──────────────────────────────────
    if !uncertainties.is_empty() {
        issues.push(warning(format!(
            "{} item(s) are unknown or unclear: {}",
            uncertainties.len(),
            uncertainties
                .iter()
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
                .join("; ")
        )));
    }

    // ── Warnings: model-reported warnings ─────────────────────────────────
    if !warnings.is_empty() {
        issues.push(warning(format!(
            "The generator reported risks: {}",
            warnings
                .iter()
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
                .join("; ")
        )));
    }

    // ── Warnings: missing references ──────────────────────────────────────
    if context_references.is_empty() && text.trim().chars().count() > SUBSTANTIVE_LENGTH {
        issues.push(warning(
            "The draft makes claims but cites no context. Confirm every claim against the \
             candidate's record."
                .to_string(),
        ));
    }

    let blocking = issues
        .iter()
        .any(|issue| issue.severity == Severity::Blocking);
    let status = if blocking {
        ValidationStatus::Blocked
    } else if !issues.is_empty() {
        ValidationStatus::Warning
    } else {
        ValidationStatus::Passed
    };

    (
        status,
        issues.into_iter().map(|issue| issue.message).collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn validate(text: &str) -> (ValidationStatus, Vec<String>) {
        validate_draft(
            text,
            CandidateStatus::Replied,
            Some("Open"),
            &[],
            &[],
            None,
            &["msg-1".to_string()],
        )
    }

    #[test]
    fn a_clean_short_message_passes() {
        let (status, messages) = validate("您好，想和您聊一下这个岗位。");
        assert_eq!(status, ValidationStatus::Passed);
        assert!(messages.is_empty());
    }

    #[test]
    fn a_promise_is_a_warning() {
        let (status, messages) = validate("我们保证一定能帮您拿到offer");
        assert_eq!(status, ValidationStatus::Warning);
        // One issue per PATTERN (Python's `re.search` + break), not per
        // occurrence: the same pattern's '保证' wins over '一定能'.
        assert!(messages.iter().any(|m| m.contains("'保证'")));
        assert_eq!(messages.len(), 2); // the promise + the leaked "offer"
    }

    #[test]
    fn a_leaked_phone_is_a_warning() {
        let (status, _) = validate("我的电话是13812345678，随时联系");
        assert_eq!(status, ValidationStatus::Warning);
        // Embedded in a longer digit run: the hand-checked boundaries refuse it.
        assert_eq!(validate("编号9138123456781").0, ValidationStatus::Passed);
    }

    #[test]
    fn long_text_without_references_is_a_warning() {
        let long = "一".repeat(41);
        let (status, messages) = validate_draft(
            &long,
            CandidateStatus::Replied,
            Some("Open"),
            &[],
            &[],
            None,
            &[],
        );
        assert_eq!(status, ValidationStatus::Warning);
        assert!(messages.iter().any(|m| m.contains("cites no context")));
    }

    #[test]
    fn blocking_conditions_win() {
        let (status, messages) = validate_draft(
            "hi",
            CandidateStatus::Withdrawn,
            Some("Closed"),
            &[],
            &[],
            Some("blocked"),
            &[],
        );
        assert_eq!(status, ValidationStatus::Blocked);
        assert_eq!(messages.len(), 3);
    }
}
