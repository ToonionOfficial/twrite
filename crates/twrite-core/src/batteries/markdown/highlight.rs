use std::ops::Range;
use std::sync::{Arc, RwLock};

use crate::folding::FoldRange;
use crate::{
    CalloutKind, ConcealedLine, DisplayPad, EditorBuffer, HighlightTag, StyleSpan,
    SyntaxHighlighter, display_width,
};

use super::config::{ConcealMode, MarkdownConfig};
use super::folding::{heading_level, markdown_fold_ranges};
use super::links::extract_markdown_links;
use super::table::{
    TABLE_CELL_TAG, TABLE_DELIMITER_TAG, TABLE_HEADER_TAG, TableAlignment, TableBlock, TableLayout,
    TableRowKind, clean_table_line, find_unescaped_pipes, is_fenced_row, split_table_cells,
    table_layouts_with_fences,
};

/// Cached table display layouts and associated document version.
type TableCache = Arc<RwLock<Option<(usize, Vec<TableLayout>)>>>;

/// Cached fence line row indices and associated document version.
type FenceCache = Arc<RwLock<Option<(usize, Vec<usize>)>>>;

/// Cached frontmatter row range and associated document version.
type FrontmatterCache = Arc<RwLock<Option<(usize, Option<Range<usize>>)>>>;

/// A syntax highlighter for CommonMark and GFM Markdown documents using `pulldown-cmark`.
#[derive(Debug, Clone)]
pub struct MarkdownHighlighter {
    config: MarkdownConfig,
    cached_fences: FenceCache,
    cached_tables: TableCache,
    cached_frontmatter: FrontmatterCache,
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
            cached_frontmatter: Arc::new(RwLock::new(None)),
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
        let fences = self.cached_fence_rows(buffer);
        is_fenced_row(&fences, current_row)
    }

    /// Returns the fence-marker rows for this document version, scanning once
    /// and sharing the result across all per-row queries in the epoch.
    ///
    /// This is the single `O(N)` fence pass per version: `highlight_line`,
    /// table-block lookup, and table-layout building all share it instead of
    /// each rescanning `0..row` per visible row per frame.
    fn cached_fence_rows(&self, buffer: &EditorBuffer) -> Vec<usize> {
        let version = buffer.version();
        if let Ok(guard) = self.cached_fences.read()
            && let Some((v, ref fences)) = *guard
            && v == version
        {
            return fences.clone();
        }

        if let Ok(mut guard) = self.cached_fences.write() {
            if let Some((v, ref fences)) = *guard
                && v == version
            {
                return fences.clone();
            }

            let fences = scan_fence_rows(buffer);
            *guard = Some((version, fences.clone()));
            fences
        } else {
            scan_fence_rows(buffer)
        }
    }

    /// Returns the frontmatter row range for this document version, scanning
    /// once and sharing the result across all per-row queries in the epoch,
    /// like the fence and table caches above.
    fn cached_frontmatter(&self, buffer: &EditorBuffer) -> Option<Range<usize>> {
        let version = buffer.version();
        if let Ok(guard) = self.cached_frontmatter.read()
            && let Some((v, ref range)) = *guard
            && v == version
        {
            return range.clone();
        }

        if let Ok(mut guard) = self.cached_frontmatter.write() {
            if let Some((v, ref range)) = *guard
                && v == version
            {
                return range.clone();
            }

            let fences = self.cached_fence_rows(buffer);
            let range = scan_frontmatter(buffer, &fences);
            *guard = Some((version, range.clone()));
            range
        } else {
            let fences = self.cached_fence_rows(buffer);
            scan_frontmatter(buffer, &fences)
        }
    }

    /// Locates the table block for `row` from the version-cached layouts.
    ///
    /// Point queries never walk the buffer: the one linear sweep per version
    /// lives in [`Self::cached_layouts`], and every per-row call
    /// (`highlight_line`, `should_wrap_line`, `expand_line`) shares it.
    /// Previously each call re-walked up to the whole table (upward search +
    /// body extension with a `line_to_string` alloc per row), costing
    /// milliseconds per row inside large tables on every selection frame.
    fn cached_table_block(&self, buffer: &EditorBuffer, row: usize) -> Option<TableBlock> {
        if row >= buffer.len_lines() {
            return None;
        }
        self.cached_layouts(buffer)
            .into_iter()
            .find(|l| l.block.contains(row))
            .map(|l| l.block)
    }

    /// Returns the table layout containing `row`, if any.
    fn layout_for_row(&self, buffer: &EditorBuffer, row: usize) -> Option<TableLayout> {
        self.cached_layouts(buffer)
            .into_iter()
            .find(|l| l.block.contains(row))
    }

    /// Returns all table layouts for this document version, sweeping once
    /// and sharing the result across every per-row query in the epoch.
    fn cached_layouts(&self, buffer: &EditorBuffer) -> Vec<TableLayout> {
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
            // Build layouts with the shared fence index: one linear sweep,
            // not one `O(row)` fence rescan per row.
            let fences = self.cached_fence_rows(buffer);
            let layouts = table_layouts_with_fences(buffer, &fences);
            *guard = Some((version, layouts.clone()));
            layouts
        } else {
            let fences = self.cached_fence_rows(buffer);
            table_layouts_with_fences(buffer, &fences)
        }
    }
}

/// Single linear fence-marker scan shared by every table query in an epoch.
fn scan_fence_rows(buffer: &EditorBuffer) -> Vec<usize> {
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
    fences
}

/// Locates a document-leading YAML frontmatter block: row 0 must be `---`
/// and the returned range runs through the first later `---`/`...` row.
/// Candidates inside fenced code blocks never close the block. An unclosed
/// opener yields `None` so row 0 keeps its horizontal-rule rendering.
fn scan_frontmatter(buffer: &EditorBuffer, fences: &[usize]) -> Option<Range<usize>> {
    if buffer.line_to_string(0).trim() != "---" {
        return None;
    }
    for row in 1..buffer.len_lines() {
        if is_fenced_row(fences, row) {
            continue;
        }
        let trimmed = buffer.line_to_string(row);
        if trimmed.trim() == "---" || trimmed.trim() == "..." {
            return Some(0..row + 1);
        }
    }
    None
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

/// Parsed callout header (`> [!KIND] Title`) with byte ranges in the line.
struct CalloutHeader {
    kind: CalloutKind,
    /// Byte range of the `[!KIND]` marker plus any `-`/`+` fold suffix and
    /// the spaces leading into the title. Concealing the spaces keeps the
    /// title flush; the cursor reveals anywhere inside for editing.
    marker: Range<usize>,
    /// Byte range of the title text after the marker, if any.
    title: Option<Range<usize>>,
}

/// Parses a callout header from a trimmed quote line whose `> ` prefix is
/// already verified. Unknown and lowercase kinds map leniently; anything
/// else yields `None`.
fn parse_callout_header(line_text: &str, trimmed_start: &str) -> Option<CalloutHeader> {
    let indent = line_text.len() - trimmed_start.len();
    let rest = trimmed_start.get(2..)?.strip_prefix("[!")?;
    let close = rest.find(']')?;
    if close == 0 {
        return None;
    }
    let kind = match rest[..close].to_ascii_uppercase().as_str() {
        "NOTE" => CalloutKind::Note,
        "TIP" => CalloutKind::Tip,
        "WARNING" => CalloutKind::Warning,
        "CAUTION" => CalloutKind::Caution,
        "IMPORTANT" => CalloutKind::Important,
        _ => CalloutKind::Other,
    };
    let mut marker_end = indent + 2 + 2 + close + 1;
    // A directly suffixed `-`/`+` is Obsidian's fold marker. Collapsing is a
    // follow-up; conceal it so it never leaks out as title text.
    if line_text
        .as_bytes()
        .get(marker_end)
        .is_some_and(|byte| *byte == b'-' || *byte == b'+')
    {
        marker_end += 1;
    }
    let mut title_start = marker_end;
    while line_text
        .as_bytes()
        .get(title_start)
        .is_some_and(|byte| *byte == b' ' || *byte == b'\t')
    {
        title_start += 1;
    }
    Some(CalloutHeader {
        kind,
        marker: indent + 2..title_start,
        title: (title_start < line_text.len()).then_some(title_start..line_text.len()),
    })
}

/// Returns the callout kind for a quote row plus its parsed header when the
/// row itself opens the block. Body rows inherit the nearest header above;
/// blank rows and non-quote rows end the block.
fn callout_at_row(
    buffer: &EditorBuffer,
    row: usize,
    line_text: &str,
    trimmed_start: &str,
) -> Option<(CalloutKind, Option<CalloutHeader>)> {
    if let Some(header) = parse_callout_header(line_text, trimmed_start) {
        return Some((header.kind, Some(header)));
    }
    let mut above = row;
    while above > 0 {
        above -= 1;
        let raw = buffer.line_to_string(above);
        let text = raw.trim_end_matches(['\r', '\n']);
        let trimmed = text.trim_start();
        if trimmed.is_empty() || !(trimmed.starts_with("> ") || trimmed == ">") {
            return None;
        }
        if let Some(header) = parse_callout_header(text, trimmed) {
            return Some((header.kind, None));
        }
    }
    None
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

        let cursor_position = buffer.cursor_point();
        let is_cursor_row = row == cursor_position.row;
        // Byte offset of the cursor within this line when the cursor sits on
        // it. Inline constructs compare against this to reveal word by word
        // instead of exposing the whole row at once.
        let cursor_line_offset = if is_cursor_row {
            Some(cursor_position.column.min(line_text.len()))
        } else {
            None
        };

        if trimmed_start.starts_with("```") || trimmed_start.starts_with("~~~") {
            spans.push(StyleSpan::tag(0..line_text.len(), HighlightTag::Code));
            return spans;
        }

        if self.is_in_fenced_code_block(buffer, row) {
            spans.push(StyleSpan::tag(0..line_text.len(), HighlightTag::Code));
            return spans;
        }

        // YAML frontmatter renders as dimmed metadata, never as a rule plus
        // plain text. Fences stay dim in every conceal mode so the rows never
        // collapse; the inline pass is skipped so YAML punctuation cannot
        // produce garbage spans. Same output on and off the cursor row.
        if self
            .cached_frontmatter(buffer)
            .is_some_and(|block| block.contains(&row))
        {
            if self.config.conceal_mode != ConcealMode::Off {
                spans.push(StyleSpan::tag(0..line_text.len(), HighlightTag::Dimmed));
            }
            return spans;
        }

        // GFM pipe tables. Runs before the thematic-break check so a
        // single-column `---` delimiter is not mistaken for an `<hr>`.
        // Pipes stay visible in every conceal mode (Hidden maps to Dimmed)
        // to preserve `ConcealedLine` source/display column alignment.
        if self.config.visual_tables
            && let Some(block) = self.cached_table_block(buffer, row)
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

        let heading_prefix = heading_level(trimmed_start)
            .map(|level| (level as usize + 1, HighlightTag::Heading(level)));

        if let Some((prefix_len, tag)) = heading_prefix {
            let indent = line_text.len() - trimmed_start.len();
            // The `#` prefix reveals only while the cursor sits on it;
            // editing the heading text keeps it concealed, Obsidian style.
            let cursor_in_prefix = cursor_line_offset
                .is_some_and(|cursor| indent <= cursor && cursor < indent + prefix_len);
            if !cursor_in_prefix && let Some(delim_tag) = delimiter_tag {
                spans.push(StyleSpan::tag(indent..indent + prefix_len, delim_tag));
                if indent + prefix_len < line_text.len() {
                    spans.push(StyleSpan::tag(indent + prefix_len..line_text.len(), tag));
                } else {
                    // Bare prefix (`# ` with no text yet): tag it so line
                    // metrics scale immediately instead of waiting for the
                    // first content character.
                    spans.push(StyleSpan::tag(indent..indent + prefix_len, tag));
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
            // The `>` prefix reveals only while the cursor sits on it, like
            // `#` and task markers.
            let cursor_in_quote_prefix = cursor_line_offset
                .is_some_and(|cursor| indent <= cursor && cursor < indent + quote_len);
            if !cursor_in_quote_prefix && let Some(delim_tag) = delimiter_tag {
                let delim_len = if trimmed_start.starts_with("> ") {
                    2
                } else {
                    1
                };
                spans.push(StyleSpan::tag(indent..indent + delim_len, delim_tag));
            } else {
                spans.push(StyleSpan::tag(indent..indent + 1, HighlightTag::Comment));
            }

            // Callout header or inherited body kind. The structural tag covers
            // the full line so tag-driven layout sees body rows too; the
            // marker conceals word-level while the title renders bold. Body
            // text dims to quote gray under inline spans pushed later.
            if let Some((kind, header)) = callout_at_row(buffer, row, line_text, trimmed_start) {
                spans.push(StyleSpan::tag(
                    0..line_text.len(),
                    HighlightTag::Callout(kind),
                ));
                if header.is_none() {
                    let content_start = indent + quote_len;
                    if content_start < line_text.len() {
                        spans.push(StyleSpan::tag(
                            content_start..line_text.len(),
                            HighlightTag::Comment,
                        ));
                    }
                }
                if let Some(header) = header {
                    let cursor_in_marker = cursor_line_offset.is_some_and(|cursor| {
                        header.marker.start <= cursor && cursor < header.marker.end
                    });
                    if !cursor_in_marker && let Some(delim_tag) = delimiter_tag {
                        spans.push(StyleSpan::tag(header.marker.clone(), delim_tag));
                    }
                    if let Some(title) = header.title {
                        spans.push(StyleSpan::tag(title, HighlightTag::Bold));
                    }
                }
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
            // The checkbox widget keys off the structural tag above, so the
            // marker bytes stay hidden except while the cursor sits on them
            // for editing.
            let cursor_in_marker = cursor_line_offset
                .is_some_and(|cursor| indent <= cursor && cursor < indent + marker_len);
            if !cursor_in_marker && let Some(delim_tag) = delimiter_tag {
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

        super::inline::highlight_inline_markdown(
            line_text,
            cursor_line_offset,
            delimiter_tag,
            &mut spans,
        );

        spans
    }

    fn extract_links(
        &self,
        buffer: &EditorBuffer,
        row: usize,
        line_text: &str,
    ) -> Vec<(Range<usize>, String)> {
        if !(line_text.contains('[') || line_text.contains('<')) {
            return Vec::new();
        }
        // YAML values shaped like links stay plain metadata, never clickable.
        if self
            .cached_frontmatter(buffer)
            .is_some_and(|block| block.contains(&row))
        {
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
        let layout = match self.layout_for_row(buffer, row) {
            Some(layout) => layout,
            None => return Vec::new(),
        };
        let kind = match layout.block.kind_at(row) {
            Some(kind) => kind,
            None => return Vec::new(),
        };
        let source = clean_table_line(&buffer.line_to_string(row)).to_string();
        table_row_pads(&layout, kind, &source, concealed)
    }

    fn should_wrap_line(&self, buffer: &EditorBuffer, row: usize) -> bool {
        if !(self.config.visual_tables && self.config.table_alignment) {
            return true;
        }
        !self
            .cached_table_block(buffer, row)
            .is_some_and(|b| b.contains(row))
    }

    fn foldable_ranges(&self, buffer: &EditorBuffer) -> Vec<FoldRange> {
        let fences = self.cached_fence_rows(buffer);
        let frontmatter = self.cached_frontmatter(buffer);
        markdown_fold_ranges(buffer, &fences, frontmatter.as_ref())
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
    fn test_markdown_underscore_bold_concealment() {
        // Issue #51: `__bold__` must conceal exactly like `**bold**`.
        // Cursor stays on row 0 so row 1 is inactive and Hidden applies.
        let buffer = EditorBuffer::new("cursor here\nThis is __bold__ here.");
        let hidden_highlighter = MarkdownHighlighter::with_config(MarkdownConfig {
            conceal_mode: ConcealMode::Hidden,
            ..Default::default()
        });

        let spans = hidden_highlighter.highlight_line(&buffer, 1, "This is __bold__ here.");
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].range, 8..10);
        assert_eq!(spans[0].style, StyleValue::Tag(HighlightTag::Hidden));
        assert_eq!(spans[1].range, 10..14);
        assert_eq!(spans[1].style, StyleValue::Tag(HighlightTag::Bold));
        assert_eq!(spans[2].range, 14..16);
        assert_eq!(spans[2].style, StyleValue::Tag(HighlightTag::Hidden));
        let concealed = ConcealedLine::build("This is __bold__ here.", &spans);
        assert_eq!(concealed.display_text, "This is bold here.");

        // Asterisk form is the control: identical spans, shifted by nothing.
        let buffer_stars = EditorBuffer::new("cursor here\nThis is **bold** here.");
        let spans_stars =
            hidden_highlighter.highlight_line(&buffer_stars, 1, "This is **bold** here.");
        assert_eq!(spans_stars, spans);

        // Cursor row keeps raw delimiters visible only inside the word
        // under the cursor (word-level reveal); elsewhere it conceals.
        let buffer_plain = EditorBuffer::new("This is __bold__ here.\nother");
        let mut buffer_word = EditorBuffer::new("This is __bold__ here.\nother");
        buffer_word.set_cursor_offset(10);
        let spans_word =
            hidden_highlighter.highlight_line(&buffer_word, 0, "This is __bold__ here.");
        let concealed_word_active = ConcealedLine::build("This is __bold__ here.", &spans_word);
        assert_eq!(concealed_word_active.display_text, "This is __bold__ here.");

        let spans_plain =
            hidden_highlighter.highlight_line(&buffer_plain, 0, "This is __bold__ here.");
        let concealed_plain = ConcealedLine::build("This is __bold__ here.", &spans_plain);
        assert_eq!(concealed_plain.display_text, "This is bold here.");

        // Intra-word underscores are literal per CommonMark, never bold.
        let buffer_word = EditorBuffer::new("cursor here\nfoo__bar__baz");
        let spans_word = hidden_highlighter.highlight_line(&buffer_word, 1, "foo__bar__baz");
        assert!(
            spans_word
                .iter()
                .all(|s| s.style != StyleValue::Tag(HighlightTag::Bold)),
            "intra-word __ must not parse as bold: {spans_word:?}"
        );
        let concealed_word = ConcealedLine::build("foo__bar__baz", &spans_word);
        assert_eq!(concealed_word.display_text, "foo__bar__baz");
    }

    #[test]
    fn test_markdown_word_level_reveal() {
        // Issue #71: on the cursor row each construct reveals independently.
        // `**alpha**` covers 0..9, `__beta__` covers 16..25.
        let line = "**alpha** plain __beta__";
        let hidden_highlighter = MarkdownHighlighter::with_config(MarkdownConfig {
            conceal_mode: ConcealMode::Hidden,
            ..Default::default()
        });
        let concealed_at = |cursor: usize| {
            let mut buffer = EditorBuffer::new(line);
            buffer.set_cursor_offset(cursor);
            let spans = hidden_highlighter.highlight_line(&buffer, 0, line);
            ConcealedLine::build(line, &spans).display_text
        };

        // Cursor inside the first word reveals only it.
        assert_eq!(concealed_at(0), "**alpha** plain beta");
        assert_eq!(concealed_at(8), "**alpha** plain beta");
        // Cursor just past the closing delimiter has left the word.
        assert_eq!(concealed_at(9), "alpha plain beta");
        // Cursor on plain text reveals nothing.
        assert_eq!(concealed_at(12), "alpha plain beta");
        // Cursor inside the second word reveals only it (`__beta__` is 16..24).
        assert_eq!(concealed_at(16), "alpha plain __beta__");
        assert_eq!(concealed_at(23), "alpha plain __beta__");
        // Cursor just past the closing delimiter has left the word.
        assert_eq!(concealed_at(24), "alpha plain beta");
    }

    #[test]
    fn test_markdown_word_level_link_reveal() {
        // `[A](http://a/x)` covers 0..16, `[B](http://b/y)` covers 21..37.
        let line = "[A](http://a/x) and [B](http://b/y)";
        let hidden_highlighter = MarkdownHighlighter::with_config(MarkdownConfig {
            conceal_mode: ConcealMode::Hidden,
            ..Default::default()
        });
        let concealed_at = |cursor: usize| {
            let mut buffer = EditorBuffer::new(line);
            buffer.set_cursor_offset(cursor);
            let spans = hidden_highlighter.highlight_line(&buffer, 0, line);
            ConcealedLine::build(line, &spans).display_text
        };

        assert_eq!(concealed_at(1), "[A](http://a/x) and B");
        assert_eq!(concealed_at(25), "A and [B](http://b/y)");
        assert_eq!(concealed_at(18), "A and B");
    }

    #[test]
    fn test_markdown_task_marker_reveal() {
        // The checkbox structural tag is always present; the raw `- [ ]`
        // bytes reveal only while the cursor sits on the marker (0..6).
        let line = "- [ ] Task";
        let hidden_highlighter = MarkdownHighlighter::with_config(MarkdownConfig {
            conceal_mode: ConcealMode::Hidden,
            ..Default::default()
        });
        let highlight_at = |cursor: usize| {
            let mut buffer = EditorBuffer::new(line);
            buffer.set_cursor_offset(cursor);
            hidden_highlighter.highlight_line(&buffer, 0, line)
        };

        let spans_marker = highlight_at(2);
        assert!(
            spans_marker
                .iter()
                .any(|s| s.style == StyleValue::Tag(HighlightTag::TaskUnchecked))
        );
        assert!(
            spans_marker
                .iter()
                .all(|s| s.style != StyleValue::Tag(HighlightTag::Hidden))
        );
        assert_eq!(
            ConcealedLine::build(line, &spans_marker).display_text,
            "- [ ] Task"
        );

        let spans_task = highlight_at(8);
        assert!(
            spans_task
                .iter()
                .any(|s| s.style == StyleValue::Tag(HighlightTag::TaskUnchecked))
        );
        assert_eq!(spans_task[1].range, 0..6);
        assert_eq!(spans_task[1].style, StyleValue::Tag(HighlightTag::Hidden));
        assert_eq!(ConcealedLine::build(line, &spans_task).display_text, "Task");
    }

    #[test]
    fn test_markdown_heading_prefix_reveal() {
        // The `# ` prefix (0..2) reveals only while the cursor sits on it;
        // editing the heading text keeps it concealed.
        let line = "# Heading";
        let hidden_highlighter = MarkdownHighlighter::with_config(MarkdownConfig {
            conceal_mode: ConcealMode::Hidden,
            ..Default::default()
        });
        let concealed_at = |cursor: usize| {
            let mut buffer = EditorBuffer::new(line);
            buffer.set_cursor_offset(cursor);
            let spans = hidden_highlighter.highlight_line(&buffer, 0, line);
            ConcealedLine::build(line, &spans).display_text
        };

        assert_eq!(concealed_at(0), "# Heading");
        assert_eq!(concealed_at(1), "# Heading");
        assert_eq!(concealed_at(2), "Heading");
        assert_eq!(concealed_at(5), "Heading");

        // Cursor on another row conceals regardless of column.
        let mut buffer_away = EditorBuffer::new(&format!("{line}\nother"));
        buffer_away.set_cursor_offset(line.len() + 1);
        let spans_away = hidden_highlighter.highlight_line(&buffer_away, 0, line);
        assert_eq!(
            ConcealedLine::build(line, &spans_away).display_text,
            "Heading"
        );
    }

    #[test]
    fn test_markdown_bare_heading_prefix_scales() {
        // Typing `# ` leaves the cursor just past the prefix; the line must
        // already carry its Heading tag so metrics (and the caret) scale
        // before the first content character arrives.
        let line = "# ";
        let hidden_highlighter = MarkdownHighlighter::with_config(MarkdownConfig {
            conceal_mode: ConcealMode::Hidden,
            ..Default::default()
        });
        let mut buffer = EditorBuffer::new(line);
        buffer.set_cursor_offset(2);
        let spans = hidden_highlighter.highlight_line(&buffer, 0, line);
        assert!(
            spans
                .iter()
                .any(|s| s.style == StyleValue::Tag(HighlightTag::Heading(1))),
            "bare `# ` must emit Heading(1): {spans:?}"
        );
        assert_eq!(ConcealedLine::build(line, &spans).display_text, "");
    }

    #[test]
    fn test_markdown_mark_concealment() {
        // Issue #53: `==mark==` conceals like emphasis with a Highlight tag.
        let line = "This is ==mark== here.";
        let hidden_highlighter = MarkdownHighlighter::with_config(MarkdownConfig {
            conceal_mode: ConcealMode::Hidden,
            ..Default::default()
        });
        let mut buffer = EditorBuffer::new(&format!("{line}\nother"));
        buffer.set_cursor_offset(line.len() + 1);
        let spans = hidden_highlighter.highlight_line(&buffer, 0, line);
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].range, 8..10);
        assert_eq!(spans[0].style, StyleValue::Tag(HighlightTag::Hidden));
        assert_eq!(spans[1].range, 10..14);
        assert_eq!(spans[1].style, StyleValue::Tag(HighlightTag::Highlight));
        assert_eq!(spans[2].range, 14..16);
        assert_eq!(spans[2].style, StyleValue::Tag(HighlightTag::Hidden));
        assert_eq!(
            ConcealedLine::build(line, &spans).display_text,
            "This is mark here."
        );
    }

    #[test]
    fn test_markdown_mark_word_level_reveal() {
        // `==a==` covers 0..5, `==b==` covers 12..17.
        let line = "==a== plain ==b==";
        let hidden_highlighter = MarkdownHighlighter::with_config(MarkdownConfig {
            conceal_mode: ConcealMode::Hidden,
            ..Default::default()
        });
        let concealed_at = |cursor: usize| {
            let mut buffer = EditorBuffer::new(line);
            buffer.set_cursor_offset(cursor);
            let spans = hidden_highlighter.highlight_line(&buffer, 0, line);
            ConcealedLine::build(line, &spans).display_text
        };

        assert_eq!(concealed_at(2), "==a== plain b");
        assert_eq!(concealed_at(5), "a plain b");
        assert_eq!(concealed_at(8), "a plain b");
        assert_eq!(concealed_at(14), "a plain ==b==");
    }

    #[test]
    fn test_markdown_mark_literal_cases() {
        // Unclosed, empty, spaced, and code-embedded `==` stay literal with
        // no Highlight tag.
        let hidden_highlighter = MarkdownHighlighter::with_config(MarkdownConfig {
            conceal_mode: ConcealMode::Hidden,
            ..Default::default()
        });
        for line in ["==open", "====", "== =="] {
            let mut buffer = EditorBuffer::new(&format!("{line}\nother"));
            buffer.set_cursor_offset(line.len() + 1);
            let spans = hidden_highlighter.highlight_line(&buffer, 0, line);
            assert!(
                spans
                    .iter()
                    .all(|s| s.style != StyleValue::Tag(HighlightTag::Highlight)),
                "{line:?} must not highlight: {spans:?}"
            );
            assert_eq!(ConcealedLine::build(line, &spans).display_text, line);
        }

        // Backticks still conceal while the `==` inside stays literal.
        let code = "`==code==` here";
        let mut buffer = EditorBuffer::new(&format!("{code}\nother"));
        buffer.set_cursor_offset(code.len() + 1);
        let spans = hidden_highlighter.highlight_line(&buffer, 0, code);
        assert!(
            spans
                .iter()
                .all(|s| s.style != StyleValue::Tag(HighlightTag::Highlight)),
            "code mark must not highlight: {spans:?}"
        );
        assert_eq!(
            ConcealedLine::build(code, &spans).display_text,
            "==code== here"
        );

        // `==` inside a link URL overlaps the concealed URL span and stays
        // literal while the link itself still conceals to its label.
        let link = "[text](http://example.com/==path==)";
        let mut buffer = EditorBuffer::new(&format!("{link}\nother"));
        buffer.set_cursor_offset(link.len() + 1);
        let spans = hidden_highlighter.highlight_line(&buffer, 0, link);
        assert!(
            spans
                .iter()
                .all(|s| s.style != StyleValue::Tag(HighlightTag::Highlight)),
            "URL mark must not highlight: {spans:?}"
        );
        assert_eq!(ConcealedLine::build(link, &spans).display_text, "text");
    }

    #[test]
    fn test_markdown_mark_nests_in_bold() {
        // `==` inside emphasis overlaps only Bold spans, so it highlights.
        let line = "**a ==b== c**";
        let hidden_highlighter = MarkdownHighlighter::with_config(MarkdownConfig {
            conceal_mode: ConcealMode::Hidden,
            ..Default::default()
        });
        let mut buffer = EditorBuffer::new(&format!("{line}\nother"));
        buffer.set_cursor_offset(line.len() + 1);
        let spans = hidden_highlighter.highlight_line(&buffer, 0, line);
        assert!(
            spans
                .iter()
                .any(|s| s.style == StyleValue::Tag(HighlightTag::Highlight)),
            "nested mark must highlight: {spans:?}"
        );
        assert!(
            spans
                .iter()
                .any(|s| s.style == StyleValue::Tag(HighlightTag::Bold)),
            "outer bold must survive: {spans:?}"
        );
        assert_eq!(ConcealedLine::build(line, &spans).display_text, "a b c");
    }

    #[test]
    fn test_markdown_frontmatter_dims_as_metadata() {
        // Issue #55: a leading `---` block is metadata, not a rule.
        let lines = ["---", "title: [a](http://x)", "tags: `a`", "---", "body"];
        let text = lines.join("\n");
        let hidden_highlighter = MarkdownHighlighter::with_config(MarkdownConfig {
            conceal_mode: ConcealMode::Hidden,
            ..Default::default()
        });
        // Cursor on the body row; rows 0..=3 are all inactive.
        let mut buffer = EditorBuffer::new(&text);
        buffer.set_cursor_offset(text.len());

        for (row, line) in lines.iter().enumerate().take(4) {
            let spans = hidden_highlighter.highlight_line(&buffer, row, line);
            assert_eq!(spans.len(), 1, "row {row} must dim wholly: {spans:?}");
            assert_eq!(spans[0].range, 0..line.len());
            assert_eq!(spans[0].style, StyleValue::Tag(HighlightTag::Dimmed));
            assert_eq!(ConcealedLine::build(line, &spans).display_text, *line);
        }

        // YAML shaped like a link or code never becomes either.
        let links = hidden_highlighter.extract_links(&buffer, 1, lines[1]);
        assert!(links.is_empty(), "yaml links must not extract: {links:?}");

        // The body row is untouched plain text.
        let body = hidden_highlighter.highlight_line(&buffer, 4, lines[4]);
        assert!(body.is_empty());
    }

    #[test]
    fn test_markdown_frontmatter_fallbacks() {
        // One highlighter per buffer: the version-keyed caches assume a
        // single document per highlighter instance.
        let hidden_highlighter = || {
            MarkdownHighlighter::with_config(MarkdownConfig {
                conceal_mode: ConcealMode::Hidden,
                ..Default::default()
            })
        };

        // Unclosed opener keeps today's horizontal-rule rendering.
        let mut unclosed = EditorBuffer::new("---\ntitle: x");
        unclosed.set_cursor_offset(0);
        let spans = hidden_highlighter().highlight_line(&unclosed, 0, "---");
        assert!(
            spans
                .iter()
                .any(|s| s.style == StyleValue::Tag(HighlightTag::HorizontalRule)),
            "unclosed fence stays a rule: {spans:?}"
        );

        // `...` closes the block too.
        let mut dotted = EditorBuffer::new("---\ntitle: x\n...\nbody");
        dotted.set_cursor_offset(11);
        let closer = hidden_highlighter().highlight_line(&dotted, 2, "...");
        assert_eq!(closer.len(), 1);
        assert_eq!(closer[0].style, StyleValue::Tag(HighlightTag::Dimmed));

        // `---` past row 0 is still a rule.
        let mut mid = EditorBuffer::new("body\n---\nmore");
        mid.set_cursor_offset(0);
        let rule = hidden_highlighter().highlight_line(&mid, 1, "---");
        assert!(
            rule.iter()
                .any(|s| s.style == StyleValue::Tag(HighlightTag::HorizontalRule)),
            "mid-document fence stays a rule: {rule:?}"
        );

        // The cursor row inside the block dims exactly like inactive rows.
        let mut active = EditorBuffer::new("---\ntitle: x\n---\nbody");
        active.set_cursor_offset(5);
        let row = hidden_highlighter().highlight_line(&active, 1, "title: x");
        assert_eq!(row.len(), 1);
        assert_eq!(row[0].style, StyleValue::Tag(HighlightTag::Dimmed));
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

    #[test]
    fn test_large_table_body_rows_stay_detected() {
        // The cached block lookup must keep working hundreds of rows deep in
        // one table (a per-row upward walk with a bounded budget would give
        // up before reaching the delimiter and drop table styling mid-table).
        let mut s = String::from("| a | b |\n| --- | --- |\n");
        for i in 0..700 {
            s.push_str(&format!("| c{i} | d |\n"));
        }
        let buffer = EditorBuffer::new(&s);
        let highlighter = MarkdownHighlighter::new();

        for row in [10usize, 300, 699, 701] {
            let line = buffer.line_to_string(row);
            let text = line.trim_end_matches(['\r', '\n']);
            let spans = highlighter.highlight_line(&buffer, row, text);
            assert!(
                spans
                    .iter()
                    .any(|sp| sp.style == StyleValue::Tag(HighlightTag::Custom(TABLE_CELL_TAG))),
                "deep body row {row} must keep its table cell tag"
            );
            assert!(
                !highlighter.should_wrap_line(&buffer, row),
                "deep body row {row} must opt out of wrapping"
            );
            let concealed = ConcealedLine::build(text, &spans);
            assert!(
                !highlighter.expand_line(&buffer, row, &concealed).is_empty(),
                "deep body row {row} must get alignment padding"
            );
        }
    }
    #[test]
    fn test_deep_table_rows_highlight_with_fence_at_top() {
        // Guards the version-cached fence index: a fence pair far above must
        // not hide a real table 1000+ lines below, and fenced pipe rows must
        // still highlight as code, not table cells.
        let mut s = String::from("```rust\nfn f() {}\n```\n");
        for i in 0..1000 {
            s.push_str(&format!("plain filler line {i}\n"));
        }
        let header_row = 1003;
        s.push_str("| a | b |\n| --- | --- |\n| c | d |\n");
        s.push_str("```\n| x |\n| --- |\n```\n");
        let buffer = EditorBuffer::new(&s);
        let highlighter = MarkdownHighlighter::new();

        let header = highlighter.highlight_line(&buffer, header_row, "| a | b |");
        assert!(
            header
                .iter()
                .any(|sp| sp.style == StyleValue::Tag(HighlightTag::Custom(TABLE_HEADER_TAG))),
            "deep header row must keep its table tag"
        );
        assert!(
            header
                .iter()
                .any(|sp| sp.style == StyleValue::Tag(HighlightTag::Bold)),
            "deep header cells must stay bold"
        );
        let body = highlighter.highlight_line(&buffer, header_row + 2, "| c | d |");
        assert!(
            body.iter()
                .any(|sp| sp.style == StyleValue::Tag(HighlightTag::Custom(TABLE_CELL_TAG)))
        );
        assert!(!highlighter.should_wrap_line(&buffer, header_row));
        assert!(!highlighter.should_wrap_line(&buffer, header_row + 2));

        // Pipe rows inside the trailing fence are code, never table cells.
        let total = buffer.len_lines();
        let fenced_pipe_row = total - 3;
        let fenced = highlighter.highlight_line(&buffer, fenced_pipe_row, "| x |");
        assert!(
            fenced
                .iter()
                .any(|sp| sp.style == StyleValue::Tag(HighlightTag::Code)),
            "fenced pipe row must highlight as code"
        );
        assert!(
            fenced
                .iter()
                .all(|sp| sp.style != StyleValue::Tag(HighlightTag::Custom(TABLE_CELL_TAG))),
            "fenced pipe row must not emit table cell tags"
        );
    }

    #[test]
    fn test_markdown_callout_header() {
        // Issue #52: `> [!NOTE] Title` tags the kind, conceals the marker,
        // and bolds the title.
        let line = "> [!NOTE] Title here";
        let hidden_highlighter = MarkdownHighlighter::with_config(MarkdownConfig {
            conceal_mode: ConcealMode::Hidden,
            ..Default::default()
        });
        // Cursor past the line so row 0 is inactive.
        let mut buffer = EditorBuffer::new(&format!("{line}\nother"));
        buffer.set_cursor_offset(line.len() + 1);
        let spans = hidden_highlighter.highlight_line(&buffer, 0, line);

        assert!(
            spans
                .iter()
                .any(|s| s.style == StyleValue::Tag(HighlightTag::Callout(CalloutKind::Note))),
            "header must tag the kind: {spans:?}"
        );
        let marker = spans
            .iter()
            .find(|s| s.style == StyleValue::Tag(HighlightTag::Hidden) && s.range.start == 2)
            .expect("marker must conceal");
        assert_eq!(marker.range, 2..10, "marker covers `[!NOTE] `");
        let title = spans
            .iter()
            .find(|s| s.style == StyleValue::Tag(HighlightTag::Bold))
            .expect("title must bold");
        assert_eq!(title.range, 10..20);
        assert_eq!(
            ConcealedLine::build(line, &spans).display_text,
            "Title here"
        );
    }

    #[test]
    fn test_markdown_callout_kinds_and_fallback() {
        let hidden_highlighter = MarkdownHighlighter::with_config(MarkdownConfig {
            conceal_mode: ConcealMode::Hidden,
            ..Default::default()
        });
        for (marker, expected) in [
            ("NOTE", CalloutKind::Note),
            ("TIP", CalloutKind::Tip),
            ("WARNING", CalloutKind::Warning),
            ("CAUTION", CalloutKind::Caution),
            ("IMPORTANT", CalloutKind::Important),
            ("note", CalloutKind::Note),
            ("WHATEVER", CalloutKind::Other),
        ] {
            let line = format!("> [!{marker}] T");
            let mut buffer = EditorBuffer::new(&format!("{line}\nother"));
            buffer.set_cursor_offset(line.len() + 1);
            let spans = hidden_highlighter.highlight_line(&buffer, 0, &line);
            assert!(
                spans
                    .iter()
                    .any(|s| s.style == StyleValue::Tag(HighlightTag::Callout(expected))),
                "[{marker}] must tag {expected:?}: {spans:?}"
            );
        }
    }

    #[test]
    fn test_markdown_callout_body_inheritance() {
        // Body `>` rows inherit the header kind; blank and non-quote rows
        // end the block.
        let text = "> [!TIP] Head\n> body one\n> body two\n\nplain\n> quote";
        let hidden_highlighter = MarkdownHighlighter::with_config(MarkdownConfig {
            conceal_mode: ConcealMode::Hidden,
            ..Default::default()
        });
        let mut buffer = EditorBuffer::new(text);
        buffer.set_cursor_offset(text.len());
        let kind_at = |row: usize, line: &str| {
            hidden_highlighter
                .highlight_line(&buffer, row, line)
                .iter()
                .find_map(|s| match s.style {
                    StyleValue::Tag(HighlightTag::Callout(kind)) => Some(kind),
                    _ => None,
                })
        };

        assert_eq!(kind_at(0, "> [!TIP] Head"), Some(CalloutKind::Tip));
        assert_eq!(kind_at(1, "> body one"), Some(CalloutKind::Tip));
        assert_eq!(kind_at(2, "> body two"), Some(CalloutKind::Tip));
        assert_eq!(kind_at(4, "plain"), None);
        assert_eq!(kind_at(5, "> quote"), None);
    }

    #[test]
    fn test_markdown_callout_marker_reveal() {
        // The marker reveals only while the cursor sits on it; the title
        // stays concealed from the cursor row otherwise.
        let line = "> [!WARNING] Careful";
        let hidden_highlighter = MarkdownHighlighter::with_config(MarkdownConfig {
            conceal_mode: ConcealMode::Hidden,
            ..Default::default()
        });
        let concealed_at = |cursor: usize| {
            let mut buffer = EditorBuffer::new(line);
            buffer.set_cursor_offset(cursor);
            let spans = hidden_highlighter.highlight_line(&buffer, 0, line);
            ConcealedLine::build(line, &spans).display_text
        };

        // Marker covers 2..13 (`[!WARNING] ` with the trailing space).
        // Cursor on the `>` prefix reveals it; cursor in the marker reveals
        // the marker; anywhere else conceals both.
        assert_eq!(concealed_at(4), "[!WARNING] Careful");
        assert_eq!(concealed_at(13), "Careful");
        assert_eq!(concealed_at(0), "> Careful");
    }

    #[test]
    fn test_markdown_callout_titleless_header() {
        // A header without title conceals to an empty row; the bar remains.
        let line = "> [!CAUTION]";
        let hidden_highlighter = MarkdownHighlighter::with_config(MarkdownConfig {
            conceal_mode: ConcealMode::Hidden,
            ..Default::default()
        });
        let mut buffer = EditorBuffer::new(&format!("{line}\nother"));
        buffer.set_cursor_offset(line.len() + 1);
        let spans = hidden_highlighter.highlight_line(&buffer, 0, line);
        assert!(
            spans
                .iter()
                .any(|s| s.style == StyleValue::Tag(HighlightTag::Callout(CalloutKind::Caution))),
            "titleless header must tag the kind: {spans:?}"
        );
        assert_eq!(ConcealedLine::build(line, &spans).display_text, "");
    }

    #[test]
    fn test_markdown_quote_prefix_word_level() {
        // The `>` prefix reveals only while the cursor sits on it, even on
        // the cursor row.
        let line = "> quote";
        let hidden_highlighter = MarkdownHighlighter::with_config(MarkdownConfig {
            conceal_mode: ConcealMode::Hidden,
            ..Default::default()
        });
        let concealed_at = |cursor: usize| {
            let mut buffer = EditorBuffer::new(line);
            buffer.set_cursor_offset(cursor);
            let spans = hidden_highlighter.highlight_line(&buffer, 0, line);
            ConcealedLine::build(line, &spans).display_text
        };

        assert_eq!(concealed_at(0), "> quote");
        assert_eq!(concealed_at(1), "> quote");
        assert_eq!(concealed_at(2), "quote");
        assert_eq!(concealed_at(5), "quote");
    }

    #[test]
    fn test_markdown_callout_body_dims() {
        // Body text dims to quote gray under inline spans; the title keeps
        // its bold styling.
        let text = "> [!TIP] Head\n> body **bold**";
        let hidden_highlighter = MarkdownHighlighter::with_config(MarkdownConfig {
            conceal_mode: ConcealMode::Hidden,
            ..Default::default()
        });
        let mut buffer = EditorBuffer::new(text);
        buffer.set_cursor_offset(text.len());

        let body = hidden_highlighter.highlight_line(&buffer, 1, "> body **bold**");
        let dim = body
            .iter()
            .find(|s| s.style == StyleValue::Tag(HighlightTag::Comment))
            .expect("body text must dim");
        assert_eq!(dim.range, 2..15);
        assert!(
            body.iter()
                .any(|s| s.style == StyleValue::Tag(HighlightTag::Bold)),
            "inline bold must survive over the dim base: {body:?}"
        );
        assert_eq!(
            ConcealedLine::build("> body **bold**", &body).display_text,
            "body bold"
        );
    }
}
