//! Query-string parsing and validation for `GET /api/positions`.
//!
//! FastAPI validates these with `Query(...)` constraints and answers 422 with
//! the project's envelope; the port has to reproduce both the accepted values
//! and the rejection codes, because the frontend and the desktop shell send
//! `0`/`1` for booleans and rely on `limit` being clamped, not silently fixed.

use serde::Deserialize;

use ica_application::ListPositions;
use ica_domain::{normalize_search, MAX_SEARCH_LEN};

use crate::error::{ApiError, ValidationIssue};

/// Page size when the caller does not ask for one (`Query(default=50)`).
pub const DEFAULT_LIMIT: usize = 50;
/// Upper bound (`Query(le=500)`).
pub const MAX_LIMIT: usize = 500;

/// Raw query values, exactly as they arrived: no coercion has happened yet.
#[derive(Debug, Default, Clone, Deserialize)]
pub struct RawListPositionsQuery {
    pub include_archived: Option<String>,
    pub search: Option<String>,
    pub offset: Option<String>,
    pub limit: Option<String>,
}

/// Interpret a boolean the way pydantic does.
///
/// The accepted spellings matter: `include_archived=1` is what a browser form
/// and the packaged shell send, and rejecting it would be a silent behaviour
/// change from the Python service.
fn parse_bool(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "t" | "yes" | "y" | "on" => Some(true),
        "0" | "false" | "f" | "no" | "n" | "off" => Some(false),
        _ => None,
    }
}

/// Parse a non-negative integer query value.
fn parse_non_negative(raw: &str, field: &str) -> Result<usize, ValidationIssue> {
    match raw.trim().parse::<i64>() {
        Ok(value) if value >= 0 => Ok(value as usize),
        Ok(_) => Err(ValidationIssue::new(
            "greater_than_equal",
            vec!["query", field],
            "Input should be greater than or equal to 0",
            raw,
        )),
        Err(_) => Err(ValidationIssue::new(
            "int_parsing",
            vec!["query", field],
            "Input should be a valid integer, unable to parse string as an integer",
            raw,
        )),
    }
}

/// Parse `limit`: `1..=500`.
fn parse_limit(raw: &str) -> Result<usize, ValidationIssue> {
    match raw.trim().parse::<i64>() {
        Ok(value) if value <= 0 => Err(ValidationIssue::new(
            "greater_than",
            vec!["query", "limit"],
            "Input should be greater than 0",
            raw,
        )),
        Ok(value) if value > MAX_LIMIT as i64 => Err(ValidationIssue::new(
            "less_than_equal",
            vec!["query", "limit"],
            "Input should be less than or equal to 500",
            raw,
        )),
        Ok(value) => Ok(value as usize),
        Err(_) => Err(ValidationIssue::new(
            "int_parsing",
            vec!["query", "limit"],
            "Input should be a valid integer, unable to parse string as an integer",
            raw,
        )),
    }
}

/// Validate every parameter, reporting all problems at once like pydantic.
pub fn validate(raw: RawListPositionsQuery) -> Result<ListPositions, ApiError> {
    let mut issues = Vec::new();

    let include_archived = match raw.include_archived.as_deref() {
        None => false,
        Some(value) => match parse_bool(value) {
            Some(parsed) => parsed,
            None => {
                issues.push(ValidationIssue::new(
                    "bool_parsing",
                    vec!["query", "include_archived"],
                    "Input should be a valid boolean, unable to interpret input",
                    value,
                ));
                false
            }
        },
    };

    let offset = match raw.offset.as_deref() {
        None => 0,
        Some(value) => match parse_non_negative(value, "offset") {
            Ok(parsed) => parsed,
            Err(issue) => {
                issues.push(issue);
                0
            }
        },
    };

    let limit = match raw.limit.as_deref() {
        None => DEFAULT_LIMIT,
        Some(value) => match parse_limit(value) {
            Ok(parsed) => parsed,
            Err(issue) => {
                issues.push(issue);
                DEFAULT_LIMIT
            }
        },
    };

    // `max_length=200` counts characters, not bytes.
    let search = match raw.search.as_deref() {
        None => None,
        Some(value) if value.chars().count() > MAX_SEARCH_LEN => {
            issues.push(ValidationIssue::new(
                "string_too_long",
                vec!["query", "search"],
                "String should have at most 200 characters",
                value,
            ));
            None
        }
        Some(value) => normalize_search(value),
    };

    if !issues.is_empty() {
        return Err(ApiError::Validation { issues });
    }

    Ok(ListPositions {
        include_archived,
        search,
        offset,
        limit,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(
        include_archived: Option<&str>,
        offset: Option<&str>,
        limit: Option<&str>,
    ) -> RawListPositionsQuery {
        RawListPositionsQuery {
            include_archived: include_archived.map(str::to_string),
            search: None,
            offset: offset.map(str::to_string),
            limit: limit.map(str::to_string),
        }
    }

    #[test]
    fn defaults_match_the_python_signature() {
        let query = validate(raw(None, None, None)).unwrap();
        assert!(!query.include_archived);
        assert_eq!(query.offset, 0);
        assert_eq!(query.limit, 50);
        assert_eq!(query.search, None);
    }

    #[test]
    fn all_pydantic_boolean_spellings_are_accepted() {
        for value in ["1", "true", "TRUE", "t", "yes", "y", "on"] {
            assert_eq!(parse_bool(value), Some(true), "{value}");
        }
        for value in ["0", "false", "False", "f", "no", "n", "off"] {
            assert_eq!(parse_bool(value), Some(false), "{value}");
        }
        assert_eq!(parse_bool("maybe"), None);
    }

    #[test]
    fn limit_zero_is_rejected_as_greater_than_zero() {
        let error = validate(raw(None, None, Some("0"))).unwrap_err();
        let ApiError::Validation { issues } = error else {
            panic!("expected a validation error");
        };
        assert_eq!(issues[0].kind, "greater_than");
        assert_eq!(issues[0].loc, ["query", "limit"]);
    }

    #[test]
    fn limit_above_the_ceiling_is_rejected() {
        let error = validate(raw(None, None, Some("501"))).unwrap_err();
        let ApiError::Validation { issues } = error else {
            panic!("expected a validation error");
        };
        assert_eq!(issues[0].kind, "less_than_equal");
    }

    #[test]
    fn limit_500_is_the_highest_accepted_value() {
        assert_eq!(validate(raw(None, None, Some("500"))).unwrap().limit, 500);
    }

    #[test]
    fn a_negative_offset_is_rejected() {
        let error = validate(raw(None, Some("-1"), None)).unwrap_err();
        let ApiError::Validation { issues } = error else {
            panic!("expected a validation error");
        };
        assert_eq!(issues[0].kind, "greater_than_equal");
        assert_eq!(issues[0].loc, ["query", "offset"]);
    }

    #[test]
    fn a_non_numeric_limit_is_an_int_parsing_error() {
        let error = validate(raw(None, None, Some("abc"))).unwrap_err();
        let ApiError::Validation { issues } = error else {
            panic!("expected a validation error");
        };
        assert_eq!(issues[0].kind, "int_parsing");
    }

    #[test]
    fn every_bad_parameter_is_reported_together() {
        let error = validate(raw(Some("maybe"), Some("-1"), Some("0"))).unwrap_err();
        let ApiError::Validation { issues } = error else {
            panic!("expected a validation error");
        };
        assert_eq!(issues.len(), 3);
    }

    #[test]
    fn a_search_term_of_exactly_200_characters_is_accepted() {
        let mut query = raw(None, None, None);
        query.search = Some("a".repeat(200));
        assert_eq!(
            validate(query).unwrap().search.as_deref(),
            Some("a".repeat(200).as_str())
        );
    }

    #[test]
    fn a_search_term_of_201_characters_is_rejected() {
        let mut query = raw(None, None, None);
        query.search = Some("a".repeat(201));
        let error = validate(query).unwrap_err();
        let ApiError::Validation { issues } = error else {
            panic!("expected a validation error");
        };
        assert_eq!(issues[0].kind, "string_too_long");
        assert_eq!(issues[0].loc, ["query", "search"]);
    }

    #[test]
    fn a_multibyte_search_term_is_measured_in_characters() {
        // 200 Chinese characters are 600 bytes; a byte-based limit would reject
        // a value the Python baseline accepts.
        let mut query = raw(None, None, None);
        query.search = Some("备".repeat(200));
        assert!(validate(query).is_ok());
    }
}
