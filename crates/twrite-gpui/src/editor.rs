use gpui::{Context, FocusHandle, Render, div};
use twrite_core::{EditorBuffer, EditorHook, Selection};

use crate::{config::EditorConfig, theme::EditorTheme};

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

    fn handle_key_down() {}

    fn handle_mouse_down() {}

    fn handle_scroll_wheel() {}
}

impl Render for Editor {
    fn render(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) -> impl gpui::prelude::IntoElement {
        div()
    }
}
