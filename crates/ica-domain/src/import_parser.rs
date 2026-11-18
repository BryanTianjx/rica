//! The paste-import text parser, ported from
//! `app/application/import_pipeline.py::parse_messages`.
//!
//! This is the part of W4 that decides what the user's pasted text turns into,
//! so it is a straight port rather than a redesign. Three baseline behaviours
//! are deliberately preserved even though they look odd:
//!
//! 1. **A bare time with seconds never parses.** `TIME_PATTERNS` accepts
//!    `10:23:45`, but the bare-time branch parses with `strptime("%H:%M")`,
//!    which rejects it, so the timestamp comes back `None` and a warning is
//!    appended. A bare `10:23` works; `10:23:45` does not.
//! 2. **`validation_messages` is the last two entries of the *global* warning
//!    list**, not the warnings that belong to this message. So a message with
//!    no problem of its own can still carry the previous message's warnings.
//! 3. **A block whose text disappears after stripping the sender/time keeps the
//!    whole block as content** and adds a warning saying so.
//!
//! None of the patterns here use look-around, so they translate directly; the
//! only ones that need hand-writing are in `redact.rs` (already done in W3).

use chrono::{DateTime, NaiveDate, NaiveTime, TimeZone, Utc};
use regex::Regex;
use std::sync::OnceLock;

use crate::conversation::SenderType;
use crate::import::RecordStatus;

// ── Patterns (mirrors SENDER_PATTERNS / TIME_PATTERNS / the date headers) ───

fn recruiter_prefix() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^(?:我|俺|招聘方|HR|我方|BOSS)\s*[:：]\s*").expect("static"))
}

fn candidate_prefix() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^(?:候选人|对方|他|她|应聘者)\s*[:：]\s*").expect("static"))
}

/// A quoted nickname prefix — `「昵称」：`, `【昵称】：`, `"昵称":` — treated as
/// the candidate, because applicants often speak under a nickname.
fn quoted_nickname_prefix() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r#"^[「『【"'].+?[」』】"']\s*[:：]\s*"#).expect("static"))
}

fn system_prefix() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^(?:系统|提示|系统消息)\s*[:：]\s*").expect("static"))
}

/// `2026-11-01 10:23` / `2026-11-01 10:23:45`.
fn full_datetime() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r"^(\d{4}-\d{2}-\d{2})\s+(\d{1,2}:\d{2}(?::\d{2})?)\s*").expect("static")
    })
}

/// `昨天 10:23` and friends.
fn relative_datetime() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r"^(昨天|今天|前天|昨日)\s+(\d{1,2}:\d{2}(?::\d{2})?)\s*").expect("static")
    })
}

/// A bare `10:23` / `10:23:45`.
fn bare_time() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^(\d{1,2}:\d{2}(?::\d{2})?)\s*").expect("static"))
}

fn date_header() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^(\d{4}-\d{2}-\d{2})$").expect("static"))
}

fn relative_date_header() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^(昨天|今天|前天|昨日)$").expect("static"))
}

/// One message parsed out of pasted text (`ParsedMessage`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedMessage {
    pub sequence: i64,
    pub content: String,
    pub sender_type: Option<SenderType>,
    pub message_time: Option<DateTime<Utc>>,
    pub content_hash: String,
    pub status: RecordStatus,
    pub duplicate_of_message_id: Option<String>,
    pub validation_messages: Vec<String>,
}

/// Outcome of parsing pasted text (`ParseResult`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseResult {
    pub messages: Vec<ParsedMessage>,
    pub warnings: Vec<String>,
    /// Categories masked for AI egress, sorted and de-duplicated.
    pub redacted_terms: Vec<String>,
}

/// Normalise line endings and strip trailing whitespace per line
/// (`normalize_text`).
pub fn normalize_text(raw: &str) -> String {
    let unified = raw.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = unified.split('\n').map(str::trim_end).collect();
    lines.join("\n").trim().to_string()
}

fn is_marker(line: &str) -> bool {
    full_datetime().is_match(line)
        || relative_datetime().is_match(line)
        || bare_time().is_match(line)
        || recruiter_prefix().is_match(line)
        || candidate_prefix().is_match(line)
        || quoted_nickname_prefix().is_match(line)
        || system_prefix().is_match(line)
}

/// Split pasted text into blocks.
///
/// A new block starts at a line carrying a timestamp, a speaker prefix, or a
/// date header; lines without either continue the previous block.
pub fn split_message_blocks(text: &str) -> Vec<String> {
    let mut blocks: Vec<String> = Vec::new();
    let mut current: Vec<String> = Vec::new();

    for line in text.split('\n') {
        let stripped = line.trim();
        if stripped.is_empty() {
            continue;
        }
        let is_date = date_header().is_match(stripped) || relative_date_header().is_match(stripped);
        if (is_marker(stripped) || is_date) && !current.is_empty() {
            blocks.push(current.join("\n"));
            current = vec![stripped.to_string()];
        } else {
            current.push(stripped.to_string());
        }
    }
    if !current.is_empty() {
        blocks.push(current.join("\n"));
    }
    blocks
}

/// Days offset for a relative date word (`_relative_date`).
fn relative_date(word: &str, now: DateTime<Utc>) -> NaiveDate {
    let delta = match word {
        "昨天" | "昨日" => -1,
        "前天" => -2,
        "今天" => 0,
        _ => 0,
    };
    let today = now.date_naive();
    if delta >= 0 {
        today
    } else {
        today - chrono::Duration::days(-i64::from(delta))
    }
}

fn combine(date: NaiveDate, hour: u32, minute: u32) -> DateTime<Utc> {
    Utc.from_utc_datetime(
        &date
            .and_hms_opt(hour, minute, 0)
            .expect("hour/minute validated by the caller"),
    )
}

/// Extract a timestamp from the head of `text` (`_parse_time`).
///
/// Returns `(time, remainder, updated date context)`. The context is only
/// updated by a full date or a relative date; a bare time reuses it (or falls
/// back to today, treating a future clock time as yesterday).
fn parse_time(
    text: &str,
    now: DateTime<Utc>,
    date_context: Option<NaiveDate>,
) -> (Option<DateTime<Utc>>, String, Option<NaiveDate>) {
    // Full date + time.
    if let Some(caps) = full_datetime().captures(text) {
        let date_part = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
        let time_part = caps.get(2).map(|m| m.as_str()).unwrap_or_default();
        let number = if let Some(first) = time_part.chars().next() {
            first.is_ascii_digit()
        } else {
            false
        };
        // The baseline distinguishes the two-group patterns by whether the
        // first group starts with a digit, so this mirrors that test rather
        // than relying on regex order alone.
        if number {
            let date = NaiveDate::parse_from_str(date_part, "%Y-%m-%d").ok();
            if let (Some(date), Some(parsed)) = (date, parse_hh_mm_ss(time_part)) {
                let parsed = Utc.from_utc_datetime(&date.and_time(parsed));
                return (
                    Some(parsed),
                    text[caps.get(0).map(|m| m.end()).unwrap_or(0)..]
                        .trim()
                        .to_string(),
                    Some(date),
                );
            }
        }
    }

    // Relative date + time.
    if let Some(caps) = relative_datetime().captures(text) {
        let word = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
        let time_part = caps.get(2).map(|m| m.as_str()).unwrap_or_default();
        if let Some(parsed_time) = parse_hh_mm(time_part) {
            let base = relative_date(word, now);
            let parsed = combine(base, parsed_time.0, parsed_time.1);
            return (
                Some(parsed),
                text[caps.get(0).map(|m| m.end()).unwrap_or(0)..]
                    .trim()
                    .to_string(),
                Some(base),
            );
        }
    }

    // Bare time. Note this uses `%H:%M` semantics: `10:23:45` does NOT parse
    // here, which is why a bare timestamp with seconds yields `None`.
    if let Some(caps) = bare_time().captures(text) {
        let time_part = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
        if let Some((hour, minute)) = parse_hh_mm(time_part) {
            let parsed = match date_context {
                Some(date) => combine(date, hour, minute),
                None => {
                    let today = now.date_naive();
                    let candidate = combine(today, hour, minute);
                    if candidate > now {
                        candidate - chrono::Duration::days(1)
                    } else {
                        candidate
                    }
                }
            };
            return (
                Some(parsed),
                text[caps.get(0).map(|m| m.end()).unwrap_or(0)..]
                    .trim()
                    .to_string(),
                date_context,
            );
        }
    }

    (None, text.to_string(), date_context)
}

/// `strptime("%H:%M")` — exactly two components, nothing else.
fn parse_hh_mm(raw: &str) -> Option<(u32, u32)> {
    let parts: Vec<&str> = raw.split(':').collect();
    if parts.len() != 2 {
        return None;
    }
    let hour: u32 = parts[0].parse().ok()?;
    let minute: u32 = parts[1].parse().ok()?;
    if hour > 23 || minute > 59 {
        return None;
    }
    Some((hour, minute))
}

/// `strptime` against `%H:%M:%S`-style input, used only for the full-datetime
/// branch (where seconds are allowed).
fn parse_hh_mm_ss(raw: &str) -> Option<NaiveTime> {
    let parts: Vec<&str> = raw.split(':').collect();
    if parts.len() < 2 || parts.len() > 3 {
        return None;
    }
    let hour: u32 = parts[0].parse().ok()?;
    let minute: u32 = parts[1].parse().ok()?;
    let second: u32 = if parts.len() == 3 {
        parts[2].parse().ok()?
    } else {
        0
    };
    if hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    NaiveTime::from_hms_opt(hour, minute, second)
}

fn extract_sender(text: &str) -> (Option<SenderType>, String) {
    for (pattern, sender) in [
        (recruiter_prefix(), SenderType::Recruiter),
        (candidate_prefix(), SenderType::Candidate),
        (quoted_nickname_prefix(), SenderType::Candidate),
        (system_prefix(), SenderType::System),
    ] {
        if let Some(m) = pattern.find(text) {
            return (Some(sender), text[m.end()..].trim().to_string());
        }
    }
    (None, text.to_string())
}

/// Parse pasted chat text into structured candidates (`parse_messages`).
///
/// Never fails: unparseable pieces come back with an unknown sender or time and
/// a warning, which is what the requirement asks for.
pub fn parse_messages(raw_text: &str, now: DateTime<Utc>) -> ParseResult {
    let text = normalize_text(raw_text);
    let mut warnings: Vec<String> = Vec::new();
    let mut redacted: Vec<String> = Vec::new();

    if text.is_empty() {
        return ParseResult {
            messages: Vec::new(),
            warnings: vec!["The pasted text was empty.".to_string()],
            redacted_terms: Vec::new(),
        };
    }

    let blocks = split_message_blocks(&text);
    if blocks.is_empty() {
        return ParseResult {
            messages: Vec::new(),
            warnings: vec!["No message content could be recognised in the text.".to_string()],
            redacted_terms: Vec::new(),
        };
    }

    let mut messages: Vec<ParsedMessage> = Vec::new();
    let mut message_index: i64 = 0;
    let mut date_context: Option<NaiveDate> = None;

    for block in &blocks {
        let stripped = block.trim();

        if let Some(caps) = date_header().captures(stripped) {
            let raw_date = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
            date_context = NaiveDate::parse_from_str(raw_date, "%Y-%m-%d").ok();
            continue;
        }
        if let Some(caps) = relative_date_header().captures(stripped) {
            let word = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
            date_context = Some(relative_date(word, now));
            continue;
        }

        let (message_time, remainder, updated_context) = parse_time(block, now, date_context);
        date_context = updated_context;
        let (sender_type, remainder) = extract_sender(&remainder);

        message_index += 1;
        let mut content = remainder.trim().to_string();
        if content.is_empty() {
            content = stripped.to_string();
            warnings.push(format!(
                "Message {message_index}: speaker and time could not be identified; \
the raw text was kept as content."
            ));
        }
        if sender_type.is_none() {
            warnings.push(format!("Message {message_index}: sender is unknown."));
        }
        if message_time.is_none() {
            warnings.push(format!("Message {message_index}: timestamp is unknown."));
        }

        let (_, categories) = crate::redact::redact_for_ai(&content, &[]);
        redacted.extend(categories);

        // The baseline slices the GLOBAL warning list here, not this message's
        // own warnings — see the module docstring.
        let tail: Vec<String> = warnings.iter().rev().take(2).rev().cloned().collect();

        messages.push(ParsedMessage {
            sequence: message_index,
            content_hash: crate::content_hash::content_hash(&content),
            content,
            sender_type,
            message_time,
            status: RecordStatus::Valid,
            duplicate_of_message_id: None,
            validation_messages: tail,
        });
    }

    redacted.sort();
    redacted.dedup();

    ParseResult {
        messages,
        warnings,
        redacted_terms: redacted,
    }
}

/// Flag records whose content hash already exists for the candidate
/// (`find_duplicates`). Duplicates are flagged, never dropped.
pub fn find_duplicates(
    messages: &mut [ParsedMessage],
    existing_hashes: &std::collections::HashMap<String, String>,
) {
    let mut seen: Vec<String> = Vec::new();
    for record in messages.iter_mut() {
        if existing_hashes.contains_key(&record.content_hash) || seen.contains(&record.content_hash)
        {
            record.status = RecordStatus::Duplicate;
            record.duplicate_of_message_id = existing_hashes.get(&record.content_hash).cloned();
            record.validation_messages.push(
                "Possibly a duplicate of an existing message; review before saving.".to_string(),
            );
        }
        seen.push(record.content_hash.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        "2026-11-18T12:00:00Z".parse().expect("fixed instant")
    }

    #[test]
    fn a_full_datetime_prefix_is_extracted() {
        let r = parse_messages("2026-11-01 10:23 我：你好", now());
        assert_eq!(r.messages.len(), 1);
        let m = &r.messages[0];
        assert_eq!(m.content, "你好");
        assert_eq!(m.sender_type, Some(SenderType::Recruiter));
        assert_eq!(
            m.message_time.map(|t| t.to_rfc3339()),
            Some("2026-11-01T10:23:00+00:00".to_string())
        );
        assert!(r.warnings.is_empty(), "warnings: {:?}", r.warnings);
    }

    /// A bare time WITH seconds must not parse: `TIME_PATTERNS` accepts
    /// `10:23:45`, but the bare branch uses `strptime("%H:%M")`, which rejects
    /// it. The timestamp comes back `None` rather than a truncated `10:23`.
    #[test]
    fn a_bare_time_with_seconds_yields_no_timestamp() {
        let r = parse_messages("10:23:45 我：带秒", now());
        let m = &r.messages[0];
        assert_eq!(
            m.message_time, None,
            "seconds must defeat the bare-time branch"
        );
        // The prefix is not consumed either, so it stays in the content.
        assert_eq!(m.content, "10:23:45 我：带秒");
        assert!(
            r.warnings
                .iter()
                .any(|w| w.contains("timestamp is unknown")),
            "warnings: {:?}",
            r.warnings
        );
    }

    #[test]
    fn a_bare_time_without_seconds_does_parse() {
        let r = parse_messages("10:23 我：纯时间", now());
        let m = &r.messages[0];
        assert_eq!(m.content, "纯时间");
        assert_eq!(
            m.message_time.map(|t| t.to_rfc3339()),
            Some("2026-11-18T10:23:00+00:00".to_string())
        );
    }

    /// A date header only sets the date context; it never becomes a message.
    #[test]
    fn a_date_header_supplies_context_and_is_not_a_message() {
        let r = parse_messages("2026-11-01\n10:23 我：之后的纯时间用这个日期", now());
        assert_eq!(r.messages.len(), 1);
        assert_eq!(
            r.messages[0].message_time.map(|t| t.to_rfc3339()),
            Some("2026-11-01T10:23:00+00:00".to_string())
        );
    }

    #[test]
    fn a_quoted_nickname_is_the_candidate() {
        for text in ["「小明」：你好", "【小明】：你好", "『小明』：你好"] {
            let r = parse_messages(text, now());
            assert_eq!(
                r.messages[0].sender_type,
                Some(SenderType::Candidate),
                "{text}"
            );
            assert_eq!(r.messages[0].content, "你好", "{text}");
        }
    }

    #[test]
    fn empty_input_warns_and_produces_nothing() {
        let r = parse_messages("   \n\n  ", now());
        assert!(r.messages.is_empty());
        assert_eq!(r.warnings, vec!["The pasted text was empty.".to_string()]);
    }

    /// `validation_messages` is the tail of the GLOBAL warning list, so a clean
    /// first message picks up nothing while a later one inherits earlier
    /// warnings. This looks wrong and is what the baseline does.
    #[test]
    fn validation_messages_come_from_the_global_warning_tail() {
        let r = parse_messages("随便一段话\n10:23 我：你好", now());
        assert_eq!(r.messages.len(), 2);
        // First message: unknown sender + unknown time -> two warnings.
        assert_eq!(r.messages[0].validation_messages.len(), 2);
        // Second message inherits the tail of the global list.
        assert!(!r.messages[1].validation_messages.is_empty());
    }

    #[test]
    fn duplicates_are_flagged_not_dropped() {
        let mut r = parse_messages("10:23 我：重复内容", now());
        let mut existing = std::collections::HashMap::new();
        existing.insert(r.messages[0].content_hash.clone(), "msg-1".to_string());
        find_duplicates(&mut r.messages, &existing);
        assert_eq!(r.messages.len(), 1);
        assert_eq!(r.messages[0].status, RecordStatus::Duplicate);
        assert_eq!(
            r.messages[0].duplicate_of_message_id.as_deref(),
            Some("msg-1")
        );
    }
}
