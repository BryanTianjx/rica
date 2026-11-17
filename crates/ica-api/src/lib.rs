//! ICA HTTP layer: axum routes, DTOs and error mapping.
//!
//! This crate is the only place that knows about HTTP. It translates a request
//! into an application use case and the result into the exact JSON the Python
//! baseline emits, so an existing frontend cannot tell the two apart.

pub mod dto;
pub mod error;
pub mod query;
pub mod routes;
pub mod state;

pub use dto::{HealthResponse, PageResponse, PositionRead};
pub use error::{ApiError, ValidationIssue};
pub use query::{RawListPositionsQuery, DEFAULT_LIMIT, MAX_LIMIT};
pub use routes::router;
pub use state::{AppConfig, AppState};
