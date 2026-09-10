use std::ops::Range;

use crate::{EditorBuffer, EditorError};

/// A headless find query: literal substring or regex, with case and
/// whole-word toggles. Compiled to a [`regex::Regex`] on use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchQuery {
    /// The raw pattern text (literal or regex source).
    pub pattern: String,
    /// Whether matching is case-sensitive (default `true`).
    pub case_sensitive: bool,
    /// Whether matches must span whole words (default `false`).
    pub whole_word: bool,
    /// Whether `pattern` is a regex (default `false` = literal substring).
    pub is_regex: bool,
}

impl SearchQuery {
    /// Creates a case-sensitive literal substring query.
    pub fn literal(pattern: &str) -> Self {
        Self {
            pattern: pattern.to_string(),
            case_sensitive: true,
            whole_word: false,
            is_regex: false,
        }
    }

    /// Creates a query with explicit options.
    pub fn new(pattern: &str, case_sensitive: bool, whole_word: bool, is_regex: bool) -> Self {
        Self {
            pattern: pattern.to_string(),
            case_sensitive,
            whole_word,
            is_regex,
        }
    }
}

impl Default for SearchQuery {
    fn default() -> Self {
        Self::literal("")
    }
}

/// Compiles a [`SearchQuery`] into a [`regex::Regex`].
///
/// Literal patterns are escaped; `whole_word` wraps the core in
/// `\b(?:...)\b`; case-insensitive queries gain a `(?i)` prefix.
pub fn compile_query(query: &SearchQuery) -> Result<regex::Regex, EditorError> {
    if query.pattern.is_empty() {
        return Err(EditorError::EmptySearchPattern);
    }
    let mut source = if query.is_regex {
        query.pattern.clone()
    } else {
        regex::escape(&query.pattern)
    };
    if query.whole_word {
        source = format!(r"\b(?:{source})\b");
    }
    if !query.case_sensitive {
        source = format!(r"(?i){source}");
    }
    regex::Regex::new(&source).map_err(|e| EditorError::InvalidRegex {
        pattern: query.pattern.clone(),
        message: e.to_string(),
    })
}

/// Returns all match byte ranges for `query` in `buffer`, in ascending order.
pub fn find_matches(
    buffer: &EditorBuffer,
    query: &SearchQuery,
) -> Result<Vec<Range<usize>>, EditorError> {
    let re = compile_query(query)?;
    let text = buffer.text().to_string();
    Ok(re
        .find_iter(&text)
        .map(|m| m.start()..m.end())
        .filter(|r| buffer.is_char_boundary(r.start) && buffer.is_char_boundary(r.end))
        .collect())
}

/// Returns the first match at or after `from_offset`, wrapping to the start
/// when `wrap` is set. Returns `Ok(None)` when there are no matches.
pub fn find_next(
    buffer: &EditorBuffer,
    query: &SearchQuery,
    from_offset: usize,
    wrap: bool,
) -> Result<Option<Range<usize>>, EditorError> {
    let matches = find_matches(buffer, query)?;
    if matches.is_empty() {
        return Ok(None);
    }
    let from = from_offset.min(buffer.len_bytes());
    if let Some(m) = matches.iter().find(|m| m.start >= from) {
        return Ok(Some(m.clone()));
    }
    if wrap {
        Ok(matches.into_iter().next())
    } else {
        Ok(None)
    }
}

/// Returns the last match at or before `from_offset`, wrapping to the end
/// when `wrap` is set. Returns `Ok(None)` when there are no matches.
pub fn find_prev(
    buffer: &EditorBuffer,
    query: &SearchQuery,
    from_offset: usize,
    wrap: bool,
) -> Result<Option<Range<usize>>, EditorError> {
    let matches = find_matches(buffer, query)?;
    if matches.is_empty() {
        return Ok(None);
    }
    let from = from_offset.min(buffer.len_bytes());
    // Strictly-before so repeated `prev` calls walk backwards instead of
    // re-reporting the match that starts exactly at the cursor.
    if let Some(m) = matches.iter().rev().find(|m| m.start < from) {
        return Ok(Some(m.clone()));
    }
    if wrap {
        Ok(matches.into_iter().next_back())
    } else {
        Ok(None)
    }
}

/// Version-cached search state for interactive find (prompt, vim `/`).
///
/// Call [`Self::refresh`] after edits or query changes; navigation is served
/// from the cache without rescanning.
#[derive(Debug, Clone, Default)]
pub struct SearchState {
    query: Option<SearchQuery>,
    matches: Vec<Range<usize>>,
    current: Option<usize>,
    version: usize,
}

impl SearchState {
    /// Creates an empty state with no query.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the active query (clears matches until [`Self::refresh`]).
    pub fn set_query(&mut self, query: SearchQuery) {
        if self.query.as_ref() != Some(&query) {
            self.query = Some(query);
            self.matches.clear();
            self.current = None;
            // Sentinel: unreachable by real edits, forces the next
            // `refresh` to rescan even when the document is unchanged.
            self.version = usize::MAX;
        }
    }

    /// Returns the active query, if any.
    pub fn query(&self) -> Option<&SearchQuery> {
        self.query.as_ref()
    }

    /// Re-scans `buffer` when the document version or query changed.
    pub fn refresh(&mut self, buffer: &EditorBuffer) -> Result<(), EditorError> {
        if self.version == buffer.version() {
            return Ok(());
        }
        let Some(query) = self.query.clone() else {
            self.matches.clear();
            self.current = None;
            return Ok(());
        };
        let matches = find_matches(buffer, &query)?;
        self.version = buffer.version();
        if self.current.is_some_and(|i| i >= matches.len()) {
            self.current = None;
        }
        self.matches = matches;
        Ok(())
    }

    /// All cached match ranges in ascending order.
    pub fn matches(&self) -> &[Range<usize>] {
        &self.matches
    }

    /// Number of cached matches.
    pub fn match_count(&self) -> usize {
        self.matches.len()
    }

    /// Index of the current match, if navigation has occurred.
    pub fn current_index(&self) -> Option<usize> {
        self.current
    }

    /// The current match range, if navigation has occurred.
    pub fn current_match(&self) -> Option<Range<usize>> {
        self.current.and_then(|i| self.matches.get(i).cloned())
    }

    /// Advances to the next match at or after `from_offset`.
    pub fn next(
        &mut self,
        buffer: &EditorBuffer,
        from_offset: usize,
        wrap: bool,
    ) -> Result<Option<Range<usize>>, EditorError> {
        self.refresh(buffer)?;
        if self.matches.is_empty() {
            self.current = None;
            return Ok(None);
        }
        let from = from_offset.min(buffer.len_bytes());
        let idx = self
            .matches
            .iter()
            .position(|m| m.start >= from)
            .or_else(|| wrap.then_some(0));
        match idx {
            Some(i) => {
                self.current = Some(i);
                Ok(Some(self.matches[i].clone()))
            }
            None => Ok(None),
        }
    }

    /// Moves to the previous match at or before `from_offset`.
    pub fn prev(
        &mut self,
        buffer: &EditorBuffer,
        from_offset: usize,
        wrap: bool,
    ) -> Result<Option<Range<usize>>, EditorError> {
        self.refresh(buffer)?;
        if self.matches.is_empty() {
            self.current = None;
            return Ok(None);
        }
        let from = from_offset.min(buffer.len_bytes());
        let idx = self
            .matches
            .iter()
            .rposition(|m| m.start < from)
            .or_else(|| wrap.then(|| self.matches.len() - 1));
        match idx {
            Some(i) => {
                self.current = Some(i);
                Ok(Some(self.matches[i].clone()))
            }
            None => Ok(None),
        }
    }
}

/// Collects `(range, expanded_text)` pairs for `query` in `text`, with
/// 0-based ascending ranges.
///
/// Ranges are non-empty and on char boundaries. Supports `$1` / `$name`
/// capture expansion in regex mode; literal mode uses `replacement` verbatim.
/// [`replace_all_query`] applies these to a whole buffer; line-scoped
/// substitutes (`:s` without `%`) collect on a line slice and offset the
/// ranges by the line start.
pub fn collect_replacements(
    text: &str,
    query: &SearchQuery,
    replacement: &str,
) -> Result<Vec<(Range<usize>, String)>, EditorError> {
    let re = compile_query(query)?;
    let mut replacements = Vec::new();
    if query.is_regex {
        // Per-match `$1` / `$name` capture expansion.
        for caps in re.captures_iter(text) {
            let m = caps.get(0).expect("captures_iter always yields group 0");
            let range = m.start()..m.end();
            if range.is_empty() {
                continue;
            }
            if !(text.is_char_boundary(range.start) && text.is_char_boundary(range.end)) {
                continue;
            }
            let mut expanded = String::new();
            caps.expand(replacement, &mut expanded);
            replacements.push((range, expanded));
        }
    } else {
        for m in re.find_iter(text) {
            let range = m.start()..m.end();
            if range.is_empty() {
                continue;
            }
            if !(text.is_char_boundary(range.start) && text.is_char_boundary(range.end)) {
                continue;
            }
            replacements.push((range, replacement.to_string()));
        }
    }
    Ok(replacements)
}

/// Replaces all matches of `query` with `replacement` as a **single**
/// undoable transaction. Supports `$1` / `$name` capture expansion in regex
/// mode; literal mode uses `replacement` verbatim. Returns the number of
/// replacements applied.
pub fn replace_all_query(
    buffer: &mut EditorBuffer,
    query: &SearchQuery,
    replacement: &str,
) -> Result<usize, EditorError> {
    let text = buffer.text().to_string();
    let replacements = collect_replacements(&text, query, replacement)?;
    Ok(buffer.replace_many(replacements))
}

/// Replaces the single match `range` with `replacement` (undoable).
///
/// Returns `Ok(false)` without touching the buffer when `range` is empty,
/// out of bounds, or not a current match of `query`. Regex replacements
/// expand `$1` / `$name` captures.
pub fn replace_one_query(
    buffer: &mut EditorBuffer,
    query: &SearchQuery,
    range: Range<usize>,
    replacement: &str,
) -> Result<bool, EditorError> {
    if range.is_empty() || range.end > buffer.len_bytes() {
        return Ok(false);
    }
    if !buffer.is_char_boundary(range.start) || !buffer.is_char_boundary(range.end) {
        return Ok(false);
    }
    let text = buffer.text().to_string();
    let expanded = collect_replacements(&text, query, replacement)?
        .into_iter()
        .find(|(r, _)| *r == range)
        .map(|(_, expanded)| expanded);
    match expanded {
        Some(expanded) => {
            buffer.replace_range(range, &expanded);
            Ok(true)
        }
        None => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_find_returns_ascending_byte_ranges() {
        let buffer = EditorBuffer::new("hello world hello");
        let matches = find_matches(&buffer, &SearchQuery::literal("hello")).unwrap();
        assert_eq!(matches, vec![0..5, 12..17]);
    }

    #[test]
    fn literal_is_case_sensitive_by_default() {
        let buffer = EditorBuffer::new("Hello hello");
        let matches = find_matches(&buffer, &SearchQuery::literal("hello")).unwrap();
        assert_eq!(matches, vec![6..11]);
    }

    #[test]
    fn case_insensitive_matches_all_cases() {
        let buffer = EditorBuffer::new("Hello HELLO hello");
        let query = SearchQuery::new("hello", false, false, false);
        let matches = find_matches(&buffer, &query).unwrap();
        assert_eq!(matches, vec![0..5, 6..11, 12..17]);
    }

    #[test]
    fn regex_mode_matches_pattern_class() {
        let buffer = EditorBuffer::new("hello hallo hxllo");
        let query = SearchQuery::new("h.llo", true, false, true);
        let matches = find_matches(&buffer, &query).unwrap();
        assert_eq!(matches, vec![0..5, 6..11, 12..17]);
    }

    #[test]
    fn literal_mode_does_not_treat_dot_as_wildcard() {
        let buffer = EditorBuffer::new("h.llo hello");
        let matches = find_matches(&buffer, &SearchQuery::literal("h.llo")).unwrap();
        assert_eq!(matches, vec![0..5]);
    }

    #[test]
    fn whole_word_skips_substring_matches() {
        let buffer = EditorBuffer::new("foo foobar foo");
        let query = SearchQuery::new("foo", true, true, false);
        let matches = find_matches(&buffer, &query).unwrap();
        assert_eq!(matches, vec![0..3, 11..14]);
    }

    #[test]
    fn empty_pattern_is_an_error() {
        let buffer = EditorBuffer::new("hello");
        let err = find_matches(&buffer, &SearchQuery::literal("")).unwrap_err();
        assert!(matches!(err, EditorError::EmptySearchPattern));
    }

    #[test]
    fn invalid_regex_is_an_error() {
        let buffer = EditorBuffer::new("hello");
        let query = SearchQuery::new("(", true, false, true);
        let err = find_matches(&buffer, &query).unwrap_err();
        assert!(matches!(err, EditorError::InvalidRegex { .. }));
    }

    #[test]
    fn no_match_returns_empty_vec() {
        let buffer = EditorBuffer::new("hello world");
        let matches = find_matches(&buffer, &SearchQuery::literal("xyz")).unwrap();
        assert!(matches.is_empty());
    }

    #[test]
    fn unicode_matches_land_on_char_boundaries() {
        let buffer = EditorBuffer::new("héllo héllo");
        // "é" is 2 bytes: "héllo" spans 6 bytes.
        let matches = find_matches(&buffer, &SearchQuery::literal("héllo")).unwrap();
        assert_eq!(matches, vec![0..6, 7..13]);
        for m in &matches {
            assert!(buffer.is_char_boundary(m.start));
            assert!(buffer.is_char_boundary(m.end));
        }
    }

    #[test]
    fn find_next_advances_and_wraps() {
        let buffer = EditorBuffer::new("aa aa aa");
        let query = SearchQuery::literal("aa");
        assert_eq!(find_next(&buffer, &query, 0, true).unwrap(), Some(0..2));
        assert_eq!(find_next(&buffer, &query, 1, true).unwrap(), Some(3..5));
        assert_eq!(find_next(&buffer, &query, 7, true).unwrap(), Some(0..2));
        assert_eq!(find_next(&buffer, &query, 7, false).unwrap(), None);
    }

    #[test]
    fn find_prev_retreats_and_wraps() {
        let buffer = EditorBuffer::new("aa aa aa");
        let query = SearchQuery::literal("aa");
        assert_eq!(find_prev(&buffer, &query, 8, true).unwrap(), Some(6..8));
        assert_eq!(find_prev(&buffer, &query, 6, true).unwrap(), Some(3..5));
        assert_eq!(find_prev(&buffer, &query, 0, true).unwrap(), Some(6..8));
        assert_eq!(find_prev(&buffer, &query, 0, false).unwrap(), None);
    }

    #[test]
    fn search_state_caches_and_rescans_on_edit() {
        let mut buffer = EditorBuffer::new("foo foo");
        let mut state = SearchState::new();
        state.set_query(SearchQuery::literal("foo"));
        state.refresh(&buffer).unwrap();
        assert_eq!(state.match_count(), 2);

        buffer.set_cursor_offset(buffer.len_bytes());
        buffer.insert(" foo");
        state.refresh(&buffer).unwrap();
        assert_eq!(state.match_count(), 3);
        assert_eq!(state.matches(), &[0..3, 4..7, 8..11]);
    }

    #[test]
    fn search_state_navigation_tracks_current() {
        let buffer = EditorBuffer::new("aa aa aa");
        let mut state = SearchState::new();
        state.set_query(SearchQuery::literal("aa"));
        assert_eq!(state.next(&buffer, 0, true).unwrap(), Some(0..2));
        assert_eq!(state.current_index(), Some(0));
        assert_eq!(state.next(&buffer, 1, true).unwrap(), Some(3..5));
        assert_eq!(state.current_index(), Some(1));
        assert_eq!(state.prev(&buffer, 4, true).unwrap(), Some(3..5));
        assert_eq!(state.current_match(), Some(3..5));
    }

    #[test]
    fn replace_many_is_a_single_undo_step() {
        let mut buffer = EditorBuffer::new("a a a");
        let n = buffer.replace_many(vec![
            (0..1, "b".to_string()),
            (2..3, "b".to_string()),
            (4..5, "b".to_string()),
        ]);
        assert_eq!(n, 3);
        assert_eq!(buffer.text().to_string(), "b b b");

        buffer.undo();
        assert_eq!(buffer.text().to_string(), "a a a");

        buffer.redo();
        assert_eq!(buffer.text().to_string(), "b b b");
    }

    #[test]
    fn replace_many_with_no_matches_changes_nothing() {
        let mut buffer = EditorBuffer::new("hello");
        let version = buffer.version();
        let n = buffer.replace_many(vec![]);
        assert_eq!(n, 0);
        assert_eq!(buffer.version(), version);
        assert_eq!(buffer.text().to_string(), "hello");
    }

    #[test]
    fn replace_many_handles_growing_replacements() {
        let mut buffer = EditorBuffer::new("ab ab");
        let n = buffer.replace_many(vec![(0..2, "abcd".to_string()), (3..5, "abcd".to_string())]);
        assert_eq!(n, 2);
        assert_eq!(buffer.text().to_string(), "abcd abcd");
        buffer.undo();
        assert_eq!(buffer.text().to_string(), "ab ab");
        buffer.redo();
        assert_eq!(buffer.text().to_string(), "abcd abcd");
    }

    #[test]
    fn replace_all_query_literal_replaces_everything_at_once() {
        let mut buffer = EditorBuffer::new("foo bar foo");
        let n = replace_all_query(&mut buffer, &SearchQuery::literal("foo"), "baz").unwrap();
        assert_eq!(n, 2);
        assert_eq!(buffer.text().to_string(), "baz bar baz");
        buffer.undo();
        assert_eq!(buffer.text().to_string(), "foo bar foo");
    }

    #[test]
    fn replace_all_query_expands_regex_captures() {
        let mut buffer = EditorBuffer::new("2024-01-02");
        let query = SearchQuery::new(r"(\d+)-(\d+)-(\d+)", true, false, true);
        let n = replace_all_query(&mut buffer, &query, "$3/$2/$1").unwrap();
        assert_eq!(n, 1);
        assert_eq!(buffer.text().to_string(), "02/01/2024");
        buffer.undo();
        assert_eq!(buffer.text().to_string(), "2024-01-02");
    }

    #[test]
    fn replace_all_query_with_no_matches_is_a_noop() {
        let mut buffer = EditorBuffer::new("hello");
        let version = buffer.version();
        let n = replace_all_query(&mut buffer, &SearchQuery::literal("xyz"), "baz").unwrap();
        assert_eq!(n, 0);
        assert_eq!(buffer.version(), version);
    }

    #[test]
    fn replace_all_query_rejects_empty_and_invalid_patterns() {
        let mut buffer = EditorBuffer::new("hello");
        let err = replace_all_query(&mut buffer, &SearchQuery::literal(""), "x").unwrap_err();
        assert!(matches!(err, EditorError::EmptySearchPattern));

        let bad = SearchQuery::new("(", true, false, true);
        let err = replace_all_query(&mut buffer, &bad, "x").unwrap_err();
        assert!(matches!(err, EditorError::InvalidRegex { .. }));
    }

    #[test]
    fn replace_one_query_replaces_exact_match_only() {
        let mut buffer = EditorBuffer::new("foo bar foo");
        assert!(replace_one_query(&mut buffer, &SearchQuery::literal("foo"), 0..3, "baz").unwrap());
        assert_eq!(buffer.text().to_string(), "baz bar foo");
        buffer.undo();
        assert_eq!(buffer.text().to_string(), "foo bar foo");

        // Not a match boundary: untouched.
        assert!(
            !replace_one_query(&mut buffer, &SearchQuery::literal("foo"), 1..4, "baz").unwrap()
        );
        assert_eq!(buffer.text().to_string(), "foo bar foo");

        // Empty / out-of-bounds ranges: untouched.
        assert!(
            !replace_one_query(&mut buffer, &SearchQuery::literal("foo"), 0..0, "baz").unwrap()
        );
        assert!(
            !replace_one_query(&mut buffer, &SearchQuery::literal("foo"), 0..99, "baz").unwrap()
        );
    }

    #[test]
    fn replace_one_query_expands_regex_captures() {
        let mut buffer = EditorBuffer::new("ab cd");
        let query = SearchQuery::new(r"(\w)(\w)", true, false, true);
        assert!(replace_one_query(&mut buffer, &query, 0..2, "$2$1").unwrap());
        assert_eq!(buffer.text().to_string(), "ba cd");
    }

    #[test]
    fn collect_replacements_supports_line_scoped_offsets() {
        let mut buffer = EditorBuffer::new("foo one\nfoo two\n");
        let query = SearchQuery::literal("foo");
        // Simulate `:s` on row 1: collect on the line slice, offset by line start.
        let line_start = buffer.point_to_offset(crate::Point::new(1, 0));
        let line = buffer.line_to_string(1);
        let local = collect_replacements(&line, &query, "bar").unwrap();
        assert_eq!(local.len(), 1);
        let scoped: Vec<(Range<usize>, String)> = local
            .into_iter()
            .map(|(r, s)| (r.start + line_start..r.end + line_start, s))
            .collect();
        assert_eq!(buffer.replace_many(scoped), 1);
        assert_eq!(buffer.text().to_string(), "foo one\nbar two\n");
        buffer.undo();
        assert_eq!(buffer.text().to_string(), "foo one\nfoo two\n");
    }
}
