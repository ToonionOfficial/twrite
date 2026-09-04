use crate::{EditorHook, HookContext, HookOutcome, KeyEvent, Point, Selection};

use super::config::MarkdownConfig;
use super::table::{
    TableRowKind, clean_table_line, find_unescaped_pipes, split_table_cells, table_block_at,
};

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
            ("* [x] ", "- [ ] "),
            ("* [X] ", "- [ ] "),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EditorBuffer, HookContext, HookOutcome, KeyEvent, Selection};

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
