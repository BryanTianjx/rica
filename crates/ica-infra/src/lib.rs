//! ICA infrastructure layer: SQLite adapters.
//!
//! Everything that knows about SQL, connection pragmas or column encodings
//! lives here, so `ica-domain` and `ica-application` stay free of a database
//! driver. [`SqliteStore`] implements every application port over one pool, so
//! a business write and the audit event it produces share a transaction.

pub mod ai_client;
pub mod ai_models;
pub mod audit;
pub mod auth_store;
pub mod backup;
pub mod candidates;
pub mod column_defaults;
pub mod conversations;
pub mod db;
pub mod drafts;
pub mod export_crypto;
pub mod export_tables;
pub mod extractions;
pub mod follow_ups;
pub mod imports;
pub mod json_fmt;
pub mod migrations;
pub mod ocr_rapid;
pub mod repository;
pub mod secret_store;
pub mod send_records;
pub mod settings;
pub mod snapshots;
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
