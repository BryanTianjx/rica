//! SQLite timestamp decoding.
//!
//! SQLAlchemy stores a Python `datetime` in SQLite as text. On the way out it
//! writes `YYYY-MM-DD HH:MM:SS[.ffffff]` with no timezone, because the column
//! is timezone-naive; the API layer is what attaches UTC on the way out
//! (`app/schemas/common.py::as_utc`). The Rust port reproduces both halves:
//! decode the naive text as UTC here, and let `ica-api` render the explicit
//! offset.

use chrono::{DateTime, NaiveDateTime, Utc};

/// Formats SQLite may hand back, in the order they are tried.
///
/// `%.f` accepts both an empty fraction and 1..9 digits, which covers the
/// `.000000` written by SQLAlchemy and a bare `...:00`.
const NAIVE_FORMATS: [&str; 4] = [
    "%Y-%m-%d %H:%M:%S%.f",
    "%Y-%m-%dT%H:%M:%S%.f",
    "%Y-%m-%d %H:%M",
    "%Y-%m-%dT%H:%M",
];

/// Parse a stored timestamp into UTC.
///
/// A value carrying an explicit offset (an RFC 3339 string, which a later
/// migration may introduce) is converted to UTC; a naive value is *assumed*
/// UTC, exactly like the Python `as_utc` helper. The error names the offending
/// value so a corrupt row is identifiable from the log alone.
pub fn parse_sqlite_datetime(raw: &str) -> Result<DateTime<Utc>, String> {
    let trimmed = raw.trim();

    if let Ok(aware) = DateTime::parse_from_rfc3339(trimmed) {
        return Ok(aware.with_timezone(&Utc));
    }

    for format in NAIVE_FORMATS {
        if let Ok(naive) = NaiveDateTime::parse_from_str(trimmed, format) {
            return Ok(naive.and_utc());
        }
    }

    Err(format!("cannot parse '{raw}' as a timestamp"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_date_is_not_a_timestamp() {
        // A date alone would silently become midnight and hide a data defect.
        assert!(parse_sqlite_datetime("2026-11-10").is_err());
    }

    #[test]
    fn surrounding_whitespace_is_tolerated() {
        assert_eq!(
            parse_sqlite_datetime("  2026-11-10 09:00:00  ").unwrap(),
            parse_sqlite_datetime("2026-11-10 09:00:00").unwrap()
        );
    }
}
