use std::ops::Range;
use thiserror::Error;

/// Error type for editor buffer validation, range operations, and file I/O.
#[derive(Debug, Error)]
pub enum EditorError {
    /// The specified byte offset is out of document bounds.
    #[error("byte offset {offset} is out of bounds (document length: {len})")]
    OutOfBounds {
        /// The requested byte offset.
        offset: usize,
        /// The total length of the document in bytes.
        len: usize,
    },

    /// The specified row index is out of document line bounds.
    #[error("line index {row} is out of bounds (total lines: {total_lines})")]
    InvalidRow {
        /// The requested row index.
        row: usize,
        /// The total number of lines in the document.
        total_lines: usize,
    },

    /// The range is invalid because start offset exceeds end offset or document bounds.
    #[error("invalid byte range: {range:?} (document length: {len})")]
    InvalidRange {
        /// The requested byte range.
        range: Range<usize>,
        /// The total length of the document in bytes.
        len: usize,
    },

    /// The byte offset does not land on a valid UTF-8 character boundary.
    #[error("byte offset {offset} is not a valid UTF-8 character boundary")]
    InvalidCharBoundary {
        /// The invalid byte offset.
        offset: usize,
    },

    /// An I/O error occurred during file reading or writing.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Specialized Result type for editor operations.
pub type Result<T> = std::result::Result<T, EditorError>;
