//! SQLite adapter for the browser candidate-extraction CONFIRM path (W7).
//!
//! The GUARDS live in `ica-application::extractions` (they are business
//! rules); this module is the SQL under three port calls:
//!
//! * `find_by_import_session` — the draft bound to one import session;
//! * `record_duplicate_block` — the committed refusal audit;
//! * `confirm_candidate` — the write path in ONE transaction: the candidate
//!   INSERT reuses the manual-create shape (validation, ownership stamping and
//!   the `candidate.created` audit identical to a manual create), then the
//!   extraction row and the session are confirmed, the draft text is cleared
//!   and `browser.candidate_extraction.confirmed` is recorded.
//!
//! Creating, listing, editing and reviewing drafts belong to the browser
//! router, whose migration is a separate scope decision (W7 card §5); they are
//! NOT implemented here.

use async_trait::async_trait;
use chrono::Utc;

use ica_application::{
    BrowserExtractionRepository, CandidateConfirm, CandidateConfirmOutcome, ExtractionDraft,
    RepositoryError,
};

use crate::audit::{insert_audit, AuditEntry};
use crate::store::{
    new_uuid, repo_error, to_sql_timestamp, SqliteStore, DEFAULT_TENANT_ID, LOCAL_OWNER_ID,
};

/// The columns the confirm path reads from a draft row, in query order.
const EXTRACTION_COLUMNS: &str = "id, session_id, snapshot_action_id, import_session_id, \
     source_url, source_domain, snapshot_sha256, fields_json, duplicate_kind, \
     duplicate_of_candidate_id, duplicate_evidence_json, status, review_digest, candidate_id";

#[derive(Debug, sqlx::FromRow)]
struct ExtractionRow {
    id: String,
    session_id: String,
    snapshot_action_id: String,
    import_session_id: Option<String>,
    source_url: String,
    source_domain: String,
    snapshot_sha256: String,
    fields_json: String,
    duplicate_kind: Option<String>,
    duplicate_of_candidate_id: Option<String>,
    duplicate_evidence_json: Option<String>,
    status: String,
    review_digest: Option<String>,
    candidate_id: Option<String>,
}

impl ExtractionRow {
    fn into_draft(self) -> ExtractionDraft {
        ExtractionDraft {
            id: self.id,
            session_id: self.session_id,
            snapshot_action_id: self.snapshot_action_id,
            import_session_id: self.import_session_id,
            source_url: self.source_url,
            source_domain: self.source_domain,
            snapshot_sha256: self.snapshot_sha256,
            fields_json: self.fields_json,
            duplicate_kind: self.duplicate_kind,
            duplicate_of_candidate_id: self.duplicate_of_candidate_id,
            duplicate_evidence_json: self.duplicate_evidence_json,
            status: self.status,
            review_digest: self.review_digest,
            candidate_id: self.candidate_id,
        }
    }
}

/// `_provenance_note`: the one-line, PII-free provenance string for
/// `Candidate.notes`, plus the reviewer's own note below it.
fn provenance_note(
    source_url: &str,
    snapshot_sha256: &str,
    snapshot_action_id: &str,
    extra: Option<&str>,
) -> String {
    let mut note = format!(
        "浏览器提取自 {source_url}（快照 sha256 {}…，动作 {snapshot_action_id}）",
        &snapshot_sha256[..12.min(snapshot_sha256.len())]
    );
    if let Some(extra) = extra {
        note.push('\n');
        note.push_str(extra);
    }
    note
}

#[async_trait]
impl BrowserExtractionRepository for SqliteStore {
    async fn find_by_import_session(
        &self,
        session_id: &str,
    ) -> Result<Option<ExtractionDraft>, RepositoryError> {
        let row: Option<ExtractionRow> = sqlx::query_as(&format!(
            "SELECT {EXTRACTION_COLUMNS} FROM browser_candidate_extractions \
             WHERE import_session_id = ?"
        ))
        .bind(session_id)
        .fetch_optional(self.pool())
        .await
        .map_err(repo_error)?;
        Ok(row.map(|row| row.into_draft()))
    }

    async fn record_duplicate_block(
        &self,
        extraction: &ExtractionDraft,
        session_id: &str,
    ) -> Result<(), RepositoryError> {
        let mut tx = self.pool().begin().await.map_err(repo_error)?;
        insert_audit(
            &mut tx,
            AuditEntry::new(
                "browser.candidate_extraction.duplicate_blocked",
                "browser_candidate_extraction",
                &extraction.id,
                "Refused to confirm a duplicate candidate without an override",
            )
            // The baseline's metadata keys, in its insertion order.
            .with_metadata(serde_json::json!({
                "duplicate_kind": extraction.duplicate_kind,
                "duplicate_of_candidate_id": extraction.duplicate_of_candidate_id,
                "import_session_id": session_id,
            })),
        )
        .await
        .map_err(repo_error)?;
        tx.commit().await.map_err(repo_error)
    }

    async fn confirm_candidate(
        &self,
        session_id: &str,
        draft: &ExtractionDraft,
        request: CandidateConfirm,
    ) -> Result<CandidateConfirmOutcome, RepositoryError> {
        let mut tx = self.pool().begin().await.map_err(repo_error)?;

        let fields = draft.present_fields();
        let field_value = |name: &str| -> Option<String> {
            fields
                .iter()
                .find(|(field, _)| *field == name)
                .map(|(_, value)| value.clone())
        };

        let name = field_value("name").unwrap_or_default();
        // CandidateCreate's pydantic rule, service-side: a blank name is a 422
        // with the validator's own message and loc (no `body` prefix — this is
        // schema-level validation, not route-level).
        if name.trim().is_empty() {
            return Err(RepositoryError::invalid(
                "Value error, name must not be blank",
                serde_json::json!({"errors": [{
                    "type": "value_error",
                    "loc": ["name"],
                    "msg": "Value error, name must not be blank",
                    "input": name,
                }]}),
            ));
        }
        if let Some(position_id) = &request.position_id {
            let exists: Option<String> =
                sqlx::query_scalar("SELECT id FROM positions WHERE id = ?")
                    .bind(position_id)
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(repo_error)?;
            if exists.is_none() {
                return Err(RepositoryError::not_found(format!(
                    "Position '{position_id}' was not found"
                )));
            }
        }

        let candidate_id = new_uuid();
        let now = to_sql_timestamp(&Utc::now());
        let source = format!("browser:{}", draft.source_domain)
            .chars()
            .take(64)
            .collect::<String>();
        let notes = provenance_note(
            &draft.source_url,
            &draft.snapshot_sha256,
            &draft.snapshot_action_id,
            request.note.as_deref(),
        );

        sqlx::query(concat!(
            "INSERT INTO candidates (id, name, source, position_id, experience_summary,",
            " skill_summary, expected_salary, expected_location, job_intention, status, notes,",
            " archived, created_at, updated_at, owner_id, tenant_id)",
            " VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?, ?, ?)"
        ))
        .bind(&candidate_id)
        .bind(&name)
        .bind(&source)
        .bind(&request.position_id)
        .bind(field_value("experience_summary"))
        .bind(field_value("skill_summary"))
        .bind(field_value("expected_salary"))
        .bind(field_value("expected_location"))
        .bind(field_value("job_intention"))
        .bind(request.status.as_str())
        .bind(&notes)
        .bind(&now)
        .bind(&now)
        .bind(LOCAL_OWNER_ID)
        .bind(DEFAULT_TENANT_ID)
        .execute(&mut *tx)
        .await
        .map_err(repo_error)?;

        insert_audit(
            &mut tx,
            AuditEntry::new(
                "candidate.created",
                "candidate",
                &candidate_id,
                "Created candidate",
            ),
        )
        .await
        .map_err(repo_error)?;

        sqlx::query(
            "UPDATE browser_candidate_extractions SET candidate_id = ?, status = 'confirmed', \
             confirmed_at = ?, updated_at = ? WHERE id = ?",
        )
        .bind(&candidate_id)
        .bind(&now)
        .bind(&now)
        .bind(&draft.id)
        .execute(&mut *tx)
        .await
        .map_err(repo_error)?;

        sqlx::query(
            "UPDATE import_sessions SET candidate_id = ?, status = 'confirmed', confirmed_at = ?, \
             raw_text = '', updated_at = ? WHERE id = ?",
        )
        .bind(&candidate_id)
        .bind(&now)
        .bind(&now)
        .bind(session_id)
        .execute(&mut *tx)
        .await
        .map_err(repo_error)?;

        // entity_id is the *candidate*: "from the archive back to the action
        // and the approval" is one query (AC-04-5). The metadata keys follow
        // the baseline's insertion order; `duplicate_override` here is the
        // plain flag (the RESPONSE's is `override && duplicate_kind.is_some()`).
        let mut fields_written: Vec<String> =
            fields.iter().map(|(name, _)| name.to_string()).collect();
        fields_written.sort();
        insert_audit(
            &mut tx,
            AuditEntry::new(
                "browser.candidate_extraction.confirmed",
                "candidate",
                &candidate_id,
                "Confirmed a browser extraction into the candidate archive",
            )
            .with_metadata(serde_json::json!({
                "extraction_id": draft.id,
                "import_session_id": session_id,
                "session_id": draft.session_id,
                "snapshot_action_id": draft.snapshot_action_id,
                "source_url": draft.source_url,
                "snapshot_sha256": draft.snapshot_sha256,
                "review_digest": draft.review_digest,
                "duplicate_kind": draft.duplicate_kind,
                "duplicate_override": request.confirm_duplicate,
                "fields_written": fields_written,
                "position_id": request.position_id,
            })),
        )
        .await
        .map_err(repo_error)?;

        tx.commit().await.map_err(repo_error)?;

        Ok(CandidateConfirmOutcome {
            session_id: session_id.to_string(),
            extraction_id: draft.id.clone(),
            candidate_id,
            duplicate_kind: draft.duplicate_kind.clone(),
            duplicate_override: request.confirm_duplicate && draft.duplicate_kind.is_some(),
            fields_written,
        })
    }
}
