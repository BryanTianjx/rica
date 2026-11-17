//! Page envelope maths.
//!
//! Mirrors ``app/schemas/common.py::Page``: ``total`` counts every row that
//! matched the filter (before the window), and ``has_more`` is derived from
//! how many rows the page actually carries.

/// One page of results plus the envelope metadata the API returns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page<T> {
    /// Rows inside the requested window.
    pub items: Vec<T>,
    /// Rows that matched the filter, ignoring ``offset``/``limit``.
    pub total: usize,
    /// Skip count the caller asked for.
    pub offset: usize,
    /// Page size the caller asked for.
    pub limit: usize,
}

impl<T> Page<T> {
    /// Build a page from the full filtered result set.
    ///
    /// The slice is taken in memory, exactly like the Python baseline does
    /// (``positions[offset : offset + limit]``), so an offset past the end
    /// yields an empty ``items`` list while ``total`` keeps the real count.
    pub fn from_items(items: Vec<T>, offset: usize, limit: usize) -> Self {
        let total = items.len();
        let items = items.into_iter().skip(offset).take(limit).collect();
        Self {
            items,
            total,
            offset,
            limit,
        }
    }

    /// Whether at least one further row exists after this page.
    ///
    /// Uses the number of rows actually returned, not ``limit``: a short page
    /// at the end reports ``false`` even when ``offset + limit < total``.
    pub fn has_more(&self) -> bool {
        self.offset + self.items.len() < self.total
    }
}
