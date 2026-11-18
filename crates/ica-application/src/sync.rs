//! Data snapshot export and import (`POST /export`, `POST /import`).
//!
//! Export produces a JSON document in the response body — not a file download:
//! the baseline sets no `Content-Disposition` and the `version` stamp is the
//! literal `"0.2.0"`. An optional `X-Encrypt-Aes-Key` seals that document with
//! AES-256-GCM and returns the opaque envelope instead.
//!
//! Two ordering rules from the baseline are preserved:
//!
//! 1. **The key is validated and the document sealed before the audit row is
//!    written.** A rejected key (422) or a sealing failure therefore leaves no
//!    `data.exported` event behind — the trail never claims an export succeeded
//!    when it did not.
//! 2. **`exported_at` is `isoformat()`, not the wire `Z` format.** It carries
//!    `+00:00` and microseconds because the baseline builds it as a string and
//!    puts it in a `str` field.

use chrono::{SecondsFormat, Utc};

use crate::error::ApplicationError;
use crate::ports::{DataSnapshotRepository, ExportSealer, RepositoryError, SnapshotOutcome};

/// The export document before any sealing (`{version, exported_at, data}`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportSnapshot {
    pub version: String,
    /// `isoformat()` shape: microseconds and `+00:00`.
    pub exported_at: String,
    pub data: serde_json::Map<String, serde_json::Value>,
}

/// A sealed export (`{alg, iv, tag, ciphertext}`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SealedSnapshot {
    pub iv: String,
    pub tag: String,
    pub ciphertext: String,
}

/// What `export_data` produced: plaintext, or the sealed envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportOutcome {
    Plain(ExportSnapshot),
    Sealed(SealedSnapshot),
}

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

/// Export every business table the caller may see (`export_data`).
///
/// With `encrypt_key` set the snapshot is sealed first; the audit event is only
/// written once a document actually exists.
pub async fn export_data(
    snapshots: &dyn DataSnapshotRepository,
    sealer: &dyn ExportSealer,
    encrypt_key: Option<&str>,
) -> Result<ExportOutcome, ApplicationError> {
    let data = snapshots.export_tables().await.map_err(unavailable)?;
    let snapshot = ExportSnapshot {
        version: "0.2.0".to_string(),
        exported_at: Utc::now().to_rfc3339_opts(SecondsFormat::Micros, false),
        data,
    };

    let outcome = match encrypt_key {
        None => ExportOutcome::Plain(snapshot),
        Some(key) => {
            let document = serde_json::json!({
                "version": snapshot.version,
                "exported_at": snapshot.exported_at,
                "data": snapshot.data,
            });
            let (iv, tag, ciphertext) = sealer.seal(&document, key)?;
            ExportOutcome::Sealed(SealedSnapshot {
                iv,
                tag,
                ciphertext,
            })
        }
    };

    // Only now is the export real, so only now may the audit claim it.
    snapshots
        .record_export_audit(&outcome)
        .await
        .map_err(unavailable)?;
    Ok(outcome)
}

/// Import a JSON snapshot (`import_data`): idempotent upsert, one transaction.
pub async fn import_data(
    snapshots: &dyn DataSnapshotRepository,
    data: serde_json::Map<String, serde_json::Value>,
) -> Result<SnapshotOutcome, ApplicationError> {
    let tables: Vec<String> = data.keys().cloned().collect();
    let outcome = snapshots.import_tables(data).await.map_err(unavailable)?;
    snapshots
        .record_import_audit(&outcome, &tables)
        .await
        .map_err(unavailable)?;
    Ok(outcome)
}
