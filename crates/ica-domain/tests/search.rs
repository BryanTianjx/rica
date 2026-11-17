//! Search-term normalisation.
//!
//! The Python baseline does ``f"%{search.strip()}%"`` and treats a falsy
//! ``search`` as "no filter", so a blank or whitespace-only value must not
//! narrow the result set.

use ica_domain::{normalize_search, MAX_SEARCH_LEN};

#[test]
fn trims_surrounding_whitespace() {
    assert_eq!(normalize_search(" beijing "), Some("beijing".to_string()));
    assert_eq!(normalize_search("\tbeijing\n"), Some("beijing".to_string()));
}

#[test]
fn keeps_inner_whitespace_of_the_term() {
    assert_eq!(
        normalize_search("  senior rust  "),
        Some("senior rust".to_string())
    );
}

#[test]
fn empty_string_means_no_filter() {
    assert_eq!(normalize_search(""), None);
}

#[test]
fn whitespace_only_means_no_filter() {
    assert_eq!(normalize_search("    "), None);
    assert_eq!(normalize_search("\t\n"), None);
}

#[test]
fn non_ascii_terms_are_preserved() {
    assert_eq!(normalize_search(" 工程师 "), Some("工程师".to_string()));
}

#[test]
fn single_character_term_is_kept() {
    assert_eq!(normalize_search("a"), Some("a".to_string()));
}

#[test]
fn max_search_length_matches_the_python_query_contract() {
    // FastAPI: search: str | None = Query(default=None, max_length=200)
    assert_eq!(MAX_SEARCH_LEN, 200);
}
