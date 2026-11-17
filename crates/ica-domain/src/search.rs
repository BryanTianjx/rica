//! Search-term normalisation.

/// Longest accepted ``search`` term, in characters.
///
/// The Python query parameter is ``Query(default=None, max_length=200)``, and
/// pydantic counts characters, not bytes.
pub const MAX_SEARCH_LEN: usize = 200;

/// Normalise a raw ``search`` query value into the term to match.
///
/// Returns ``None`` when the value carries no filter (empty or whitespace
/// only), which is what the Python baseline does by treating a falsy
/// ``search`` as "no filter" and by stripping before building the pattern.
pub fn normalize_search(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}
