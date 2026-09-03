use gpui::{Pixels, px};

#[derive(Debug, Clone)]
pub struct EditorConfig {
    pub line_numbers: bool,
    pub line_height: Pixels,
    pub font_size: Pixels,
    pub tab_size: usize,
    pub highlight_active_line: bool,
    pub block_cursor: bool,
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            line_numbers: false,
            line_height: px(22.0),
            font_size: px(16.0),
            tab_size: 4,
            highlight_active_line: false,
            block_cursor: false,
        }
    }
}
