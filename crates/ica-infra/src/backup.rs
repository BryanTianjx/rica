//! The backup/restore adapter (W5 backups).
//!
//! Every copy goes through the SQLite **online-backup API** (rusqlite's
//! `backup::Backup`, the same `sqlite3_backup` calls the baseline makes with
//! Python's `sqlite3.Connection.backup`): consistent snapshots of the live
//! WAL database while it stays open, and an in-place restore through the
//! same inode so already-open connections see the restored content instead
//! of holding stale handles to a replaced file. After a restore the adapter
//! runs `PRAGMA wal_checkpoint(TRUNCATE)` so the on-disk state is fully
//! checkpointed (the baseline recycles SQLAlchemy's pool; sqlx acquires a
//! connection per operation, so the checkpoint is the equivalent here).
//!
//! Retention and listing treat both families (`ica-backup-*` /
//! `pre-restore-*`) as one set, newest by NAME first — the same invariant the
//! baseline's prune relies on. The drill in
//! `docs/testing/backup-restore-drill.md` is the acceptance manual.

use std::path::{Path, PathBuf};

use rusqlite::Connection;

use ica_application::ports::{BackupRecord, BackupStore, RestoreOutcome};
use ica_application::ApplicationError;

use crate::store::{new_uuid, to_sql_timestamp, DEFAULT_TENANT_ID, LOCAL_ACTOR};

const BACKUP_PREFIX: &str = "ica-backup-";
const PRE_RESTORE_PREFIX: &str = "pre-restore-";
/// The baseline's `_SAFE_BACKUP_NAME` families.
const MANAGED_PREFIXES: [&str; 2] = [BACKUP_PREFIX, PRE_RESTORE_PREFIX];

/// The backup/restore file-subsystem adapter. Stateless apart from its three
/// paths, so cloning it into the blocking task is all the concurrency the
/// port needs.
#[derive(Clone)]
pub struct BackupManager {
    database_path: PathBuf,
    backup_dir: PathBuf,
    keep_count: i64,
}

impl BackupManager {
    pub fn new(database_path: PathBuf, backup_dir: PathBuf, keep_count: i64) -> Self {
        Self {
            database_path,
            backup_dir,
            keep_count,
        }
    }

    fn timestamp_suffix() -> String {
        let now = chrono::Utc::now();
        format!(
            "{}{:03}",
            now.format("%Y%m%dT%H%M%SZ"),
            now.timestamp_subsec_millis()
        )
    }

    fn managed_files(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&self.backup_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if name.ends_with(".db")
                    && MANAGED_PREFIXES
                        .iter()
                        .any(|prefix| name.starts_with(prefix))
                    && path.is_file()
                {
                    files.push(path);
                }
            }
        }
        // By NAME, oldest first — listing and pruning can never disagree
        // about which file is newest.
        files.sort_by_key(|path| path.file_name().unwrap_or_default().to_os_string());
        files
    }

    /// One consistent copy of `source` into `target` through the online
    /// backup API (the target must not exist: SQLite refuses to overwrite,
    /// which also guards the "fresh unique name" invariant).
    fn copy_database(source: &Path, target: &Path) -> Result<(), ApplicationError> {
        let outcome = (|| -> Result<(), rusqlite::Error> {
            let source = Connection::open(source)?;
            let mut destination = Connection::open(target)?;
            let backup = rusqlite::backup::Backup::new(&source, &mut destination)?;
            backup.run_to_completion(64, std::time::Duration::from_millis(5), None)
        })();
        outcome.map_err(|error| ApplicationError::StatusWithCode {
            status: 500,
            code: "backup_failed",
            message: format!("The backup failed: {error}"),
            details: serde_json::json!({}),
        })
    }

    fn restrict_permissions(path: &Path) -> Result<(), ApplicationError> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).map_err(
                |error| ApplicationError::StatusWithCode {
                    status: 500,
                    code: "backup_failed",
                    message: format!("could not restrict the backup permissions: {error}"),
                    details: serde_json::json!({}),
                },
            )?;
        }
        #[cfg(not(unix))]
        {
            let _ = path;
        }
        Ok(())
    }

    fn prune(&self) -> Result<(), ApplicationError> {
        if self.keep_count <= 0 {
            return Ok(());
        }
        let files = self.managed_files();
        let excess = files.len() as i64 - self.keep_count;
        if excess <= 0 {
            return Ok(());
        }
        for path in files.into_iter().take(excess as usize) {
            std::fs::remove_file(&path).map_err(|error| ApplicationError::StatusWithCode {
                status: 500,
                code: "backup_failed",
                message: format!("could not prune {}: {error}", path.display()),
                details: serde_json::json!({}),
            })?;
        }
        Ok(())
    }

    /// Remove `-wal`/`-shm` sidecars whose main database is gone.
    fn remove_orphan_sidecars(&self) {
        if let Ok(entries) = std::fs::read_dir(&self.backup_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                for suffix in ["-wal", "-shm"] {
                    if let Some(main) = name.strip_suffix(suffix) {
                        if !self.backup_dir.join(main).exists() {
                            let _ = std::fs::remove_file(&path);
                        }
                    }
                }
            }
        }
    }

    fn record(&self, path: &Path) -> Result<BackupRecord, ApplicationError> {
        let metadata =
            std::fs::metadata(path).map_err(|error| ApplicationError::StatusWithCode {
                status: 500,
                code: "backup_failed",
                message: format!("cannot stat the backup file: {error}"),
                details: serde_json::json!({}),
            })?;
        let modified: std::time::SystemTime = metadata
            .modified()
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        let created_at: chrono::DateTime<chrono::Utc> = modified.into();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        let kind = if name.starts_with(PRE_RESTORE_PREFIX) {
            "pre_restore"
        } else {
            "backup"
        };
        Ok(BackupRecord {
            name,
            path: path.display().to_string(),
            size_bytes: metadata.len() as i64,
            created_at: created_at.to_rfc3339(),
            kind: kind.to_string(),
        })
    }
}

#[async_trait::async_trait]
impl BackupStore for BackupManager {
    async fn list(&self) -> Result<Vec<BackupRecord>, ApplicationError> {
        let manager = self.clone();
        tokio::task::spawn_blocking(move || {
            let mut records = Vec::new();
            for path in manager.managed_files().into_iter().rev() {
                records.push(manager.record(&path)?);
            }
            Ok(records)
        })
        .await
        .map_err(|error| ApplicationError::Unavailable {
            message: format!("backup task failed: {error}"),
        })?
    }

    async fn create(&self) -> Result<BackupRecord, ApplicationError> {
        let manager = self.clone();
        tokio::task::spawn_blocking(move || {
            if !manager.database_path.is_file() {
                return Err(ApplicationError::StatusWithCode {
                    status: 409,
                    code: "backup_source_missing",
                    message: format!(
                        "No database file found at '{}'; there is nothing to back up.",
                        manager.database_path.display()
                    ),
                    details: serde_json::json!({}),
                });
            }
            std::fs::create_dir_all(&manager.backup_dir).map_err(|error| {
                ApplicationError::StatusWithCode {
                    status: 500,
                    code: "backup_failed",
                    message: format!("cannot create the backup directory: {error}"),
                    details: serde_json::json!({}),
                }
            })?;
            let target = manager.backup_dir.join(format!(
                "ica-backup-{}.db",
                BackupManager::timestamp_suffix()
            ));
            Self::copy_database(&manager.database_path, &target)?;
            Self::restrict_permissions(&target)?;
            manager.prune()?;
            manager.remove_orphan_sidecars();
            manager.record(&target)
        })
        .await
        .map_err(|error| ApplicationError::Unavailable {
            message: format!("backup task failed: {error}"),
        })?
    }

    async fn restore(&self, name: &str) -> Result<RestoreOutcome, ApplicationError> {
        let manager = self.clone();
        let name = name.to_string();
        tokio::task::spawn_blocking(move || {
            // Defense in depth: the route validates the shape, and the
            // adapter re-checks the managed-name family (a name is not a
            // path — no separators, no traversal).
            let shape_ok = name.ends_with(".db")
                && MANAGED_PREFIXES
                    .iter()
                    .any(|prefix| name.starts_with(prefix))
                && !name.contains('/')
                && !name.contains('\\');
            if !shape_ok {
                return Err(ApplicationError::StatusWithCode {
                    status: 422,
                    code: "backup_not_managed",
                    message: format!(
                        "'{name}' is not a managed backup name; only backups this \
                         application produced (ica-backup-*, pre-restore-*) can be restored."
                    ),
                    details: serde_json::json!({}),
                });
            }
            let source = manager.backup_dir.join(&name);
            if !source.is_file() {
                return Err(ApplicationError::StatusWithCode {
                    status: 404,
                    code: "backup_missing",
                    message: format!("No backup file at '{}'.", source.display()),
                    details: serde_json::json!({}),
                });
            }
            std::fs::create_dir_all(&manager.backup_dir).map_err(|error| {
                ApplicationError::StatusWithCode {
                    status: 500,
                    code: "restore_failed",
                    message: format!("cannot create the backup directory: {error}"),
                    details: serde_json::json!({}),
                }
            })?;

            // The pre-restore undo point goes through the backup API too: a
            // raw file copy of a WAL database drops the not-yet-checkpointed
            // transactions (the baseline hit exactly this).
            let stamp = BackupManager::timestamp_suffix();
            let safety = manager.backup_dir.join(format!("pre-restore-{stamp}.db"));
            if manager.database_path.exists() {
                Self::copy_database(&manager.database_path, &safety)?;
                Self::restrict_permissions(&safety)?;
            }

            // The restore itself: the backup file's content flows into the
            // LIVE database through the same file (the online backup API
            // copies page by page into the existing target), so open
            // connections stay valid.
            Self::copy_database(&source, &manager.database_path)?;
            let checkpoint = Connection::open(&manager.database_path).and_then(|connection| {
                connection
                    .query_row("PRAGMA wal_checkpoint(TRUNCATE);", [], |_| Ok(()))
                    .map(|_| 0)
            });
            checkpoint.map_err(|error| ApplicationError::Unavailable {
                message: format!("checkpoint failed: {error}"),
            })?;

            // Restores are the only writer of pre-restore-* undo points, so
            // they are bounded here too.
            manager.prune()?;
            manager.remove_orphan_sidecars();

            // The trail: the restore replaced the live data, so it is
            // recorded with the safety copy it produced (written through the
            // same connection family as the baseline's record_audit).
            let mut connection = Connection::open(&manager.database_path).map_err(|error| {
                ApplicationError::Unavailable {
                    message: format!("cannot reopen the live database: {error}"),
                }
            })?;
            write_audit(
                &mut connection,
                "backup.restored",
                Some("backup"),
                Some(&name),
                &format!("Restored the database from backup '{name}'"),
                &serde_json::json!({ "previous_copy": safety.display().to_string() }),
            )
            .map_err(|error| ApplicationError::Unavailable {
                message: format!("audit write failed: {error}"),
            })?;

            Ok(RestoreOutcome {
                restored_from: source.display().to_string(),
                database: manager.database_path.display().to_string(),
                previous_copy: safety.display().to_string(),
            })
        })
        .await
        .map_err(|error| ApplicationError::Unavailable {
            message: format!("restore task failed: {error}"),
        })?
    }
}

/// The audit row, written through rusqlite into the restored database.
fn write_audit(
    connection: &mut Connection,
    event_type: &str,
    entity_type: Option<&str>,
    entity_id: Option<&str>,
    summary: &str,
    metadata: &serde_json::Value,
) -> Result<(), rusqlite::Error> {
    let now = to_sql_timestamp(&chrono::Utc::now());
    connection.execute(
        "INSERT INTO audit_events (id, event_type, entity_type, entity_id, actor, \
         summary, metadata_json, tenant_id, actor_id, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, NULL, ?, ?)",
        rusqlite::params![
            new_uuid(),
            event_type,
            entity_type,
            entity_id,
            LOCAL_ACTOR,
            summary,
            serde_json::to_string(metadata).ok(),
            DEFAULT_TENANT_ID,
            &now,
            &now,
        ],
    )?;
    Ok(())
}
