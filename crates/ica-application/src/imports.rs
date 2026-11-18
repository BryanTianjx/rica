//! Import session use cases: preview, fetch, confirm, reject.
//!
//! The flow is two-step and the split matters:
//!
//! * `preview` parses pasted text and persists the *reading* — a session, its
//!   records and an `import.previewed` audit row. It never writes business
//!   messages, so nothing reaches the conversation timeline until the user
//!   confirms. The baseline's docs call this "zero write"; what it actually
//!   guarantees is that `candidates` and `messages` are untouched
//!   (`docs/contract-baseline/w4-python-facts.md` §1).
//! * `confirm` turns those records into messages in **one** transaction, so a
//!   failure part-way leaves no partial timeline behind.
//!
//! `reject` marks the session rejected and clears its raw text, but the port
//! deliberately does **not** stop a rejected session from being confirmed
//! afterwards: the baseline's `confirm` guards `confirmed` only, and its
//! `import_records` are still there, so a rejected session can still be
//! confirmed (measured, not assumed). Adding a 409 here would be a behaviour
//! change and would have to be registered as a declared improvement.

use chrono::Utc;

use crate::error::ApplicationError;
use crate::ports::{
    ImportConfirmOutcome, ImportRepository, NewImportRecordRow, NewImportSessionRow,
    RepositoryError,
};

use crate::payload::ImportCorrection;
pub use crate::payload::{NewImportConfirm, NewImportPreview};

fn unavailable(error: RepositoryError) -> ApplicationError {
    if let Some(details) = error.conflict_details() {
        return ApplicationError::Conflict {
            message: error.message().to_string(),
            details: details.clone(),
        };
    }
    ApplicationError::Unavailable {
        message: error.message().to_string(),
    }
}

/// Parse pasted text into a preview (`preview_import`).
///
/// Writes `import_sessions`, `import_records` and `import.previewed`; never
/// `messages`. A candidate that does not exist is a 404 before anything is
/// written.
pub async fn preview_import(
    imports: &dyn ImportRepository,
    request: NewImportPreview,
) -> Result<ica_domain::ImportSession, ApplicationError> {
    if let Some(candidate_id) = &request.candidate_id {
        let exists = imports
            .candidate_exists(candidate_id)
            .await
            .map_err(unavailable)?;
        if !exists {
            return Err(ApplicationError::not_found("Candidate", candidate_id));
        }
    }

    let parsed = ica_domain::parse_messages(&request.raw_text, Utc::now());

    // A parse that produced nothing is a `failed` session carrying the
    // parser's warnings, not an error response.
    let (status, error_message) = if parsed.messages.is_empty() {
        let joined = parsed.warnings.join("; ");
        (
            ica_domain::ImportStatus::Failed,
            if joined.is_empty() {
                None
            } else {
                Some(joined)
            },
        )
    } else {
        (ica_domain::ImportStatus::Previewed, None)
    };

    let existing = match &request.candidate_id {
        Some(candidate_id) => imports
            .message_hashes(candidate_id)
            .await
            .map_err(unavailable)?,
        None => Default::default(),
    };

    let mut messages = parsed.messages;
    ica_domain::find_duplicates(&mut messages, &existing);
    let detected_duplicate_count = messages
        .iter()
        .filter(|message| message.status == ica_domain::RecordStatus::Duplicate)
        .count() as i64;

    let records: Vec<NewImportRecordRow> = messages
        .into_iter()
        .map(|message| NewImportRecordRow {
            sequence: message.sequence,
            sender_type: message.sender_type.map(|s| s.as_str().to_string()),
            content: message.content,
            message_time: message.message_time,
            status: message.status,
            duplicate_of_message_id: message.duplicate_of_message_id,
            content_hash: Some(message.content_hash),
            validation_messages: if message.validation_messages.is_empty() {
                None
            } else {
                Some(message.validation_messages.join("\n"))
            },
        })
        .collect();

    let created = imports
        .create_preview(NewImportSessionRow {
            candidate_id: request.candidate_id,
            source: request.source.as_str().to_string(),
            raw_text: request.raw_text,
            status,
            error_message,
            records,
            detected_duplicate_count,
        })
        .await
        .map_err(unavailable)?;

    Ok(created)
}

/// Fetch one import session with its records (`get_import_session`).
pub async fn get_import(
    imports: &dyn ImportRepository,
    session_id: &str,
) -> Result<ica_domain::ImportSession, ApplicationError> {
    let session = imports
        .find(session_id)
        .await
        .map_err(unavailable)?
        .ok_or_else(|| ApplicationError::not_found("Import session", session_id))?;
    Ok(session)
}

/// Confirm an import session (`confirm_import`).
///
/// Already-confirmed is a 409. A rejected session is **not** refused — see the
/// module docstring.
pub async fn confirm_import(
    imports: &dyn ImportRepository,
    session_id: &str,
    request: NewImportConfirm,
) -> Result<ImportConfirmOutcome, ApplicationError> {
    let session = imports
        .find(session_id)
        .await
        .map_err(unavailable)?
        .ok_or_else(|| ApplicationError::not_found("Import session", session_id))?;

    if session.status == ica_domain::ImportStatus::Confirmed {
        return Err(ApplicationError::conflict(
            "This import session was already confirmed",
            Default::default(),
        ));
    }

    let outcome: ImportConfirmOutcome = imports
        .confirm(
            session_id,
            &request.candidate_id,
            request.conversation_id,
            request
                .corrections
                .into_iter()
                .map(
                    |correction: ImportCorrection| crate::ports::ImportRecordCorrection {
                        id: correction.id,
                        sender_type: correction.sender_type,
                        content: correction.content,
                        message_time: correction.message_time,
                        skip: correction.skip,
                    },
                )
                .collect(),
        )
        .await
        .map_err(unavailable)?;

    Ok(outcome)
}

/// Reject an import session (`reject_import`).
///
/// A confirmed session cannot be rejected (409); rejecting clears the raw text.
pub async fn reject_import(
    imports: &dyn ImportRepository,
    session_id: &str,
) -> Result<ica_domain::ImportSession, ApplicationError> {
    let session = imports
        .find(session_id)
        .await
        .map_err(unavailable)?
        .ok_or_else(|| ApplicationError::not_found("Import session", session_id))?;

    if session.status == ica_domain::ImportStatus::Confirmed {
        return Err(ApplicationError::conflict(
            "A confirmed import session cannot be rejected",
            Default::default(),
        ));
    }

    let updated = imports
        .reject(session_id)
        .await
        .map_err(unavailable)?
        .ok_or_else(|| ApplicationError::not_found("Import session", session_id))?;
    Ok(updated)
}
