pub mod buffer;
pub mod coordinates;
pub mod history;
pub mod hook;
pub mod movement;
pub mod selection;
pub mod syntax;

pub use buffer::EditorBuffer;
pub use coordinates::Point;
pub use hook::{
    AutoPairsHook, CursorStyle, EditorHook, HookContext, HookOutcome, KeyEvent, Modifiers,
};
pub use movement::{
    CharKind, classify_char, find_line_end, find_line_start, find_next_word_end,
    find_prev_word_start,
};
pub use selection::Selection;
pub use syntax::{
    HighlightTag, Rgba, StyleSpan, StyleValue, StyledSegment, SyntaxHighlighter, TextStyle,
    UnderlineDecoration, split_line_intervals,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delete_removes_character_at_cursor() {
        let mut buffer = EditorBuffer::new("hello");
        buffer.set_cursor_offset(1);

        buffer.delete();

        assert_eq!(buffer.text().to_string(), "hllo");
        assert_eq!(buffer.cursor_offset(), 1);
    }

    #[test]
    fn delete_at_end_does_nothing() {
        let mut buffer = EditorBuffer::new("hello");
        buffer.set_cursor_offset(5);

        buffer.delete();

        assert_eq!(buffer.text().to_string(), "hello");
        assert_eq!(buffer.cursor_offset(), 5);
    }

    #[test]
    fn delete_from_empty_buffer_does_nothing() {
        let mut buffer = EditorBuffer::new("");

        buffer.delete();

        assert_eq!(buffer.text().to_string(), "");
        assert_eq!(buffer.cursor_offset(), 0);
    }

    #[test]
    fn delete_handles_unicode() {
        let mut buffer = EditorBuffer::new("héllo");

        buffer.set_cursor_offset(1);

        assert_eq!(buffer.cursor_offset(), 1);
        assert_eq!(buffer.text().to_string(), "héllo");

        buffer.delete();

        assert_eq!(buffer.text().to_string(), "hllo");
        assert_eq!(buffer.cursor_offset(), 1);
    }

    #[test]
    fn delete_records_undo_history() {
        let mut buffer = EditorBuffer::new("hello");
        buffer.set_cursor_offset(1);

        buffer.delete();
        buffer.undo();

        assert_eq!(buffer.text().to_string(), "hello");
        assert_eq!(buffer.cursor_offset(), 1);
    }

    #[test]
    fn delete_can_be_redone() {
        let mut buffer = EditorBuffer::new("hello");
        buffer.set_cursor_offset(1);

        buffer.delete();
        buffer.undo();
        buffer.redo();

        assert_eq!(buffer.text().to_string(), "hllo");
        assert_eq!(buffer.cursor_offset(), 1);
    }

    #[test]
    fn delete_does_not_move_cursor() {
        let mut buffer = EditorBuffer::new("hello");
        buffer.set_cursor_offset(2);

        buffer.delete();

        assert_eq!(buffer.text().to_string(), "helo");
        assert_eq!(buffer.cursor_offset(), 2);
    }

    #[test]
    fn delete_removes_newline() {
        let mut buffer = EditorBuffer::new("hello\nworld");
        buffer.set_cursor_offset(5);

        buffer.delete();

        assert_eq!(buffer.text().to_string(), "helloworld");
        assert_eq!(buffer.cursor_offset(), 5);
    }

    #[test]
    fn delete_range_removes_text_and_sets_cursor() {
        let mut buffer = EditorBuffer::new("hello world");
        buffer.delete_range(5..11);

        assert_eq!(buffer.text().to_string(), "hello");
        assert_eq!(buffer.cursor_offset(), 5);

        buffer.undo();
        assert_eq!(buffer.text().to_string(), "hello world");

        buffer.redo();
        assert_eq!(buffer.text().to_string(), "hello");
    }

    #[test]
    fn delete_range_all() {
        let mut buffer = EditorBuffer::new("hello world");
        buffer.delete_range(0..buffer.len_bytes());

        assert_eq!(buffer.text().to_string(), "");
        assert_eq!(buffer.cursor_offset(), 0);

        buffer.undo();
        assert_eq!(buffer.text().to_string(), "hello world");
    }

    #[test]
    fn replace_range_works_and_is_undoable() {
        let mut buffer = EditorBuffer::new("hello world");
        buffer.replace_range(6..11, "there");

        assert_eq!(buffer.text().to_string(), "hello there");
        assert_eq!(buffer.cursor_offset(), 11);

        buffer.undo();
        assert_eq!(buffer.text().to_string(), "hello world");

        buffer.redo();
        assert_eq!(buffer.text().to_string(), "hello there");
    }

    #[test]
    fn delete_prev_word_removes_word_and_is_undoable() {
        let mut buffer = EditorBuffer::new("hello world");
        buffer.set_cursor_offset(11);

        assert!(buffer.delete_prev_word());
        assert_eq!(buffer.text().to_string(), "hello ");
        assert_eq!(buffer.cursor_offset(), 6);

        buffer.undo();
        assert_eq!(buffer.text().to_string(), "hello world");
        assert_eq!(buffer.cursor_offset(), 11);

        buffer.redo();
        assert_eq!(buffer.text().to_string(), "hello ");
        assert_eq!(buffer.cursor_offset(), 6);
    }

    #[test]
    fn delete_next_word_removes_word_and_is_undoable() {
        let mut buffer = EditorBuffer::new("hello world");
        buffer.set_cursor_offset(0);

        assert!(buffer.delete_next_word());
        assert_eq!(buffer.text().to_string(), " world");
        assert_eq!(buffer.cursor_offset(), 0);

        buffer.undo();
        assert_eq!(buffer.text().to_string(), "hello world");
        assert_eq!(buffer.cursor_offset(), 0);

        buffer.redo();
        assert_eq!(buffer.text().to_string(), " world");
        assert_eq!(buffer.cursor_offset(), 0);
    }
}
