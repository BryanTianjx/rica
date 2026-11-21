//! The backup/restore use cases (W5 backups).
//!
//! Thin pass-throughs over the [`BackupStore`] port: the file mechanics
//! (online-backup API, retention pruning, the pre-restore safety copy) live
//! in the adapter; the route owns the confirm guard and the name validation
//! order, exactly like the baseline's `restore_backup_endpoint`. The drill
//! in `docs/testing/backup-restore-drill.md` is the acceptance manual.

use crate::error::ApplicationError;
use crate::ports::{BackupStore, RestoreOutcome};

/// `GET /api/backups` — both families, newest first.
pub async fn list_backups(
    store: &dyn BackupStore,
) -> Result<Vec<crate::ports::BackupRecord>, ApplicationError> {
    store.list().await
}

/// `POST /api/backups` — one online-backup copy.
pub async fn create_backup(
    store: &dyn BackupStore,
) -> Result<crate::ports::BackupRecord, ApplicationError> {
    store.create().await
}

/// `POST /api/backups/{name}/restore` — the caller has validated `confirm`
/// and the file-name shape; the adapter takes the safety copy, restores in
/// place and writes the audit row.
pub async fn restore_backup(
    store: &dyn BackupStore,
    name: &str,
) -> Result<RestoreOutcome, ApplicationError> {
    store.restore(name).await
}
