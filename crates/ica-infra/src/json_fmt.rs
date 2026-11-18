//! JSON rendering that matches the baseline's `json.dumps` byte for byte.
//!
//! The baseline uses `json.dumps` in two different shapes, and both are
//! contract text because the result is stored in a column and compared:
//!
//! * **audit metadata** — `json.dumps(metadata, ensure_ascii=False)`: the
//!   DEFAULT separators (`", "` between items and `": "` after a key) and the
//!   INSERTION order of the keys. Python does NOT sort them here.
//! * **sealed export** — `json.dumps(..., separators=(",", ":"), sort_keys=True)`:
//!   compact AND sorted.
//!
//! `serde_json`'s `to_string` is compact and (without `preserve_order`) sorted
//! by key — which is the opposite of what the audit column needs. So the two
//! shapes get their own functions here rather than a shared default.

use serde_json::Value;

/// Render a value the way `json.dumps(value, ensure_ascii=False)` does.
///
/// Keys keep their insertion order (the crate enables serde_json's
/// `preserve_order`), and the separators are Python's defaults: a comma and a
/// space between items, a colon and a space after a key.
pub fn python_json(value: &Value) -> String {
    let mut out = String::new();
    write_python(value, &mut out);
    out
}

fn write_python(value: &Value, out: &mut String) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(flag) => out.push_str(if *flag { "true" } else { "false" }),
        Value::Number(number) => out.push_str(&number.to_string()),
        Value::String(text) => write_python_string(text, out),
        Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                write_python(item, out);
            }
            out.push(']');
        }
        Value::Object(entries) => {
            out.push('{');
            for (index, (key, item)) in entries.iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                write_python_string(key, out);
                out.push_str(": ");
                write_python(item, out);
            }
            out.push('}');
        }
    }
}

/// Write a JSON string with Python's escaping.
///
/// `ensure_ascii=False` means non-ASCII characters are written as themselves,
/// not as `\uXXXX` escapes. Control characters still get the short escapes
/// Python uses (`\n`, `\t`, `\r`, `\b`, `\f`) and anything else below 0x20
/// becomes `\u00xx`.
fn write_python_string(text: &str, out: &mut String) {
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            other if (other as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", other as u32));
            }
            other => out.push(other),
        }
    }
    out.push('"');
}

/// Render a value the way `json.dumps(value, separators=(",", ":"), sort_keys=True)`
/// does: compact, with the object's keys sorted.
pub fn python_json_sorted_compact(value: &Value) -> String {
    let mut out = String::new();
    write_sorted_compact(value, &mut out);
    out
}

fn write_sorted_compact(value: &Value, out: &mut String) {
    match value {
        Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_sorted_compact(item, out);
            }
            out.push(']');
        }
        Value::Object(entries) => {
            let mut sorted: Vec<(&String, &Value)> = entries.iter().collect();
            sorted.sort_by(|left, right| left.0.cmp(right.0));
            out.push('{');
            for (index, (key, item)) in sorted.into_iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_python_string(key, out);
                out.push(':');
                write_sorted_compact(item, out);
            }
            out.push('}');
        }
        other => write_python(other, out),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_simple_object_uses_the_python_separators() {
        let value = json!({ "source": "manual" });
        assert_eq!(python_json(&value), r#"{"source": "manual"}"#);
    }

    #[test]
    fn the_keys_keep_their_insertion_order() {
        // Python's `json.dumps` does NOT sort; a sorted rendering would be
        // `{"a": 1, "b": 2}` -> the same here only because the input is
        // already in that order. Feed it the other way round.
        let value = json!({ "b": 2, "a": 1 });
        assert_eq!(python_json(&value), r#"{"b": 2, "a": 1}"#);
    }

    #[test]
    fn nested_objects_keep_their_order_too() {
        let value = json!({ "candidate_id": "c-1", "task_id": "t-1" });
        assert_eq!(
            python_json(&value),
            r#"{"candidate_id": "c-1", "task_id": "t-1"}"#
        );
    }

    #[test]
    fn non_ascii_is_written_as_itself() {
        // `ensure_ascii=False`: the Chinese text stays readable.
        let value = json!({ "note": "确认" });
        assert_eq!(python_json(&value), "{\"note\": \"确认\"}");
    }

    #[test]
    fn control_characters_use_the_short_escapes() {
        let value = json!({ "text": "a\nb\tc" });
        assert_eq!(python_json(&value), r#"{"text": "a\nb\tc"}"#);
    }

    #[test]
    fn the_sorted_compact_form_sorts_and_is_tight() {
        let value = json!({ "b": 2, "a": [1, 2] });
        assert_eq!(python_json_sorted_compact(&value), r#"{"a":[1,2],"b":2}"#);
    }

    #[test]
    fn the_sorted_compact_form_escapes_like_python() {
        let value = json!({ "text": "确认", "n": 1 });
        assert_eq!(
            python_json_sorted_compact(&value),
            "{\"n\":1,\"text\":\"确认\"}"
        );
    }
}
