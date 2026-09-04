use std::ops::Range;
use std::sync::{Arc, RwLock};

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

use crate::{
    ConcealedLine, DisplayPad, EditorBuffer, HighlightTag, StyleSpan, SyntaxHighlighter, TextStyle,
    display_width,
};

use super::config::{ConcealMode, MarkdownConfig};
use super::links::extract_markdown_links;
use super::table::{
    TABLE_CELL_TAG, TABLE_DELIMITER_TAG, TABLE_HEADER_TAG, TableAlignment, TableLayout,
    TableRowKind, clean_table_line, find_unescaped_pipes, split_table_cells, table_block_at,
    table_layouts,
};

/// Cached table display layouts and associated document version.
type TableCache = Arc<RwLock<Option<(usize, Vec<TableLayout>)>>>;

/// Cached fence line row indices and associated document version.
type FenceCache = Arc<RwLock<Option<(usize, Vec<usize>)>>>;

/// A syntax highlighter for CommonMark and GFM Markdown documents using `pulldown-cmark`.
#[derive(Debug, Clone)]
pub struct MarkdownHighlighter {
    config: MarkdownConfig,
    cached_fences: FenceCache,
    cached_tables: TableCache,
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
            cached_tables: Arc::new(RwLock::new(None)),
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

    /// Returns the cached display layouts for every table in the buffer,
    /// recomputing them when the document version changed.
    fn table_layouts(&self, buffer: &EditorBuffer) -> Vec<TableLayout> {
        let version = buffer.version();
        if let Ok(guard) = self.cached_tables.read()
            && let Some((v, ref layouts)) = *guard
            && v == version
        {
            return layouts.clone();
        }
        if let Ok(mut guard) = self.cached_tables.write() {
            if let Some((v, ref layouts)) = *guard
                && v == version
            {
                return layouts.clone();
            }
            let layouts = table_layouts(buffer);
            *guard = Some((version, layouts.clone()));
            layouts
        } else {
            table_layouts(buffer)
        }
    }
}

/// Snaps a display byte offset forward to a char boundary.
fn snap_display_fwd(display: &str, mut i: usize) -> usize {
    i = i.min(display.len());
    while i < display.len() && !display.is_char_boundary(i) {
        i += 1;
    }
    i
}

/// Snaps a display byte offset backward to a char boundary.
fn snap_display_back(display: &str, mut i: usize) -> usize {
    i = i.min(display.len());
    while i > 0 && !display.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Computes display-only padding aligning one table row's cells to the
/// block's column widths.
///
/// `source` is the stripped source line, `concealed` its collapsed display
/// form. Column widths are measured on unconcealed source text (see
/// [`TableLayout`]); per-row padding absorbs concealment shrinkage so pipes
/// align on active and inactive rows alike. Delimiter dashes are extended
/// with `-` fill; body/header cells are space-padded honoring the column's
/// delimiter alignment.
fn table_row_pads(
    layout: &TableLayout,
    kind: TableRowKind,
    source: &str,
    concealed: &ConcealedLine,
) -> Vec<DisplayPad> {
    let display = &concealed.display_text;
    let (_, cells) = split_table_cells(source);
    let mut pads = Vec::new();
    for (i, cell) in cells.iter().enumerate().take(layout.col_widths.len()) {
        let width = layout.col_widths[i];
        let ds = snap_display_fwd(
            display,
            concealed.source_to_display(cell.start.min(source.len())),
        );
        let de = snap_display_back(
            display,
            concealed.source_to_display(cell.end.min(source.len())),
        );
        if ds >= de {
            continue;
        }
        // Trim padding already present in the display slice.
        let bytes = display.as_bytes();
        let mut cs = ds;
        while cs < de && (bytes[cs] == b' ' || bytes[cs] == b'\t') {
            cs += 1;
        }
        let mut ce = de;
        while ce > cs && (bytes[ce - 1] == b' ' || bytes[ce - 1] == b'\t') {
            ce -= 1;
        }
        let content_width = display_width(&display[cs..ce]);
        if content_width >= width {
            continue;
        }
        let need = width - content_width;
        if kind == TableRowKind::Delimiter {
            // Extend the dash run, keeping a trailing alignment colon last.
            let at = if display[cs..ce].ends_with(':') {
                ce - 1
            } else {
                ce
            };
            pads.push(DisplayPad {
                display_at: at,
                fill: '-',
                len: need,
            });
            continue;
        }
        match layout
            .block
            .aligns
            .get(i)
            .copied()
            .unwrap_or(TableAlignment::None)
        {
            TableAlignment::Right => pads.push(DisplayPad {
                display_at: cs,
                fill: ' ',
                len: need,
            }),
            TableAlignment::Center => {
                let left = need / 2;
                let right = need - left;
                if left > 0 {
                    pads.push(DisplayPad {
                        display_at: cs,
                        fill: ' ',
                        len: left,
                    });
                }
                if right > 0 {
                    pads.push(DisplayPad {
                        display_at: ce,
                        fill: ' ',
                        len: right,
                    });
                }
            }
            TableAlignment::Left | TableAlignment::None => pads.push(DisplayPad {
                display_at: ce,
                fill: ' ',
                len: need,
            }),
        }
    }
    pads
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

    fn expand_line(
        &self,
        buffer: &EditorBuffer,
        row: usize,
        concealed: &ConcealedLine,
    ) -> Vec<DisplayPad> {
        if !(self.config.visual_tables && self.config.table_alignment) {
            return Vec::new();
        }
        let layouts = self.table_layouts(buffer);
        let layout = match layouts.iter().find(|l| l.block.contains(row)) {
            Some(layout) => layout,
            None => return Vec::new(),
        };
        let kind = match layout.block.kind_at(row) {
            Some(kind) => kind,
            None => return Vec::new(),
        };
        let source = clean_table_line(&buffer.line_to_string(row)).to_string();
        table_row_pads(layout, kind, &source, concealed)
    }

    fn should_wrap_line(&self, buffer: &EditorBuffer, row: usize) -> bool {
        if !(self.config.visual_tables && self.config.table_alignment) {
            return true;
        }
        !table_block_at(buffer, row).is_some_and(|b| b.contains(row))
    }
}

#[cfg(test)]
mod tests {
    use super::super::links::extract_markdown_links;
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

    /// Renders one row through highlight + conceal + expand, like the canvas does.
    fn expanded_display(
        highlighter: &MarkdownHighlighter,
        buffer: &EditorBuffer,
        row: usize,
        line: &str,
    ) -> String {
        let spans = highlighter.highlight_line(buffer, row, line);
        let concealed = ConcealedLine::build(line, &spans);
        let pads = highlighter.expand_line(buffer, row, &concealed);
        concealed.expanded(&pads).display_text
    }

    #[test]
    fn test_table_columns_share_widths_when_a_cell_grows() {
        let buffer = EditorBuffer::new("| a | b |\n| --- | --- |\n| looong | c |\n| d | e |");
        let highlighter = MarkdownHighlighter::new();

        let header = expanded_display(&highlighter, &buffer, 0, "| a | b |");
        let body_long = expanded_display(&highlighter, &buffer, 2, "| looong | c |");
        let body_short = expanded_display(&highlighter, &buffer, 3, "| d | e |");
        let delim = expanded_display(&highlighter, &buffer, 1, "| --- | --- |");

        // Pipes land on the same display columns in every row.
        let pipe_cols = |s: &str| {
            s.char_indices()
                .filter(|(_, c)| *c == '|')
                .map(|(i, _)| i)
                .collect::<Vec<_>>()
        };
        assert_eq!(pipe_cols(&header), pipe_cols(&body_long));
        assert_eq!(pipe_cols(&header), pipe_cols(&body_short));
        assert_eq!(pipe_cols(&header), pipe_cols(&delim));
        assert_eq!(header, "| a      | b   |");
        assert_eq!(body_long, "| looong | c   |");
        assert_eq!(delim, "| ------ | --- |");
    }

    #[test]
    fn test_table_alignment_honors_delimiter_sides() {
        let buffer = EditorBuffer::new("| ab | c |\n| ---: | --- |\n| d | ef |");
        let highlighter = MarkdownHighlighter::new();

        // Right-aligned column pads before the content, plain column after.
        let body = expanded_display(&highlighter, &buffer, 2, "| d | ef |");
        assert_eq!(body, "|    d | ef  |");
        // The delimiter already fits, so it renders unchanged and aligned.
        let delim = expanded_display(&highlighter, &buffer, 1, "| ---: | --- |");
        assert_eq!(delim, "| ---: | --- |");
        let pipe_cols = |s: &str| {
            s.char_indices()
                .filter(|(_, c)| *c == '|')
                .map(|(i, _)| i)
                .collect::<Vec<_>>()
        };
        assert_eq!(pipe_cols(&body), pipe_cols(&delim));
    }

    #[test]
    fn test_table_rows_opt_out_of_wrapping() {
        let buffer = EditorBuffer::new("| a | b |\n| --- | --- |\n| c | d |\nplain");
        let highlighter = MarkdownHighlighter::new();
        assert!(!highlighter.should_wrap_line(&buffer, 0));
        assert!(!highlighter.should_wrap_line(&buffer, 1));
        assert!(!highlighter.should_wrap_line(&buffer, 2));
        assert!(highlighter.should_wrap_line(&buffer, 3));

        let off = MarkdownHighlighter::with_config(MarkdownConfig {
            table_alignment: false,
            ..Default::default()
        });
        assert!(off.should_wrap_line(&buffer, 0));
        assert!(
            off.expand_line(&buffer, 0, &ConcealedLine::build("| a | b |", &[]))
                .is_empty()
        );
    }

    #[test]
    fn test_table_layout_cache_invalidates_on_edit() {
        let mut buffer = EditorBuffer::new("| a |\n| --- |\n| b |");
        let highlighter = MarkdownHighlighter::new();
        assert_eq!(
            expanded_display(&highlighter, &buffer, 2, "| b |"),
            "| b   |"
        );
        buffer.replace_range(16..17, "much-longer");
        assert_eq!(
            expanded_display(&highlighter, &buffer, 2, "| much-longer |"),
            "| much-longer |"
        );
    }
}
