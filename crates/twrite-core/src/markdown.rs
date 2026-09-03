use std::sync::{Arc, RwLock};

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

use crate::{
    EditorBuffer, EditorHook, HighlightTag, HookContext, HookOutcome, KeyEvent, Point, Selection,
    StyleSpan, SyntaxHighlighter, TextStyle,
};

/// Cached fence line row indices and associated document version.
type FenceCache = Arc<RwLock<Option<(usize, Vec<usize>)>>>;

/// A syntax highlighter for CommonMark and GFM Markdown documents using `pulldown-cmark`.
#[derive(Debug, Clone, Default)]
pub struct MarkdownHighlighter {
    cached_fences: FenceCache,
}

impl MarkdownHighlighter {
    /// Creates a new Markdown syntax highlighter.
    pub fn new() -> Self {
        Self {
            cached_fences: Arc::new(RwLock::new(None)),
        }
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

        if trimmed_start.starts_with("```") || trimmed_start.starts_with("~~~") {
            spans.push(StyleSpan::tag(0..line_text.len(), HighlightTag::Code));
            return spans;
        }

        if self.is_in_fenced_code_block(buffer, row) {
            spans.push(StyleSpan::tag(0..line_text.len(), HighlightTag::Code));
            return spans;
        }

        let is_cursor_row = row == buffer.cursor_point().row;

        let trimmed_break = trimmed_start.trim_end();
        if (trimmed_break == "---" || trimmed_break == "***" || trimmed_break == "___")
            && line_text.len() >= 3
        {
            spans.push(StyleSpan::tag(0..line_text.len(), HighlightTag::Dimmed));
            return spans;
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
            if !is_cursor_row {
                spans.push(StyleSpan::tag(
                    indent..indent + prefix_len,
                    HighlightTag::Dimmed,
                ));
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
            spans.push(StyleSpan::tag(indent..indent + 1, HighlightTag::Comment));
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
                            if !is_cursor_row && end >= start + 4 {
                                spans.push(StyleSpan::tag(start..start + 2, HighlightTag::Dimmed));
                                spans.push(StyleSpan::tag(start + 2..end - 2, HighlightTag::Bold));
                                spans.push(StyleSpan::tag(end - 2..end, HighlightTag::Dimmed));
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
                            if !is_cursor_row && end >= start + 2 {
                                spans.push(StyleSpan::tag(start..start + 1, HighlightTag::Dimmed));
                                spans
                                    .push(StyleSpan::tag(start + 1..end - 1, HighlightTag::Italic));
                                spans.push(StyleSpan::tag(end - 1..end, HighlightTag::Dimmed));
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
                            if !is_cursor_row && end >= start + 4 {
                                spans.push(StyleSpan::tag(start..start + 2, HighlightTag::Dimmed));
                                spans.push(StyleSpan::direct(
                                    start + 2..end - 2,
                                    TextStyle {
                                        strikethrough: true,
                                        ..Default::default()
                                    },
                                ));
                                spans.push(StyleSpan::tag(end - 2..end, HighlightTag::Dimmed));
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
                    if !is_cursor_row && end >= range.start + 2 {
                        spans.push(StyleSpan::tag(
                            range.start..range.start + 1,
                            HighlightTag::Dimmed,
                        ));
                        spans.push(StyleSpan::tag(range.start + 1..end - 1, HighlightTag::Code));
                        spans.push(StyleSpan::tag(end - 1..end, HighlightTag::Dimmed));
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
                            spans.push(StyleSpan::tag(start..end, HighlightTag::Link));
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
}

/// An editor hook providing Markdown shortcuts (Ctrl+B, Ctrl+I, Ctrl+K), smart list continuation, and task list toggles.
#[derive(Debug, Clone, Default)]
pub struct MarkdownHook;

impl MarkdownHook {
    /// Creates a new Markdown editing hook.
    pub fn new() -> Self {
        Self
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

    fn status_text(&self) -> Option<&str> {
        Some("MARKDOWN")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StyleValue;

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
        assert_eq!(spans4.len(), 1);
        assert_eq!(spans4[0].style, StyleValue::Tag(HighlightTag::Dimmed));
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
}
