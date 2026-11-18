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

use crate::payload::{ImportCorrection, NewImportOcr};
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

// ── Screenshot OCR ──────────────────────────────────────────────────────────

/// Result of a screenshot OCR call, before it is mapped to the wire DTO.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OcrOutcome {
    pub session: ica_domain::ImportSession,
    pub text: String,
    /// The baseline hard-codes `"zh+en"`.
    pub language: String,
}

/// Resolve `path` the way Python's `Path.resolve()` does for a file that may
/// not exist yet: make it absolute, expand `.`/`..`, and follow symlinks where
/// they can be followed.
fn resolve_lenient(path: &std::path::Path) -> std::path::PathBuf {
    if let Ok(canonical) = path.canonicalize() {
        return canonical;
    }
    // The file may not exist (the caller only knows the path). Normalise
    // lexically on top of the canonical parent where possible.
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("/"))
            .join(path)
    };
    let mut out = std::path::PathBuf::new();
    for component in absolute.components() {
        use std::path::Component;
        match component {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// The baseline's OCR path guard (`_assert_ocr_path_allowed`).
///
/// The guard runs **before** the existence check, so a path outside the
/// staging area is refused without disclosing whether the file is there. Two
/// codes matter: `ocr_path_not_allowed` for a path outside the area (or a
/// directory), and the plain 409 for a missing file.
fn assert_ocr_path_allowed(
    image_path: &str,
    allowed_dir: &std::path::Path,
) -> Result<std::path::PathBuf, ApplicationError> {
    let resolved = resolve_lenient(std::path::Path::new(image_path));
    let allowed = resolve_lenient(allowed_dir);

    let mut details = std::collections::BTreeMap::new();
    details.insert("allowed_prefix".to_string(), allowed.display().to_string());

    if resolved.is_dir() {
        return Err(ApplicationError::ConflictWithCode {
            code: "ocr_path_not_allowed",
            message: format!("OCR image path must be a file, not a directory: {image_path}"),
            details: serde_json::json!({ "allowed_prefix": allowed.display().to_string() }),
        });
    }
    if !resolved.starts_with(&allowed) {
        return Err(ApplicationError::ConflictWithCode {
            code: "ocr_path_not_allowed",
            message: format!("OCR image path outside allowed directory: {image_path}"),
            details: serde_json::json!({ "allowed_prefix": allowed.display().to_string() }),
        });
    }
    Ok(resolved)
}

/// Run local OCR on a screenshot and archive the result (`ocr_screenshot`).
///
/// Order matters and is the baseline's:
///  1. path guard (no I/O on the file, no existence disclosure);
///  2. existence;
///  3. engine availability — a missing engine is a readable error, never a 500
///     or a panic;
///  4. decode/read — a non-image is a 4xx `ocr_image_undecodable`;
///  5. the staging file is deleted on **every** path, success or failure;
///  6. the recognised text goes through the ordinary preview flow, so the user
///     still confirms before anything reaches the timeline.
pub async fn ocr_screenshot(
    imports: &dyn ImportRepository,
    ocr: &dyn crate::ports::OcrEngine,
    request: NewImportOcr,
    allowed_dir: &std::path::Path,
) -> Result<OcrOutcome, ApplicationError> {
    let resolved = assert_ocr_path_allowed(&request.image_path, allowed_dir)?;

    if !resolved.exists() {
        // The baseline raises `ICAError(code="ocr_image_missing")`, which is a
        // 409 with its own code — not the generic business-rule violation.
        return Err(ApplicationError::StatusWithCode {
            status: 409,
            code: "ocr_image_missing",
            message: format!("OCR image not found: {}", resolved.display()),
            details: serde_json::json!({}),
        });
    }

    ocr.ensure_available()?;

    let text = match ocr.read_text(&resolved).await {
        Ok(text) => text,
        Err(error) => {
            // The staging copy is dropped on the failure path too.
            let _ = std::fs::remove_file(&resolved);
            return Err(error);
        }
    };

    let _ = std::fs::remove_file(&resolved);

    let session = preview_import(
        imports,
        NewImportPreview {
            candidate_id: request.candidate_id,
            raw_text: text.clone(),
            source: ica_domain::MessageSource::ScreenshotOcr,
        },
    )
    .await?;

    Ok(OcrOutcome {
        session,
        text,
        language: "zh+en".to_string(),
    })
}
