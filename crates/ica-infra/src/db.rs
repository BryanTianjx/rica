//! Connection bootstrap: URL parsing, pragmas and schema introspection.
//!
//! The Python baseline configures SQLite through SQLAlchemy. The two settings
//! that change observable behaviour are reproduced here explicitly:
//! `foreign_keys=ON` (so a bad reference fails instead of silently landing)
//! and WAL (so a reader never blocks the writer). A read-only pool must not
//! try to enable WAL: SQLite cannot create the `-wal` file through a read-only
//! handle, and opening the baseline database must leave it byte-identical.

use std::fmt;
use std::str::FromStr;
use std::time::Duration;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::SqlitePool;

/// A parsed `sqlite://` database URL.
///
/// Mirrors the subset of SQLAlchemy's URL syntax the project actually uses:
///
/// * `sqlite:///./ica.db`            — relative to the process working dir
/// * `sqlite:////abs/path/ica.db`    — absolute (four slashes)
/// * `?mode=ro`                      — open read-only
/// * `?immutable=true`               — no locking at all (safe for a snapshot)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseUrl {
    /// Filesystem path (or `:memory:`), exactly as written in the URL.
    pub path: String,
    /// `mode=ro` was requested.
    pub read_only: bool,
    /// `immutable=true` was requested.
    pub immutable: bool,
}

/// Why a database URL could not be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseUrlError {
    message: String,
}

impl DatabaseUrlError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for DatabaseUrlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for DatabaseUrlError {}

impl DatabaseUrl {
    /// Parse a `sqlite://` URL.
    ///
    /// The scheme check is deliberate: silently treating a PostgreSQL URL as a
    /// file path would produce a confusing "unable to open database" error far
    /// from the misconfiguration.
    pub fn parse(raw: &str) -> Result<Self, DatabaseUrlError> {
        let (path_part, query) = match raw.split_once('?') {
            Some((path, query)) => (path, Some(query)),
            None => (raw, None),
        };

        let path = path_part
            .strip_prefix("sqlite:///")
            .or_else(|| path_part.strip_prefix("sqlite://"))
            .or_else(|| path_part.strip_prefix("sqlite:"))
            .ok_or_else(|| {
                DatabaseUrlError::new(format!(
                    "unsupported database URL (expected a sqlite: URL): {raw}"
                ))
            })?;

        if path.is_empty() {
            return Err(DatabaseUrlError::new("sqlite URL has an empty path"));
        }

        let mut read_only = false;
        let mut immutable = false;
        for pair in query
            .unwrap_or("")
            .split('&')
            .filter(|pair| !pair.is_empty())
        {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            match key.to_ascii_lowercase().as_str() {
                "mode" => match value.to_ascii_lowercase().as_str() {
                    // `mode=ro` is the only read-only spelling SQLAlchemy and
                    // sqlx share; rw/rwc both mean "may write".
                    "ro" => read_only = true,
                    "rw" | "rwc" | "" => read_only = false,
                    other => {
                        return Err(DatabaseUrlError::new(format!(
                            "unsupported sqlite mode '{other}'"
                        )))
                    }
                },
                "immutable" => {
                    immutable = matches!(
                        value.to_ascii_lowercase().as_str(),
                        "1" | "true" | "yes" | "on"
                    )
                }
                // Unknown parameters are ignored rather than rejected: the
                // baseline URL may carry driver-specific knobs we do not model.
                _ => {}
            }
        }

        Ok(Self {
            path: path.to_string(),
            read_only,
            immutable,
        })
    }
}

impl FromStr for DatabaseUrl {
    type Err = DatabaseUrlError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        Self::parse(raw)
    }
}

/// Open a pool for `url`, applying the project's pragmas.
///
/// Write connections get `foreign_keys=ON`, WAL and a busy timeout; read-only
/// connections get the same pragmas that do not require write access, and
/// leave the journal mode alone (a read-only handle cannot switch it).
pub async fn connect(url: &str) -> Result<SqlitePool, sqlx::Error> {
    let parsed =
        DatabaseUrl::parse(url).map_err(|error| sqlx::Error::Configuration(Box::new(error)))?;

    let mut options = SqliteConnectOptions::new()
        .filename(&parsed.path)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));

    if parsed.read_only {
        options = options.read_only(true);
    } else {
        options = options
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal);
    }

    if parsed.immutable {
        options = options.immutable(true);
    }

    SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
}

/// Every user table in the database, sorted, excluding SQLite internals.
///
/// The server's startup self-check compares this against the tables the binary
/// needs, so a database migrated by an older release is reported by name
/// instead of failing on the first query.
pub async fn table_names(pool: &SqlitePool) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT name FROM sqlite_master \
         WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )
    .fetch_all(pool)
    .await
}
