//! Browser candidate-extraction use cases (W7): the confirm path.
//!
//! The pipeline is page → draft → human review → confirm. Only the LAST step
//! lives here for now: `confirm_extraction` is reached from
//! `POST /api/imports/{session_id}/confirm-candidate` and is the only path
//! from a page into `candidates` (baseline §6.2 — the browser router can draft
//! and review, never write). The draft/review endpoints stay with the browser
//! router's scope decision (W7 card §5).
//!
//! The guards below are the AC-04-4 negative path — nothing is written unless
//! a human approved *this exact* field set:
//!
//! * the session must BE a browser extraction draft (`wrong_import_type`, 409);
//! * a draft must exist for the session (`extraction_not_found`, 404);
//! * an already-confirmed draft refuses again (`extraction_already_confirmed`);
//! * only `approved` may confirm (`review_required`);
//! * the approval's digest must still match the fields (`review_stale`);
//! * a stored `exact`/`name` duplicate verdict blocks unless the caller
//!   explicitly overrides (`duplicate_candidate`) — and the BLOCK is audited
//!   and committed before the refusal.

use serde_json::json;

use crate::error::ApplicationError;
use crate::ports::{
    BrowserExtractionRepository, CandidateConfirm, CandidateConfirmOutcome, ImportRepository,
};

fn unavailable(error: crate::ports::RepositoryError) -> ApplicationError {
    error.application_error()
}

/// `BrowserError(code, message, details=...)`: the browser protocol's stable
/// codes map to their own statuses (`protocol.py::ERROR_STATUS`); unknown
/// codes would be a 500, never a 200.
fn browser_error(
    code: &'static str,
    message: impl Into<String>,
    details: serde_json::Value,
) -> ApplicationError {
    ApplicationError::StatusWithCode {
        status: match code {
            "extraction_not_found" => 404,
            "wrong_import_type"
            | "extraction_already_confirmed"
            | "review_required"
            | "review_stale"
            | "duplicate_candidate" => 409,
            _ => 500,
        },
        code,
        message: message.into(),
        details,
    }
}

/// Confirm a browser extraction draft (`confirm_candidate_import` +
/// `candidate_extraction.confirm_extraction`).
pub async fn confirm_candidate(
    imports: &dyn ImportRepository,
    extractions: &dyn BrowserExtractionRepository,
    session_id: &str,
    request: CandidateConfirm,
) -> Result<CandidateConfirmOutcome, ApplicationError> {
    // The route resolves the session first: an unknown session is the import
    // 404, and a session that is not a browser draft is `wrong_import_type`
    // (409) — the message-shaped confirm body does not apply to it.
    let session = imports
        .find(session_id)
        .await
        .map_err(unavailable)?
        .ok_or_else(|| ApplicationError::not_found("Import session", session_id))?;

    if session.source != ica_domain::MessageSource::BrowserExtract.as_str() {
        return Err(browser_error(
            "wrong_import_type",
            "This import session is not a browser extraction draft",
            json!({
                "session_id": session_id,
                "source": session.source,
            }),
        ));
    }

    let draft = extractions
        .find_by_import_session(session_id)
        .await
        .map_err(unavailable)?
        .ok_or_else(|| {
            browser_error(
                "extraction_not_found",
                "This import session is not a browser extraction draft",
                json!({ "session_id": session_id }),
            )
        })?;

    if draft.status == "confirmed" {
        return Err(browser_error(
            "extraction_already_confirmed",
            "This draft was already confirmed",
            json!({
                "extraction_id": draft.id,
                "candidate_id": draft.candidate_id,
            }),
        ));
    }

    if draft.status != "approved" {
        return Err(browser_error(
            "review_required",
            "The draft must be approved by a human before it is written",
            json!({
                "extraction_id": draft.id,
                "status": draft.status,
                "import_session_id": session_id,
            }),
        ));
    }

    // The approval binds a digest of the fields a human actually read; if the
    // draft no longer renders to the same text — edited after approval, or two
    // writes raced — refuse rather than write content nobody approved.
    if draft.review_digest_now() != draft.review_digest.as_deref().unwrap_or("") {
        return Err(browser_error(
            "review_stale",
            "The approved content no longer matches the draft; review it again",
            json!({ "extraction_id": draft.id }),
        ));
    }

    if matches!(
        draft.duplicate_kind.as_deref(),
        Some("exact") | Some("name")
    ) && !request.confirm_duplicate
    {
        // The BLOCK is audited and committed BEFORE the 409: the caller can
        // query why the confirmation was refused (AC-04-3 evidence).
        extractions
            .record_duplicate_block(&draft, session_id)
            .await
            .map_err(unavailable)?;
        let evidence: serde_json::Value = draft
            .duplicate_evidence_json
            .as_deref()
            .and_then(|text| serde_json::from_str(text).ok())
            .unwrap_or_else(|| json!({}));
        return Err(browser_error(
            "duplicate_candidate",
            "This candidate already exists; confirm again with confirm_duplicate=true",
            json!({
                "extraction_id": draft.id,
                "duplicate_kind": draft.duplicate_kind,
                "candidate_id": draft.duplicate_of_candidate_id,
                "evidence": evidence,
            }),
        ));
    }

    extractions
        .confirm_candidate(session_id, &draft, request)
        .await
        .map_err(unavailable)
}
