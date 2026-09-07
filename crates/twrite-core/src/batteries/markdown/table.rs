use std::ops::Range;

use crate::{EditorBuffer, display_width};

/// Custom highlight tag names emitted for GFM tables.
///
/// Battery-only extension point: the core engine never interprets these.
/// Apps map them to colors via `SyntaxTheme::set_custom_tag_color`; unmapped
/// names fall back to the editor foreground.
pub const TABLE_HEADER_TAG: &str = "markdown.table.header";
/// Custom tag for GFM table body cell content.
pub const TABLE_CELL_TAG: &str = "markdown.table.cell";
/// Custom tag for GFM table delimiter rows (`| --- | :-: |`).
pub const TABLE_DELIMITER_TAG: &str = "markdown.table.delimiter";

/// Column alignment parsed from a GFM delimiter cell (`---`, `:--`, `--:`, `:-:`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableAlignment {
    /// `:--` / `:---`.
    Left,
    /// `:-:` / `:--:`.
    Center,
    /// `--:` / `---:`.
    Right,
    /// `---`.
    None,
}

/// Which row of a [`TableBlock`] a buffer row is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableRowKind {
    /// First row of the block (column names).
    Header,
    /// Second row (`| --- | --- |`).
    Delimiter,
    /// Any data row below the delimiter.
    Body,
}

/// A contiguous GFM pipe-table block: header + delimiter + 0..N body rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableBlock {
    /// Buffer row of the header line.
    pub header_row: usize,
    /// Buffer row of the delimiter line.
    pub delimiter_row: usize,
    /// Last body row (inclusive); equals `delimiter_row` when bodiless.
    pub end_row: usize,
    /// Number of columns (header and delimiter agree; body rows may vary).
    pub col_count: usize,
    /// Per-column alignment from the delimiter row.
    pub aligns: Vec<TableAlignment>,
}

impl TableBlock {
    /// Returns which part of the block `row` is, or `None` when outside it.
    pub fn kind_at(&self, row: usize) -> Option<TableRowKind> {
        if row == self.header_row {
            Some(TableRowKind::Header)
        } else if row == self.delimiter_row {
            Some(TableRowKind::Delimiter)
        } else if row > self.delimiter_row && row <= self.end_row {
            Some(TableRowKind::Body)
        } else {
            None
        }
    }

    /// Returns whether `row` lies anywhere inside the block.
    pub fn contains(&self, row: usize) -> bool {
        self.kind_at(row).is_some()
    }
}

/// Strips a single trailing `\n` / `\r\n` for table scanning.
pub(crate) fn clean_table_line(raw: &str) -> &str {
    raw.strip_suffix("\r\n")
        .or_else(|| raw.strip_suffix('\n'))
        .unwrap_or(raw)
}

fn is_fence_line(cleaned: &str) -> bool {
    let t = cleaned.trim_start();
    t.starts_with("```") || t.starts_with("~~~")
}

/// Collects every fence-marker row in a single linear pass.
///
/// Hot paths (per-frame highlight, table layout) must call this once per
/// document version and share the result — never scan per row.
pub fn fence_rows(buffer: &EditorBuffer) -> Vec<usize> {
    let total = buffer.len_lines();
    let mut fences = Vec::new();
    for r in 0..total {
        if is_fence_line(clean_table_line(&buffer.line_to_string(r))) {
            fences.push(r);
        }
    }
    fences
}

/// Returns whether `row` sits inside a fenced code block.
///
/// `fences` is the sorted output of [`fence_rows`]; the query is
/// `O(log F)` via `partition_point`. Counts only markers strictly above
/// `row`, matching the previous per-call scan semantics.
pub fn is_fenced_row(fences: &[usize], row: usize) -> bool {
    fences.partition_point(|&f_row| f_row < row) % 2 == 1
}

/// Byte ranges of inline code spans (backtick runs) in `line`.
///
/// Pipes inside these ranges are literal text, not cell separators.
/// Unmatched backticks are treated as literal characters.
fn code_span_ranges(line: &str) -> Vec<Range<usize>> {
    let bytes = line.as_bytes();
    let mut ranges = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        // Skip `\x` escapes entirely so an escaped backtick can't open a span.
        if bytes[i] == b'\\' {
            i += if i + 1 < bytes.len() { 2 } else { 1 };
            continue;
        }
        if bytes[i] != b'`' {
            i += 1;
            continue;
        }
        let mut run = 0;
        while i + run < bytes.len() && bytes[i + run] == b'`' {
            run += 1;
        }
        // Find a closing run of exactly `run` backticks.
        let mut j = i + run;
        let mut closed = None;
        while j < bytes.len() {
            if bytes[j] == b'\\' {
                j += if j + 1 < bytes.len() { 2 } else { 1 };
                continue;
            }
            if bytes[j] == b'`' {
                let mut k = 0;
                while j + k < bytes.len() && bytes[j + k] == b'`' {
                    k += 1;
                }
                if k == run {
                    closed = Some(j + k);
                    break;
                }
                j += k;
                continue;
            }
            j += 1;
        }
        if let Some(end) = closed {
            ranges.push(i..end);
            i = end;
        } else {
            i += run;
        }
    }
    ranges
}

fn is_escaped_at(line: &str, pos: usize) -> bool {
    let bytes = line.as_bytes();
    let mut slashes = 0;
    let mut i = pos;
    while i > 0 && bytes[i - 1] == b'\\' {
        slashes += 1;
        i -= 1;
    }
    slashes % 2 == 1
}

/// Byte offsets of cell-separator pipes in `line`.
///
/// Ignores `\|` escapes and pipes inside inline code spans.
/// All returned offsets are ASCII `|` bytes, so byte/char indices coincide.
pub fn find_unescaped_pipes(line: &str) -> Vec<usize> {
    let cleaned = clean_table_line(line);
    let code = code_span_ranges(cleaned);
    let mut pipes = Vec::new();
    for (idx, ch) in cleaned.char_indices() {
        if ch != '|' {
            continue;
        }
        if is_escaped_at(cleaned, idx) {
            continue;
        }
        if code.iter().any(|r| r.contains(&idx)) {
            continue;
        }
        pipes.push(idx);
    }
    pipes
}

/// Splits `line` into cell-content byte ranges plus separator pipe offsets.
///
/// Outer pipes are optional per GFM: a leading `|` drops the empty segment
/// before it and a trailing `|` drops the one after it. With no pipes at all
/// the whole line is a single cell.
pub fn split_table_cells(line: &str) -> (Vec<usize>, Vec<Range<usize>>) {
    let cleaned = clean_table_line(line);
    let pipes = find_unescaped_pipes(cleaned);
    if pipes.is_empty() {
        let whole = 0..cleaned.len();
        return (Vec::new(), vec![whole]);
    }
    let mut bounds = Vec::with_capacity(pipes.len() + 2);
    bounds.push(0);
    bounds.extend(pipes.iter().copied());
    bounds.push(cleaned.len());
    let mut cells: Vec<Range<usize>> = Vec::with_capacity(bounds.len() - 1);
    for i in 0..bounds.len() - 1 {
        // Segment `bounds[i]..bounds[i+1]` sits between two pipes (or a line
        // edge and a pipe); skip the opening pipe byte for non-first segments.
        let s = if i > 0 { bounds[i] + 1 } else { bounds[i] };
        let e = bounds[i + 1];
        cells.push(s..e.min(cleaned.len()));
    }
    // Drop the empty outer segments produced by leading/trailing pipes.
    if cleaned.trim_start().starts_with('|') && !cells.is_empty() {
        cells.remove(0);
    }
    if cleaned.trim_end().ends_with('|') && !cells.is_empty() {
        cells.pop();
    }
    (pipes, cells)
}

/// Parses a GFM delimiter row into per-column alignments.
///
/// Returns `None` when any cell is not `:?-+:?` (after trimming whitespace).
pub fn parse_delimiter_row(line: &str) -> Option<Vec<TableAlignment>> {
    let cleaned = clean_table_line(line);
    if cleaned.trim().is_empty() {
        return None;
    }
    let (_, cells) = split_table_cells(cleaned);
    if cells.is_empty() {
        return None;
    }
    // A delimiter without pipes is only a table delimiter when the caller
    // pairs it with a piped header (checked in `table_block_at`); cell-level
    // validation is identical either way.
    let mut aligns = Vec::with_capacity(cells.len());
    for cell in &cells {
        let content = cleaned
            .get(cell.clone())
            .unwrap_or("")
            .trim()
            .trim_matches(['\r', '\n']);
        if content.is_empty() || !content.contains('-') {
            return None;
        }
        let inner = content.trim_matches(':');
        if inner.is_empty() || !inner.chars().all(|c| c == '-') {
            return None;
        }
        // Colons are only legal as a single leading and/or trailing marker.
        let stripped_leading = content.strip_prefix(':').unwrap_or(content);
        let stripped_both = stripped_leading
            .strip_suffix(':')
            .unwrap_or(stripped_leading);
        if stripped_both.contains(':') {
            return None;
        }
        let left = content.starts_with(':');
        let right = content.ends_with(':');
        aligns.push(match (left, right) {
            (true, true) => TableAlignment::Center,
            (true, false) => TableAlignment::Left,
            (false, true) => TableAlignment::Right,
            (false, false) => TableAlignment::None,
        });
    }
    Some(aligns)
}

fn table_line_has_pipe(cleaned: &str) -> bool {
    !find_unescaped_pipes(cleaned).is_empty()
}

/// Locates the GFM pipe-table block containing `row`, if any.
///
/// Pure in buffer text: looks for a `header` / `delimiter` pair adjacent to
/// `row` and extends through contiguous piped body rows. Returns `None` for
/// blank lines, fence lines, blockquotes, single-column pipe-less text (which
/// is a setext heading, not a table), and bare `---` (thematic break).
///
/// Single-shot helper: computes the fence index in one `O(N)` pass. Hot
/// per-frame paths must hoist that pass per document version and call
/// [`table_block_at_with_fences`] instead.
pub fn table_block_at(buffer: &EditorBuffer, row: usize) -> Option<TableBlock> {
    let fences = fence_rows(buffer);
    table_block_at_with_fences(buffer, row, &fences)
}

/// [`table_block_at`] with a caller-provided fence index.
///
/// `fences` is the sorted output of [`fence_rows`]; the fenced-region check
/// is `O(log F)`. Remaining work per call is bounded (upward walk budget
/// 512, body extension), so this is safe per visible row per frame as long
/// as `fences` is computed once per document version.
pub fn table_block_at_with_fences(
    buffer: &EditorBuffer,
    row: usize,
    fences: &[usize],
) -> Option<TableBlock> {
    let total = buffer.len_lines();
    if row >= total {
        return None;
    }
    let cur = clean_table_line(&buffer.line_to_string(row)).to_string();
    if cur.trim().is_empty() || is_fence_line(&cur) || cur.trim_start().starts_with('>') {
        return None;
    }
    if is_fenced_row(fences, row) {
        return None;
    }

    // Find the delimiter row: it is either `row` itself, the line below a
    // header row, or somewhere above a body row.
    let mut delim: Option<usize> = None;
    if parse_delimiter_row(&cur).is_some() {
        delim = Some(row);
    } else if row + 1 < total {
        let next = clean_table_line(&buffer.line_to_string(row + 1)).to_string();
        if !is_fence_line(&next) && parse_delimiter_row(&next).is_some() {
            delim = Some(row + 1);
        }
    }
    if delim.is_none() {
        // Walk upward through piped body candidates to find the delimiter.
        // Bounded so pathological documents can't turn highlight into O(N^2).
        let mut r = row.checked_sub(1);
        let mut budget = 512;
        while let Some(j) = r {
            if budget == 0 {
                break;
            }
            budget -= 1;
            let text = clean_table_line(&buffer.line_to_string(j)).to_string();
            if text.trim().is_empty() || is_fence_line(&text) {
                break;
            }
            if parse_delimiter_row(&text).is_some() {
                delim = Some(j);
                break;
            }
            if !table_line_has_pipe(&text) {
                break;
            }
            r = j.checked_sub(1);
        }
    }
    let d = delim?;
    if d == 0 {
        return None; // delimiter needs a header line above it.
    }
    let header = clean_table_line(&buffer.line_to_string(d - 1)).to_string();
    if header.trim().is_empty() || is_fence_line(&header) {
        return None;
    }
    let header_pipes = table_line_has_pipe(&header);
    let delim_text = clean_table_line(&buffer.line_to_string(d)).to_string();
    let delim_pipes = table_line_has_pipe(&delim_text);
    // Without a pipe in either line this is a setext heading (`foo\n---`)
    // or a thematic break, never a table.
    if !header_pipes && !delim_pipes {
        return None;
    }
    let aligns = parse_delimiter_row(&delim_text)?;
    let (_, header_cells) = split_table_cells(&header);
    if header_cells.len() != aligns.len() {
        return None;
    }
    let header_row = d - 1;
    // Extend through contiguous piped, non-fence body rows.
    let mut end = d;
    let mut r = d + 1;
    while r < total {
        let text = clean_table_line(&buffer.line_to_string(r)).to_string();
        if text.trim().is_empty() || is_fence_line(&text) || !table_line_has_pipe(&text) {
            break;
        }
        end = r;
        r += 1;
        if r - d > 4096 {
            break;
        }
    }
    // `row` must lie inside header..=end (it can be above the block when the
    // upward walk overshoots, e.g. delimiter search from a paragraph below).
    if row < header_row || row > end {
        return None;
    }
    Some(TableBlock {
        header_row,
        delimiter_row: d,
        end_row: end,
        col_count: aligns.len(),
        aligns,
    })
}

/// Display-column widths for one [`TableBlock`], measured on unconcealed
/// source cell content so widths stay stable as the cursor moves (inactive
/// rows conceal markers; per-row padding absorbs the difference and pipes
/// still align everywhere).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableLayout {
    /// The block these widths were measured for.
    pub block: TableBlock,
    /// Max trimmed content width per column (minimum 3, keeping the
    /// delimiter shape valid).
    pub col_widths: Vec<usize>,
}

impl TableLayout {
    /// Measures column widths for `block` from header, delimiter, and body rows.
    ///
    /// Delimiter cells count so wide markers (`:---:`) still fit; the
    /// delimiter has no concealment, so its width is always stable.
    pub fn build(buffer: &EditorBuffer, block: &TableBlock) -> Self {
        let mut col_widths = vec![3; block.col_count];
        for row in block.header_row..=block.end_row {
            let line = clean_table_line(&buffer.line_to_string(row)).to_string();
            let (_, cells) = split_table_cells(&line);
            for (i, cell) in cells.iter().enumerate().take(block.col_count) {
                let content = line.get(cell.clone()).unwrap_or("").trim();
                col_widths[i] = col_widths[i].max(display_width(content));
            }
        }
        Self {
            block: block.clone(),
            col_widths,
        }
    }
}

/// Finds every table block in the buffer with its measured [`TableLayout`].
///
/// Pure in buffer text; callers cache the result per document version.
///
/// Single-shot helper: computes the fence index in one `O(N)` pass. Hot
/// paths must hoist that pass and call [`table_layouts_with_fences`].
pub fn table_layouts(buffer: &EditorBuffer) -> Vec<TableLayout> {
    let fences = fence_rows(buffer);
    table_layouts_with_fences(buffer, &fences)
}

/// [`table_layouts`] with a caller-provided fence index.
///
/// Sweeps rows once, reusing `fences` for every point query instead of
/// rescanning `0..row` per row (which made the naive version quadratic).
pub fn table_layouts_with_fences(buffer: &EditorBuffer, fences: &[usize]) -> Vec<TableLayout> {
    let mut layouts = Vec::new();
    let mut row = 0;
    let total = buffer.len_lines();
    while row < total {
        if let Some(block) = table_block_at_with_fences(buffer, row, fences) {
            let end = block.end_row;
            layouts.push(TableLayout::build(buffer, &block));
            row = end + 1;
        } else {
            row += 1;
        }
    }
    layouts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_table_pipes_ignore_escapes_and_code() {
        assert_eq!(find_unescaped_pipes("| a | b |"), vec![0, 4, 8]);
        // Escaped pipe is literal text.
        assert_eq!(find_unescaped_pipes("| a \\| b |"), vec![0, 9]);
        // Pipes inside code spans are literal text.
        assert_eq!(find_unescaped_pipes("| `a|b` | c |"), vec![0, 8, 12]);
        // Double backslash means the pipe still separates.
        assert_eq!(find_unescaped_pipes("| a \\\\| b |"), vec![0, 6, 10]);
    }

    #[test]
    fn test_table_split_cells_outer_pipes_optional() {
        let (pipes, cells) = split_table_cells("| a | b |");
        assert_eq!(pipes, vec![0, 4, 8]);
        assert_eq!(cells.len(), 2);

        let (pipes_bare, cells_bare) = split_table_cells("a | b");
        assert_eq!(pipes_bare, vec![2]);
        assert_eq!(cells_bare.len(), 2);

        let (pipes_none, cells_none) = split_table_cells("plain");
        assert!(pipes_none.is_empty());
        assert_eq!(cells_none, vec![0..5]);
    }

    #[test]
    fn test_table_parse_delimiter_alignments() {
        assert_eq!(
            parse_delimiter_row("| --- | :--- | ---: | :---: |"),
            Some(vec![
                TableAlignment::None,
                TableAlignment::Left,
                TableAlignment::Right,
                TableAlignment::Center
            ])
        );
        assert_eq!(
            parse_delimiter_row("--- | :-:"),
            Some(vec![TableAlignment::None, TableAlignment::Center])
        );
        assert!(parse_delimiter_row("| --- | nope |").is_none());
        assert!(parse_delimiter_row("| --- | :: |").is_none());
        assert!(parse_delimiter_row("").is_none());
    }

    #[test]
    fn test_table_block_detection_and_kinds() {
        let buffer = EditorBuffer::new("| a | b |\n| --- | --- |\n| c | d |\n| e | f |");
        let block = table_block_at(&buffer, 0).expect("header must detect block");
        assert_eq!(block.header_row, 0);
        assert_eq!(block.delimiter_row, 1);
        assert_eq!(block.end_row, 3);
        assert_eq!(block.col_count, 2);
        assert_eq!(block.kind_at(0), Some(TableRowKind::Header));
        assert_eq!(block.kind_at(1), Some(TableRowKind::Delimiter));
        assert_eq!(block.kind_at(2), Some(TableRowKind::Body));
        assert!(block.contains(3));
        assert!(!block.contains(4));

        // Outer pipes optional.
        let bare = EditorBuffer::new("a | b\n--- | ---\n c | d ");
        let bare_block = table_block_at(&bare, 2).expect("bare pipes must detect");
        assert_eq!(bare_block.col_count, 2);

        // Plain paragraph below the table is not part of it.
        let trailed = EditorBuffer::new("| a |\n| --- |\n| b |\nplain");
        assert!(table_block_at(&trailed, 3).is_none());
    }

    #[test]
    fn test_table_rejects_setext_hr_fence_and_quote() {
        // Setext heading, not a table: no pipes anywhere.
        let setext = EditorBuffer::new("foo\n---\n");
        assert!(table_block_at(&setext, 0).is_none());
        assert!(table_block_at(&setext, 1).is_none());

        // Bare thematic break.
        let hr = EditorBuffer::new("---\n");
        assert!(table_block_at(&hr, 0).is_none());

        // Mismatched column counts.
        let uneven = EditorBuffer::new("| a | b |\n| --- |\n| c | d |");
        assert!(table_block_at(&uneven, 0).is_none());

        // Fenced code is never a table.
        let fence = EditorBuffer::new("```\n| a |\n| --- |\n```");
        assert!(table_block_at(&fence, 1).is_none());
        assert!(table_block_at(&fence, 2).is_none());

        // Blockquote tables are out of scope for v1.
        let quote = EditorBuffer::new("> | a |\n> | --- |");
        assert!(table_block_at(&quote, 0).is_none());
    }

    #[test]
    fn test_table_layout_measures_max_source_width() {
        let buffer = EditorBuffer::new("| a | b |\n| --- | --- |\n| looong | c |\n| d | e |");
        let layouts = table_layouts(&buffer);
        assert_eq!(layouts.len(), 1);
        // Long word in one row widens the whole column (minimum 3).
        assert_eq!(layouts[0].col_widths, vec![6, 3]);

        // Multiple blocks each get their own layout.
        let two = EditorBuffer::new("| a |\n| --- |\n| b |\n\n| x | yy |\n| --- | --- |");
        assert_eq!(table_layouts(&two).len(), 2);
    }

    #[test]
    fn test_fenced_index_queries_agree_with_single_shot() {
        // Fence pair at the top, a real table deep in the doc, and a
        // pipe-table lookalike inside a second fence (must stay invisible).
        let mut s = String::from("```rust\nfn f() {}\n```\n");
        for i in 0..200 {
            s.push_str(&format!("plain line {i}\n"));
        }
        s.push_str("| a | b |\n| --- | --- |\n| c | d |\n");
        s.push_str("```\n| x |\n| --- |\n```\n");
        let buffer = EditorBuffer::new(&s);
        let fences = fence_rows(&buffer);
        // Fence rows: 0, 2, and the second block's open/close.
        assert!(fences.contains(&0) && fences.contains(&2));
        assert_eq!(fences.len(), 4);

        let total = buffer.len_lines();
        for row in 0..total {
            assert_eq!(
                table_block_at_with_fences(&buffer, row, &fences),
                table_block_at(&buffer, row),
                "indexed query must agree with single-shot at row {row}"
            );
        }
        assert_eq!(
            table_layouts_with_fences(&buffer, &fences),
            table_layouts(&buffer),
            "indexed layouts must agree with single-shot layouts"
        );

        // The real table is detected; the fenced lookalike is not.
        let table_row = 203;
        assert!(table_block_at(&buffer, table_row).is_some());
        assert!(!is_fenced_row(&fences, table_row));
        let fenced_row = total - 3;
        assert!(is_fenced_row(&fences, fenced_row));
        assert!(table_block_at(&buffer, fenced_row).is_none());
    }
}
