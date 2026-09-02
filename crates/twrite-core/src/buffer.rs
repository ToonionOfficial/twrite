use ropey::Rope;

use crate::history::History;

pub struct EditorBuffer {
    text: Rope,
    cursor: usize,
    history: History,
}

impl EditorBuffer {
    pub fn new(initial_text: &str) -> Self {
        Self {
            text: Rope::from_str(initial_text),
            cursor: 0,
            history: History::default(),
        }
    }

    pub fn text(&self) -> &Rope {
        &self.text
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn undo(&mut self) {
        if let Some(tx) = self.history.undo_stack.pop() {
            for edit in tx.edits.iter().rev() {
                let start = edit.bytes_range.start;
                let end = start + edit.inserted_text.len();

                if end > start {
                    self.text.remove(start..end);
                }
                if !edit.deleted_text.is_empty() {
                    self.text.insert(start, &edit.deleted_text);
                }
            }
            self.cursor = tx.previous_cursor;
            self.history.redo_stack.push(tx);
        }
    }

    pub fn redo(&mut self) {
        if let Some(tx) = self.history.redo_stack.pop() {
            for edit in &tx.edits {
                let start = edit.bytes_range.start;
                let end = start + edit.deleted_text.len();

                if end > start {
                    self.text.remove(start..end);
                }
                if !edit.inserted_text.is_empty() {
                    self.text.insert(start, &edit.inserted_text);
                }
            }
            self.cursor = tx.resulting_cursor;
            self.history.undo_stack.push(tx);
        }
    }
}
