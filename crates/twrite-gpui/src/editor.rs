use gpui::*;
use twrite_core::{EditorBuffer, EditorHook, Point as BufferPoint, Selection};

use crate::{canvas::EditorCanvas, config::EditorConfig, theme::EditorTheme};

pub struct Editor {
    pub buffer: EditorBuffer,
    pub theme: EditorTheme,
    pub config: EditorConfig,
    pub hooks: Vec<Box<dyn EditorHook>>,
    pub focus_handle: FocusHandle,
    pub scroll_row: usize,
    pub selection: Option<Selection>,
    pub is_block_cursor: bool,
    pub is_selecting: bool,
    pub last_bounds: Option<Bounds<Pixels>>,
}

impl Editor {
    pub fn new(initial_text: &str, cx: &mut Context<Self>) -> Self {
        Self {
            buffer: EditorBuffer::new(initial_text),
            theme: EditorTheme::default(),
            config: EditorConfig::default(),
            hooks: Vec::new(),
            focus_handle: cx.focus_handle(),
            scroll_row: 0,
            selection: None,
            is_block_cursor: false,
            is_selecting: false,
            last_bounds: None,
        }
    }

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

        let relative_y = (pos.y - bounds.top()).max(px(0.0));
        let row_offset = (relative_y / self.config.line_height).floor() as usize;
        let total_lines = self.buffer.len_lines();
        if total_lines == 0 {
            return 0;
        }
        let row = (self.scroll_row + row_offset).min(total_lines.saturating_sub(1));

        let raw_line = self.buffer.line_to_string(row);
        let line_text = raw_line.trim_end_matches(['\r', '\n']);
        let line_start_byte = self.buffer.point_to_offset(BufferPoint::new(row, 0));

        if pos.x <= text_origin_x || line_text.is_empty() {
            return line_start_byte;
        }

        let rel_x = pos.x - text_origin_x;
        let font = window.text_style().font();
        let runs = [TextRun {
            len: line_text.len(),
            font,
            color: self.theme.foreground,
            background_color: None,
            underline: None,
            strikethrough: None,
        }];

        let shaped = window.text_system().shape_line(
            line_text.to_string().into(),
            self.config.font_size,
            &runs,
            None,
        );

        let col_byte = shaped.closest_index_for_x(rel_x);
        line_start_byte + col_byte.min(line_text.len())
    }

    fn handle_key_down(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key_event = match crate::input::translate_key_down(event) {
            Some(ke) => ke,
            None => return,
        };

        // 1. Run hooks
        for hook in &mut self.hooks {
            if hook.on_key(&mut self.buffer, &key_event) == twrite_core::HookOutcome::Consumed {
                cx.notify();
                return;
            }
        }

        let mut edited = false;

        // 2. Default keys
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
                    if let Some(sel) = self.selection.take().filter(|s| !s.is_empty()) {
                        self.buffer.set_cursor_offset(sel.byte_range().start);
                    } else {
                        self.buffer.move_cursor_left();
                    }
                    self.selection = None;
                }
                "right" | "arrowright" => {
                    if let Some(sel) = self.selection.take().filter(|s| !s.is_empty()) {
                        self.buffer.set_cursor_offset(sel.byte_range().end);
                    } else {
                        self.buffer.move_cursor_right();
                    }
                    self.selection = None;
                }
                "up" | "arrowup" => {
                    self.buffer.move_cursor_up();
                    self.selection = None;
                }
                "down" | "arrowdown" => {
                    self.buffer.move_cursor_down();
                    self.selection = None;
                }
                key => {
                    if !key_event.modifiers.alt
                        && !key_event.modifiers.ctrl
                        && !key_event.modifiers.meta
                        && key.chars().count() == 1
                    {
                        self.replace_selection_or_insert(key);
                        self.selection = None;
                        edited = true;
                    }
                }
            }
        }

        if edited {
            for hook in &mut self.hooks {
                hook.after_edit(&mut self.buffer);
            }
        }

        let cursor_row = self.buffer.cursor_point().row;
        if cursor_row < self.scroll_row {
            self.scroll_row = cursor_row;
        }

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

        if event.modifiers.shift {
            if let Some(sel) = self.selection {
                self.selection = Some(Selection::range(sel.anchor, offset));
            } else {
                self.selection = Some(Selection::point(offset));
            }
        } else {
            self.selection = Some(Selection::point(offset));
        }

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
        if let Some(sel) = self.selection {
            if sel.is_empty() {
                self.selection = None;
            }
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
            .cursor(CursorStyle::IBeam)
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
