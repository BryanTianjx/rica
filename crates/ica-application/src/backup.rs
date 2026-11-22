//! The backup/restore use cases (W5 backups).
//!
//! Thin pass-throughs over the [`BackupStore`] port: the file mechanics
//! (online-backup API, retention pruning, the pre-restore safety copy) live
//! in the adapter; the route owns the confirm guard and the name validation
//! order, exactly like the baseline's `restore_backup_endpoint`. The drill
//! in `docs/testing/backup-restore-drill.md` is the acceptance manual.

use std::collections::BTreeMap;

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

/// `POST /api/backups/{name}/restore` — full business rule, shared by HTTP and
/// IPC (ARCH-P2 AC3/AC5). The baseline's guard order is contract: confirm is
/// checked before the file-name shape, and existence before any file is
/// touched. Keeping all three here stops one transport from forgetting the
/// 404 pre-check (the IPC path used to skip it) and leaves each adapter with
/// parse → call → DTO only.
pub async fn restore_backup_checked(
    store: &dyn BackupStore,
    name: &str,
    confirm: bool,
) -> Result<RestoreOutcome, ApplicationError> {
    let mut details = BTreeMap::new();
    details.insert("backup".to_string(), name.to_string());

    if !confirm {
        return Err(ApplicationError::Conflict {
            message:
                "Restoring a backup replaces the current data. Send confirm=true once you are sure."
                    .to_string(),
            details,
        });
    }
    if name.contains('/') || name.contains('\\') || name.is_empty() || name == "." || name == ".." {
        return Err(ApplicationError::Validation {
            message: "A backup is addressed by its file name, not by a path.".to_string(),
            details,
        });
    }
    let known = store.list().await?.iter().any(|record| record.name == name);
    if !known {
        return Err(ApplicationError::NotFound {
            message: format!("No backup file named '{name}' was found"),
        });
    }
    // `details` was moved into one of the guards above on the error paths;
    // the success path needs nothing from it.
    restore_backup(store, name).await
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
