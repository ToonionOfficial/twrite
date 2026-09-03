use std::ops::Range;

use ropey::Rope;

use crate::{
    coordinates::Point,
    history::{Edit, History, Transaction},
};

/// A text buffer that manages document contents, cursor position,
/// and undo/redo history.
///
/// `EditorBuffer` stores its text in a [`Rope`], making insertion,
/// deletion, and line-based operations efficient for an editor.
///
/// Cursor positions are represented internally as byte offsets.
///
/// # Examples
///
/// ```
/// use twrite_core::EditorBuffer;
///
/// let buffer = EditorBuffer::new("Hello, world!");
///
/// assert_eq!(buffer.len_bytes(), 13);
/// assert_eq!(buffer.len_lines(), 1);
/// assert_eq!(buffer.cursor_offset(), 0);
/// ```
#[derive(Debug)]
pub struct EditorBuffer {
    text: Rope,
    cursor: usize,
    history: History,
}

impl EditorBuffer {
    /// Creates a new editor buffer containing `initial_text`.
    ///
    /// The cursor is initially positioned at byte offset `0`, and the
    /// undo/redo history starts empty.
    pub fn new(initial_text: &str) -> Self {
        Self {
            text: Rope::from_str(initial_text),
            cursor: 0,
            history: History::default(),
        }
    }

    /// Returns a reference to the underlying text.
    ///
    /// The returned [`Rope`] can be used to inspect the document without
    /// copying its contents.
    pub fn text(&self) -> &Rope {
        &self.text
    }

    /// Returns the current cursor position as a byte offset.
    ///
    /// The cursor is always maintained at a valid UTF-8 character boundary.
    pub fn cursor_offset(&self) -> usize {
        self.cursor
    }

    /// Returns the total number of bytes in the document.
    pub fn len_bytes(&self) -> usize {
        self.text.len_bytes()
    }

    /// Returns the number of lines in the document.
    pub fn len_lines(&self) -> usize {
        self.text.len_lines()
    }

    /// Returns the contents of a line as a [`String`].
    ///
    /// Returns an empty string if `line_idx` is outside the document.
    pub fn line_to_string(&self, line_idx: usize) -> String {
        if line_idx >= self.text.len_lines() {
            return String::new();
        }

        self.text.line(line_idx).to_string()
    }

    /// Converts a byte offset into a [`Point`].
    ///
    /// The returned point contains a zero-based row and a byte-based
    /// column. If `offset` is beyond the end of the document, it is
    /// clamped to the document's end.
    pub fn offset_to_point(&self, offset: usize) -> Point {
        let clamped = offset.min(self.text.len_bytes());
        let row = self.text.byte_to_line(clamped);
        let line_start_byte = self.text.line_to_byte(row);
        let column = clamped - line_start_byte;

        Point::new(row, column)
    }

    /// Converts a [`Point`] into a byte offset.
    ///
    /// If the row is outside the document, the returned offset points to
    /// the end of the document. If the column exceeds the length of the
    /// line, it is clamped to the end of that line.
    pub fn point_to_offset(&self, point: Point) -> usize {
        if point.row >= self.text.len_lines() {
            return self.text.len_bytes();
        }
        let line_start_byte = self.text.line_to_byte(point.row);
        let line_len = self.text.line(point.row).len_bytes();
        let col = point.column.min(line_len);
        line_start_byte + col
    }

    /// Returns the current cursor position as a [`Point`].
    pub fn cursor_point(&self) -> Point {
        self.offset_to_point(self.cursor)
    }

    /// Sets the cursor to the given byte offset.
    ///
    /// The offset is clamped to the document's bounds.
    ///
    /// The resulting cursor position is kept on a valid UTF-8 character
    /// boundary.
    pub fn set_cursor_offset(&mut self, offset: usize) {
        let offset = offset.min(self.text.len_bytes());
        self.cursor = self.text.char_to_byte(self.text.byte_to_char(offset));
    }

    /// Sets the cursor to the given document position.
    ///
    /// The row and column are clamped to the document's bounds.
    pub fn set_cursor_point(&mut self, point: Point) {
        self.cursor = self.point_to_offset(point);
    }

    /// Moves the cursor one character to the right.
    ///
    /// Does nothing if the cursor is already at the end of the document.
    pub fn move_cursor_right(&mut self) {
        if self.cursor < self.text.len_bytes() {
            let char_idx = self.text.byte_to_char(self.cursor);
            let next_char = (char_idx + 1).min(self.text.len_chars());
            self.cursor = self.text.char_to_byte(next_char);
        }
    }

    /// Moves the cursor one line upward.
    ///
    /// The column is preserved when possible. If the target line is shorter,
    /// the cursor is placed at the end of that line.
    pub fn move_cursor_up(&mut self) {
        let point = self.cursor_point();
        if point.row > 0 {
            self.set_cursor_point(Point::new(point.row - 1, point.column));
        }
    }

    /// Moves the cursor one line downward.
    ///
    /// The column is preserved when possible. If the target line is shorter,
    /// the cursor is placed at the end of that line.
    pub fn move_cursor_down(&mut self) {
        let point = self.cursor_point();
        if point.row + 1 < self.text.len_lines() {
            self.set_cursor_point(Point::new(point.row + 1, point.column));
        }
    }

    /// Moves the cursor one character to the left.
    ///
    /// Does nothing if the cursor is already at the beginning of the
    /// document.
    pub fn move_cursor_left(&mut self) {
        if self.cursor > 0 {
            let char_idx = self.text.byte_to_char(self.cursor);
            self.cursor = self.text.char_to_byte(char_idx - 1);
        }
    }

    /// Returns the byte offset of the previous word start relative to current cursor.
    pub fn prev_word_offset(&self) -> usize {
        crate::movement::find_prev_word_start(&self.text, self.cursor)
    }

    /// Returns the byte offset of the next word end relative to current cursor.
    pub fn next_word_offset(&self) -> usize {
        crate::movement::find_next_word_end(&self.text, self.cursor)
    }

    /// Returns the byte offset of the start of the current line.
    pub fn line_start_offset(&self) -> usize {
        crate::movement::find_line_start(&self.text, self.cursor)
    }

    /// Returns the byte offset of the end of the current line (excluding trailing newline).
    pub fn line_end_offset(&self) -> usize {
        crate::movement::find_line_end(&self.text, self.cursor)
    }

    /// Moves the cursor to the start of the previous word.
    pub fn move_cursor_prev_word(&mut self) {
        self.cursor = self.prev_word_offset();
    }

    /// Moves the cursor to the end of the next word.
    pub fn move_cursor_next_word(&mut self) {
        self.cursor = self.next_word_offset();
    }

    /// Moves the cursor to the beginning of the current line.
    pub fn move_cursor_line_start(&mut self) {
        self.cursor = self.line_start_offset();
    }

    /// Moves the cursor to the end of the current line.
    pub fn move_cursor_line_end(&mut self) {
        self.cursor = self.line_end_offset();
    }

    /// Deletes the text from the previous word boundary up to the cursor.
    ///
    /// Returns `true` if text was deleted, or `false` if the cursor was already at the beginning.
    pub fn delete_prev_word(&mut self) -> bool {
        let target = self.prev_word_offset();
        if target < self.cursor {
            self.delete_range(target..self.cursor);
            true
        } else {
            false
        }
    }

    /// Deletes the text from the cursor up to the next word boundary.
    ///
    /// Returns `true` if text was deleted, or `false` if the cursor was already at the end.
    pub fn delete_next_word(&mut self) -> bool {
        let target = self.next_word_offset();
        if self.cursor < target {
            self.delete_range(self.cursor..target);
            true
        } else {
            false
        }
    }

    /// Inserts `text` at the current cursor position.
    ///
    /// The inserted text becomes a single undoable transaction, and the
    /// cursor is moved to the end of the inserted text.
    ///
    /// Inserting new text after undoing clears the redo history.
    pub fn insert(&mut self, text: &str) {
        let previous_cursor = self.cursor;
        let char_idx = self.text.byte_to_char(self.cursor);
        self.text.insert(char_idx, text);
        self.cursor += text.len();

        let tx = Transaction {
            edits: vec![Edit {
                bytes_range: previous_cursor..previous_cursor,
                inserted_text: text.to_string(),
                deleted_text: String::new(),
            }],
            previous_cursor,
            resulting_cursor: self.cursor,
        };

        self.history.undo_stack.push(tx);
        self.history.redo_stack.clear();
    }

    /// Deletes the character immediately before the cursor.
    ///
    /// If the cursor is at the beginning of the document, this method does
    /// nothing.
    ///
    /// The deleted character is recorded as an undoable transaction and
    /// the cursor moves to the beginning of the deleted character.
    ///
    /// Inserting new text after undoing clears the redo history.
    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }

        let char_idx = self.text.byte_to_char(self.cursor);
        let previous_char_byte = self.text.char_to_byte(char_idx - 1);
        let range_to_delete = previous_char_byte..self.cursor;
        let deleted_text = self.text.byte_slice(range_to_delete.clone()).to_string();

        let previous_cursor = self.cursor;
        self.text.remove((char_idx - 1)..char_idx);
        self.cursor = previous_char_byte;

        let tx = Transaction {
            edits: vec![Edit {
                bytes_range: range_to_delete,
                inserted_text: String::new(),
                deleted_text,
            }],
            previous_cursor,
            resulting_cursor: self.cursor,
        };

        self.history.undo_stack.push(tx);
        self.history.redo_stack.clear();
    }

    /// Deletes the character at the current cursor position.
    ///
    /// If the cursor is at the end of the document, this method does nothing.
    /// The cursor remains at the same byte offset after the deletion.
    ///
    /// The deleted text is recorded as a transaction so the operation can be
    /// undone and redone.
    pub fn delete(&mut self) {
        if self.cursor >= self.text.len_bytes() {
            return;
        }

        let char_idx = self.text.byte_to_char(self.cursor);
        let next_char = char_idx + 1;

        let end = self.text.char_to_byte(next_char);
        let byte_range = self.cursor..end;
        let deleted_text = self.text.byte_slice(byte_range.clone()).to_string();

        self.text.remove(char_idx..next_char);

        let tx = Transaction {
            edits: vec![Edit {
                bytes_range: byte_range,
                inserted_text: String::new(),
                deleted_text,
            }],
            previous_cursor: self.cursor,
            resulting_cursor: self.cursor,
        };

        self.history.undo_stack.push(tx);
        self.history.redo_stack.clear();
    }

    /// Deletes the text within `range`.
    ///
    /// The deletion is recorded as an undoable transaction and the cursor
    /// is set to the start of `range`.
    pub fn delete_range(&mut self, range: Range<usize>) {
        let start = range.start.min(self.text.len_bytes());
        let end = range.end.min(self.text.len_bytes());
        if start >= end {
            return;
        }

        let start_char = self.text.byte_to_char(start);
        let end_char = self.text.byte_to_char(end);
        let deleted_text = self.text.byte_slice(start..end).to_string();
        let previous_cursor = self.cursor;

        self.text.remove(start_char..end_char);
        self.cursor = start;

        let tx = Transaction {
            edits: vec![Edit {
                bytes_range: start..end,
                inserted_text: String::new(),
                deleted_text,
            }],
            previous_cursor,
            resulting_cursor: self.cursor,
        };

        self.history.undo_stack.push(tx);
        self.history.redo_stack.clear();
    }

    /// Replaces the text within `range` with `text`.
    ///
    /// If `range` is empty, this is equivalent to [`Self::insert`].
    pub fn replace_range(&mut self, range: Range<usize>, text: &str) {
        let start = range.start.min(self.text.len_bytes());
        let end = range.end.min(self.text.len_bytes());
        if start == end {
            self.cursor = start;
            self.insert(text);
            return;
        }

        let start_char = self.text.byte_to_char(start);
        let end_char = self.text.byte_to_char(end);
        let deleted_text = self.text.byte_slice(start..end).to_string();
        let previous_cursor = self.cursor;

        self.text.remove(start_char..end_char);
        self.text.insert(start_char, text);
        self.cursor = start + text.len();

        let tx = Transaction {
            edits: vec![Edit {
                bytes_range: start..end,
                inserted_text: text.to_string(),
                deleted_text,
            }],
            previous_cursor,
            resulting_cursor: self.cursor,
        };

        self.history.undo_stack.push(tx);
        self.history.redo_stack.clear();
    }

    /// Undoes the most recent transaction.
    ///
    /// If there is no transaction to undo, this method does nothing.
    /// The undone transaction is moved to the redo stack.
    pub fn undo(&mut self) {
        if let Some(tx) = self.history.undo_stack.pop() {
            for edit in tx.edits.iter().rev() {
                let start = edit.bytes_range.start;
                let end = start + edit.inserted_text.len();

                if end > start {
                    let start_char = self.text.byte_to_char(start);
                    let end_char = self.text.byte_to_char(end);
                    self.text.remove(start_char..end_char);
                }
                if !edit.deleted_text.is_empty() {
                    let start_char = self.text.byte_to_char(start);
                    self.text.insert(start_char, &edit.deleted_text);
                }
            }
            self.cursor = tx.previous_cursor;
            self.history.redo_stack.push(tx);
        }
    }

    /// Redoes the most recently undone transaction.
    ///
    /// If there is no transaction to redo, this method does nothing.
    /// The redone transaction is moved back to the undo stack.
    pub fn redo(&mut self) {
        if let Some(tx) = self.history.redo_stack.pop() {
            for edit in &tx.edits {
                let start = edit.bytes_range.start;
                let end = start + edit.deleted_text.len();

                if end > start {
                    let start_char = self.text.byte_to_char(start);
                    let end_char = self.text.byte_to_char(end);
                    self.text.remove(start_char..end_char);
                }
                if !edit.inserted_text.is_empty() {
                    let start_char = self.text.byte_to_char(start);
                    self.text.insert(start_char, &edit.inserted_text);
                }
            }
            self.cursor = tx.resulting_cursor;
            self.history.undo_stack.push(tx);
        }
    }
}
