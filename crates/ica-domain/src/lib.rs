//! ICA domain layer: entities, enumerations and pure pagination/search rules.
//!
//! This crate intentionally has no dependency on a web framework, a database
//! driver or an async runtime, so domain code cannot accidentally take on an
//! infrastructure concern: the layering rule is enforced by the compiler.

pub mod pagination;
pub mod position;
pub mod search;

pub use pagination::Page;
pub use position::{Position, PositionStatus};
pub use search::{normalize_search, MAX_SEARCH_LEN};
