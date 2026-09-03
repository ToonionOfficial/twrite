//! Per-version viewport cache for expensive per-line inputs.
//!
//! `highlight_line` (pulldown parse), `ConcealedLine::build`, and `extract_links`
//! are pure in `(buffer version, highlighter revision, row, active-row flag, line
//! text)` but were recomputed for every visible row on every prepaint *and* again
//! on every hit-test (`offset_for_position`). This cache computes each row once
//! per epoch and shares it across prepaint, hover, and click paths.
//!
//! Deliberately *not* cached here: `shape_text` output (`WrappedLine` is neither
//! `Clone` nor reconstructible via public GPUI API) and `TextRun`s (depend on the
//! live selection). Glyph layout itself is already deduped inside GPUI's
//! `line_layout_cache` across consecutive frames.

use std::collections::HashMap;
use std::ops::Range;

use twrite_core::{ConcealedLine, EditorBuffer, StyleSpan, SyntaxHighlighter};

/// Upper bound on cached rows; exceeded maps are dropped wholesale (one full
/// re-parse, no incremental eviction bookkeeping).
const MAX_CACHED_ROWS: usize = 2048;

/// Owned per-line inputs shared by prepaint and hit-testing.
#[derive(Debug, Clone)]
pub struct CachedInput {
    /// Original (pre-concealment) highlight spans.
    pub spans: Vec<StyleSpan>,
    /// Concealed display text, remapped spans, and source/display mapping.
    pub concealed: ConcealedLine,
    /// Hyperlink source ranges and URLs from `SyntaxHighlighter::extract_links`.
    pub link_src: Vec<(Range<usize>, String)>,
}

#[derive(Debug, Clone)]
struct CachedRow {
    /// Whether the row was the cursor row when computed (active lines expose
    /// markers instead of concealing them, so spans differ).
    active: bool,
    input: CachedInput,
}

/// Viewport input cache keyed by buffer version + highlighter revision.
#[derive(Debug, Default)]
pub struct LayoutCache {
    version: Option<usize>,
    highlighter_rev: Option<u64>,
    rows: HashMap<usize, CachedRow>,
    hits: u64,
    misses: u64,
}

impl LayoutCache {
    /// Creates an empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Drops all cached rows and hit/miss counters.
    pub fn clear(&mut self) {
        self.rows.clear();
        self.version = None;
        self.highlighter_rev = None;
        self.hits = 0;
        self.misses = 0;
    }

    /// Returns `(hits, misses)` since creation or the last [`Self::clear`].
    pub fn stats(&self) -> (u64, u64) {
        (self.hits, self.misses)
    }

    /// Number of rows currently cached.
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Whether the cache holds no rows.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Returns the cached input for `row`, computing and storing it on miss.
    ///
    /// `cursor_row` is hoisted by the caller so `buffer.cursor_point()` (two
    /// `O(log n)` walks) runs once per frame, not once per row. `line_text`
    /// must be the raw line *without* trailing `\r\n`, matching prepaint.
    pub fn cached_input(
        &mut self,
        buffer: &EditorBuffer,
        highlighter: Option<&dyn SyntaxHighlighter>,
        highlighter_rev: u64,
        cursor_row: usize,
        row: usize,
        line_text: &str,
    ) -> &CachedInput {
        let version = buffer.version();
        if self.version != Some(version) || self.highlighter_rev != Some(highlighter_rev) {
            self.rows.clear();
            self.version = Some(version);
            self.highlighter_rev = Some(highlighter_rev);
        }
        let active = row == cursor_row;
        if let Some(cached) = self.rows.get(&row)
            && cached.active == active
        {
            self.hits += 1;
            // Re-borrow to satisfy the borrow checker across the counter bump.
            return &self.rows.get(&row).expect("row present").input;
        }
        self.misses += 1;
        if self.rows.len() >= MAX_CACHED_ROWS {
            self.rows.clear();
        }
        let spans = highlighter
            .map(|h| h.highlight_line(buffer, row, line_text))
            .unwrap_or_default();
        let concealed = ConcealedLine::build(line_text, &spans);
        let link_src = highlighter
            .map(|h| h.extract_links(buffer, row, line_text))
            .unwrap_or_default();
        self.rows.insert(
            row,
            CachedRow {
                active,
                input: CachedInput {
                    spans,
                    concealed,
                    link_src,
                },
            },
        );
        &self.rows.get(&row).expect("row just inserted").input
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_buffer(lines: usize) -> EditorBuffer {
        let text = (0..lines)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        EditorBuffer::new(&text)
    }

    #[test]
    fn second_pass_is_all_hits() {
        let buf = empty_buffer(50);
        let mut cache = LayoutCache::new();
        for row in 0..buf.len_lines() {
            let line = buf.line_to_string(row);
            let text = line.trim_end_matches(['\r', '\n']);
            cache.cached_input(&buf, None, 0, usize::MAX, row, text);
        }
        assert_eq!(cache.stats(), (0, 50));
        for row in 0..buf.len_lines() {
            let line = buf.line_to_string(row);
            let text = line.trim_end_matches(['\r', '\n']);
            cache.cached_input(&buf, None, 0, usize::MAX, row, text);
        }
        assert_eq!(cache.stats(), (50, 50));
        assert_eq!(cache.len(), 50);
    }

    #[test]
    fn version_bump_invalidates() {
        let mut buf = empty_buffer(10);
        let mut cache = LayoutCache::new();
        let line = buf.line_to_string(0);
        let text = line.trim_end_matches(['\r', '\n']).to_string();
        cache.cached_input(&buf, None, 0, usize::MAX, 0, &text);
        assert_eq!(cache.stats(), (0, 1));
        buf.insert("x");
        let line = buf.line_to_string(0);
        let text = line.trim_end_matches(['\r', '\n']).to_string();
        cache.cached_input(&buf, None, 0, usize::MAX, 0, &text);
        // Epoch change clears rows: stats keep accumulating, row count restarts.
        assert_eq!(cache.stats(), (0, 2));
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn cursor_row_flip_recomputes_only_flipped_rows() {
        let buf = empty_buffer(4);
        let mut cache = LayoutCache::new();
        for row in 0..4 {
            let line = buf.line_to_string(row);
            let text = line.trim_end_matches(['\r', '\n']).to_string();
            cache.cached_input(&buf, None, 0, 0, row, &text);
        }
        assert_eq!(cache.stats(), (0, 4));
        // Same cursor row -> all hits.
        for row in 0..4 {
            let line = buf.line_to_string(row);
            let text = line.trim_end_matches(['\r', '\n']).to_string();
            cache.cached_input(&buf, None, 0, 0, row, &text);
        }
        assert_eq!(cache.stats(), (4, 4));
        // Cursor moves 0 -> 1: rows 0 and 1 miss (active flag flips), 2-3 hit.
        for row in 0..4 {
            let line = buf.line_to_string(row);
            let text = line.trim_end_matches(['\r', '\n']).to_string();
            cache.cached_input(&buf, None, 0, 1, row, &text);
        }
        assert_eq!(cache.stats(), (6, 6));
    }

    #[test]
    fn highlighter_rev_bump_invalidates() {
        let buf = empty_buffer(5);
        let mut cache = LayoutCache::new();
        for row in 0..5 {
            let line = buf.line_to_string(row);
            let text = line.trim_end_matches(['\r', '\n']).to_string();
            cache.cached_input(&buf, None, 0, usize::MAX, row, &text);
        }
        assert_eq!(cache.len(), 5);
        let line = buf.line_to_string(0);
        let text = line.trim_end_matches(['\r', '\n']).to_string();
        cache.cached_input(&buf, None, 1, usize::MAX, 0, &text);
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn clear_resets_stats() {
        let buf = empty_buffer(3);
        let mut cache = LayoutCache::new();
        let line = buf.line_to_string(0);
        let text = line.trim_end_matches(['\r', '\n']).to_string();
        cache.cached_input(&buf, None, 0, usize::MAX, 0, &text);
        cache.clear();
        assert_eq!(cache.stats(), (0, 0));
        assert!(cache.is_empty());
    }
}
