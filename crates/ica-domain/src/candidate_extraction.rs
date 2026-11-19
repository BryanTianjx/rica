//! Candidate-extraction domain rules (`app/domain/candidate_extraction.py`).
//!
//! The browser pipeline reads a page, drafts a candidate, a human reviews the
//! draft, and only then may anything reach the `candidates` table. This module
//! carries the pure rules of that pipeline that the CONFIRM path needs:
//! the field vocabulary and the digests that bind an approval to the exact
//! content a human read.
//!
//! The page-parsing half (`extract_candidate` and friends) is NOT ported here:
//! it belongs to the browser router's draft endpoints, whose migration is a
//! separate scope decision (W7 card §5). Confirm only ever READS what a
//! browser-side draft wrote, so it needs the vocabulary and the digests, not
//! the parser.
//!
//! Byte-for-byte note: `review_digest` hashes a JSON object the baseline
//! renders with `json.dumps(..., ensure_ascii=False, sort_keys=True)`. The
//! canonical text is what the baseline's verifier compares, so the key order
//! (sorted), the separators (`, ` / `: `) and the non-ASCII passthrough all
//! matter.

use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

/// The fields an extraction drafts, in the baseline's order (`FIELD_NAMES`).
/// `fields_written` in the confirm result is the SORTED version of this.
pub const FIELD_NAMES: [&str; 6] = [
    "name",
    "job_intention",
    "expected_salary",
    "expected_location",
    "experience_summary",
    "skill_summary",
];

/// Caps mirror the `Candidate` columns (`MAX_LEN`).
pub const MAX_LEN_NAME: usize = 200;

/// `normalize_name`: NFKC, strip, drop the 简历/主页 suffixes, drop the
/// punctuation class, lowercase, truncate to the name cap.
///
/// `_NAME_SUFFIX_RE = (的个人简历|的简历|个人简历|简历|的主页|主页)$` and
/// `_PUNCT_RE` is an explicit class — reproduced verbatim, because both feed
/// `identity_key`, whose value is stored on the row and compared later.
pub fn normalize_name(name: &str) -> String {
    if name.is_empty() {
        return String::new();
    }
    // `unicodedata.normalize("NFKC", name)` — full-width forms collapse.
    let text: String = name.nfkc().collect();
    let text = text.trim();
    let text = strip_name_suffix(text);
    let text = strip_punct(text);
    let lowered = text.to_lowercase();
    lowered.chars().take(MAX_LEN_NAME).collect()
}

/// The baseline's `个人的简历|的简历|个人简历|简历|的主页|主页` suffix strip.
/// Longest alternatives first: regex alternation tries them left to right, so
/// `的个人简历` wins over `的简历` on the same anchor — order is semantics.
fn strip_name_suffix(text: &str) -> &str {
    const SUFFIXES: [&str; 6] = ["的个人简历", "的简历", "个人简历", "简历", "的主页", "主页"];
    for suffix in SUFFIXES {
        if let Some(stripped) = text.strip_suffix(suffix) {
            return stripped;
        }
    }
    text
}

/// `_PUNCT_RE`: whitespace, full-width space, the bullet/dot family, CJK and
/// ASCII punctuation, brackets (CJK included), underscores, slashes, pipes.
fn strip_punct(text: &str) -> String {
    text.chars()
        .filter(|character| !is_punct_like(*character))
        .collect()
}

fn is_punct_like(character: char) -> bool {
    // `\s` in Python matches the Unicode whitespace set; Rust's
    // `char::is_whitespace` covers the same planes for the characters OCR and
    // page text actually carry (U+3000 included).
    matches!(
        character,
        '\u{3000}' // full-width space (in the class explicitly)
        | '·' | '•' | '.' | ',' | '，' | '。' | ';' | '；' | ':' | '：'
        | '!' | '！' | '?' | '？'
        | '\'' | '"' | '“' | '”' | '‘' | '’'
        | '(' | ')' | '（' | '）'
        | '[' | ']' | '【' | '】'
        | '<' | '>' | '《' | '》'
        | '_' | '-' | '/' | '\\' | '|'
    ) || character.is_whitespace()
}

/// `identity_key`: `sha256(f"{normalize_name(name)}\x1f{(domain or '').strip().lower()}")`
/// — the same person on the same site, without storing a phone number.
pub fn identity_key(name: &str, domain: Option<&str>) -> String {
    let payload = format!(
        "{}\u{1f}{}",
        normalize_name(name),
        domain.unwrap_or("").trim().to_lowercase()
    );
    hex_sha256(payload.as_bytes())
}

/// `content_fingerprint`: the same experience/skill text on any page; empty
/// when BOTH texts are empty (and then it never matches anything).
pub fn content_fingerprint(
    experience_summary: Option<&str>,
    skill_summary: Option<&str>,
) -> String {
    let experience = normalize_name(experience_summary.unwrap_or("").trim());
    let skills = normalize_name(skill_summary.unwrap_or("").trim());
    if experience.is_empty() && skills.is_empty() {
        return String::new();
    }
    let experience: String = experience.chars().take(200).collect();
    let skills: String = skills.chars().take(200).collect();
    let payload = format!("{experience}\u{1f}{skills}");
    hex_sha256(payload.as_bytes())
}

/// `review_digest`: sha256 of the canonical JSON of ALL six field slots
/// (`fields.get(key)` — a missing field is JSON null, not absent), keys
/// sorted, `ensure_ascii=False`, Python's default separators.
///
/// The approval stores this digest; confirm refuses when the draft's current
/// fields no longer render to the same text — that is the "nobody can write
/// what a human did not approve" guarantee (AC-04-4).
pub fn review_digest(fields: &[(&str, Option<&str>)]) -> String {
    // Sorted by key: expected_location, expected_salary, experience_summary,
    // job_intention, name, skill_summary. Built as Python's json.dumps would:
    // `{"k": v, ...}` with `, ` and `: ` separators, non-ASCII verbatim,
    // null for an absent value, and JSON string escaping for the rest.
    let mut slots: Vec<(&str, Option<&str>)> = Vec::new();
    for name in FIELD_NAMES {
        let value = fields
            .iter()
            .find(|(key, _)| *key == name)
            .and_then(|(_, value)| *value);
        slots.push((name, value));
    }
    slots.sort_by(|left, right| left.0.cmp(right.0));

    let body: String = slots
        .iter()
        .map(|(key, value)| match value {
            Some(text) => format!("{}: {}", py_json_string(key), py_json_string(text)),
            None => format!("{}: null", py_json_string(key)),
        })
        .collect::<Vec<_>>()
        .join(", ");
    let canonical = format!("{{{body}}}");
    hex_sha256(canonical.as_bytes())
}

/// One JSON string the way Python's `json.dumps(ensure_ascii=False)` writes
/// it: verbatim non-ASCII, and the two-character escapes the C encoder
/// produces (`\"`, `\\`, `\n`, `\r`, `\t`, control chars as `\uXXXX`).
fn py_json_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            control if (control as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", control as u32));
            }
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(64);
    for byte in digest {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The baseline's own values, computed with the Python module (the same
    /// expressions the audit probes used).
    #[test]
    fn normalize_name_matches_the_baseline() {
        assert_eq!(normalize_name("张伟"), "张伟");
        assert_eq!(normalize_name("张伟的个人简历"), "张伟");
        assert_eq!(normalize_name("Zhang Wei 简历"), "zhangwei");
        assert_eq!(normalize_name("  Ａｎｎａ "), "anna"); // NFKC folds width
        assert_eq!(normalize_name(""), "");
    }

    #[test]
    fn identity_key_is_sha256_of_name_and_domain() {
        // Measured on the frozen baseline (9ed173c):
        //   identity_key("张伟", "zhipin.com")
        assert_eq!(
            identity_key("张伟", Some("zhipin.com")),
            "86ee96e8a06d6e10b02ba5684d2e42a52d6c62fb7c1145be9c6cfa0074269943"
        );
        // The domain is lowercased and trimmed; a missing domain is "".
        assert_eq!(
            identity_key("张伟", Some(" ZHIPIN.COM ")),
            identity_key("张伟", Some("zhipin.com"))
        );
    }

    #[test]
    fn review_digest_is_sha256_of_the_sorted_canonical_json() {
        // The canonical text the baseline renders (json.dumps, sort_keys,
        // ensure_ascii=False, default separators, missing keys as null):
        //   {"expected_location": null, "expected_salary": null,
        //    "experience_summary": null, "job_intention": null,
        //    "name": "张伟", "skill_summary": null}
        // Measured on the frozen baseline: review_digest({"name": "张伟"}).
        assert_eq!(
            review_digest(&[("name", Some("张伟"))]),
            "87cf2bea731dabe4eaeea0ad1e1ba91d55cf547b0c0f067d5438293782101614"
        );
    }

    #[test]
    fn an_empty_pair_of_texts_has_no_content_fingerprint() {
        assert_eq!(content_fingerprint(None, None), "");
        assert_eq!(content_fingerprint(Some("  "), None), "");
        // Measured on the frozen baseline:
        //   content_fingerprint({"experience_summary": "五年 Rust",
        //                        "skill_summary": "系统设计"})
        assert_eq!(
            content_fingerprint(Some("五年 Rust"), Some("系统设计")),
            "ea7b84f8a276df0e8b670c2d0bf2d38556cac00f1a795ec6593996ea5c6f999a"
        );
    }
}
