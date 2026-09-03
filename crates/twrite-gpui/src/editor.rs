use std::sync::Arc;

use gpui::*;
use twrite_core::{
    CursorStyle, EditorBuffer, EditorHook, HookContext, Point as BufferPoint, Selection,
    SyntaxHighlighter,
};

use crate::{
    canvas::{EditorCanvas, LineMetrics, build_line_text_runs},
    config::EditorConfig,
    theme::EditorTheme,
};

/// The main GPUI text editor view and controller.
pub struct Editor {
    /// The underlying text buffer managing document contents and undo/redo history.
    pub buffer: EditorBuffer,
    /// Visual color palette for canvas background, text, cursor, and syntax tokens.
    pub theme: EditorTheme,
    /// Display and layout configurations (font size, line height, line numbers, wrapping).
    pub config: EditorConfig,
    /// Extensible hooks chain intercepting keystrokes, edits, and selection changes.
    pub hooks: Vec<Box<dyn EditorHook>>,
    /// Active syntax highlighter computing semantic and direct style spans.
    pub highlighter: Option<Arc<dyn SyntaxHighlighter>>,
    /// Focus handle for keyboard input tracking within GPUI.
    pub focus_handle: FocusHandle,
    /// First visible row index in the viewport.
    pub scroll_row: usize,
    /// Active selection range, if any.
    pub selection: Option<Selection>,
    /// Active visual cursor style (Bar, Block, Underline, Hidden).
    pub cursor_style: CursorStyle,
    /// Whether the user is currently mouse-drag selecting text.
    pub is_selecting: bool,
    /// Last rendered bounds in window pixel coordinates.
    pub last_bounds: Option<Bounds<Pixels>>,
    /// Last rendered cursor position in window pixel coordinates, computed during canvas prepaint.
    pub last_cursor_pixel: Option<Point<Pixels>>,
}

impl Editor {
    /// Creates a new editor entity with `initial_text`.
    pub fn new(initial_text: &str, cx: &mut Context<Self>) -> Self {
        let config = EditorConfig::default();
        let cursor_style = if config.block_cursor {
            CursorStyle::Block
        } else {
            CursorStyle::Bar
        };
        Self {
            buffer: EditorBuffer::new(initial_text),
            theme: EditorTheme::default(),
            config,
            hooks: Vec::new(),
            highlighter: None,
            focus_handle: cx.focus_handle(),
            scroll_row: 0,
            selection: None,
            cursor_style,
            is_selecting: false,
            last_bounds: None,
            last_cursor_pixel: None,
        }
    }

    /// Adds an editor hook to the execution chain.
    pub fn add_hook(&mut self, hook: impl EditorHook) {
        self.hooks.push(Box::new(hook));
    }

    /// Clears all registered editor hooks.
    pub fn clear_hooks(&mut self) {
        self.hooks.clear();
    }

    /// Returns the active status or mode text reported by registered hooks.
    pub fn status_text(&self) -> Option<&str> {
        self.hooks.iter().find_map(|h| h.status_text())
    }

    /// Returns true if the cursor is currently in block mode.
    pub fn is_block_cursor(&self) -> bool {
        self.cursor_style == CursorStyle::Block || self.config.block_cursor
    }

    /// Sets the active syntax highlighter.
    pub fn set_highlighter(&mut self, highlighter: impl SyntaxHighlighter) {
        self.highlighter = Some(Arc::new(highlighter));
    }

    /// Clears the active syntax highlighter, reverting to plain text.
    pub fn clear_highlighter(&mut self) {
        self.highlighter = None;
    }

    /// Enables out-of-the-box CommonMark and GFM editing.
    ///
    /// Automatically configures [`twrite_core::MarkdownHighlighter`], [`twrite_core::AutoPairsHook`],
    /// and [`twrite_core::MarkdownHook`] (for formatting shortcuts, smart list continuation,
    /// and checkbox toggles).
    /// Enables out-of-the-box CommonMark and GFM editing using `self.config.markdown`.
    ///
    /// Automatically configures [`twrite_core::MarkdownHighlighter`], [`twrite_core::AutoPairsHook`],
    /// and [`twrite_core::MarkdownHook`].
    #[cfg(feature = "markdown")]
    pub fn enable_markdown(&mut self) {
        self.enable_markdown_with_config(self.config.markdown);
    }

    /// Enables out-of-the-box CommonMark and GFM editing with custom Markdown configuration.
    #[cfg(feature = "markdown")]
    pub fn enable_markdown_with_config(&mut self, config: twrite_core::markdown::MarkdownConfig) {
        use twrite_core::{AutoPairsHook, MarkdownHighlighter, MarkdownHook};
        self.config.markdown = config;
        self.set_highlighter(MarkdownHighlighter::with_config(config));
        self.add_hook(AutoPairsHook::new());
        self.add_hook(MarkdownHook::new());
    }

    /// Loads document text from a file into the editor, resetting cursor and undo history.
    pub fn load_file<P: AsRef<std::path::Path>>(
        &mut self,
        path: P,
    ) -> Result<(), twrite_core::EditorError> {
        let new_buffer = EditorBuffer::from_file(path)?;
        self.buffer = new_buffer;
        self.scroll_row = 0;
        self.selection = None;
        Ok(())
    }

    /// Saves the current editor document contents to a file.
    pub fn save_file<P: AsRef<std::path::Path>>(
        &self,
        path: P,
    ) -> Result<(), twrite_core::EditorError> {
        self.buffer.save_to_file(path)
    }

    /// Deletes the currently selected text, returning true if text was deleted.
    pub fn delete_selection(&mut self) -> bool {
        if let Some(selection) = self.selection.take() {
            let range = selection.byte_range();
            if !range.is_empty() {
                self.buffer.delete_range(range);
                return true;
            }
        }
        false
    }

    /// Replaces the active selection with `text`, or inserts `text` at the cursor position.
    pub fn replace_selection_or_insert(&mut self, text: &str) {
        if let Some(selection) = self.selection.take() {
            let range = selection.byte_range();
            if !range.is_empty() {
                self.buffer.replace_range(range, text);
                return;
            }
        }
        self.buffer.insert(text);
    }

    /// Copies the currently selected text to the system clipboard.
    pub fn copy(&self, cx: &App) {
        if let Some(sel) = self.selection {
            let range = sel.byte_range();
            if !range.is_empty() {
                let text = self.buffer.text().byte_slice(range).to_string();
                cx.write_to_clipboard(ClipboardItem::new_string(text));
            }
        }
    }

    /// Cuts the currently selected text and copies it to the system clipboard.
    ///
    /// Returns `true` if text was cut, or `false` if there was no selection.
    pub fn cut(&mut self, cx: &App) -> bool {
        if let Some(sel) = self.selection.take() {
            let range = sel.byte_range();
            if !range.is_empty() {
                let text = self.buffer.text().byte_slice(range.clone()).to_string();
                cx.write_to_clipboard(ClipboardItem::new_string(text));
                self.buffer.delete_range(range);
                self.selection = None;
                return true;
            }
        }
        false
    }

    /// Pastes text from the system clipboard, replacing the current selection or inserting at cursor.
    ///
    /// Returns `true` if text was pasted, or `false` if the clipboard was empty.
    pub fn paste(&mut self, cx: &App) -> bool {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text())
            && !text.is_empty()
        {
            self.replace_selection_or_insert(&text);
            self.selection = None;
            return true;
        }
        false
    }

    /// Scrolls the viewport upward by a given number of lines.
    pub fn scroll_up(&mut self, count: usize) {
        self.scroll_row = self.scroll_row.saturating_sub(count);
    }

    /// Scrolls the viewport downward by a given number of lines.
    pub fn scroll_down(&mut self, count: usize) {
        let total_lines = self.buffer.len_lines();
        self.scroll_row = (self.scroll_row + count).min(total_lines.saturating_sub(1));
    }

    /// Moves the cursor to `new_offset`, expanding or creating a selection if `select` is true.
    pub fn move_cursor_to(&mut self, new_offset: usize, select: bool) {
        if select {
            let anchor = self
                .selection
                .map(|s| s.anchor)
                .unwrap_or_else(|| self.buffer.cursor_offset());
            self.buffer.set_cursor_offset(new_offset);
            if anchor != new_offset {
                self.selection = Some(Selection::range(anchor, new_offset));
            } else {
                self.selection = None;
            }
        } else {
            self.buffer.set_cursor_offset(new_offset);
            self.selection = None;
        }
    }

    /// Scrolls the viewport so that the cursor is visible.
    ///
    /// Ensures a 1-line margin above and below the cursor when possible.
    pub fn scroll_to_cursor(&mut self, window: Option<&Window>) {
        let total_lines = self.buffer.len_lines();
        if total_lines == 0 {
            self.scroll_row = 0;
            return;
        }

        let cursor_row = self
            .buffer
            .cursor_point()
            .row
            .min(total_lines.saturating_sub(1));

        let margin_lines = 1;
        if cursor_row < self.scroll_row + margin_lines {
            self.scroll_row = cursor_row.saturating_sub(margin_lines);
            return;
        }

        let bounds = match self.last_bounds {
            Some(b) => b,
            None => return,
        };

        let viewport_height = bounds.size.height;
        if viewport_height <= px(0.0) {
            return;
        }

        let line_height = self.config.line_height;
        let margin = line_height * margin_lines as f32;

        if self.config.line_wrap
            && let Some(win) = window
        {
            let gutter_width = if self.config.line_numbers {
                px(48.0)
            } else {
                px(0.0)
            };
            let wrap_width = Some((bounds.size.width - gutter_width - px(24.0)).max(px(50.0)));
            let font = win.text_style().font();

            let get_row_visual_lines = |row: usize| -> usize {
                let raw_line = self.buffer.line_to_string(row);
                let line_text = raw_line.trim_end_matches(['\r', '\n']);
                if line_text.is_empty() {
                    return 1;
                }
                let runs = [TextRun {
                    len: line_text.len(),
                    font: font.clone(),
                    color: self.theme.foreground,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                }];
                win.text_system()
                    .shape_text(
                        line_text.to_string().into(),
                        self.config.font_size,
                        &runs,
                        wrap_width,
                        None,
                    )
                    .ok()
                    .and_then(|mut l| l.pop())
                    .map(|l| l.wrap_boundaries.len() + 1)
                    .unwrap_or(1)
            };

            let mut accumulated = line_height * get_row_visual_lines(cursor_row) as f32;
            let mut new_scroll_row = cursor_row;

            while new_scroll_row > 0 {
                let prev_lines = get_row_visual_lines(new_scroll_row - 1);
                let prev_height = line_height * prev_lines as f32;
                if accumulated + prev_height + margin > viewport_height {
                    break;
                }
                accumulated += prev_height;
                new_scroll_row -= 1;
            }

            if new_scroll_row > self.scroll_row {
                self.scroll_row = new_scroll_row;
            }
        } else {
            let visible_lines = (viewport_height / line_height).floor() as usize;
            let effective_visible = visible_lines.saturating_sub(margin_lines).max(1);

            if cursor_row >= self.scroll_row + effective_visible {
                self.scroll_row = cursor_row.saturating_sub(effective_visible.saturating_sub(1));
            }
        }
    }

    /// Calculates the byte offset in the text buffer corresponding to a window pixel position.
    pub fn offset_for_position(&self, pos: Point<Pixels>, window: &Window) -> usize {
        let bounds = match self.last_bounds {
            Some(b) => b,
            None => return self.buffer.cursor_offset(),
        };

        let gutter_width = if self.config.line_numbers {
            px(48.0)
        } else {
            px(0.0)
        };
        let text_origin_x = bounds.left() + gutter_width + px(12.0);

        let wrap_width = if self.config.line_wrap {
            let available = bounds.size.width - gutter_width - px(24.0);
            Some(available.max(px(50.0)))
        } else {
            None
        };

        let total_lines = self.buffer.len_lines();
        if total_lines == 0 {
            return 0;
        }

        let relative_y = (pos.y - bounds.top()).max(px(0.0));
        let mut current_y_offset = px(0.0);
        let font = window.text_style().font();

        for row in self.scroll_row..total_lines {
            let raw_line = self.buffer.line_to_string(row);
            let line_text = raw_line.trim_end_matches(['\r', '\n']);
            let line_start_byte = self.buffer.point_to_offset(BufferPoint::new(row, 0));

            let spans = if let Some(ref highlighter) = self.highlighter {
                highlighter.highlight_line(&self.buffer, row, line_text)
            } else {
                Vec::new()
            };

            let concealed = twrite_core::ConcealedLine::build(line_text, &spans);

            let metrics = LineMetrics::for_line(
                &concealed.display_text,
                &concealed.spans,
                self.config.font_size,
                self.config.line_height,
            );

            let runs = build_line_text_runs(
                &concealed.display_text,
                &concealed.spans,
                None,
                &font,
                &self.theme,
            );

            let text_line = window
                .text_system()
                .shape_text(
                    concealed.display_text.clone().into(),
                    metrics.font_size,
                    &runs,
                    wrap_width,
                    None,
                )
                .ok()
                .and_then(|mut l| l.pop())
                .unwrap_or_default();

            let line_visual_lines = text_line.wrap_boundaries.len() + 1;
            let line_h = metrics.line_height * line_visual_lines;

            let is_last_line = row + 1 == total_lines;
            if relative_y < current_y_offset + line_h || is_last_line {
                if pos.x <= text_origin_x || line_text.is_empty() {
                    let col_src = concealed.display_to_source(0);
                    return line_start_byte + col_src;
                }

                let line_rel_y = (relative_y - current_y_offset).max(px(0.0));
                let line_rel_x = (pos.x - text_origin_x).max(px(0.0));
                let rel_pos = point(line_rel_x, line_rel_y);

                let col_display = text_line
                    .closest_index_for_position(rel_pos, metrics.line_height)
                    .unwrap_or_else(|idx| idx);

                let col_src = concealed.display_to_source(col_display);
                return line_start_byte + col_src.min(line_text.len());
            }

            current_y_offset += line_h;
        }

        self.buffer.len_bytes()
    }

    /// Returns the window pixel coordinates (X, Y) at the bottom of the active cursor.
    ///
    /// This value is automatically computed and cached during each canvas render pass.
    /// Returns `None` if the editor has not yet been rendered, or if the cursor is scrolled
    /// outside the visible viewport.
    pub fn cursor_pixel_position(&self) -> Option<Point<Pixels>> {
        self.last_cursor_pixel
    }

    fn handle_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key_event = match crate::input::translate_key_down(event) {
            Some(ke) => ke,
            None => return,
        };

        let initial_version = self.buffer.version();
        let mut consumed = false;

        let mut hook_idx = 0;
        while hook_idx < self.hooks.len() {
            let mut ctx = HookContext::new(
                &mut self.buffer,
                &mut self.selection,
                &mut self.cursor_style,
            );
            let outcome = self.hooks[hook_idx].on_key(&mut ctx, &key_event);
            if outcome == twrite_core::HookOutcome::Consumed {
                consumed = true;
                break;
            }
            hook_idx += 1;
        }

        if consumed {
            if self.buffer.version() != initial_version {
                for hook in &mut self.hooks {
                    hook.after_edit(&mut self.buffer);
                }
            }
            for hook in &mut self.hooks {
                hook.on_selection_change(&self.buffer, self.selection.as_ref());
            }
            self.scroll_to_cursor(Some(window));
            cx.notify();
            return;
        }

        let mut edited = false;
        let select = key_event.modifiers.shift;

        if key_event.modifiers.ctrl || key_event.modifiers.meta {
            match key_event.key.to_lowercase().as_str() {
                "z" => {
                    if key_event.modifiers.shift {
                        self.buffer.redo();
                    } else {
                        self.buffer.undo();
                    }
                    self.selection = None;
                    edited = true;
                }
                "y" => {
                    self.buffer.redo();
                    self.selection = None;
                    edited = true;
                }
                "a" => {
                    self.selection = Some(Selection::range(0, self.buffer.len_bytes()));
                }
                "c" => {
                    self.copy(cx);
                }
                "x" => {
                    edited = self.cut(cx);
                }
                "v" => {
                    edited = self.paste(cx);
                }
                "backspace" => {
                    if !self.delete_selection() {
                        edited = self.buffer.delete_prev_word();
                    } else {
                        edited = true;
                    }
                    self.selection = None;
                }
                "delete" => {
                    if !self.delete_selection() {
                        edited = self.buffer.delete_next_word();
                    } else {
                        edited = true;
                    }
                    self.selection = None;
                }
                "left" | "arrowleft" => {
                    let target = self.buffer.prev_word_offset();
                    self.move_cursor_to(target, select);
                }
                "right" | "arrowright" => {
                    let target = self.buffer.next_word_offset();
                    self.move_cursor_to(target, select);
                }
                "home" => {
                    self.move_cursor_to(0, select);
                }
                "end" => {
                    self.move_cursor_to(self.buffer.len_bytes(), select);
                }
                "up" | "arrowup" => {
                    self.scroll_up(1);
                    cx.notify();
                    return;
                }
                "down" | "arrowdown" => {
                    self.scroll_down(1);
                    cx.notify();
                    return;
                }
                _ => {}
            }
        } else {
            match key_event.key.as_str() {
                "backspace" => {
                    if !self.delete_selection() {
                        self.buffer.backspace();
                    }
                    self.selection = None;
                    edited = true;
                }
                "delete" => {
                    if !self.delete_selection() {
                        self.buffer.delete();
                    }
                    self.selection = None;
                    edited = true;
                }
                "enter" => {
                    self.replace_selection_or_insert("\n");
                    self.selection = None;
                    edited = true;
                }
                "tab" => {
                    self.replace_selection_or_insert(&" ".repeat(self.config.tab_size));
                    self.selection = None;
                    edited = true;
                }
                "space" | " " => {
                    self.replace_selection_or_insert(" ");
                    self.selection = None;
                    edited = true;
                }
                "left" | "arrowleft" => {
                    if !select && self.selection.is_some() {
                        let sel = self.selection.take().unwrap();
                        self.buffer.set_cursor_offset(sel.byte_range().start);
                    } else {
                        let target = if self.buffer.cursor_offset() > 0 {
                            let char_idx =
                                self.buffer.text().byte_to_char(self.buffer.cursor_offset());
                            self.buffer.text().char_to_byte(char_idx - 1)
                        } else {
                            0
                        };
                        self.move_cursor_to(target, select);
                    }
                }
                "right" | "arrowright" => {
                    if !select && self.selection.is_some() {
                        let sel = self.selection.take().unwrap();
                        self.buffer.set_cursor_offset(sel.byte_range().end);
                    } else {
                        let target = if self.buffer.cursor_offset() < self.buffer.len_bytes() {
                            let char_idx =
                                self.buffer.text().byte_to_char(self.buffer.cursor_offset());
                            self.buffer
                                .text()
                                .char_to_byte((char_idx + 1).min(self.buffer.text().len_chars()))
                        } else {
                            self.buffer.len_bytes()
                        };
                        self.move_cursor_to(target, select);
                    }
                }
                "up" | "arrowup" => {
                    let point = self.buffer.cursor_point();
                    if point.row > 0 {
                        let target = self
                            .buffer
                            .point_to_offset(BufferPoint::new(point.row - 1, point.column));
                        self.move_cursor_to(target, select);
                    } else {
                        self.move_cursor_to(0, select);
                    }
                }
                "down" | "arrowdown" => {
                    let point = self.buffer.cursor_point();
                    let total_lines = self.buffer.len_lines();
                    if point.row + 1 < total_lines {
                        let target = self
                            .buffer
                            .point_to_offset(BufferPoint::new(point.row + 1, point.column));
                        self.move_cursor_to(target, select);
                    } else {
                        self.move_cursor_to(self.buffer.len_bytes(), select);
                    }
                }
                "home" => {
                    let target = self.buffer.line_start_offset();
                    self.move_cursor_to(target, select);
                }
                "end" => {
                    let target = self.buffer.line_end_offset();
                    self.move_cursor_to(target, select);
                }
                key => {
                    if !key_event.modifiers.alt
                        && !key_event.modifiers.ctrl
                        && !key_event.modifiers.meta
                        && key.chars().count() == 1
                    {
                        let mut insert_consumed = false;
                        let mut hook_idx = 0;
                        while hook_idx < self.hooks.len() {
                            let mut ctx = HookContext::new(
                                &mut self.buffer,
                                &mut self.selection,
                                &mut self.cursor_style,
                            );
                            if self.hooks[hook_idx].before_insert(&mut ctx, key)
                                == twrite_core::HookOutcome::Consumed
                            {
                                insert_consumed = true;
                                break;
                            }
                            hook_idx += 1;
                        }

                        if !insert_consumed {
                            self.replace_selection_or_insert(key);
                            self.selection = None;
                            edited = true;
                        }
                    }
                }
            }
        }

        if edited {
            for hook in &mut self.hooks {
                hook.after_edit(&mut self.buffer);
            }
        }

        for hook in &mut self.hooks {
            hook.on_selection_change(&self.buffer, self.selection.as_ref());
        }

        self.scroll_to_cursor(Some(window));
        cx.notify();
    }

    fn handle_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_handle.focus(window, cx);
        self.is_selecting = true;

        let offset = self.offset_for_position(event.position, window);
        self.buffer.set_cursor_offset(offset);

        let cursor_pt = self.buffer.cursor_point();
        let line = self.buffer.line_to_string(cursor_pt.row);
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        let is_task_empty = trimmed.starts_with("- [ ] ") || trimmed.starts_with("* [ ] ");
        let is_task_checked = trimmed.starts_with("- [x] ")
            || trimmed.starts_with("* [x] ")
            || trimmed.starts_with("- [X] ");

        let interactive_tasks = {
            #[cfg(feature = "markdown")]
            {
                self.config.markdown.interactive_tasks
            }
            #[cfg(not(feature = "markdown"))]
            {
                true
            }
        };

        if interactive_tasks && !event.modifiers.shift && (is_task_empty || is_task_checked) {
            let line_start = self
                .buffer
                .point_to_offset(BufferPoint::new(cursor_pt.row, 0));
            let col = offset.saturating_sub(line_start);
            if col >= indent && col <= indent + 5 {
                let check_offset = line_start + indent + 3;
                let new_char = if is_task_empty { "x" } else { " " };
                self.buffer
                    .replace_range(check_offset..check_offset + 1, new_char);
                self.buffer.set_cursor_offset(check_offset + 2);
                self.selection = None;
                self.is_selecting = false;
                for hook in &mut self.hooks {
                    hook.after_edit(&mut self.buffer);
                }
                self.scroll_to_cursor(Some(window));
                cx.notify();
                return;
            }
        }

        if event.modifiers.shift {
            if let Some(sel) = self.selection {
                self.selection = Some(Selection::range(sel.anchor, offset));
            } else {
                self.selection = Some(Selection::point(offset));
            }
        } else {
            self.selection = Some(Selection::point(offset));
        }

        self.scroll_to_cursor(Some(window));
        cx.notify();
    }

    fn handle_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_selecting {
            let offset = self.offset_for_position(event.position, window);
            self.buffer.set_cursor_offset(offset);

            if let Some(sel) = self.selection {
                self.selection = Some(Selection::range(sel.anchor, offset));
            } else {
                self.selection = Some(Selection::point(offset));
            }

            self.scroll_to_cursor(Some(window));
            cx.notify();
        }
    }

    fn handle_mouse_up(
        &mut self,
        _event: &MouseUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.is_selecting = false;
        if let Some(sel) = self.selection
            && sel.is_empty()
        {
            self.selection = None;
        }
        cx.notify();
    }

    fn handle_scroll_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let delta_lines = match event.delta {
            ScrollDelta::Lines(delta) => -delta.y,
            ScrollDelta::Pixels(delta) => -(delta.y / self.config.line_height),
        };

        let total_lines = self.buffer.len_lines();
        if delta_lines > 0.0 {
            let count = delta_lines.round() as usize;
            self.scroll_row = (self.scroll_row + count).min(total_lines.saturating_sub(1));
        } else if delta_lines < 0.0 {
            let count = (-delta_lines).round() as usize;
            self.scroll_row = self.scroll_row.saturating_sub(count);
        }

        cx.notify();
    }
}

impl Render for Editor {
    fn render(
        &mut self,
        _window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) -> impl gpui::prelude::IntoElement {
        div()
            .track_focus(&self.focus_handle)
            .key_context("Editor")
            .size_full()
            .overflow_hidden()
            .cursor(gpui::CursorStyle::IBeam)
            .bg(self.theme.background)
            .on_key_down(cx.listener(Self::handle_key_down))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::handle_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::handle_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::handle_mouse_up))
            .on_mouse_move(cx.listener(Self::handle_mouse_move))
            .on_scroll_wheel(cx.listener(Self::handle_scroll_wheel))
            .child(EditorCanvas::new(cx.entity().clone()))
    }
}
