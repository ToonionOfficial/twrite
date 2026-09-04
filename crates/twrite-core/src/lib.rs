//! Core headless text buffer, syntax, movement, selection, and hook primitives.

/// Battery registry: optional, feature-gated editor batteries (see it for the
/// contract and checklist for adding new batteries).
pub mod batteries;
/// Text buffer implementation backed by a Rope.
pub mod buffer;
/// 2D text coordinates (row and column).
pub mod coordinates;
/// Strongly-typed error types and results for editor operations.
pub mod error;
/// Granular undo/redo transaction history.
pub mod history;
/// Extensible hook system and input interceptors.
pub mod hook;
/// CommonMark and GitHub Flavored Markdown highlighter and interactive hook.
///
/// Lives at `src/batteries/markdown/`; this shim keeps the public path
/// `twrite_core::markdown` stable regardless of file layout.
#[cfg(feature = "markdown")]
#[path = "batteries/markdown/mod.rs"]
pub mod markdown;
/// Text movement and boundary calculation primitives.
pub mod movement;
/// Text selection ranges and anchor/head management.
pub mod selection;
/// Headless syntax highlighting, styling tokens, and interval splitting.
pub mod syntax;

pub use buffer::EditorBuffer;
pub use coordinates::Point;
pub use error::{EditorError, Result as EditorResult};
pub use hook::{
    AutoPairsHook, CursorStyle, EditorHook, HookContext, HookOutcome, KeyEvent, Modifiers,
};
#[cfg(feature = "markdown")]
pub use markdown::{
    ConcealMode, MarkdownConfig, MarkdownHighlighter, MarkdownHook, TABLE_CELL_TAG,
    TABLE_DELIMITER_TAG, TABLE_HEADER_TAG, TableAlignment, TableBlock, TableLayout, TableRowKind,
    find_unescaped_pipes, parse_delimiter_row, split_table_cells, table_block_at, table_layouts,
};
pub use movement::{
    CharKind, classify_char, find_line_end, find_line_start, find_next_word_end,
    find_prev_word_start,
};
pub use selection::Selection;
pub use syntax::{
    ConcealedLine, DisplayPad, HighlightTag, Rgba, StyleSpan, StyleValue, StyledSegment,
    SyntaxHighlighter, TextStyle, UnderlineDecoration, display_width, split_line_intervals,
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

    #[test]
    fn test_buffer_version_increments_on_edits() {
        let mut buffer = EditorBuffer::new("initial");
        assert_eq!(buffer.version(), 0);

        buffer.insert(" text");
        assert_eq!(buffer.version(), 1);

        buffer.backspace();
        assert_eq!(buffer.version(), 2);

        buffer.set_cursor_offset(0);
        buffer.delete();
        assert_eq!(buffer.version(), 3);

        buffer.undo();
        assert_eq!(buffer.version(), 4);

        buffer.redo();
        assert_eq!(buffer.version(), 5);
    }

    #[test]
    fn test_error_handling_validation() {
        let mut buffer = EditorBuffer::new("hello\nworld");

        assert!(buffer.validate_offset(0).is_ok());
        assert!(buffer.validate_offset(11).is_ok());
        assert!(matches!(
            buffer.validate_offset(12),
            Err(EditorError::OutOfBounds {
                offset: 12,
                len: 11
            })
        ));

        assert!(buffer.validate_range(&(0..5)).is_ok());
        let (inverted_start, inverted_end) = (5, 3);
        assert!(matches!(
            buffer.validate_range(&(inverted_start..inverted_end)),
            Err(EditorError::InvalidRange { .. })
        ));
        assert!(matches!(
            buffer.validate_range(&(0..20)),
            Err(EditorError::InvalidRange { .. })
        ));

        assert_eq!(buffer.try_line_to_string(0).unwrap(), "hello\n");
        assert_eq!(buffer.try_line_to_string(1).unwrap(), "world");
        assert!(matches!(
            buffer.try_line_to_string(2),
            Err(EditorError::InvalidRow {
                row: 2,
                total_lines: 2
            })
        ));

        assert!(buffer.try_replace_range(0..5, "hi").is_ok());
        assert_eq!(buffer.text().to_string(), "hi\nworld");
        assert!(buffer.try_delete_range(0..3).is_ok());
        assert_eq!(buffer.text().to_string(), "world");
    }

    #[test]
    fn test_file_io_roundtrip() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join(format!("twrite_test_{}.txt", buffer_version_rand()));

        let buffer = EditorBuffer::new("Persistent story content\nLine 2");
        assert!(buffer.save_to_file(&file_path).is_ok());

        let loaded = EditorBuffer::from_file(&file_path);
        assert!(loaded.is_ok());
        let loaded = loaded.unwrap();
        assert_eq!(
            loaded.text().to_string(),
            "Persistent story content\nLine 2"
        );

        let _ = std::fs::remove_file(&file_path);
    }

    fn buffer_version_rand() -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64
    }
}
