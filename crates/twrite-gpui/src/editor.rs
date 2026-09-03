use gpui::*;
use twrite_core::{EditorBuffer, EditorHook, Selection};

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
        }
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
                    self.buffer.backspace();
                    self.selection = None;
                    edited = true;
                }
                "delete" => {
                    self.buffer.delete();
                    self.selection = None;
                    edited = true;
                }
                "enter" => {
                    self.buffer.insert("\n");
                    self.selection = None;
                    edited = true;
                }
                "tab" => {
                    self.buffer.insert(&" ".repeat(self.config.tab_size));
                    self.selection = None;
                    edited = true;
                }
                "left" | "arrowleft" => {
                    self.buffer.move_cursor_left();
                    self.selection = None;
                }
                "right" | "arrowright" => {
                    self.buffer.move_cursor_right();
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
                    if !key_event.modifiers.alt && key.chars().count() == 1 {
                        self.buffer.insert(key);
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
        _event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_handle.focus(window, cx);
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
            .bg(self.theme.background)
            .on_key_down(cx.listener(Self::handle_key_down))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::handle_mouse_down))
            .on_scroll_wheel(cx.listener(Self::handle_scroll_wheel))
            .child(EditorCanvas::new(cx.entity().clone()))
    }
}
