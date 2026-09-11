//! Getting-started example 6 of 6: a full modal Vim system on hooks alone.
//!
//! Read last, after `hooks` and `prompt`. Zero Vim code lives in the engine:
//! everything is one `EditorHook`.
//!
//! Reading guide: `VimHook` state first (`mode`, `pending_key` for `dd`/`gg`,
//! owned `SearchHook`, one-shot `status_override`), then the `on_key`
//! dispatch order (shared prompt, `escape`, mode arms), then `execute_ex` /
//! `execute_substitute`, finally the `AppView` shell and `main` at the bottom.
//!
//! Run with: `cargo run --example vim`
use gpui::*;
use gpui_platform::application;
use twrite::{
    CharKind, CursorStyle, Editor, EditorHook, HookContext, HookEffect, HookOutcome, KeyEvent,
    Point, PromptAction, PromptPlacement, PromptSpec, SEARCH_PROMPT_ID, SearchHook, SearchQuery,
    SearchSnapshot, Selection, collect_replacements,
};

/// The operating mode of the Vim state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum VimMode {
    #[default]
    Normal,
    Insert,
    Visual,
}

/// An editor hook implementing modal Vim keybindings (Normal, Insert, and Visual modes).
#[derive(Default)]
struct VimHook {
    mode: VimMode,
    pending_key: Option<char>,
    /// Whether Visual mode is linewise (`V`) rather than charwise (`v`).
    visual_linewise: bool,
    search: SearchHook,
    /// Forward (`/`) or backward (`?`) for `n` / `N`.
    search_backward: bool,
    /// One-shot status line (search counts, `:w` feedback, `E...` errors).
    status_override: Option<String>,
}

impl VimHook {
    fn new() -> Self {
        Self {
            mode: VimMode::Normal,
            pending_key: None,
            visual_linewise: false,
            search: SearchHook::new(),
            search_backward: false,
            status_override: None,
        }
    }

    fn enter_normal_mode(&mut self, ctx: &mut HookContext) {
        self.mode = VimMode::Normal;
        self.pending_key = None;
        self.visual_linewise = false;
        *ctx.cursor_style = CursorStyle::Block;
        *ctx.selection = None;
    }

    fn enter_insert_mode(&mut self, ctx: &mut HookContext) {
        self.mode = VimMode::Insert;
        self.pending_key = None;
        self.visual_linewise = false;
        *ctx.cursor_style = CursorStyle::Bar;
        *ctx.selection = None;
    }

    fn enter_visual_mode(&mut self, ctx: &mut HookContext) {
        self.mode = VimMode::Visual;
        self.pending_key = None;
        self.visual_linewise = false;
        *ctx.cursor_style = CursorStyle::Block;
        let cursor = ctx.buffer.cursor_offset();
        *ctx.selection = Some(Selection::range(
            cursor,
            (cursor + 1).min(ctx.buffer.len_bytes()),
        ));
    }

    /// Enters linewise Visual mode: the whole current line (including its
    /// terminator) is selected, so `d` deletes lines and motions grow by line.
    fn enter_visual_line_mode(&mut self, ctx: &mut HookContext) {
        self.mode = VimMode::Visual;
        self.pending_key = None;
        self.visual_linewise = true;
        *ctx.cursor_style = CursorStyle::Block;
        let cursor = ctx.buffer.cursor_offset();
        let line = ctx.buffer.line_range_at(cursor);
        // Cursor rests at the line start so `j`/`k` advance exactly one line.
        ctx.buffer.set_cursor_offset(line.start);
        *ctx.selection = Some(Selection::range(line.start, line.end));
    }

    /// Expands the visual selection to whole lines, preserving direction.
    fn snap_visual_to_lines(&self, ctx: &mut HookContext) {
        if let Some(sel) = ctx.selection.take() {
            let anchor_line = ctx.buffer.line_range_at(sel.anchor);
            let head_line = ctx.buffer.line_range_at(sel.head);
            let (anchor, head) = if sel.anchor <= sel.head {
                (anchor_line.start, head_line.end)
            } else {
                (anchor_line.end, head_line.start)
            };
            ctx.buffer.set_cursor_offset(head);
            *ctx.selection = Some(Selection::range(anchor, head));
        }
    }

    fn move_visual(&mut self, ctx: &mut HookContext, new_head: usize) {
        if let Some(sel) = ctx.selection.take() {
            ctx.buffer.set_cursor_offset(new_head);
            *ctx.selection = Some(Selection::range(sel.anchor, new_head));
        }
        if self.visual_linewise {
            self.snap_visual_to_lines(ctx);
        }
    }

    /// Mirrors the owned [`SearchHook`] status into the one-shot line.
    fn sync_search_status(&mut self) {
        self.status_override = self.search.status_text().map(|s| s.to_string());
    }

    /// Keyword under the cursor (`*` / `#`), if any.
    fn word_under_cursor(ctx: &HookContext) -> Option<String> {
        let cursor = ctx.buffer.cursor_offset();
        let text = ctx.buffer.text();
        if cursor >= text.len_bytes() {
            return None;
        }
        let char_idx = text.byte_to_char(cursor);
        if twrite::classify_char(text.char(char_idx)) != CharKind::Word {
            return None;
        }
        let start = ctx.buffer.prev_word_offset();
        let end = ctx.buffer.next_word_offset();
        if start >= end {
            return None;
        }
        Some(text.byte_slice(start..end).to_string())
    }

    /// Splits `s/old/new/flags` bodies on unescaped `/` (`\/` and `\\` unescape;
    /// other backslash sequences are kept verbatim for regexes).
    fn split_ex_parts(body: &str) -> Vec<String> {
        let mut parts = vec![String::new()];
        let mut chars = body.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.next() {
                    Some('/') => parts.last_mut().unwrap().push('/'),
                    Some('\\') => parts.last_mut().unwrap().push('\\'),
                    Some(n) => {
                        parts.last_mut().unwrap().push('\\');
                        parts.last_mut().unwrap().push(n);
                    }
                    None => parts.last_mut().unwrap().push('\\'),
                }
            } else if c == '/' {
                parts.push(String::new());
            } else {
                parts.last_mut().unwrap().push(c);
            }
        }
        parts
    }

    fn handle_ex_key(&mut self, ctx: &mut HookContext, event: &KeyEvent) -> HookOutcome {
        match ctx.prompt.handle_key(event) {
            PromptAction::Editing => HookOutcome::Consumed,
            PromptAction::Submitted(input) => {
                let cmd = input.trim().to_string();
                self.execute_ex(ctx, &cmd);
                HookOutcome::Consumed
            }
            PromptAction::Cancelled => {
                self.status_override = None;
                HookOutcome::Consumed
            }
            PromptAction::Ignored => HookOutcome::Consumed,
        }
    }

    fn execute_ex(&mut self, ctx: &mut HookContext, cmd: &str) {
        ctx.prompt.close();
        if cmd.is_empty() {
            self.status_override = None;
            return;
        }
        // :<num> goes to a line.
        if let Ok(num) = cmd.parse::<usize>()
            && num >= 1
            && num <= ctx.buffer.len_lines()
        {
            let target = ctx.buffer.point_to_offset(Point::new(num - 1, 0));
            ctx.buffer.set_cursor_offset(target);
            *ctx.selection = None;
            self.status_override = None;
            return;
        }
        // File / lifecycle commands.
        let (head, rest) = match cmd.find([' ', '\t']) {
            Some(i) => (&cmd[..i], cmd[i..].trim()),
            None => (cmd, ""),
        };
        match head {
            "w" => {
                ctx.effects.push(HookEffect::Save {
                    path: if rest.is_empty() {
                        None
                    } else {
                        Some(rest.to_string())
                    },
                });
                self.status_override = None;
            }
            "q" => {
                ctx.effects.push(HookEffect::Quit { force: false });
                self.status_override = None;
            }
            "q!" => {
                ctx.effects.push(HookEffect::Quit { force: true });
                self.status_override = None;
            }
            "wq" | "x" => {
                ctx.effects.push(HookEffect::Save {
                    path: if rest.is_empty() {
                        None
                    } else {
                        Some(rest.to_string())
                    },
                });
                ctx.effects.push(HookEffect::Quit { force: false });
                self.status_override = None;
            }
            "e" => {
                if rest.is_empty() {
                    self.status_override = Some("E471: Argument required".to_string());
                } else {
                    ctx.effects.push(HookEffect::Load {
                        path: rest.to_string(),
                    });
                    self.status_override = None;
                }
            }
            _ if cmd.starts_with('s') || cmd.starts_with("%s") => self.execute_substitute(ctx, cmd),
            _ => {
                self.status_override = Some(format!("E492: Not an editor command: {cmd}"));
            }
        }
    }

    fn execute_substitute(&mut self, ctx: &mut HookContext, cmd: &str) {
        let (whole, rest) = match cmd.strip_prefix("%s") {
            Some(rest) => (true, rest),
            None => match cmd.strip_prefix('s') {
                Some(rest) => (false, rest),
                None => {
                    self.status_override = Some(format!("E492: Not an editor command: {cmd}"));
                    return;
                }
            },
        };
        let Some(body) = rest.strip_prefix('/') else {
            self.status_override = Some("E488: Trailing characters".to_string());
            return;
        };
        let parts = Self::split_ex_parts(body);
        if parts.len() < 2 || parts.len() > 3 || parts[1..].iter().any(|p| p.contains('\n')) {
            self.status_override = Some("E488: Trailing characters".to_string());
            return;
        }
        let mut old = parts[0].clone();
        let new = parts[1].clone();
        let flags = parts.get(2).cloned().unwrap_or_default();
        if old.is_empty() {
            if self.search.query_text().is_empty() {
                self.status_override = Some("E35: No previous regular expression".to_string());
                return;
            }
            old = self.search.query_text().to_string();
        }
        let mut global = false;
        let mut insensitive = false;
        for flag in flags.chars() {
            match flag {
                'g' => global = true,
                'i' => insensitive = true,
                _ => {
                    self.status_override = Some("E488: Trailing characters".to_string());
                    return;
                }
            }
        }

        let query = SearchQuery::new(&old, !insensitive, false, true);
        let (scope_text, base) = if whole {
            (ctx.buffer.text().to_string(), 0)
        } else {
            let row = ctx.buffer.cursor_point().row;
            (
                ctx.buffer.line_to_string(row),
                ctx.buffer.point_to_offset(Point::new(row, 0)),
            )
        };
        let mut replacements = match collect_replacements(&scope_text, &query, &new) {
            Ok(replacements) => replacements,
            Err(e) => {
                self.status_override = Some(e.to_string());
                return;
            }
        };
        for (range, _) in &mut replacements {
            range.start += base;
            range.end += base;
        }
        if !global {
            replacements.truncate(1);
        }
        if replacements.is_empty() {
            self.status_override = Some(format!("E486: Pattern not found: {old}"));
            return;
        }
        let rows = replacements
            .iter()
            .map(|(r, _)| ctx.buffer.offset_to_point(r.start).row)
            .collect::<std::collections::HashSet<_>>()
            .len();
        let n = replacements.len();
        let first = replacements[0].0.start;
        ctx.buffer.replace_many(replacements);
        ctx.buffer.set_cursor_offset(first);
        *ctx.selection = None;
        self.status_override = Some(if rows == 1 && n == 1 {
            "1 substitution on 1 line".to_string()
        } else if rows == 1 {
            format!("{n} substitutions on 1 line")
        } else {
            format!("{n} substitutions on {rows} lines")
        });
    }
}

impl EditorHook for VimHook {
    fn on_key(&mut self, ctx: &mut HookContext, event: &KeyEvent) -> HookOutcome {
        // Shared prompt open: search prompts delegate to the owned SearchHook,
        // `:` ex-commands are handled here. Either way nothing else runs.
        if ctx.prompt.is_open() {
            if ctx.prompt.spec().is_some_and(|s| s.id == SEARCH_PROMPT_ID) {
                let outcome = self.search.on_key(ctx, event);
                self.sync_search_status();
                return outcome;
            }
            return self.handle_ex_key(ctx, event);
        }

        if event.key == "escape" || (event.modifiers.ctrl && event.key == "[") {
            self.enter_normal_mode(ctx);
            self.status_override = None;
            return HookOutcome::Consumed;
        }

        match self.mode {
            VimMode::Insert => HookOutcome::PassThrough,

            VimMode::Visual => {
                // Search function keys / toggles share the owned SearchHook.
                let outcome = self.search.on_key(ctx, event);
                if outcome == HookOutcome::Consumed {
                    self.sync_search_status();
                    return HookOutcome::Consumed;
                }

                // `gg` in Visual mode jumps to the top while extending.
                if let Some(pending) = self.pending_key.take() {
                    match (pending, event.key.as_str()) {
                        ('g', "g") => {
                            self.move_visual(ctx, 0);
                            return HookOutcome::Consumed;
                        }
                        _ => return HookOutcome::Consumed,
                    }
                }

                let cursor = ctx.buffer.cursor_offset();

                match event.key.as_str() {
                    "v" => {
                        self.visual_linewise = false;
                        HookOutcome::Consumed
                    }
                    "V" => {
                        self.visual_linewise = true;
                        self.snap_visual_to_lines(ctx);
                        HookOutcome::Consumed
                    }
                    "G" => {
                        let target = ctx.buffer.len_bytes();
                        self.move_visual(ctx, target);
                        HookOutcome::Consumed
                    }
                    "g" => {
                        self.pending_key = Some('g');
                        HookOutcome::Consumed
                    }
                    "h" | "left" | "arrowleft" => {
                        let target = cursor.saturating_sub(1);
                        self.move_visual(ctx, target);
                        HookOutcome::Consumed
                    }
                    "l" | "right" | "arrowright" => {
                        let target = (cursor + 1).min(ctx.buffer.len_bytes());
                        self.move_visual(ctx, target);
                        HookOutcome::Consumed
                    }
                    "k" | "up" | "arrowup" => {
                        ctx.buffer.move_cursor_up();
                        let target = ctx.buffer.cursor_offset();
                        self.move_visual(ctx, target);
                        HookOutcome::Consumed
                    }
                    "j" | "down" | "arrowdown" => {
                        ctx.buffer.move_cursor_down();
                        let target = ctx.buffer.cursor_offset();
                        self.move_visual(ctx, target);
                        HookOutcome::Consumed
                    }
                    "w" => {
                        let target = ctx.buffer.next_word_offset();
                        self.move_visual(ctx, target);
                        HookOutcome::Consumed
                    }
                    "b" => {
                        let target = ctx.buffer.prev_word_offset();
                        self.move_visual(ctx, target);
                        HookOutcome::Consumed
                    }
                    "0" => {
                        let target = ctx.buffer.line_start_offset();
                        self.move_visual(ctx, target);
                        HookOutcome::Consumed
                    }
                    "$" => {
                        let target = ctx.buffer.line_end_offset();
                        self.move_visual(ctx, target);
                        HookOutcome::Consumed
                    }
                    "d" | "x" => {
                        if let Some(sel) = ctx.selection.take() {
                            let range = sel.byte_range();
                            if !range.is_empty() {
                                ctx.buffer.delete_range(range);
                            }
                        }
                        self.enter_normal_mode(ctx);
                        HookOutcome::Consumed
                    }
                    "y" => {
                        self.enter_normal_mode(ctx);
                        HookOutcome::Consumed
                    }
                    _ => HookOutcome::Consumed,
                }
            }

            VimMode::Normal => {
                if event.modifiers.ctrl {
                    match event.key.as_str() {
                        "r" => {
                            ctx.buffer.redo();
                            return HookOutcome::Consumed;
                        }
                        _ => return HookOutcome::PassThrough,
                    }
                }

                // Search function keys (Ctrl+F / Ctrl+H / F3) share the owned
                // SearchHook; a consumed key abandons any pending operator.
                let outcome = self.search.on_key(ctx, event);
                if outcome == HookOutcome::Consumed {
                    self.pending_key = None;
                    self.sync_search_status();
                    return HookOutcome::Consumed;
                }
                self.status_override = None;

                if let Some(pending) = self.pending_key.take() {
                    match (pending, event.key.as_str()) {
                        ('d', "d") => {
                            let row = ctx.buffer.cursor_point().row;
                            let line_start = ctx.buffer.point_to_offset(Point::new(row, 0));
                            let line_end = if row + 1 < ctx.buffer.len_lines() {
                                ctx.buffer.point_to_offset(Point::new(row + 1, 0))
                            } else {
                                ctx.buffer.len_bytes()
                            };
                            ctx.buffer.delete_range(line_start..line_end);
                            return HookOutcome::Consumed;
                        }
                        ('d', "w") => {
                            ctx.buffer.delete_next_word();
                            return HookOutcome::Consumed;
                        }
                        ('g', "g") => {
                            ctx.buffer.set_cursor_offset(0);
                            return HookOutcome::Consumed;
                        }
                        _ => return HookOutcome::Consumed,
                    }
                }

                match event.key.as_str() {
                    "h" | "left" | "arrowleft" => {
                        ctx.buffer.move_cursor_left();
                        HookOutcome::Consumed
                    }
                    "l" | "right" | "arrowright" => {
                        ctx.buffer.move_cursor_right();
                        HookOutcome::Consumed
                    }
                    "k" | "up" | "arrowup" => {
                        ctx.buffer.move_cursor_up();
                        HookOutcome::Consumed
                    }
                    "j" | "down" | "arrowdown" => {
                        ctx.buffer.move_cursor_down();
                        HookOutcome::Consumed
                    }
                    "w" => {
                        let target = ctx.buffer.next_word_offset();
                        ctx.buffer.set_cursor_offset(target);
                        HookOutcome::Consumed
                    }
                    "b" => {
                        let target = ctx.buffer.prev_word_offset();
                        ctx.buffer.set_cursor_offset(target);
                        HookOutcome::Consumed
                    }
                    "0" => {
                        let target = ctx.buffer.line_start_offset();
                        ctx.buffer.set_cursor_offset(target);
                        HookOutcome::Consumed
                    }
                    "$" => {
                        let target = ctx.buffer.line_end_offset();
                        ctx.buffer.set_cursor_offset(target);
                        HookOutcome::Consumed
                    }
                    "G" => {
                        let target = ctx.buffer.len_bytes();
                        ctx.buffer.set_cursor_offset(target);
                        HookOutcome::Consumed
                    }
                    "/" => {
                        self.pending_key = None;
                        self.search_backward = false;
                        self.search.open_search(ctx, "");
                        self.sync_search_status();
                        HookOutcome::Consumed
                    }
                    "?" => {
                        self.pending_key = None;
                        self.search_backward = true;
                        self.search.open_search(ctx, "");
                        self.sync_search_status();
                        HookOutcome::Consumed
                    }
                    "n" => {
                        self.pending_key = None;
                        if !self.search.query_text().is_empty() {
                            if self.search_backward {
                                self.search.navigate_prev(ctx, true);
                            } else {
                                self.search.navigate_next(ctx, true);
                            }
                            self.sync_search_status();
                        }
                        HookOutcome::Consumed
                    }
                    "N" => {
                        self.pending_key = None;
                        if !self.search.query_text().is_empty() {
                            if self.search_backward {
                                self.search.navigate_next(ctx, true);
                            } else {
                                self.search.navigate_prev(ctx, true);
                            }
                            self.sync_search_status();
                        }
                        HookOutcome::Consumed
                    }
                    "*" => {
                        self.pending_key = None;
                        match Self::word_under_cursor(ctx) {
                            Some(word) => {
                                let from = ctx.buffer.cursor_offset() + 1;
                                self.search_backward = false;
                                self.search.open_search(ctx, &word);
                                self.search.navigate_next_from(ctx, from, true);
                                self.sync_search_status();
                            }
                            None => {
                                self.status_override =
                                    Some("E348: No string under cursor".to_string());
                            }
                        }
                        HookOutcome::Consumed
                    }
                    "#" => {
                        self.pending_key = None;
                        match Self::word_under_cursor(ctx) {
                            Some(word) => {
                                let from = ctx.buffer.cursor_offset();
                                self.search_backward = true;
                                self.search.open_search(ctx, &word);
                                self.search.navigate_prev_from(ctx, from, true);
                                self.sync_search_status();
                            }
                            None => {
                                self.status_override =
                                    Some("E348: No string under cursor".to_string());
                            }
                        }
                        HookOutcome::Consumed
                    }
                    ":" => {
                        self.pending_key = None;
                        self.status_override = None;
                        ctx.prompt.open(
                            PromptSpec::new("vim-ex", ":", "", PromptPlacement::BottomBar, false),
                            "",
                        );
                        HookOutcome::Consumed
                    }
                    "i" => {
                        self.enter_insert_mode(ctx);
                        HookOutcome::Consumed
                    }
                    "a" => {
                        ctx.buffer.move_cursor_right();
                        self.enter_insert_mode(ctx);
                        HookOutcome::Consumed
                    }
                    "o" => {
                        let end = ctx.buffer.line_end_offset();
                        ctx.buffer.set_cursor_offset(end);
                        ctx.buffer.insert("\n");
                        self.enter_insert_mode(ctx);
                        HookOutcome::Consumed
                    }
                    "O" => {
                        let start = ctx.buffer.line_start_offset();
                        ctx.buffer.set_cursor_offset(start);
                        ctx.buffer.insert("\n");
                        ctx.buffer.set_cursor_offset(start);
                        self.enter_insert_mode(ctx);
                        HookOutcome::Consumed
                    }
                    "x" => {
                        ctx.buffer.delete();
                        HookOutcome::Consumed
                    }
                    "u" => {
                        ctx.buffer.undo();
                        HookOutcome::Consumed
                    }
                    "v" => {
                        self.enter_visual_mode(ctx);
                        HookOutcome::Consumed
                    }
                    "V" => {
                        self.enter_visual_line_mode(ctx);
                        HookOutcome::Consumed
                    }
                    "d" => {
                        self.pending_key = Some('d');
                        HookOutcome::Consumed
                    }
                    "g" => {
                        self.pending_key = Some('g');
                        HookOutcome::Consumed
                    }
                    _ => HookOutcome::Consumed,
                }
            }
        }
    }

    fn status_text(&self) -> Option<&str> {
        if let Some(line) = &self.status_override {
            return Some(line);
        }
        match (self.mode, self.pending_key) {
            (VimMode::Normal, Some('d')) => Some("-- NORMAL (d) --"),
            (VimMode::Normal, Some('g')) => Some("-- NORMAL (g) --"),
            (VimMode::Normal, _) => Some("-- NORMAL --"),
            (VimMode::Insert, _) => Some("-- INSERT --"),
            (VimMode::Visual, _) if self.visual_linewise => Some("-- VISUAL LINE --"),
            (VimMode::Visual, _) => Some("-- VISUAL --"),
        }
    }

    fn search_snapshot(&self) -> Option<SearchSnapshot> {
        self.search.search_snapshot()
    }
}

struct AppView {
    editor: Entity<Editor>,
}

impl Render for AppView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let editor_read = self.editor.read(cx);
        let cursor_point = editor_read.buffer.cursor_point();
        let status = editor_read.status_text().unwrap_or("-- NORMAL --");

        let status_badge_color = match status {
            s if s.starts_with("-- INSERT") => rgb(0x89b4fa),
            s if s.starts_with("-- VISUAL") => rgb(0xcba6f7),
            _ => rgb(0xa6e3a1),
        };

        div()
            .size_full()
            .flex()
            .flex_col()
            .child(div().flex_1().child(self.editor.clone()))
            .child(
                div()
                    .h(px(28.0))
                    .bg(rgb(0x11111b))
                    .px(px(12.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(12.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(status_badge_color)
                            .child(status.to_string()),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(rgb(0x6c7086))
                            .child(format!(
                                "Ln {}, Col {}",
                                cursor_point.row + 1,
                                cursor_point.column + 1
                            )),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::VimHook;
    use twrite::{
        CursorStyle, EditorBuffer, EditorHook, HookContext, HookEffect, HookOutcome, KeyEvent,
        PromptState, Selection,
    };

    fn harness(
        text: &str,
    ) -> (
        VimHook,
        EditorBuffer,
        Option<Selection>,
        CursorStyle,
        PromptState,
        Vec<HookEffect>,
    ) {
        (
            VimHook::new(),
            EditorBuffer::new(text),
            None,
            CursorStyle::Block,
            PromptState::new(),
            Vec::new(),
        )
    }

    fn press(
        vim: &mut VimHook,
        buffer: &mut EditorBuffer,
        selection: &mut Option<Selection>,
        cursor_style: &mut CursorStyle,
        prompt: &mut PromptState,
        effects: &mut Vec<HookEffect>,
        key: &str,
    ) -> HookOutcome {
        let mut ctx = HookContext::new(buffer, selection, cursor_style, prompt, effects);
        vim.on_key(&mut ctx, &KeyEvent::plain(key))
    }

    #[test]
    fn v_selects_current_line() {
        let (mut vim, mut buffer, mut selection, mut style, mut prompt, mut effects) =
            harness("l1\nl2\nl3\n");
        buffer.set_cursor_offset(4);
        press(
            &mut vim,
            &mut buffer,
            &mut selection,
            &mut style,
            &mut prompt,
            &mut effects,
            "V",
        );
        assert_eq!(selection.unwrap().byte_range(), 3..6);
        assert_eq!(vim.status_text(), Some("-- VISUAL LINE --"));
    }

    #[test]
    fn ggvg_selects_whole_document() {
        let (mut vim, mut buffer, mut selection, mut style, mut prompt, mut effects) =
            harness("l1\nl2\nl3\n");
        for key in ["G", "g", "g", "V", "G"] {
            press(
                &mut vim,
                &mut buffer,
                &mut selection,
                &mut style,
                &mut prompt,
                &mut effects,
                key,
            );
        }
        assert_eq!(selection.unwrap().byte_range(), 0..buffer.len_bytes());
    }

    #[test]
    fn linewise_j_extends_by_whole_line() {
        let (mut vim, mut buffer, mut selection, mut style, mut prompt, mut effects) =
            harness("l1\nl2\nl3\n");
        for key in ["V", "j"] {
            press(
                &mut vim,
                &mut buffer,
                &mut selection,
                &mut style,
                &mut prompt,
                &mut effects,
                key,
            );
        }
        assert_eq!(selection.unwrap().byte_range(), 0..6);
    }

    #[test]
    fn linewise_delete_removes_whole_lines() {
        let (mut vim, mut buffer, mut selection, mut style, mut prompt, mut effects) =
            harness("l1\nl2\nl3\n");
        buffer.set_cursor_offset(4);
        for key in ["V", "d"] {
            press(
                &mut vim,
                &mut buffer,
                &mut selection,
                &mut style,
                &mut prompt,
                &mut effects,
                key,
            );
        }
        assert_eq!(buffer.text().to_string(), "l1\nl3\n");
        assert_eq!(vim.status_text(), Some("-- NORMAL --"));
    }

    #[test]
    fn visual_v_toggles_back_to_charwise() {
        let (mut vim, mut buffer, mut selection, mut style, mut prompt, mut effects) =
            harness("l1\nl2\nl3\n");
        for key in ["V", "v"] {
            press(
                &mut vim,
                &mut buffer,
                &mut selection,
                &mut style,
                &mut prompt,
                &mut effects,
                key,
            );
        }
        assert!(!vim.visual_linewise);
        assert_eq!(vim.status_text(), Some("-- VISUAL --"));
    }
}

fn main() {
    application().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(850.0), px(620.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("TWrite - Vim Mode Example".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |window, cx| {
                let editor = cx.new(|cx| {
                    let mut ed = Editor::new(
                        "# Vim Mode in TWrite\n\nThis entire Vim modal editing system is powered by an EditorHook.\nZero lines of Vim code exist in the core twrite engine!\n\nKeybindings supported:\n- Normal Mode (Block cursor):\n  * h, j, k, l or arrow keys : move cursor\n  * w, b : next / previous word\n  * 0, $ : line start / line end\n  * G, gg : document bottom / document top\n  * x : delete character\n  * dd : delete current line\n  * dw : delete word\n  * u : undo, Ctrl+r : redo\n  * i, a : enter Insert mode\n  * o, O : open line below / above and enter Insert mode\n  * v / V : enter Visual mode (charwise / linewise)\n- Visual Mode:\n  * Expand selection with h/j/k/l, w, b, G, gg\n  * v / V : charwise / linewise selection (V selects whole lines)\n  * ggVG : select the whole document\n  * d or x : delete selection and return to Normal\n  * Escape : cancel selection\n- Search (prompt box, powered by SearchHook):\n  * /, ? : forward / backward search, n / N : next / previous match\n  * *, # : word under cursor forward / backward\n  * Ctrl+F : search, Ctrl+H : replace, Ctrl+Enter : replace one, Alt+A : replace all\n  * Alt+C / Alt+W / Alt+H : Match Case / Whole Word / Highlight All, Up/Down : prev/next match\n- Ex commands (:):\n  * :w [path], :q, :q!, :wq, :e path, :<num> (go to line)\n  * :s/old/new/[g][i], :%s/old/new/[g][i] (regex, $1 captures)\n- Insert Mode (Bar cursor):\n  * Type normally\n  * Escape : return to Normal mode\n",
                        cx,
                    );
                    ed.config.line_numbers = true;
                    // No explicit family: the editor auto-selects the first
                    // platform monospace with bold + italic faces (see
                    // `Editor::face_availability`). Set `ed.config.font_family
                    // explicitly to override (e.g. Menlo, Consolas).
                    let mut vim = VimHook::new();
                    let mut ctx = HookContext::new(
                        &mut ed.buffer,
                        &mut ed.selection,
                        &mut ed.cursor_style,
                        &mut ed.prompt,
                        &mut ed.pending_effects,
                    );
                    vim.enter_normal_mode(&mut ctx);
                    ed.add_hook(vim);
                    ed
                });

                let focus_handle = editor.read(cx).focus_handle.clone();
                focus_handle.focus(window, cx);

                cx.new(|_| AppView { editor })
            },
        )
        .unwrap();

        cx.activate(true);
    });
}
