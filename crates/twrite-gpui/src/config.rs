use gpui::{Pixels, px};

/// Layout and visual settings for the editor canvas.
#[derive(Debug, Clone)]
pub struct EditorConfig {
    /// Whether to render line numbers in the left gutter.
    pub line_numbers: bool,
    /// Vertical line height in pixels.
    pub line_height: Pixels,
    /// Text font size in pixels.
    pub font_size: Pixels,
    /// Number of spaces per tab indentation.
    pub tab_size: usize,
    /// Whether to highlight the background of the active cursor line.
    pub highlight_active_line: bool,
    /// Default cursor shape: true for block, false for line/bar.
    pub block_cursor: bool,
    /// Whether to soft-wrap lines at the viewport boundary.
    pub line_wrap: bool,
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
            line_wrap: true,
        }
    }
}
