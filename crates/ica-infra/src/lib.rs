//! ICA infrastructure layer: SQLite adapters.
//!
//! Everything that knows about SQL, connection pragmas or column encodings
//! lives here, so `ica-domain` and `ica-application` stay free of a database
//! driver. The adapter implements the application's `PositionRepository` port
//! and nothing else.

pub mod db;
pub mod repository;
pub mod time;

pub use db::{connect, table_names, DatabaseUrl, DatabaseUrlError};
pub use repository::SqlitePositionRepository;
pub use time::parse_sqlite_datetime;

/// The connection pool type, re-exported so the binary crate never needs a
/// direct `sqlx` dependency (and therefore cannot grow SQL of its own).
pub use sqlx::SqlitePool;
