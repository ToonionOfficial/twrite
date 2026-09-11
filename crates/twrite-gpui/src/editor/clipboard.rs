use gpui::{App, ClipboardItem};

use super::Editor;

impl Editor {
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
}
