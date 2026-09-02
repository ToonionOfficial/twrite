use ropey::Rope;

use crate::{coordinates::Point, history::History};

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
    /// Creates a new buffer containing `initial_text`.
    ///
    /// The cursor is initially positioned at byte offset `0` and the
    /// undo/redo history starts empty.
    pub fn new(initial_text: &str) -> Self {
        Self {
            text: Rope::from_str(initial_text),
            cursor: 0,
            history: History::default(),
        }
    }

    /// Returns the underlying text buffer.
    ///
    /// The returned [`Rope`] provides access to the document contents
    pub fn text(&self) -> &Rope {
        &self.text
    }

    /// Returns the current cursor position as a byte offset.
    pub fn cursor_offset(&self) -> usize {
        self.cursor
    }

    /// Returns the total number of bytes in the buffer.
    pub fn len_bytes(&self) -> usize {
        self.text.len_bytes()
    }

    /// Returns the number of lines in the buffer.
    pub fn len_lines(&self) -> usize {
        self.text.len_lines()
    }

    /// Returns a line as a [`String`].
    ///
    /// If `line_idx` is outside the buffer, an empty string is returned.
    pub fn line_to_string(&self, line_idx: usize) -> String {
        if line_idx >= self.text.len_lines() {
            return String::new();
        }

        self.text.line(line_idx).to_string()
    }

    /// Converts a byte offset into a [`Point`].
    ///
    /// The resulting point contains a zero-based row and a byte-based
    /// column. If `offset` is greater than the end of the document, it
    /// is clamped to the document's final byte.
    pub fn offset_to_point(&self, offset: usize) -> Point {
        let clamped = offset.min(self.text.len_bytes());
        let row = self.text.byte_to_line(clamped);
        let line_start_byte = self.text.line_to_byte(row);
        let column = clamped - line_start_byte;

        Point::new(row, column)
    }

    /// Converts a [`Point`] into a byte offset.
    ///
    /// The point's row is clamped to the document's last valid line and
    /// its column is clamped to the length of that line.
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

    /// Undoes the most recent transaction.
    ///
    /// If there is no transaction to undo, this method does nothing.
    ///
    /// The undone transaction is moved to the redo stack.
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

    /// Redoes the most recently undone transaction.
    ///
    /// If there is no transaction to redo, this method does nothing.
    ///
    /// The redone transaction is moved back to the undo stack.
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
