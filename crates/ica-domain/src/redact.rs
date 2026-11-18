//! AI-context redaction — a pure port of
//! `app/application/import_pipeline.py::redact_for_ai` (and its privacy
//! filter). Every rule below is byte-for-byte the baseline's, because the
//! masked context is part of the `/candidates/{id}/context` response body.
//!
//! The baseline's digit patterns carry look-arounds — `(?<!\d)…(?!\d)` — and
//! Rust's `regex` crate deliberately does not implement look-around. The
//! guards are not lost: scanning **maximal digit runs** makes "not adjacent to
//! another digit" true by construction, so the same inputs match and the same
//! inputs are left alone. Getting this wrong is a privacy regression, not a
//! cosmetic one, hence the equivalence table in [`DigitRule`].

use regex::Regex;

/// Direct identifiers that should not be sent to an AI provider — the
/// `NAME_KEYS` prefix rules (`姓名：X` → `姓名：[REDACTED]`, …).
const NAME_KEYS: [&str; 7] = ["姓名", "名字", "电话", "手机", "邮箱", "微信", "身份证"];

/// Placeholder that replaces a known identity value (the candidate's name).
pub const NAME_PLACEHOLDER: &str = "[NAME]";

/// Placeholder for an e-mail address.
const EMAIL_PATTERN: &str = r"[\w.+-]+@[\w-]+\.[\w.-]+";

/// Which digit-run rule is being applied. The length/suffix tests reproduce
/// the baseline's patterns exactly, under a maximal-run scan:
///
/// | baseline pattern                  | maximal digit run                      |
/// | --------------------------------- | -------------------------------------- |
/// | `(?<!\d)(1[3-9]\d{9})(?!\d)`      | exactly 11, starts `1` then `3`–`9`    |
/// | `(?<!\d)(\d{17}[\dXx])(?!\d)`     | exactly 18, or exactly 17 then `X`/`x` |
/// | `(?<!\d)(\d{16,19})(?!\d)`        | 16–19                                  |
#[derive(Clone, Copy, PartialEq, Eq)]
enum DigitRule {
    Phone,
    IdNumber,
    CardNumber,
}

/// Replace every digit run matching `rule` with `placeholder`.
///
/// Returns the new text and whether anything was replaced. `\d+` is greedy,
/// so each match is a maximal digit run: the baseline's `(?<!\d)` / `(?!\d)`
/// guards hold without any extra check.
fn mask_digit_runs(text: &str, rule: DigitRule, placeholder: &str) -> (String, bool) {
    let digits = Regex::new(r"\d+").expect("static pattern");
    let mut out = String::with_capacity(text.len());
    let mut last = 0usize;
    let mut hit = false;
    for found in digits.find_iter(text) {
        let start = found.start();
        let end = found.end();
        let run: Vec<char> = found.as_str().chars().collect();
        let length = run.len();
        // The ID pattern's `[\dXx]` tail may sit just past the digit run.
        let next = text[end..].chars().next();
        let (matched, consumed_end) = match rule {
            DigitRule::Phone => (
                length == 11 && run[0] == '1' && ('3'..='9').contains(&run[1]),
                end,
            ),
            DigitRule::IdNumber => {
                if length == 18 {
                    (true, end)
                } else if length == 17 && matches!(next, Some('X' | 'x')) {
                    // Consume the trailing X/x, exactly like `[\dXx]` does.
                    (true, end + next.map_or(0, char::len_utf8))
                } else {
                    (false, end)
                }
            }
            DigitRule::CardNumber => ((16..=19).contains(&length), end),
        };
        if matched {
            out.push_str(&text[last..start]);
            out.push_str(placeholder);
            last = consumed_end;
            hit = true;
        }
    }
    if !hit {
        return (text.to_string(), false);
    }
    out.push_str(&text[last..]);
    (out, true)
}

/// The compiled e-mail pattern, built once per call — never inside the
/// rule loop (a regex compiled per iteration is a per-call allocation of a
/// DFA, and clippy's `regex_creation_in_loops` rightly rejects it).
fn email_regex() -> Regex {
    Regex::new(EMAIL_PATTERN).expect("static pattern")
}

/// Mask direct identifiers in a copy of `text`; returns the masked text and
/// the sorted categories that were masked. The original data is never touched.
pub fn apply_privacy_filter(text: &str) -> (String, Vec<String>) {
    let mut masked = text.to_string();
    let mut found: Vec<String> = Vec::new();
    let email = email_regex();
    // The baseline's order — phone, email, id, card — is preserved. It cannot
    // matter (a placeholder contains no digits), but keeping it makes the two
    // implementations diffable side by side.
    for (rule, placeholder) in [
        (Some(DigitRule::Phone), "[PHONE]"),
        (None, "[EMAIL]"),
        (Some(DigitRule::IdNumber), "[ID_NUMBER]"),
        (Some(DigitRule::CardNumber), "[CARD_NUMBER]"),
    ] {
        match rule {
            Some(rule) => {
                let (next, hit) = mask_digit_runs(&masked, rule, placeholder);
                if hit {
                    masked = next;
                    found.push(placeholder.trim_matches(['[', ']']).to_lowercase());
                }
            }
            None => {
                if email.is_match(&masked) {
                    masked = email.replace_all(&masked, placeholder).to_string();
                    found.push(placeholder.trim_matches(['[', ']']).to_lowercase());
                }
            }
        }
    }
    found.sort();
    found.dedup();
    (masked, found)
}

/// Prepare a message body (or any free text) for AI consumption:
/// privacy filter first, then the `NAME_KEYS` prefix rules, then a literal
/// replacement of the candidate's name (tokens of two or more characters — a
/// one-character name would match ordinary prose and corrupt the context).
pub fn redact_for_ai(text: &str, names: &[&str]) -> (String, Vec<String>) {
    let (mut masked, mut categories) = apply_privacy_filter(text);
    for key in NAME_KEYS {
        // Python: rf"{key}\s*[:：]\s*[^\s,，。;；\n]+" → f"{key}：[REDACTED]"
        let pattern = format!(r"{key}\s*[:：]\s*[^\s,，。;；\n]+");
        let compiled = Regex::new(&pattern).expect("static pattern");
        let replacement = format!("{key}：[REDACTED]");
        masked = compiled
            .replace_all(&masked, replacement.as_str())
            .to_string();
    }
    for name in names {
        let token = name.trim();
        if token.chars().count() < 2 {
            continue;
        }
        if let Ok(compiled) = Regex::new(&regex::escape(token)) {
            let count = compiled.find_iter(&masked).count();
            if count > 0 {
                masked = compiled.replace_all(&masked, NAME_PLACEHOLDER).to_string();
                categories.push("name".to_string());
            }
        }
    }
    categories.sort();
    categories.dedup();
    (masked, categories)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn privacy_patterns_mask_phone_email_id_and_card() {
        let (masked, categories) = apply_privacy_filter(
            "电话13812345678，邮箱 a.b@corp.com，证件 11010119900307891X，卡号 6222021234567890123",
        );
        assert!(masked.contains("[PHONE]"), "{masked}");
        assert!(masked.contains("[EMAIL]"), "{masked}");
        assert!(masked.contains("[ID_NUMBER]"), "{masked}");
        assert!(masked.contains("[CARD_NUMBER]"), "{masked}");
        assert_eq!(
            categories,
            vec!["card_number", "email", "id_number", "phone"]
        );
    }

    #[test]
    fn name_key_prefixes_are_redacted_with_the_key_kept() {
        // `apply_privacy_filter` does not touch NAME_KEYS — that is
        // `redact_for_ai`'s job — so this test drives the full pipeline.
        let (masked, categories) = redact_for_ai("姓名：张三 电话：13812345678", &[]);
        assert!(masked.contains("姓名：[REDACTED]"), "{masked}");
        // The `电话` key rule also rewrites the already-masked placeholder:
        // the key survives, the value is gone either way.
        assert!(masked.contains("电话：[REDACTED]"), "{masked}");
        assert_eq!(categories, vec!["phone"]);
    }

    #[test]
    fn the_candidate_name_is_replaced_everywhere_it_occurs() {
        let (masked, categories) = redact_for_ai("张三来电，张三说稍后回复。", &["张三"]);
        assert_eq!(masked, "[NAME]来电，[NAME]说稍后回复。");
        assert!(categories.contains(&"name".to_string()));
    }

    #[test]
    fn one_character_names_are_ignored_like_python() {
        let (masked, _) = redact_for_ai("我叫王，王先生", &["王"]);
        assert_eq!(masked, "我叫王，王先生");
    }

    // ── Maximal-run equivalence with the baseline's look-around patterns ──

    #[test]
    fn a_twelve_digit_run_is_not_a_phone_number() {
        // `(?<!\d)(1[3-9]\d{9})(?!\d)` cannot match inside a 12-digit run.
        let (masked, categories) = apply_privacy_filter("138123456789");
        assert_eq!(masked, "138123456789");
        assert!(categories.is_empty());
    }

    #[test]
    fn a_twenty_digit_run_matches_no_rule() {
        // Too long for the card pattern's 19-digit ceiling.
        let (masked, categories) = apply_privacy_filter("62220212345678901234");
        assert_eq!(masked, "62220212345678901234");
        assert!(categories.is_empty());
    }

    #[test]
    fn a_seventeen_digit_run_is_a_card_number_not_an_id() {
        // `\d{17}[\dXx]` needs 18 characters, so 17 digits fall to the card rule.
        let (masked, categories) = apply_privacy_filter("12345678901234567");
        assert_eq!(masked, "[CARD_NUMBER]");
        assert_eq!(categories, vec!["card_number"]);
    }

    #[test]
    fn an_eighteen_digit_run_prefers_the_id_rule() {
        // Both `\d{17}[\dXx]` and `\d{16,19}` match; the baseline lists ID first.
        let (masked, categories) = apply_privacy_filter("110101199003078912");
        assert_eq!(masked, "[ID_NUMBER]");
        assert_eq!(categories, vec!["id_number"]);
    }
}
