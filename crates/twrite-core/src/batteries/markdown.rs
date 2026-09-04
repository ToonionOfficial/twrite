use std::ops::Range;
use std::sync::{Arc, RwLock};

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

use crate::{
    EditorBuffer, EditorHook, HighlightTag, HookContext, HookOutcome, KeyEvent, Point, Selection,
    StyleSpan, SyntaxHighlighter, TextStyle,
};

/// Cached fence line row indices and associated document version.
type FenceCache = Arc<RwLock<Option<(usize, Vec<usize>)>>>;

/// Display mode for markdown syntax delimiters (like `# `, `**`, `*`, `~~`, `` ` ``) on inactive lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConcealMode {
    /// Markdown markers are always visible with normal syntax coloring.
    Off,
    /// Markdown markers on inactive lines are rendered with faint opacity.
    #[default]
    Dimmed,
    /// Markdown markers on inactive lines are completely hidden (invisible).
    Hidden,
}

/// Configuration settings for Markdown editing, highlighting, and WYSIWYG rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarkdownConfig {
    /// How syntax delimiters (`#`, `**`, `*`, `~~`, `` ` ``) are displayed on inactive lines.
    pub conceal_mode: ConcealMode,
    /// Whether horizontal rules (`---`, `***`, `___`) are rendered as visual divider quads.
    pub visual_thematic_breaks: bool,
    /// Whether clicking on task checkboxes (`- [ ]` / `- [x]`) toggles their state.
    pub interactive_tasks: bool,
    /// Whether GFM table pipes, header rows, and delimiter rows get structural styling.
    pub visual_tables: bool,
    /// Whether `Tab` / `Shift+Tab` move between table cells and `Enter` continues table rows.
    pub table_navigation: bool,
}

impl Default for MarkdownConfig {
    fn default() -> Self {
        Self {
            conceal_mode: ConcealMode::Dimmed,
            visual_thematic_breaks: true,
            interactive_tasks: true,
            visual_tables: true,
            table_navigation: true,
        }
    }
}

/// A syntax highlighter for CommonMark and GFM Markdown documents using `pulldown-cmark`.
#[derive(Debug, Clone)]
pub struct MarkdownHighlighter {
    config: MarkdownConfig,
    cached_fences: FenceCache,
}

impl Default for MarkdownHighlighter {
    fn default() -> Self {
        Self::new()
    }
}

impl MarkdownHighlighter {
    /// Creates a new Markdown syntax highlighter with default configuration.
    pub fn new() -> Self {
        Self::with_config(MarkdownConfig::default())
    }

    /// Creates a new Markdown syntax highlighter with custom configuration.
    pub fn with_config(config: MarkdownConfig) -> Self {
        Self {
            config,
            cached_fences: Arc::new(RwLock::new(None)),
        }
    }

    /// Returns the active Markdown configuration.
    pub fn config(&self) -> &MarkdownConfig {
        &self.config
    }

    /// Updates the Markdown configuration.
    pub fn set_config(&mut self, config: MarkdownConfig) {
        self.config = config;
    }

    fn is_in_fenced_code_block(&self, buffer: &EditorBuffer, current_row: usize) -> bool {
        let version = buffer.version();
        if let Ok(guard) = self.cached_fences.read()
            && let Some((v, ref fences)) = *guard
            && v == version
        {
            let count = fences.partition_point(|&f_row| f_row < current_row);
            return count % 2 == 1;
        }

        if let Ok(mut guard) = self.cached_fences.write() {
            if let Some((v, ref fences)) = *guard
                && v == version
            {
                let count = fences.partition_point(|&f_row| f_row < current_row);
                return count % 2 == 1;
            }

            let mut fences = Vec::new();
            let total_lines = buffer.len_lines();
            let rope = buffer.text();
            for r in 0..total_lines {
                let line = rope.line(r);
                let mut chars = line.chars();
                while let Some(c) = chars.next() {
                    if !c.is_whitespace() {
                        if (c == '`' && chars.next() == Some('`') && chars.next() == Some('`'))
                            || (c == '~' && chars.next() == Some('~') && chars.next() == Some('~'))
                        {
                            fences.push(r);
                        }
                        break;
                    }
                }
            }

            let count = fences.partition_point(|&f_row| f_row < current_row);
            let in_block = count % 2 == 1;
            *guard = Some((version, fences));
            in_block
        } else {
            false
        }
    }
}

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
fn clean_table_line(raw: &str) -> &str {
    raw.strip_suffix("\r\n")
        .or_else(|| raw.strip_suffix('\n'))
        .unwrap_or(raw)
}

fn is_fence_line(cleaned: &str) -> bool {
    let t = cleaned.trim_start();
    t.starts_with("```") || t.starts_with("~~~")
}

/// Counts fence toggles above `row`: odd means `row` is inside a fenced block.
///
/// `table_block_at` is a fence-unaware free function, so it needs its own
/// check (the highlighter additionally guards via its cached fence scan).
fn row_in_fenced_block(buffer: &EditorBuffer, row: usize) -> bool {
    let mut fences = 0;
    for r in 0..row.min(buffer.len_lines()) {
        if is_fence_line(clean_table_line(&buffer.line_to_string(r))) {
            fences += 1;
        }
    }
    fences % 2 == 1
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
pub fn table_block_at(buffer: &EditorBuffer, row: usize) -> Option<TableBlock> {
    let total = buffer.len_lines();
    if row >= total {
        return None;
    }
    let cur = clean_table_line(&buffer.line_to_string(row)).to_string();
    if cur.trim().is_empty() || is_fence_line(&cur) || cur.trim_start().starts_with('>') {
        return None;
    }
    if row_in_fenced_block(buffer, row) {
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

impl SyntaxHighlighter for MarkdownHighlighter {
    fn highlight_line(&self, buffer: &EditorBuffer, row: usize, line_text: &str) -> Vec<StyleSpan> {
        let mut spans = Vec::new();
        let trimmed_start = line_text.trim_start();

        let delimiter_tag = match self.config.conceal_mode {
            ConcealMode::Off => None,
            ConcealMode::Dimmed => Some(HighlightTag::Dimmed),
            ConcealMode::Hidden => Some(HighlightTag::Hidden),
        };

        let is_cursor_row = row == buffer.cursor_point().row;

        if trimmed_start.starts_with("```") || trimmed_start.starts_with("~~~") {
            spans.push(StyleSpan::tag(0..line_text.len(), HighlightTag::Code));
            return spans;
        }

        if self.is_in_fenced_code_block(buffer, row) {
            spans.push(StyleSpan::tag(0..line_text.len(), HighlightTag::Code));
            return spans;
        }

        // GFM pipe tables. Runs before the thematic-break check so a
        // single-column `---` delimiter is not mistaken for an `<hr>`.
        // Pipes stay visible in every conceal mode (Hidden maps to Dimmed)
        // to preserve `ConcealedLine` source/display column alignment.
        if self.config.visual_tables
            && let Some(block) = table_block_at(buffer, row)
            && let Some(kind) = block.kind_at(row)
        {
            // Pipes dim on inactive rows but are never concealed.
            let pipe_dim = match self.config.conceal_mode {
                ConcealMode::Off => None,
                ConcealMode::Dimmed | ConcealMode::Hidden => Some(HighlightTag::Dimmed),
            };
            let pipes = find_unescaped_pipes(line_text);
            match kind {
                TableRowKind::Delimiter => {
                    spans.push(StyleSpan::tag(
                        0..line_text.len(),
                        HighlightTag::Custom(TABLE_DELIMITER_TAG),
                    ));
                    for p in &pipes {
                        spans.push(StyleSpan::tag(*p..*p + 1, HighlightTag::Punctuation));
                    }
                    if !is_cursor_row {
                        let tag = delimiter_tag.unwrap_or(HighlightTag::Comment);
                        // Map Hidden -> Dimmed: concealing dashes would collapse
                        // the row to nothing and break cursor mapping.
                        let tag = if tag == HighlightTag::Hidden {
                            HighlightTag::Dimmed
                        } else {
                            tag
                        };
                        spans.push(StyleSpan::tag(0..line_text.len(), tag));
                    }
                    return spans;
                }
                TableRowKind::Header | TableRowKind::Body => {
                    let cell_tag = if kind == TableRowKind::Header {
                        HighlightTag::Custom(TABLE_HEADER_TAG)
                    } else {
                        HighlightTag::Custom(TABLE_CELL_TAG)
                    };
                    let (_, cells) = split_table_cells(line_text);
                    for cell in &cells {
                        let end = cell.end.min(line_text.len());
                        if cell.start < end {
                            spans.push(StyleSpan::tag(cell.start..end, cell_tag));
                            if kind == TableRowKind::Header
                                && let Some(content) = line_text.get(cell.start..end)
                                && !content.trim().is_empty()
                            {
                                spans.push(StyleSpan::tag(cell.start..end, HighlightTag::Bold));
                            }
                        }
                    }
                    for p in &pipes {
                        spans.push(StyleSpan::tag(*p..*p + 1, HighlightTag::Punctuation));
                        if !is_cursor_row && let Some(dim) = pipe_dim {
                            spans.push(StyleSpan::tag(*p..*p + 1, dim));
                        }
                    }
                    // Fall through to the inline pulldown pass so emphasis,
                    // code spans, and links inside cells keep working.
                }
            }
        }

        if self.config.visual_thematic_breaks {
            let trimmed_break = trimmed_start.trim_end();
            if (trimmed_break == "---" || trimmed_break == "***" || trimmed_break == "___")
                && line_text.len() >= 3
            {
                // Structural tag first so tag-driven layout survives concealment;
                // visual span last so text colors are unchanged.
                spans.push(StyleSpan::tag(
                    0..line_text.len(),
                    HighlightTag::HorizontalRule,
                ));
                let tag = delimiter_tag.unwrap_or(HighlightTag::Comment);
                spans.push(StyleSpan::tag(0..line_text.len(), tag));
                return spans;
            }
        }

        let heading_prefix = if trimmed_start.starts_with("# ") {
            Some((2, HighlightTag::Heading(1)))
        } else if trimmed_start.starts_with("## ") {
            Some((3, HighlightTag::Heading(2)))
        } else if trimmed_start.starts_with("### ") {
            Some((4, HighlightTag::Heading(3)))
        } else if trimmed_start.starts_with("#### ") {
            Some((5, HighlightTag::Heading(4)))
        } else if trimmed_start.starts_with("##### ") {
            Some((6, HighlightTag::Heading(5)))
        } else if trimmed_start.starts_with("###### ") {
            Some((7, HighlightTag::Heading(6)))
        } else {
            None
        };

        if let Some((prefix_len, tag)) = heading_prefix {
            let indent = line_text.len() - trimmed_start.len();
            if !is_cursor_row && let Some(delim_tag) = delimiter_tag {
                spans.push(StyleSpan::tag(indent..indent + prefix_len, delim_tag));
                if indent + prefix_len < line_text.len() {
                    spans.push(StyleSpan::tag(indent + prefix_len..line_text.len(), tag));
                }
            } else {
                spans.push(StyleSpan::tag(0..line_text.len(), tag));
            }
            return spans;
        }

        if trimmed_start.starts_with("> ") || trimmed_start == ">" {
            let indent = line_text.len() - trimmed_start.len();
            let quote_len = if trimmed_start.starts_with("> ") {
                2
            } else {
                1
            };
            // Structural tag first; visual span last so colors are unchanged.
            spans.push(StyleSpan::tag(
                indent..indent + quote_len,
                HighlightTag::Blockquote,
            ));
            if !is_cursor_row && let Some(delim_tag) = delimiter_tag {
                let delim_len = if trimmed_start.starts_with("> ") {
                    2
                } else {
                    1
                };
                spans.push(StyleSpan::tag(indent..indent + delim_len, delim_tag));
            } else {
                spans.push(StyleSpan::tag(indent..indent + 1, HighlightTag::Comment));
            }
        }

        let is_task_unchecked = trimmed_start.starts_with("- [ ] ")
            || trimmed_start == "- [ ]"
            || trimmed_start.starts_with("* [ ] ")
            || trimmed_start == "* [ ]";
        let is_task_checked = trimmed_start.starts_with("- [x] ")
            || trimmed_start == "- [x]"
            || trimmed_start.starts_with("- [X] ")
            || trimmed_start == "- [X]"
            || trimmed_start.starts_with("* [x] ")
            || trimmed_start == "* [x]"
            || trimmed_start.starts_with("* [X] ")
            || trimmed_start == "* [X]";
        let is_task_list = is_task_unchecked || is_task_checked;

        if is_task_list {
            let indent = line_text.len() - trimmed_start.len();
            // Structural tag first so tag-driven layout works even when the
            // marker bytes are concealed; visual span last so colors stay.
            let marker_len = if trimmed_start.len() >= 6 {
                6
            } else {
                trimmed_start.len()
            };
            let task_tag = if is_task_checked {
                HighlightTag::TaskChecked
            } else {
                HighlightTag::TaskUnchecked
            };
            spans.push(StyleSpan::tag(indent..indent + marker_len, task_tag));
            if !is_cursor_row && let Some(delim_tag) = delimiter_tag {
                if delim_tag == HighlightTag::Hidden {
                    spans.push(StyleSpan::tag(
                        indent..indent + marker_len,
                        HighlightTag::Hidden,
                    ));
                } else {
                    spans.push(StyleSpan::tag(indent..indent + 2, delim_tag));
                }
            }
        }

        let mut options = Options::empty();
        options.insert(Options::ENABLE_STRIKETHROUGH);
        options.insert(Options::ENABLE_TASKLISTS);
        options.insert(Options::ENABLE_TABLES);

        let parser = Parser::new_ext(line_text, options).into_offset_iter();

        let mut active_strong = None;
        let mut active_emphasis = None;
        let mut active_strike = None;
        let mut active_link = None;

        for (event, range) in parser {
            match event {
                Event::Start(Tag::Strong) => {
                    active_strong = Some(range.start);
                }
                Event::End(TagEnd::Strong) => {
                    if let Some(start) = active_strong.take() {
                        let end = range.end.min(line_text.len());
                        if start < end {
                            if !is_cursor_row
                                && end >= start + 4
                                && let Some(delim_tag) = delimiter_tag
                            {
                                spans.push(StyleSpan::tag(start..start + 2, delim_tag));
                                spans.push(StyleSpan::tag(start + 2..end - 2, HighlightTag::Bold));
                                spans.push(StyleSpan::tag(end - 2..end, delim_tag));
                            } else {
                                spans.push(StyleSpan::tag(start..end, HighlightTag::Bold));
                            }
                        }
                    }
                }
                Event::Start(Tag::Emphasis) => {
                    active_emphasis = Some(range.start);
                }
                Event::End(TagEnd::Emphasis) => {
                    if let Some(start) = active_emphasis.take() {
                        let end = range.end.min(line_text.len());
                        if start < end {
                            if !is_cursor_row
                                && end >= start + 2
                                && let Some(delim_tag) = delimiter_tag
                            {
                                spans.push(StyleSpan::tag(start..start + 1, delim_tag));
                                spans
                                    .push(StyleSpan::tag(start + 1..end - 1, HighlightTag::Italic));
                                spans.push(StyleSpan::tag(end - 1..end, delim_tag));
                            } else {
                                spans.push(StyleSpan::tag(start..end, HighlightTag::Italic));
                            }
                        }
                    }
                }
                Event::Start(Tag::Strikethrough) => {
                    active_strike = Some(range.start);
                }
                Event::End(TagEnd::Strikethrough) => {
                    if let Some(start) = active_strike.take() {
                        let end = range.end.min(line_text.len());
                        if start < end {
                            if !is_cursor_row
                                && end >= start + 4
                                && let Some(delim_tag) = delimiter_tag
                            {
                                spans.push(StyleSpan::tag(start..start + 2, delim_tag));
                                spans.push(StyleSpan::direct(
                                    start + 2..end - 2,
                                    TextStyle {
                                        strikethrough: true,
                                        ..Default::default()
                                    },
                                ));
                                spans.push(StyleSpan::tag(end - 2..end, delim_tag));
                            } else {
                                spans.push(StyleSpan::direct(
                                    start..end,
                                    TextStyle {
                                        strikethrough: true,
                                        ..Default::default()
                                    },
                                ));
                            }
                        }
                    }
                }
                Event::Code(cow) => {
                    let end = (range.start + cow.len() + 2).min(line_text.len());
                    if !is_cursor_row
                        && end >= range.start + 2
                        && let Some(delim_tag) = delimiter_tag
                    {
                        spans.push(StyleSpan::tag(range.start..range.start + 1, delim_tag));
                        spans.push(StyleSpan::tag(range.start + 1..end - 1, HighlightTag::Code));
                        spans.push(StyleSpan::tag(end - 1..end, delim_tag));
                    } else {
                        spans.push(StyleSpan::tag(range.start..end, HighlightTag::Code));
                    }
                }
                Event::Start(Tag::Link { .. }) => {
                    active_link = Some(range.start);
                }
                Event::End(TagEnd::Link) => {
                    if let Some(start) = active_link.take() {
                        let end = range.end.min(line_text.len());
                        if start < end {
                            if !is_cursor_row && let Some(delim_tag) = delimiter_tag {
                                if let Some(bracket_idx) = line_text[start..end].find("](") {
                                    let label_start = start + 1;
                                    let label_end = start + bracket_idx;
                                    spans.push(StyleSpan::tag(start..label_start, delim_tag));
                                    spans.push(StyleSpan::tag(
                                        label_start..label_end,
                                        HighlightTag::Link,
                                    ));
                                    spans.push(StyleSpan::tag(label_end..end, delim_tag));
                                } else if let Some(bracket_idx) = line_text[start..end].find("][") {
                                    let label_start = start + 1;
                                    let label_end = start + bracket_idx;
                                    spans.push(StyleSpan::tag(start..label_start, delim_tag));
                                    spans.push(StyleSpan::tag(
                                        label_start..label_end,
                                        HighlightTag::Link,
                                    ));
                                    spans.push(StyleSpan::tag(label_end..end, delim_tag));
                                } else if line_text[start..end].starts_with('<')
                                    && line_text[start..end].ends_with('>')
                                {
                                    spans.push(StyleSpan::tag(start..start + 1, delim_tag));
                                    spans.push(StyleSpan::tag(
                                        start + 1..end - 1,
                                        HighlightTag::Link,
                                    ));
                                    spans.push(StyleSpan::tag(end - 1..end, delim_tag));
                                } else {
                                    spans.push(StyleSpan::tag(start..end, HighlightTag::Link));
                                }
                            } else {
                                spans.push(StyleSpan::tag(start..end, HighlightTag::Link));
                            }
                        }
                    }
                }
                Event::TaskListMarker(checked) => {
                    let tag = if checked {
                        HighlightTag::String
                    } else {
                        HighlightTag::Comment
                    };
                    spans.push(StyleSpan::tag(range, tag));
                }
                _ => {}
            }
        }

        spans
    }

    fn extract_links(
        &self,
        _buffer: &EditorBuffer,
        _row: usize,
        line_text: &str,
    ) -> Vec<(Range<usize>, String)> {
        if !(line_text.contains('[') || line_text.contains('<')) {
            return Vec::new();
        }
        extract_markdown_links(line_text)
    }
}

/// An editor hook providing Markdown shortcuts (Ctrl+B, Ctrl+I, Ctrl+K), smart list continuation, and task list toggles.
#[derive(Debug, Clone)]
pub struct MarkdownHook {
    interactive_tasks: bool,
    table_navigation: bool,
}

impl Default for MarkdownHook {
    fn default() -> Self {
        Self::new()
    }
}

impl MarkdownHook {
    /// Creates a new Markdown editing hook.
    pub fn new() -> Self {
        Self {
            interactive_tasks: true,
            table_navigation: true,
        }
    }

    /// Creates a hook honoring the given Markdown configuration.
    pub fn with_config(config: MarkdownConfig) -> Self {
        Self {
            interactive_tasks: config.interactive_tasks,
            table_navigation: config.table_navigation,
        }
    }

    /// Updates whether mouse clicks toggle task checkboxes.
    pub fn set_interactive_tasks(&mut self, interactive: bool) {
        self.interactive_tasks = interactive;
    }

    /// Returns whether mouse clicks toggle task checkboxes.
    pub fn interactive_tasks(&self) -> bool {
        self.interactive_tasks
    }

    /// Updates whether `Tab` / `Shift+Tab` move between table cells.
    pub fn set_table_navigation(&mut self, enabled: bool) {
        self.table_navigation = enabled;
    }

    /// Returns whether table cell navigation is enabled.
    pub fn table_navigation(&self) -> bool {
        self.table_navigation
    }

    fn toggle_marker_at_row(ctx: &mut HookContext, row: usize) -> bool {
        if row >= ctx.buffer.len_lines() {
            return false;
        }
        let line = ctx.buffer.line_to_string(row);
        let line_start = ctx.buffer.point_to_offset(Point::new(row, 0));
        let old_cursor = ctx.buffer.cursor_offset();

        // Unchecked -> checked (lowercase x, matching Ctrl+Enter behavior).
        for (empty, checked) in [("- [ ] ", "- [x] "), ("* [ ] ", "* [x] ")] {
            if let Some(idx) = line.find(empty) {
                let s = line_start + idx;
                ctx.buffer.replace_range(s..s + 6, checked);
                ctx.buffer.set_cursor_offset(old_cursor);
                return true;
            }
        }
        // Checked (x or X) -> unchecked.
        for (checked, empty) in [
            ("- [x] ", "- [ ] "),
            ("- [X] ", "- [ ] "),
            ("* [x] ", "* [ ] "),
            ("* [X] ", "* [ ] "),
        ] {
            if let Some(idx) = line.find(checked) {
                let s = line_start + idx;
                ctx.buffer.replace_range(s..s + 6, empty);
                ctx.buffer.set_cursor_offset(old_cursor);
                return true;
            }
        }
        false
    }

    fn toggle_checkbox(ctx: &mut HookContext) -> bool {
        let row = ctx.buffer.cursor_point().row;
        let line = ctx.buffer.line_to_string(row);
        let line_start = ctx.buffer.point_to_offset(Point::new(row, 0));

        if let Some(idx) = line.find("- [ ] ") {
            let target_start = line_start + idx;
            ctx.buffer
                .replace_range(target_start..target_start + 6, "- [x] ");
            return true;
        } else if let Some(idx) = line.find("- [x] ") {
            let target_start = line_start + idx;
            ctx.buffer
                .replace_range(target_start..target_start + 6, "- [ ] ");
            return true;
        } else if let Some(idx) = line.find("* [ ] ") {
            let target_start = line_start + idx;
            ctx.buffer
                .replace_range(target_start..target_start + 6, "* [x] ");
            return true;
        } else if let Some(idx) = line.find("* [x] ") {
            let target_start = line_start + idx;
            ctx.buffer
                .replace_range(target_start..target_start + 6, "* [ ] ");
            return true;
        }
        false
    }

    /// Cell-content starts for a stripped table line: byte offset just after
    /// each separator pipe (skipping one run of padding spaces), plus offset
    /// `0` when the line does not open with a pipe.
    fn table_cell_starts(stripped: &str) -> Vec<usize> {
        let bytes = stripped.as_bytes();
        let mut starts = Vec::new();
        if !stripped.trim_start().starts_with('|') {
            starts.push(0);
        }
        for p in find_unescaped_pipes(stripped) {
            let mut s = (p + 1).min(stripped.len());
            while s < stripped.len() && (bytes[s] == b' ' || bytes[s] == b'\t') {
                s += 1;
            }
            starts.push(s);
        }
        starts
    }

    /// Computes the cursor target for `Tab` (forward) / `Shift+Tab` (backward)
    /// inside GFM table header/body rows, appending a skeleton row when
    /// tabbing past the last cell. Returns `None` to fall through to the
    /// default handler (e.g. outside tables, on delimiter rows).
    fn table_tab_target(ctx: &mut HookContext, backwards: bool) -> Option<usize> {
        let row = ctx.buffer.cursor_point().row;
        let block = table_block_at(ctx.buffer, row)?;
        if !matches!(
            block.kind_at(row)?,
            TableRowKind::Header | TableRowKind::Body
        ) {
            return None;
        }
        let line_start = ctx.buffer.point_to_offset(Point::new(row, 0));
        let stripped = clean_table_line(&ctx.buffer.line_to_string(row)).to_string();
        let cursor_col = ctx
            .buffer
            .cursor_offset()
            .saturating_sub(line_start)
            .min(stripped.len());
        let starts = Self::table_cell_starts(&stripped);

        if !backwards {
            if let Some(&s) = starts.iter().find(|&&s| s > cursor_col) {
                return Some(line_start + s);
            }
            // Last cell: move into the next data row, else append a skeleton.
            for r in row + 1..=block.end_row {
                if matches!(
                    block.kind_at(r),
                    Some(TableRowKind::Header) | Some(TableRowKind::Body)
                ) {
                    let next_start = ctx.buffer.point_to_offset(Point::new(r, 0));
                    let next_stripped = clean_table_line(&ctx.buffer.line_to_string(r)).to_string();
                    let next_cells = Self::table_cell_starts(&next_stripped);
                    return Some(next_start + next_cells.first().copied().unwrap_or(0));
                }
            }
            let indent_len = stripped.len() - stripped.trim_start().len();
            let indent = &stripped[..indent_len];
            let skeleton = format!("{}|{}", indent, " |".repeat(block.col_count));
            let line_end = line_start + stripped.len();
            ctx.buffer.set_cursor_offset(line_end);
            ctx.buffer.insert(&format!("\n{skeleton}"));
            return Some(line_end + 1 + indent_len + 2);
        }

        if let Some(&s) = starts.iter().rev().find(|&&s| s < cursor_col) {
            return Some(line_start + s);
        }
        // First cell: move into the previous data row's last cell.
        for r in (block.header_row..row).rev() {
            if matches!(
                block.kind_at(r),
                Some(TableRowKind::Header) | Some(TableRowKind::Body)
            ) {
                let prev_start = ctx.buffer.point_to_offset(Point::new(r, 0));
                let prev_stripped = clean_table_line(&ctx.buffer.line_to_string(r)).to_string();
                let prev_cells = Self::table_cell_starts(&prev_stripped);
                return Some(prev_start + prev_cells.last().copied().unwrap_or(0));
            }
        }
        None
    }
}

impl EditorHook for MarkdownHook {
    fn on_key(&mut self, ctx: &mut HookContext, event: &KeyEvent) -> HookOutcome {
        if event.modifiers.ctrl || event.modifiers.meta {
            match event.key.to_lowercase().as_str() {
                "b" => {
                    if let Some(sel) = ctx.selection.take() {
                        let range = sel.byte_range();
                        let text = ctx.buffer.text().byte_slice(range.clone()).to_string();
                        let wrapped = format!("**{}**", text);
                        ctx.buffer.replace_range(range.clone(), &wrapped);
                        *ctx.selection = Some(Selection::range(range.start + 2, range.end + 2));
                    } else {
                        ctx.buffer.insert("****");
                        ctx.buffer.move_cursor_left();
                        ctx.buffer.move_cursor_left();
                    }
                    return HookOutcome::Consumed;
                }
                "i" => {
                    if let Some(sel) = ctx.selection.take() {
                        let range = sel.byte_range();
                        let text = ctx.buffer.text().byte_slice(range.clone()).to_string();
                        let wrapped = format!("*{}*", text);
                        ctx.buffer.replace_range(range.clone(), &wrapped);
                        *ctx.selection = Some(Selection::range(range.start + 1, range.end + 1));
                    } else {
                        ctx.buffer.insert("**");
                        ctx.buffer.move_cursor_left();
                    }
                    return HookOutcome::Consumed;
                }
                "k" => {
                    if let Some(sel) = ctx.selection.take() {
                        let range = sel.byte_range();
                        let text = ctx.buffer.text().byte_slice(range.clone()).to_string();
                        let wrapped = format!("[{}](url)", text);
                        ctx.buffer.replace_range(range.clone(), &wrapped);
                        let url_start = range.start + 1 + text.len() + 2;
                        *ctx.selection = Some(Selection::range(url_start, url_start + 3));
                    } else {
                        ctx.buffer.insert("[](url)");
                        ctx.buffer.move_cursor_left();
                        ctx.buffer.move_cursor_left();
                        ctx.buffer.move_cursor_left();
                        ctx.buffer.move_cursor_left();
                        ctx.buffer.move_cursor_left();
                    }
                    return HookOutcome::Consumed;
                }
                "enter" if Self::toggle_checkbox(ctx) => {
                    return HookOutcome::Consumed;
                }
                _ => {}
            }
        }

        if event.key == "enter" && !event.modifiers.shift {
            let cursor = ctx.buffer.cursor_offset();
            let row = ctx.buffer.cursor_point().row;
            let line = ctx.buffer.line_to_string(row);
            let trimmed = line.trim_start();
            let indent_len = line.len() - trimmed.len();
            let indent = &line[..indent_len];

            // GFM table row continuation (before list handling: table rows
            // start with `|`, never with a list marker).
            if self.table_navigation
                && let Some(block) = table_block_at(ctx.buffer, row)
                && let Some(kind) = block.kind_at(row)
                && matches!(kind, TableRowKind::Header | TableRowKind::Body)
            {
                let stripped = clean_table_line(&line).to_string();
                let (_, cells) = split_table_cells(&stripped);
                let all_empty = cells.iter().all(|c| {
                    stripped
                        .get(c.clone())
                        .map(|s| s.trim().is_empty())
                        .unwrap_or(true)
                });
                if all_empty {
                    // Empty row exits the table, mirroring empty list items.
                    let line_start = ctx.buffer.point_to_offset(Point::new(row, 0));
                    ctx.buffer.delete_range(line_start..cursor);
                    return HookOutcome::Consumed;
                }
                let table_indent_len = stripped.len() - stripped.trim_start().len();
                let table_indent = &stripped[..table_indent_len];
                let skeleton = format!("{}|{}", table_indent, " |".repeat(block.col_count));
                ctx.buffer.insert(&format!("\n{skeleton}"));
                return HookOutcome::Consumed;
            }

            if trimmed.starts_with("- [ ] ") || trimmed.starts_with("- [x] ") {
                if trimmed == "- [ ] \n"
                    || trimmed == "- [ ] \r\n"
                    || trimmed == "- [ ] "
                    || trimmed == "- [x] \n"
                    || trimmed == "- [x] \r\n"
                    || trimmed == "- [x] "
                {
                    let line_start = ctx.buffer.point_to_offset(Point::new(row, 0));
                    ctx.buffer.delete_range(line_start..cursor);
                    return HookOutcome::Consumed;
                }
                ctx.buffer.insert(&format!("\n{}- [ ] ", indent));
                return HookOutcome::Consumed;
            }

            if trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ ") {
                let bullet = &trimmed[..2];
                if trimmed == "- \n"
                    || trimmed == "- \r\n"
                    || trimmed == "- "
                    || trimmed == "* \n"
                    || trimmed == "* \r\n"
                    || trimmed == "* "
                    || trimmed == "+ \n"
                    || trimmed == "+ \r\n"
                    || trimmed == "+ "
                {
                    let line_start = ctx.buffer.point_to_offset(Point::new(row, 0));
                    ctx.buffer.delete_range(line_start..cursor);
                    return HookOutcome::Consumed;
                }
                ctx.buffer.insert(&format!("\n{}{}", indent, bullet));
                return HookOutcome::Consumed;
            }

            if let Some(dot_idx) = trimmed.find(". ") {
                let num_str = &trimmed[..dot_idx];
                if let Ok(num) = num_str.parse::<usize>() {
                    let rest = &trimmed[dot_idx + 2..];
                    if rest == "\n" || rest == "\r\n" || rest.is_empty() {
                        let line_start = ctx.buffer.point_to_offset(Point::new(row, 0));
                        ctx.buffer.delete_range(line_start..cursor);
                        return HookOutcome::Consumed;
                    }
                    ctx.buffer.insert(&format!("\n{}{}. ", indent, num + 1));
                    return HookOutcome::Consumed;
                }
            }
        }

        // `Tab` / `Shift+Tab` cell navigation inside GFM tables. Runs after
        // `Enter` handling so plain indent-Tab still applies outside tables;
        // returning `Consumed` overrides the editor's default tab-size spaces.
        if event.key == "tab"
            && !event.modifiers.ctrl
            && !event.modifiers.meta
            && !event.modifiers.alt
            && self.table_navigation
            && let Some(target) = Self::table_tab_target(ctx, event.modifiers.shift)
        {
            ctx.buffer.set_cursor_offset(target);
            *ctx.selection = None;
            return HookOutcome::Consumed;
        }

        HookOutcome::PassThrough
    }

    fn on_click(&mut self, ctx: &mut HookContext, row: usize, _col: usize) -> HookOutcome {
        if !self.interactive_tasks {
            return HookOutcome::PassThrough;
        }
        if Self::toggle_marker_at_row(ctx, row) {
            return HookOutcome::Consumed;
        }
        HookOutcome::PassThrough
    }

    fn status_text(&self) -> Option<&str> {
        Some("MARKDOWN")
    }
}

/// Extracts all markdown hyperlink destinations and their label character ranges from a single line.
pub fn extract_markdown_links(line_text: &str) -> Vec<(Range<usize>, String)> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    let parser = Parser::new_ext(line_text, options).into_offset_iter();
    let mut links = Vec::new();
    let mut current_link: Option<(usize, String)> = None;

    for (event, range) in parser {
        match event {
            Event::Start(Tag::Link { dest_url, .. }) => {
                current_link = Some((range.start, dest_url.to_string()));
            }
            Event::End(TagEnd::Link) => {
                if let Some((start, dest_url)) = current_link.take() {
                    let end = range.end.min(line_text.len());
                    if let Some(bracket_idx) = line_text[start..end].find("](") {
                        let label_start = start + 1;
                        let label_end = start + bracket_idx;
                        links.push((label_start..label_end, dest_url));
                    } else if let Some(bracket_idx) = line_text[start..end].find("][") {
                        let label_start = start + 1;
                        let label_end = start + bracket_idx;
                        links.push((label_start..label_end, dest_url));
                    } else if line_text[start..end].starts_with('<')
                        && line_text[start..end].ends_with('>')
                    {
                        links.push((start + 1..end - 1, dest_url));
                    } else {
                        links.push((start..end, dest_url));
                    }
                }
            }
            _ => {}
        }
    }
    links
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ConcealedLine, StyleValue};

    #[test]
    fn test_markdown_heading_spans() {
        let buffer = EditorBuffer::new("# Heading 1\n## Heading 2\nplain text\n---");
        let highlighter = MarkdownHighlighter::new();

        let spans1 = highlighter.highlight_line(&buffer, 0, "# Heading 1");
        assert_eq!(spans1.len(), 1);
        assert_eq!(spans1[0].style, StyleValue::Tag(HighlightTag::Heading(1)));

        let spans2 = highlighter.highlight_line(&buffer, 1, "## Heading 2");
        assert_eq!(spans2.len(), 2);
        assert_eq!(spans2[0].style, StyleValue::Tag(HighlightTag::Dimmed));
        assert_eq!(spans2[1].style, StyleValue::Tag(HighlightTag::Heading(2)));

        let spans3 = highlighter.highlight_line(&buffer, 2, "plain text");
        assert!(spans3.is_empty());

        let spans4 = highlighter.highlight_line(&buffer, 3, "---");
        assert_eq!(spans4.len(), 2);
        assert_eq!(
            spans4[0].style,
            StyleValue::Tag(HighlightTag::HorizontalRule)
        );
        assert_eq!(spans4[1].style, StyleValue::Tag(HighlightTag::Dimmed));
    }

    #[test]
    fn test_markdown_heading_levels_4_to_6() {
        let buffer = EditorBuffer::new("#### H4\n##### H5\n###### H6");
        let highlighter = MarkdownHighlighter::new();

        for (row, text, level) in [
            (0, "#### H4", 4u8),
            (1, "##### H5", 5u8),
            (2, "###### H6", 6u8),
        ] {
            let spans = highlighter.highlight_line(&buffer, row, text);
            assert!(
                spans
                    .iter()
                    .any(|s| s.style == StyleValue::Tag(HighlightTag::Heading(level))),
                "row {row} must emit Heading({level})"
            );
        }
    }

    #[test]
    fn test_markdown_inline_bold_and_code() {
        let buffer = EditorBuffer::new("This is **bold** and `code` here.");
        let highlighter = MarkdownHighlighter::new();

        let spans = highlighter.highlight_line(&buffer, 0, "This is **bold** and `code` here.");
        let bold_span = spans
            .iter()
            .find(|s| s.style == StyleValue::Tag(HighlightTag::Bold));
        assert!(bold_span.is_some());

        let code_span = spans
            .iter()
            .find(|s| s.style == StyleValue::Tag(HighlightTag::Code));
        assert!(code_span.is_some());
    }

    #[test]
    fn test_markdown_hook_bold_wrapping() {
        let mut buffer = EditorBuffer::new("hello world");
        let mut selection = Some(Selection::range(0, 5));
        let mut cursor_style = crate::CursorStyle::Bar;
        let mut hook = MarkdownHook::new();

        let mut ctx = HookContext::new(&mut buffer, &mut selection, &mut cursor_style);
        let event = KeyEvent {
            key: "b".into(),
            modifiers: crate::Modifiers {
                ctrl: true,
                ..Default::default()
            },
        };

        let outcome = hook.on_key(&mut ctx, &event);
        assert_eq!(outcome, HookOutcome::Consumed);
        assert_eq!(ctx.buffer.text().to_string(), "**hello** world");
        assert_eq!(ctx.selection.unwrap().byte_range(), 2..7);
    }

    #[test]
    fn test_markdown_hook_checkbox_toggle() {
        let mut buffer = EditorBuffer::new("- [ ] Task item");
        let mut selection = None;
        let mut cursor_style = crate::CursorStyle::Bar;
        let mut hook = MarkdownHook::new();

        let mut ctx = HookContext::new(&mut buffer, &mut selection, &mut cursor_style);
        let event = KeyEvent {
            key: "enter".into(),
            modifiers: crate::Modifiers {
                ctrl: true,
                ..Default::default()
            },
        };

        let outcome = hook.on_key(&mut ctx, &event);
        assert_eq!(outcome, HookOutcome::Consumed);
        assert_eq!(ctx.buffer.text().to_string(), "- [x] Task item");

        let outcome2 = hook.on_key(&mut ctx, &event);
        assert_eq!(outcome2, HookOutcome::Consumed);
        assert_eq!(ctx.buffer.text().to_string(), "- [ ] Task item");
    }

    #[test]
    fn test_markdown_hook_numbered_list_continuation() {
        let mut buffer = EditorBuffer::new("1. First item");
        buffer.set_cursor_offset(13);
        let mut selection = None;
        let mut cursor_style = crate::CursorStyle::Bar;
        let mut hook = MarkdownHook::new();

        let mut ctx = HookContext::new(&mut buffer, &mut selection, &mut cursor_style);
        let event = KeyEvent::plain("enter");

        let outcome = hook.on_key(&mut ctx, &event);
        assert_eq!(outcome, HookOutcome::Consumed);
        assert_eq!(ctx.buffer.text().to_string(), "1. First item\n2. ");
    }

    #[test]
    fn test_markdown_conceal_modes() {
        let buffer = EditorBuffer::new("# Heading 1\n## Heading 2");

        let hidden_highlighter = MarkdownHighlighter::with_config(MarkdownConfig {
            conceal_mode: ConcealMode::Hidden,
            ..Default::default()
        });
        let spans_hidden = hidden_highlighter.highlight_line(&buffer, 1, "## Heading 2");
        assert_eq!(spans_hidden.len(), 2);
        assert_eq!(spans_hidden[0].style, StyleValue::Tag(HighlightTag::Hidden));
        assert_eq!(
            spans_hidden[1].style,
            StyleValue::Tag(HighlightTag::Heading(2))
        );

        let off_highlighter = MarkdownHighlighter::with_config(MarkdownConfig {
            conceal_mode: ConcealMode::Off,
            ..Default::default()
        });
        let spans_off = off_highlighter.highlight_line(&buffer, 1, "## Heading 2");
        assert_eq!(spans_off.len(), 1);
        assert_eq!(
            spans_off[0].style,
            StyleValue::Tag(HighlightTag::Heading(2))
        );
    }

    #[test]
    fn test_markdown_task_list_and_quote_concealment() {
        let buffer = EditorBuffer::new("- [ ] Task 1\n> Quote line\n```rust\nfn main() {}\n```");
        let hidden_highlighter = MarkdownHighlighter::with_config(MarkdownConfig {
            conceal_mode: ConcealMode::Hidden,
            ..Default::default()
        });

        // Row 0 is cursor row (buffer cursor is at 0)
        // Row 1 (Quote) is inactive
        let spans_quote = hidden_highlighter.highlight_line(&buffer, 1, "> Quote line");
        assert!(!spans_quote.is_empty());
        // Structural tag first, visual concealment last.
        assert_eq!(spans_quote[0].range, 0..2);
        assert_eq!(
            spans_quote[0].style,
            StyleValue::Tag(HighlightTag::Blockquote)
        );
        assert_eq!(spans_quote[1].range, 0..2);
        assert_eq!(spans_quote[1].style, StyleValue::Tag(HighlightTag::Hidden));
        let concealed_quote = ConcealedLine::build("> Quote line", &spans_quote);
        assert_eq!(concealed_quote.display_text, "Quote line");

        // Row 2 (Opening fence) remains visible with HighlightTag::Code
        let spans_fence = hidden_highlighter.highlight_line(&buffer, 2, "```rust");
        assert_eq!(spans_fence.len(), 1);
        assert_eq!(spans_fence[0].range, 0..7);
        assert_eq!(spans_fence[0].style, StyleValue::Tag(HighlightTag::Code));
        let concealed_fence = ConcealedLine::build("```rust", &spans_fence);
        assert_eq!(concealed_fence.display_text, "```rust");

        // When buffer cursor moves to row 1, row 0 becomes inactive
        let mut buffer_moved = buffer;
        buffer_moved.set_cursor_offset(13); // on row 1
        let spans_task = hidden_highlighter.highlight_line(&buffer_moved, 0, "- [ ] Task 1");
        assert!(!spans_task.is_empty());
        assert_eq!(spans_task[0].range, 0..6);
        assert_eq!(
            spans_task[0].style,
            StyleValue::Tag(HighlightTag::TaskUnchecked)
        );
        assert_eq!(spans_task[1].range, 0..6);
        assert_eq!(spans_task[1].style, StyleValue::Tag(HighlightTag::Hidden));
        let concealed_task = ConcealedLine::build("- [ ] Task 1", &spans_task);
        assert_eq!(concealed_task.display_text, "Task 1");
    }

    #[test]
    fn test_markdown_link_concealment() {
        let buffer = EditorBuffer::new("[Google](https://google.com)\nActive line");
        let hidden_highlighter = MarkdownHighlighter::with_config(MarkdownConfig {
            conceal_mode: ConcealMode::Hidden,
            ..Default::default()
        });

        // Buffer cursor is at 0 (row 0), so row 0 is active, full link visible
        let spans_active =
            hidden_highlighter.highlight_line(&buffer, 0, "[Google](https://google.com)");
        let concealed_active = ConcealedLine::build("[Google](https://google.com)", &spans_active);
        assert_eq!(
            concealed_active.display_text,
            "[Google](https://google.com)"
        );

        // Move cursor to row 1, row 0 becomes inactive
        let mut buffer_moved = buffer;
        buffer_moved.set_cursor_offset(30);
        let spans_hidden =
            hidden_highlighter.highlight_line(&buffer_moved, 0, "[Google](https://google.com)");
        let concealed_hidden = ConcealedLine::build("[Google](https://google.com)", &spans_hidden);
        assert_eq!(concealed_hidden.display_text, "Google");

        let extracted = extract_markdown_links("[Google](https://google.com)");
        assert_eq!(extracted.len(), 1);
        assert_eq!(extracted[0].0, 1..7);
        assert_eq!(extracted[0].1, "https://google.com");
    }

    #[test]
    fn test_markdown_structural_tags_in_dimmed_mode() {
        let buffer = EditorBuffer::new("- [ ] Todo\n- [x] Done\n> Quote\n---");
        let highlighter = MarkdownHighlighter::new();
        // Move cursor away so no row is the active cursor row side-effect... row 3 check
        // uses default cursor at row 0, so rows 1-3 are inactive.
        let unchecked = highlighter.highlight_line(&buffer, 0, "- [ ] Todo");
        // Row 0 is the cursor row: structural tag still emitted, no concealment.
        assert!(
            unchecked
                .iter()
                .any(|s| s.style == StyleValue::Tag(HighlightTag::TaskUnchecked))
        );

        let mut moved = buffer;
        moved.set_cursor_offset(30);
        let unchecked_inactive = highlighter.highlight_line(&moved, 0, "- [ ] Todo");
        assert!(
            unchecked_inactive
                .iter()
                .any(|s| s.style == StyleValue::Tag(HighlightTag::TaskUnchecked))
        );
        let checked = highlighter.highlight_line(&moved, 1, "- [x] Done");
        assert!(
            checked
                .iter()
                .any(|s| s.style == StyleValue::Tag(HighlightTag::TaskChecked))
        );
        let quote = highlighter.highlight_line(&moved, 2, "> Quote");
        assert!(
            quote
                .iter()
                .any(|s| s.style == StyleValue::Tag(HighlightTag::Blockquote))
        );
        let hr = highlighter.highlight_line(&moved, 3, "---");
        assert!(
            hr.iter()
                .any(|s| s.style == StyleValue::Tag(HighlightTag::HorizontalRule))
        );
    }

    #[test]
    fn test_markdown_hook_on_click_toggles_task() {
        let mut buffer = EditorBuffer::new("- [ ] Task one\n- [x] Task two");
        let mut selection = None;
        let mut cursor_style = crate::CursorStyle::Bar;
        let mut hook = MarkdownHook::new();

        // Click row 1 (checked -> unchecked), cursor stays put.
        buffer.set_cursor_offset(0);
        let mut ctx = HookContext::new(&mut buffer, &mut selection, &mut cursor_style);
        assert_eq!(hook.on_click(&mut ctx, 1, 0), HookOutcome::Consumed);
        assert_eq!(
            ctx.buffer.text().to_string(),
            "- [ ] Task one\n- [ ] Task two"
        );
        assert_eq!(ctx.buffer.cursor_offset(), 0);

        // Click row 0 (unchecked -> checked).
        let mut ctx = HookContext::new(&mut buffer, &mut selection, &mut cursor_style);
        assert_eq!(hook.on_click(&mut ctx, 0, 2), HookOutcome::Consumed);
        assert_eq!(
            ctx.buffer.text().to_string(),
            "- [x] Task one\n- [ ] Task two"
        );

        // Uppercase [X] also toggles.
        ctx.buffer.replace_range(0..14, "- [X] Task one");
        let mut ctx = HookContext::new(&mut buffer, &mut selection, &mut cursor_style);
        assert_eq!(hook.on_click(&mut ctx, 0, 3), HookOutcome::Consumed);
        assert_eq!(
            ctx.buffer.text().to_string(),
            "- [ ] Task one\n- [ ] Task two"
        );

        // Plain line passes through.
        let mut plain = EditorBuffer::new("hello");
        let mut ctx = HookContext::new(&mut plain, &mut selection, &mut cursor_style);
        assert_eq!(hook.on_click(&mut ctx, 0, 0), HookOutcome::PassThrough);
    }

    #[test]
    fn test_markdown_hook_on_click_respects_config() {
        let mut buffer = EditorBuffer::new("- [ ] Task");
        let mut selection = None;
        let mut cursor_style = crate::CursorStyle::Bar;
        let mut hook = MarkdownHook::with_config(MarkdownConfig {
            interactive_tasks: false,
            ..Default::default()
        });
        let mut ctx = HookContext::new(&mut buffer, &mut selection, &mut cursor_style);
        assert_eq!(hook.on_click(&mut ctx, 0, 0), HookOutcome::PassThrough);
        assert_eq!(ctx.buffer.text().to_string(), "- [ ] Task");
    }

    #[test]
    fn test_markdown_highlighter_extract_links_trait() {
        use crate::SyntaxHighlighter;
        let buffer = EditorBuffer::new("[Google](https://google.com)");
        let highlighter = MarkdownHighlighter::new();
        let links = highlighter.extract_links(&buffer, 0, "[Google](https://google.com)");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].0, 1..7);
        assert_eq!(links[0].1, "https://google.com");

        let none = highlighter.extract_links(&buffer, 0, "plain text");
        assert!(none.is_empty());
    }

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
    fn test_table_highlight_uses_existing_tags_only() {
        let buffer = EditorBuffer::new("| Name | Age |\n| --- | ---: |\n| Ada | 36 |");
        let highlighter = MarkdownHighlighter::new();

        let header = highlighter.highlight_line(&buffer, 0, "| Name | Age |");
        assert!(
            header
                .iter()
                .any(|s| s.style == StyleValue::Tag(HighlightTag::Punctuation))
        );
        assert!(
            header
                .iter()
                .any(|s| s.style == StyleValue::Tag(HighlightTag::Bold))
        );
        assert!(
            header
                .iter()
                .any(|s| s.style == StyleValue::Tag(HighlightTag::Custom(TABLE_HEADER_TAG)))
        );

        let body = highlighter.highlight_line(&buffer, 2, "| Ada | 36 |");
        assert!(
            body.iter()
                .any(|s| s.style == StyleValue::Tag(HighlightTag::Custom(TABLE_CELL_TAG)))
        );
        assert!(
            body.iter()
                .all(|s| s.style != StyleValue::Tag(HighlightTag::Bold))
        );

        let delim = highlighter.highlight_line(&buffer, 1, "| --- | ---: |");
        assert!(
            delim
                .iter()
                .any(|s| s.style == StyleValue::Tag(HighlightTag::Custom(TABLE_DELIMITER_TAG)))
        );

        // Inline code inside cells still highlights.
        let code_row = EditorBuffer::new("| `x|y` | b |\n| --- | --- |\n| c | d |");
        let code_spans = highlighter.highlight_line(&code_row, 0, "| `x|y` | b |");
        assert!(
            code_spans
                .iter()
                .any(|s| s.style == StyleValue::Tag(HighlightTag::Code))
        );

        // Pipes are never fully concealed: Hidden maps to Dimmed.
        let hidden = MarkdownHighlighter::with_config(MarkdownConfig {
            conceal_mode: ConcealMode::Hidden,
            ..Default::default()
        });
        let mut moved = EditorBuffer::new("| Name | Age |\n| --- | ---: |\n| Ada | 36 |");
        moved.set_cursor_offset(40);
        let inactive = hidden.highlight_line(&moved, 0, "| Name | Age |");
        assert!(
            inactive
                .iter()
                .all(|s| s.style != StyleValue::Tag(HighlightTag::Hidden))
        );
        assert!(
            inactive
                .iter()
                .any(|s| s.style == StyleValue::Tag(HighlightTag::Dimmed))
        );
        let concealed = ConcealedLine::build("| Name | Age |", &inactive);
        assert_eq!(concealed.display_text, "| Name | Age |");

        // Opt-out flag disables everything.
        let off = MarkdownHighlighter::with_config(MarkdownConfig {
            visual_tables: false,
            ..Default::default()
        });
        let plain = off.highlight_line(&buffer, 0, "| Name | Age |");
        assert!(plain.iter().all(|s| !matches!(
            s.style,
            StyleValue::Tag(
                HighlightTag::Custom(TABLE_HEADER_TAG)
                    | HighlightTag::Custom(TABLE_CELL_TAG)
                    | HighlightTag::Custom(TABLE_DELIMITER_TAG)
            )
        )));
    }

    #[test]
    fn test_table_hook_tab_moves_between_cells() {
        let mut buffer = EditorBuffer::new("| a | b |\n| --- | --- |\n| c | d |");
        let mut selection = None;
        let mut cursor_style = crate::CursorStyle::Bar;
        let mut hook = MarkdownHook::new();
        buffer.set_cursor_offset(0);

        let mut ctx = HookContext::new(&mut buffer, &mut selection, &mut cursor_style);
        assert_eq!(
            hook.on_key(&mut ctx, &KeyEvent::plain("tab")),
            HookOutcome::Consumed
        );
        assert_eq!(ctx.buffer.cursor_offset(), 2); // start of `a`

        let mut ctx = HookContext::new(&mut buffer, &mut selection, &mut cursor_style);
        assert_eq!(
            hook.on_key(&mut ctx, &KeyEvent::plain("tab")),
            HookOutcome::Consumed
        );
        assert_eq!(ctx.buffer.cursor_offset(), 6); // start of `b`

        // Shift+Tab goes back.
        let back = KeyEvent {
            key: "tab".into(),
            modifiers: crate::Modifiers {
                shift: true,
                ..Default::default()
            },
        };
        let mut ctx = HookContext::new(&mut buffer, &mut selection, &mut cursor_style);
        assert_eq!(hook.on_key(&mut ctx, &back), HookOutcome::Consumed);
        assert_eq!(ctx.buffer.cursor_offset(), 2);
    }

    #[test]
    fn test_table_hook_tab_appends_row_at_end() {
        let mut buffer = EditorBuffer::new("| a |\n| --- |\n| b |");
        let mut selection = None;
        let mut cursor_style = crate::CursorStyle::Bar;
        let mut hook = MarkdownHook::new();
        buffer.set_cursor_offset(buffer.len_bytes());

        let mut ctx = HookContext::new(&mut buffer, &mut selection, &mut cursor_style);
        assert_eq!(
            hook.on_key(&mut ctx, &KeyEvent::plain("tab")),
            HookOutcome::Consumed
        );
        assert_eq!(ctx.buffer.text().to_string(), "| a |\n| --- |\n| b |\n| |");
    }

    #[test]
    fn test_table_hook_tab_passthrough_outside_tables() {
        let mut buffer = EditorBuffer::new("plain text");
        let mut selection = None;
        let mut cursor_style = crate::CursorStyle::Bar;
        let mut hook = MarkdownHook::new();
        buffer.set_cursor_offset(3);

        let mut ctx = HookContext::new(&mut buffer, &mut selection, &mut cursor_style);
        assert_eq!(
            hook.on_key(&mut ctx, &KeyEvent::plain("tab")),
            HookOutcome::PassThrough
        );

        let mut disabled = EditorBuffer::new("| a |\n| --- |\n| b |");
        disabled.set_cursor_offset(0);
        let mut hook_off = MarkdownHook::with_config(MarkdownConfig {
            table_navigation: false,
            ..Default::default()
        });
        let mut ctx = HookContext::new(&mut disabled, &mut selection, &mut cursor_style);
        assert_eq!(
            hook_off.on_key(&mut ctx, &KeyEvent::plain("tab")),
            HookOutcome::PassThrough
        );
    }

    #[test]
    fn test_table_hook_enter_continues_and_exits() {
        // Continuation inserts a skeleton row at the cursor.
        let mut buffer = EditorBuffer::new("| a | b |\n| --- | --- |\n| c | d |");
        buffer.set_cursor_offset(buffer.len_bytes());
        let mut selection = None;
        let mut cursor_style = crate::CursorStyle::Bar;
        let mut hook = MarkdownHook::new();
        let mut ctx = HookContext::new(&mut buffer, &mut selection, &mut cursor_style);
        assert_eq!(
            hook.on_key(&mut ctx, &KeyEvent::plain("enter")),
            HookOutcome::Consumed
        );
        assert_eq!(
            ctx.buffer.text().to_string(),
            "| a | b |\n| --- | --- |\n| c | d |\n| | |"
        );

        // An all-blank row exits the table like empty list items do.
        let mut empty = EditorBuffer::new("| a |\n| --- |\n| |");
        empty.set_cursor_offset(empty.len_bytes());
        let mut ctx = HookContext::new(&mut empty, &mut selection, &mut cursor_style);
        assert_eq!(
            hook.on_key(&mut ctx, &KeyEvent::plain("enter")),
            HookOutcome::Consumed
        );
        assert_eq!(ctx.buffer.text().to_string(), "| a |\n| --- |\n");
    }
}
