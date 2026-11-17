//! ICA application layer: use cases and repository ports.
//!
//! Dependency rule (enforced by this crate's manifest): the domain plus the
//! ``async-trait`` proc macro. No axum, no sqlx, no tokio, no hyper — the
//! use cases talk to a port trait that ``ica-infra`` implements.

pub mod error;
pub mod ports;
pub mod positions;

pub use error::ApplicationError;
pub use ports::{PositionFilter, PositionRepository, RepositoryError};
pub use positions::{get_position, list_positions, ListPositions};
