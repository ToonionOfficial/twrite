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
}

impl Default for MarkdownConfig {
    fn default() -> Self {
        Self {
            conceal_mode: ConcealMode::Dimmed,
            visual_thematic_breaks: true,
            interactive_tasks: true,
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
            Some((2, HighlightTag::Heading1))
        } else if trimmed_start.starts_with("## ") {
            Some((3, HighlightTag::Heading2))
        } else if trimmed_start.starts_with("### ") {
            Some((4, HighlightTag::Heading3))
        } else if trimmed_start.starts_with("#### ") {
            Some((5, HighlightTag::Heading4))
        } else if trimmed_start.starts_with("##### ") {
            Some((6, HighlightTag::Heading5))
        } else if trimmed_start.starts_with("###### ") {
            Some((7, HighlightTag::Heading6))
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
        }
    }

    /// Creates a hook honoring the given Markdown configuration.
    pub fn with_config(config: MarkdownConfig) -> Self {
        Self {
            interactive_tasks: config.interactive_tasks,
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
        assert_eq!(spans1[0].style, StyleValue::Tag(HighlightTag::Heading1));

        let spans2 = highlighter.highlight_line(&buffer, 1, "## Heading 2");
        assert_eq!(spans2.len(), 2);
        assert_eq!(spans2[0].style, StyleValue::Tag(HighlightTag::Dimmed));
        assert_eq!(spans2[1].style, StyleValue::Tag(HighlightTag::Heading2));

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
            StyleValue::Tag(HighlightTag::Heading2)
        );

        let off_highlighter = MarkdownHighlighter::with_config(MarkdownConfig {
            conceal_mode: ConcealMode::Off,
            ..Default::default()
        });
        let spans_off = off_highlighter.highlight_line(&buffer, 1, "## Heading 2");
        assert_eq!(spans_off.len(), 1);
        assert_eq!(spans_off[0].style, StyleValue::Tag(HighlightTag::Heading2));
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
}
