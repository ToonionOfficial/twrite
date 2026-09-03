use gpui::{Hsla, hsla, rgb};

#[derive(Clone)]
pub struct EditorTheme {
    pub background: Hsla,
    pub foreground: Hsla,
    pub cursor: Hsla,
    pub selection: Hsla,
    pub line_number: Hsla,
    pub line_number_active: Hsla,
}

impl Default for EditorTheme {
    fn default() -> Self {
        Self {
            background: rgb(0x181825).into(),
            foreground: rgb(0xcdd6f4).into(),
            cursor: rgb(0xf5e0dc).into(),
            selection: hsla(0.65, 0.4, 0.6, 0.25),
            line_number: rgb(0x6c7086).into(),
            line_number_active: rgb(0xcdd6f4).into(),
        }
    }
}

impl EditorTheme {}
