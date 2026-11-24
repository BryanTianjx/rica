//! The single SQLite adapter object and the helpers every repository shares.
//!
//! One struct implements every application port. It owns one connection pool,
//! so a write and the audit event it produces can share a transaction, and the
//! HTTP layer never sees two adapters pointing at the same file.
//!
//! ## Conventions that exist for parity with Python, not for taste
//!
//! * **Timestamps** are stored as the text SQLAlchemy writes:
//!   `YYYY-MM-DD HH:MM:SS.ffffff`, UTC, no offset. A value that arrives with a
//!   different shape is still readable (`parse_sqlite_datetime`), but everything
//!   written here uses that one shape so the rows compare byte-for-byte.
//! * **Booleans** are `INTEGER` 0/1.
//! * **Ownership columns** are stamped with the local-mode defaults
//!   (`owner_id` = the V1.0 local user, `tenant_id` = the default tenant), the
//!   same values the Python `_owner_tenant()` helper returns for a local caller.
//! * **Audit rows** carry `actor='local_user'` and `actor_id=NULL` in local
//!   mode, and use one shared writer so the column list cannot drift.

use chrono::{DateTime, Utc};
use sqlx::SqlitePool;

/// `settings.default_user_id` — the V1.0 local user every row belongs to.
pub const LOCAL_OWNER_ID: &str = "00000000-0000-0000-0000-000000000001";
/// `settings.tenant_id_default` — the V1.0 default tenant.
pub const DEFAULT_TENANT_ID: &str = "00000000-0000-0000-0000-000000000000";
/// `send_record_service.LOCAL_ACTOR` — the actor label for local callers.
pub const LOCAL_ACTOR: &str = "local_user";

/// A fresh UUID4 string, like `app.models.mixins.new_uuid`.
pub fn new_uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Render an instant the way SQLAlchemy stores a naive UTC datetime.
pub fn to_sql_timestamp(value: &DateTime<Utc>) -> String {
    value.format("%Y-%m-%d %H:%M:%S%.6f").to_string()
}

/// The `OFFSET ?` bind value for a page window.
///
/// SQLite offsets are `i64`, and a `usize as i64` past `i64::MAX` wraps to a
/// negative number, which SQLite treats as "no offset" — an attacker-style
/// `?offset=9223372036854775808` would silently serve page one instead of an
/// empty window (caught by `positions_offset_beyond_i64` in the contract). Any
/// offset past `i64::MAX` is past the end of every table anyway, so clamping
/// to `i64::MAX` yields the same empty page the Python slice would.
pub fn offset_bind(offset: usize) -> i64 {
    i64::try_from(offset).unwrap_or(i64::MAX)
}

/// Every port in one adapter over one pool.
#[derive(Debug, Clone)]
pub struct SqliteStore {
    pool: SqlitePool,
}

impl SqliteStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// The pool this adapter uses.
    ///
    /// Exposed so the binary's startup self-check can reuse the same handle
    /// instead of opening a second one to the same file.
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

/// Wrap an adapter error into the application's opaque error.
pub(crate) fn repo_error(error: sqlx::Error) -> ica_application::RepositoryError {
    ica_application::RepositoryError::unavailable(error.to_string())
}

/// Run `SELECT COUNT(*)`-style queries over an optional `WHERE` clause.
pub(crate) fn where_clause<S: AsRef<str>>(conditions: &[S]) -> String {
    if conditions.is_empty() {
        String::new()
    } else {
        format!(
            " WHERE {}",
            conditions
                .iter()
                .map(|condition| condition.as_ref())
                .collect::<Vec<_>>()
                .join(" AND ")
        )
    }
}

/// SEC-04 remediation: the `WHERE` predicates a repository must add for an
/// authenticated scope, with their binds in the same order.
///
/// `None` is Local mode and contributes nothing. `Some` always restricts the
/// tenant; a non-admin additionally sees only rows it owns. Predicates are
/// built here so every adapter uses the exact same tenant/owner rule, and the
/// binds never become part of SQL text.
pub(crate) fn scope_predicates(
    scope: Option<&ica_application::Scope>,
    alias: &str,
) -> (Vec<String>, Vec<String>) {
    let Some(scope) = scope else {
        return (Vec::new(), Vec::new());
    };
    let mut predicates = vec![format!("{alias}.tenant_id = ?")];
    let mut binds = vec![scope.tenant_id.clone()];
    if !scope.is_admin {
        predicates.push(format!("{alias}.owner_id = ?"));
        binds.push(scope.actor_id.clone());
    }
    (predicates, binds)
}

/// Which ownership stamps a newly inserted row receives: the Local-mode
/// defaults for `None`, otherwise the authenticated actor's identity.
pub(crate) fn ownership(scope: Option<&ica_application::Scope>) -> (&str, &str) {
    match scope {
        Some(scope) => (&scope.actor_id, &scope.tenant_id),
        None => (LOCAL_OWNER_ID, DEFAULT_TENANT_ID),
    }
}
