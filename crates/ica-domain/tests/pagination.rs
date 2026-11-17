//! Pagination maths, pinned to the Python baseline's behaviour.
//!
//! ``app/schemas/common.py::Page.has_more`` is
//! ``offset + len(items) < total`` — it uses the number of rows actually
//! returned, not the requested ``limit``, so a short page (offset near the
//! end) reports ``has_more = false`` even when ``offset + limit < total``.

use ica_domain::Page;

fn rows(n: usize) -> Vec<usize> {
    (0..n).collect()
}

#[test]
fn first_page_slices_and_reports_more() {
    let page = Page::from_items(rows(6), 0, 2);

    assert_eq!(page.items, vec![0, 1]);
    assert_eq!(page.total, 6);
    assert_eq!(page.offset, 0);
    assert_eq!(page.limit, 2);
    assert!(page.has_more());
}

#[test]
fn middle_page_reports_more() {
    let page = Page::from_items(rows(6), 2, 2);

    assert_eq!(page.items, vec![2, 3]);
    assert!(page.has_more());
}

#[test]
fn last_full_page_reports_no_more() {
    let page = Page::from_items(rows(6), 4, 2);

    assert_eq!(page.items, vec![4, 5]);
    assert!(!page.has_more());
}

#[test]
fn exact_fit_reports_no_more() {
    let page = Page::from_items(rows(6), 0, 6);

    assert_eq!(page.items.len(), 6);
    assert!(!page.has_more());
}

#[test]
fn short_page_near_the_end_reports_no_more() {
    // offset 3 + 3 returned rows == total 6, even though offset+limit is 13.
    let page = Page::from_items(rows(6), 3, 10);

    assert_eq!(page.items, vec![3, 4, 5]);
    assert_eq!(page.total, 6);
    assert!(!page.has_more());
}

#[test]
fn offset_past_total_returns_empty_items_but_keeps_total() {
    let page = Page::from_items(rows(6), 100, 50);

    assert!(page.items.is_empty());
    assert_eq!(page.total, 6);
    assert_eq!(page.offset, 100);
    assert_eq!(page.limit, 50);
    assert!(!page.has_more());
}

#[test]
fn limit_above_total_returns_every_row() {
    let page = Page::from_items(rows(6), 0, 500);

    assert_eq!(page.items.len(), 6);
    assert!(!page.has_more());
}

#[test]
fn empty_source_is_an_empty_page() {
    let page = Page::from_items(Vec::<usize>::new(), 0, 50);

    assert!(page.items.is_empty());
    assert_eq!(page.total, 0);
    assert!(!page.has_more());
}

#[test]
fn single_row_page_has_no_more() {
    let page = Page::from_items(rows(1), 0, 1);

    assert_eq!(page.items, vec![0]);
    assert!(!page.has_more());
}
