//! The axum binding of the contract's query parsing.
//!
//! This is the only place HTTP specifics touch the query contract: axum's
//! built-in `Query<T>` rejects a repeated key (`?limit=1&limit=2`) with a
//! plain-text 400 — a format the frontend's single error path cannot parse,
//! and a semantic divergence too: Starlette answers that request 200 with the
//! **last** value. This extractor decodes with the same rules as Python's
//! `urllib.parse.parse_qsl` (form decoding: `+` is a space, `%XX` decoded,
//! invalid UTF-8 replaced, a bare key becomes an empty value) and keeps the
//! last occurrence of every key before deserialising.

use std::collections::HashSet;

use ica_contracts::error::ContractError;
use serde::de::DeserializeOwned;

use axum::extract::FromRequestParts;

/// A query extractor that mirrors the Python baseline's `QueryParams`.
pub struct ApiQuery<T>(pub T);

impl<S, T> FromRequestParts<S> for ApiQuery<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let raw = parts.uri.query().unwrap_or("");
        let pairs = last_value_wins(raw);
        let encoded = form_urlencoded::Serializer::new(String::new())
            .extend_pairs(
                pairs
                    .iter()
                    .map(|(key, value)| (key.as_str(), value.as_str())),
            )
            .finish();
        let value = serde_urlencoded::from_str(&encoded).map_err(|error| {
            // Unreachable for the current query structs: every field is an
            // `Option<String>` and keys are unique after the merge. Mapped to
            // the envelope anyway, so a future field can never reintroduce
            // axum's plain-text rejection here.
            ApiError(ContractError::Http {
                status: 400,
                message: format!("Failed to deserialize query string: {error}"),
            })
        })?;
        Ok(ApiQuery(value))
    }
}

/// The HTTP-facing rejection type: the contract error bound to axum.
pub use crate::error::ApiError;

/// Decode a raw query string into pairs, keeping only the **last** occurrence
/// of every key, in the order those last occurrences appear.
fn last_value_wins(raw: &str) -> Vec<(String, String)> {
    let mut pairs: Vec<(String, String)> = form_urlencoded::parse(raw.as_bytes())
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    let mut seen = HashSet::new();
    pairs.reverse();
    pairs.retain(|(key, _)| seen.insert(key.clone()));
    pairs.reverse();
    pairs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_last_occurrence_of_a_repeated_key_wins() {
        let pairs = last_value_wins("limit=1&limit=2&search=x&search=");
        assert_eq!(
            pairs,
            vec![
                ("limit".to_string(), "2".to_string()),
                ("search".to_string(), String::new()),
            ]
        );
        // A bare key arrives as an empty value, like `parse_qsl(keep_blank_values=True)`.
        assert_eq!(
            last_value_wins("limit"),
            vec![("limit".to_string(), String::new())]
        );
    }
}
