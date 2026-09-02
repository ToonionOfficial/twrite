use ropey::Rope;

pub struct EditorBuffer {
    text: Rope,
    cursor: usize,
}

impl EditorBuffer {
    pub fn new(initial_text: &str) -> Self {
        Self {
            text: Rope::from_str(initial_text),
            cursor: 0,
        }
    }

    pub fn text(&self) -> &Rope {
        &self.text
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }
}
