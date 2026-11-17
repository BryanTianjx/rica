//! ICA infrastructure layer: SQLite adapters.
//!
//! Everything that knows about SQL, connection pragmas or column encodings
//! lives here, so `ica-domain` and `ica-application` stay free of a database
//! driver. [`SqliteStore`] implements every application port over one pool, so
//! a business write and the audit event it produces share a transaction.

pub mod audit;
pub mod candidates;
pub mod conversations;
pub mod db;
pub mod follow_ups;
pub mod repository;
pub mod send_records;
pub mod store;
pub mod time;

pub use db::{connect, table_names, DatabaseUrl, DatabaseUrlError};
pub use store::SqliteStore;
pub use time::parse_sqlite_datetime;

/// R0's name for the adapter.
///
/// It is the same object as [`SqliteStore`]: `positions` was the only aggregate
/// R0 served, so the name is kept rather than churning every call site while the
/// other waves land.
pub type SqlitePositionRepository = SqliteStore;

/// The connection pool type, re-exported so the binary crate never needs a
/// direct `sqlx` dependency (and therefore cannot grow SQL of its own).
pub use sqlx::SqlitePool;
