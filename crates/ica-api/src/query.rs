//! Query-string parsing and validation for the list endpoints.
//!
//! FastAPI validates these with `Query(...)` constraints and answers 422 with
//! the project's envelope; the port has to reproduce both the accepted values
//! and the rejection codes, because the frontend and the desktop shell send
//! `0`/`1` for booleans and rely on `limit` being clamped, not silently fixed.
//!
//! Extraction goes through [`ApiQuery`], not axum's `Query`: Starlette keeps
//! the **last** occurrence of a repeated key where axum rejects with a
//! plain-text 400, and pydantic's number grammar (underscores, `+` sign, no
//! upper bound) differs from what `str::parse::<i64>` accepts.
//!
//! Every validator collects *all* problems before failing, the way pydantic
//! reports one 422 with a list of errors instead of stopping at the first one.

use std::collections::HashSet;

use axum::extract::FromRequestParts;
use serde::de::DeserializeOwned;
use serde::Deserialize;

use ica_application::{
    ListCandidates, ListConversations, ListMessages, ListPositions, ListSendRecords,
};
use ica_domain::{
    normalize_search, CandidateStatus, FollowUpTaskStatus, ResultType, MAX_SEARCH_LEN,
};

use crate::error::{ApiError, ValidationIssue};

/// Page size when the caller does not ask for one (`Query(default=50)`).
pub const DEFAULT_LIMIT: usize = 50;
/// Upper bound for the 50-page lists (`Query(le=500)`).
pub const MAX_LIMIT: usize = 500;
/// `GET /api/messages` defaults to 200 with a 1000 ceiling.
pub const DEFAULT_MESSAGE_LIMIT: usize = 200;
/// `GET /api/candidates/{id}/timeline` shares the message ceiling.
pub const MAX_MESSAGE_LIMIT: usize = 1000;

// ── Shared parsing helpers ─────────────────────────────────────────────────

/// A query extractor that mirrors the Python baseline's `QueryParams`.
///
/// axum's built-in `Query<T>` rejects a repeated key (`?limit=1&limit=2`) with
/// a plain-text 400 — a format the frontend's single error path cannot parse,
/// and a semantic divergence too: Starlette answers that request 200 with the
/// **last** value. This extractor decodes with the same rules as Python's
/// `urllib.parse.parse_qsl` (form decoding: `+` is a space, `%XX` decoded,
/// invalid UTF-8 replaced, a bare key becomes an empty value) and keeps the
/// last occurrence of every key before deserialising.
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
            ApiError::Http {
                status: 400,
                message: format!("Failed to deserialize query string: {error}"),
            }
        })?;
        Ok(ApiQuery(value))
    }
}

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

/// Raw query values, exactly as they arrived: no coercion has happened yet.
#[derive(Debug, Default, Clone, Deserialize)]
pub struct RawListPositionsQuery {
    pub include_archived: Option<String>,
    pub search: Option<String>,
    pub offset: Option<String>,
    pub limit: Option<String>,
}

/// A query integer parsed with Python's grammar, classified for the validators.
///
/// Python `int` has no upper bound, so a "too big" spelling is still an
/// integer to pydantic — but the page envelope echoes `offset` as a JSON
/// number, and nothing wider than `u64` can be echoed, hence [`PyInt::Overflow`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PyInt {
    /// Fits the envelope's echo width.
    Value(u64),
    /// A positive integer beyond `u64` (accepted by Python, unechoable here).
    Overflow,
    /// Any negative value: only the sign matters for `offset`/`limit`.
    Negative,
}

/// Parse a decimal integer the way Python's `int()` — and therefore pydantic's
/// lax int parsing — does: surrounding whitespace stripped, an optional sign,
/// ASCII digits with `_` allowed *between* digits only (`1_0` → 10; `_1`, `1_`,
/// `1__0` and `1 0` all fail). Plain `parse::<i64>()` would reject inputs the
/// baseline accepts (`1_0`, `+5`, anything past `i64::MAX`).
fn parse_python_int(raw: &str) -> Option<PyInt> {
    let body = raw.trim();
    let digits = body
        .strip_prefix('-')
        .or_else(|| body.strip_prefix('+'))
        .unwrap_or(body);
    if digits.is_empty() {
        return None; // a bare sign: `+` / `-`
    }
    let negative = body.starts_with('-');

    let mut compact = String::with_capacity(digits.len());
    let mut previous = '_';
    for (index, character) in digits.chars().enumerate() {
        match character {
            '0'..='9' => compact.push(character),
            // An underscore is legal only *between* two digits.
            '_' if index > 0 && previous != '_' => {}
            _ => return None,
        }
        previous = character;
    }
    if previous == '_' {
        return None; // a trailing underscore: `1_`
    }
    if negative {
        return Some(PyInt::Negative);
    }
    // Leading zeros do not carry magnitude: `007` is seven, not a 3-digit value.
    let digits = compact.trim_start_matches('0');
    if digits.is_empty() {
        return Some(PyInt::Value(0));
    }
    if digits.len() > 20 {
        return Some(PyInt::Overflow);
    }
    match digits.parse::<u64>() {
        Ok(value) => Some(PyInt::Value(value)),
        // 20 digits can still exceed u64::MAX (`18446744073709551616`).
        Err(_) => Some(PyInt::Overflow),
    }
}

/// Interpret a boolean the way pydantic does.
///
/// The accepted spellings matter: `include_archived=1` is what a browser form
/// and the packaged shell send, and rejecting it would be a silent behaviour
/// change from the Python service. No trimming: pydantic rejects `" true "`
/// with `bool_parsing`, so the port must too — int values, by contrast, *are*
/// stripped by pydantic, and [`parse_python_int`] keeps that.
fn parse_bool(raw: &str) -> Option<bool> {
    match raw.to_ascii_lowercase().as_str() {
        "1" | "true" | "t" | "yes" | "y" | "on" => Some(true),
        "0" | "false" | "f" | "no" | "n" | "off" => Some(false),
        _ => None,
    }
}

/// Parse a non-negative integer query value.
fn parse_non_negative(raw: &str, field: &str) -> Result<usize, ValidationIssue> {
    match parse_python_int(raw) {
        Some(PyInt::Value(value)) => Ok(usize::try_from(value).unwrap_or(usize::MAX)),
        Some(PyInt::Negative) => Err(ValidationIssue::new(
            "greater_than_equal",
            vec!["query", field],
            "Input should be greater than or equal to 0",
            raw,
        )),
        // Python answers 200 here with an empty page and the huge offset
        // echoed; echoing needs a JSON number wider than `u64`, so the port
        // rejects instead — registered as IMP-4 in the migration contract §7.
        Some(PyInt::Overflow) => Err(ValidationIssue::new(
            "int_parsing",
            vec!["query", field],
            "Input should be a valid integer, unable to parse string as an integer",
            raw,
        )),
        None => Err(ValidationIssue::new(
            "int_parsing",
            vec!["query", field],
            "Input should be a valid integer, unable to parse string as an integer",
            raw,
        )),
    }
}

/// Parse `limit`: `1..=max`.
fn parse_limit(raw: &str, max: usize, field: &str) -> Result<usize, ValidationIssue> {
    match parse_python_int(raw) {
        // `0` fails the *lower* bound on both sides (`greater_than`), not the
        // upper one — pydantic's constraint is `ge=1`.
        Some(PyInt::Value(0)) => Err(ValidationIssue::new(
            "greater_than",
            vec!["query", field],
            "Input should be greater than 0",
            raw,
        )),
        Some(PyInt::Value(value)) if value <= max as u64 => {
            Ok(usize::try_from(value).unwrap_or(usize::MAX))
        }
        // Every magnitude beyond `u64` is also beyond `max`, so the port gives
        // the same `less_than_equal` the baseline does for huge limits.
        Some(PyInt::Value(_)) | Some(PyInt::Overflow) => Err(ValidationIssue::new(
            "less_than_equal",
            vec!["query", field],
            &format!("Input should be less than or equal to {max}"),
            raw,
        )),
        Some(PyInt::Negative) => Err(ValidationIssue::new(
            "greater_than",
            vec!["query", field],
            "Input should be greater than 0",
            raw,
        )),
        None => Err(ValidationIssue::new(
            "int_parsing",
            vec!["query", field],
            "Input should be a valid integer, unable to parse string as an integer",
            raw,
        )),
    }
}

/// Parse an optional enum query value; an unknown value is a 422 `enum` error.
fn parse_enum<T>(
    raw: Option<&str>,
    field: &str,
    expected: &str,
    parse: impl Fn(&str) -> Option<T>,
    issues: &mut Vec<ValidationIssue>,
) -> Option<T> {
    let raw = raw?;
    let parsed = parse(raw);
    if parsed.is_none() {
        issues.push(ValidationIssue::new(
            "enum",
            vec!["query", field],
            &format!("Input should be {expected}"),
            raw,
        ));
    }
    parsed
}

/// Parse `limit` for a list endpoint, pushing the issue on failure.
fn limit_or_default(
    raw: Option<&str>,
    default: usize,
    max: usize,
    issues: &mut Vec<ValidationIssue>,
) -> usize {
    match raw {
        None => default,
        Some(value) => match parse_limit(value, max, "limit") {
            Ok(parsed) => parsed,
            Err(issue) => {
                issues.push(issue);
                default
            }
        },
    }
}

/// Parse `offset` (default 0), pushing the issue on failure.
fn offset_or_default(raw: Option<&str>, issues: &mut Vec<ValidationIssue>) -> usize {
    match raw {
        None => 0,
        Some(value) => match parse_non_negative(value, "offset") {
            Ok(parsed) => parsed,
            Err(issue) => {
                issues.push(issue);
                0
            }
        },
    }
}

/// Parse `search` (`max_length=200` counts characters, not bytes), pushing the
/// issue on failure.
fn search_or_none(raw: Option<&str>, issues: &mut Vec<ValidationIssue>) -> Option<String> {
    match raw {
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
    }
}

fn finish<T>(issues: Vec<ValidationIssue>, value: T) -> Result<T, ApiError> {
    if issues.is_empty() {
        Ok(value)
    } else {
        Err(ApiError::validation_issues(issues))
    }
}

// ── Positions ──────────────────────────────────────────────────────────────

/// Validate every `GET /api/positions` parameter.
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
    let offset = offset_or_default(raw.offset.as_deref(), &mut issues);
    let limit = limit_or_default(raw.limit.as_deref(), DEFAULT_LIMIT, MAX_LIMIT, &mut issues);
    let search = search_or_none(raw.search.as_deref(), &mut issues);

    finish(
        issues,
        ListPositions {
            include_archived,
            search,
            offset,
            limit,
        },
    )
}

// ── Candidates ─────────────────────────────────────────────────────────────

/// Raw `GET /api/candidates` query.
#[derive(Debug, Default, Clone, Deserialize)]
pub struct RawListCandidatesQuery {
    pub search: Option<String>,
    /// Query alias `status` (`candidate_status` in the Python signature).
    pub status: Option<String>,
    pub position_id: Option<String>,
    pub include_archived: Option<String>,
    pub offset: Option<String>,
    pub limit: Option<String>,
}

/// Validate every `GET /api/candidates` parameter.
pub fn validate_candidates(raw: RawListCandidatesQuery) -> Result<ListCandidates, ApiError> {
    let mut issues = Vec::new();

    let search = search_or_none(raw.search.as_deref(), &mut issues);
    let status = parse_enum(
        raw.status.as_deref(),
        "status",
        "'New', 'Contacted', 'ReadNoReply', 'Replied', 'Active', 'Interview', 'Rejected', 'Withdrawn' or 'Closed'",
        CandidateStatus::parse,
        &mut issues,
    );
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
    let offset = offset_or_default(raw.offset.as_deref(), &mut issues);
    let limit = limit_or_default(raw.limit.as_deref(), DEFAULT_LIMIT, MAX_LIMIT, &mut issues);

    finish(
        issues,
        ListCandidates {
            search,
            status,
            position_id: raw.position_id,
            include_archived,
            offset,
            limit,
        },
    )
}

// ── Conversations ──────────────────────────────────────────────────────────

/// Raw `GET /api/conversations` query.
#[derive(Debug, Default, Clone, Deserialize)]
pub struct RawListConversationsQuery {
    pub candidate_id: Option<String>,
    pub offset: Option<String>,
    pub limit: Option<String>,
}

/// Validate every `GET /api/conversations` parameter.
pub fn validate_conversations(
    raw: RawListConversationsQuery,
) -> Result<ListConversations, ApiError> {
    let mut issues = Vec::new();
    let offset = offset_or_default(raw.offset.as_deref(), &mut issues);
    let limit = limit_or_default(raw.limit.as_deref(), DEFAULT_LIMIT, MAX_LIMIT, &mut issues);
    finish(
        issues,
        ListConversations {
            candidate_id: raw.candidate_id,
            offset,
            limit,
        },
    )
}

// ── Messages and timeline ──────────────────────────────────────────────────

/// Raw `GET /api/messages` query.
#[derive(Debug, Default, Clone, Deserialize)]
pub struct RawListMessagesQuery {
    pub candidate_id: Option<String>,
    pub conversation_id: Option<String>,
    pub offset: Option<String>,
    pub limit: Option<String>,
}

/// Validate every `GET /api/messages` parameter.
pub fn validate_messages(raw: RawListMessagesQuery) -> Result<ListMessages, ApiError> {
    let mut issues = Vec::new();
    let offset = offset_or_default(raw.offset.as_deref(), &mut issues);
    let limit = limit_or_default(
        raw.limit.as_deref(),
        DEFAULT_MESSAGE_LIMIT,
        MAX_MESSAGE_LIMIT,
        &mut issues,
    );
    finish(
        issues,
        ListMessages {
            candidate_id: raw.candidate_id,
            conversation_id: raw.conversation_id,
            offset,
            limit,
        },
    )
}

/// Raw `GET /api/candidates/{id}/timeline` query: only `limit`.
#[derive(Debug, Default, Clone, Deserialize)]
pub struct RawTimelineQuery {
    pub limit: Option<String>,
}

/// Validate `limit` for the timeline endpoint.
pub fn validate_timeline(raw: RawTimelineQuery) -> Result<usize, ApiError> {
    let mut issues = Vec::new();
    let limit = limit_or_default(
        raw.limit.as_deref(),
        DEFAULT_MESSAGE_LIMIT,
        MAX_MESSAGE_LIMIT,
        &mut issues,
    );
    finish(issues, limit)
}

// ── Send records ───────────────────────────────────────────────────────────

/// Raw `GET /api/send-records` query.
#[derive(Debug, Default, Clone, Deserialize)]
pub struct RawListSendRecordsQuery {
    pub candidate_id: Option<String>,
    pub task_id: Option<String>,
    pub draft_id: Option<String>,
    /// Query alias `status` (`send_status` in the Python signature).
    pub status: Option<String>,
    pub offset: Option<String>,
    pub limit: Option<String>,
}

/// Validate every `GET /api/send-records` parameter.
pub fn validate_send_records(raw: RawListSendRecordsQuery) -> Result<ListSendRecords, ApiError> {
    let mut issues = Vec::new();
    let status: Option<ResultType> = parse_enum(
        raw.status.as_deref(),
        "status",
        "'sent', 'not_sent', 'candidate_replied', 'candidate_no_reply', 'candidate_declined', 'interview' or 'other'",
        ResultType::parse,
        &mut issues,
    );
    let offset = offset_or_default(raw.offset.as_deref(), &mut issues);
    let limit = limit_or_default(raw.limit.as_deref(), DEFAULT_LIMIT, MAX_LIMIT, &mut issues);
    finish(
        issues,
        ListSendRecords {
            candidate_id: raw.candidate_id,
            task_id: raw.task_id,
            draft_id: raw.draft_id,
            status,
            offset,
            limit,
        },
    )
}

// ── Follow-ups (W2) ────────────────────────────────────────────────────────

/// Raw `GET /api/follow-ups` query.
#[derive(Debug, Default, Clone, Deserialize)]
pub struct RawListFollowUpsQuery {
    pub candidate_id: Option<String>,
    /// Query alias `status` (`task_status` in the Python signature).
    pub status: Option<String>,
    pub offset: Option<String>,
    pub limit: Option<String>,
}

/// A validated `GET /api/follow-ups` request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListFollowUps {
    pub candidate_id: Option<String>,
    pub status: Option<FollowUpTaskStatus>,
    pub offset: usize,
    pub limit: usize,
}

/// Validate every `GET /api/follow-ups` parameter.
pub fn validate_follow_ups(raw: RawListFollowUpsQuery) -> Result<ListFollowUps, ApiError> {
    let mut issues = Vec::new();
    let status = parse_enum(
        raw.status.as_deref(),
        "status",
        "'Pending', 'Eligible', 'Drafting', 'Reviewing', 'Rejected', 'Approved', \
         'Copied', 'AwaitingResult', 'Completed', 'Cancelled', 'Expired' or 'Blocked'",
        FollowUpTaskStatus::parse,
        &mut issues,
    );
    let offset = offset_or_default(raw.offset.as_deref(), &mut issues);
    let limit = limit_or_default(raw.limit.as_deref(), DEFAULT_LIMIT, MAX_LIMIT, &mut issues);
    finish(
        issues,
        ListFollowUps {
            candidate_id: raw.candidate_id,
            status,
            offset,
            limit,
        },
    )
}

/// Raw `POST /api/follow-ups/{id}/cancel` query: only the reason.
#[derive(Debug, Default, Clone, Deserialize)]
pub struct RawFollowUpCancelQuery {
    pub reason: Option<String>,
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
    fn a_padded_boolean_is_not_trimmed_like_pydantic() {
        // pydantic strips whitespace for ints but *not* for bools, so the port
        // must reject `" true "` with `bool_parsing` instead of answering 200.
        for value in [" true ", " true", "true ", "\t1"] {
            assert_eq!(parse_bool(value), None, "{value:?}");
        }
    }

    #[test]
    fn integers_use_the_python_grammar() {
        // Underscores between digits, an optional sign, leading zeros.
        assert_eq!(parse_python_int("1_0"), Some(PyInt::Value(10)));
        assert_eq!(parse_python_int("+5"), Some(PyInt::Value(5)));
        assert_eq!(parse_python_int("007"), Some(PyInt::Value(7)));
        assert_eq!(parse_python_int("-1"), Some(PyInt::Negative));
        assert_eq!(parse_python_int("-1_000"), Some(PyInt::Negative));
        // Spellings pydantic rejects (verified against the baseline's own
        // pydantic, including Unicode digits: `TypeAdapter(int)` rejects `١٢`
        // even though raw `int()` would accept it).
        for raw in ["_1", "1_", "1__0", "1 0", "+", "-", "", "1.0", "0x10", "١٢"] {
            assert_eq!(parse_python_int(raw), None, "{raw:?}");
        }
    }

    #[test]
    fn integers_beyond_i64_are_accepted_like_python() {
        // Just past i64::MAX: the baseline echoes it, so the port must too.
        assert_eq!(
            parse_python_int("9223372036854775808"),
            Some(PyInt::Value(9223372036854775808))
        );
        // u64::MAX itself still echoes.
        assert_eq!(
            parse_python_int("18446744073709551615"),
            Some(PyInt::Value(u64::MAX))
        );
        // One past u64::MAX: an integer to Python, unechoable for the envelope.
        assert_eq!(
            parse_python_int("18446744073709551616"),
            Some(PyInt::Overflow)
        );
        // Any 40-digit spelling is the same classification.
        assert_eq!(
            parse_python_int("1234567890123456789012345678901234567890"),
            Some(PyInt::Overflow)
        );
        // ...but only magnitude counts: leading zeros do not inflate it.
        assert_eq!(
            parse_python_int("000000000000000000000000000001"),
            Some(PyInt::Value(1))
        );
    }

    #[test]
    fn an_offset_beyond_u64_is_an_int_parsing_error_imp4() {
        let error = validate(raw(None, Some("18446744073709551616"), None)).unwrap_err();
        let ApiError::ValidationIssues { issues } = error else {
            panic!("expected a validation error");
        };
        assert_eq!(issues[0].kind, "int_parsing");
    }

    #[test]
    fn a_limit_beyond_u64_is_less_than_equal_like_python() {
        let error = validate(raw(None, None, Some("18446744073709551616"))).unwrap_err();
        let ApiError::ValidationIssues { issues } = error else {
            panic!("expected a validation error");
        };
        assert_eq!(issues[0].kind, "less_than_equal");
    }

    #[test]
    fn underscored_and_signed_numbers_are_accepted() {
        assert_eq!(validate(raw(None, Some("1_0"), None)).unwrap().offset, 10);
        assert_eq!(validate(raw(None, Some("+5"), None)).unwrap().offset, 5);
        assert_eq!(validate(raw(None, None, Some("2_0"))).unwrap().limit, 20);
    }

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

    #[test]
    fn limit_zero_is_rejected_as_greater_than_zero() {
        let error = validate(raw(None, None, Some("0"))).unwrap_err();
        let ApiError::ValidationIssues { issues } = error else {
            panic!("expected a validation error");
        };
        assert_eq!(issues[0].kind, "greater_than");
        assert_eq!(issues[0].loc, ["query", "limit"]);
    }

    #[test]
    fn limit_above_the_ceiling_is_rejected() {
        let error = validate(raw(None, None, Some("501"))).unwrap_err();
        let ApiError::ValidationIssues { issues } = error else {
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
        let ApiError::ValidationIssues { issues } = error else {
            panic!("expected a validation error");
        };
        assert_eq!(issues[0].kind, "greater_than_equal");
        assert_eq!(issues[0].loc, ["query", "offset"]);
    }

    #[test]
    fn a_non_numeric_limit_is_an_int_parsing_error() {
        let error = validate(raw(None, None, Some("abc"))).unwrap_err();
        let ApiError::ValidationIssues { issues } = error else {
            panic!("expected a validation error");
        };
        assert_eq!(issues[0].kind, "int_parsing");
    }

    #[test]
    fn every_bad_parameter_is_reported_together() {
        let error = validate(raw(Some("maybe"), Some("-1"), Some("0"))).unwrap_err();
        let ApiError::ValidationIssues { issues } = error else {
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
        let ApiError::ValidationIssues { issues } = error else {
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

    #[test]
    fn messages_default_to_200_and_cap_at_1000() {
        let query = validate_messages(RawListMessagesQuery::default()).unwrap();
        assert_eq!(query.limit, 200);
        let error = validate_messages(RawListMessagesQuery {
            limit: Some("1001".to_string()),
            ..Default::default()
        })
        .unwrap_err();
        assert!(matches!(error, ApiError::ValidationIssues { .. }));
        assert_eq!(
            validate_messages(RawListMessagesQuery {
                limit: Some("1000".to_string()),
                ..Default::default()
            })
            .unwrap()
            .limit,
            1000
        );
    }

    #[test]
    fn an_unknown_candidate_status_is_a_422_enum_error() {
        let error = validate_candidates(RawListCandidatesQuery {
            status: Some("Nope".to_string()),
            ..Default::default()
        })
        .unwrap_err();
        let ApiError::ValidationIssues { issues } = error else {
            panic!("expected a validation error");
        };
        assert_eq!(issues[0].kind, "enum");
        assert_eq!(issues[0].loc, ["query", "status"]);
    }

    #[test]
    fn an_unknown_send_status_is_a_422_enum_error() {
        let error = validate_send_records(RawListSendRecordsQuery {
            status: Some("maybe".to_string()),
            ..Default::default()
        })
        .unwrap_err();
        let ApiError::ValidationIssues { issues } = error else {
            panic!("expected a validation error");
        };
        assert_eq!(issues[0].kind, "enum");
    }

    #[test]
    fn timeline_accepts_and_rejects_the_message_ceiling() {
        assert_eq!(validate_timeline(RawTimelineQuery::default()).unwrap(), 200);
        assert!(validate_timeline(RawTimelineQuery {
            limit: Some("1001".to_string())
        })
        .is_err());
    }
}
