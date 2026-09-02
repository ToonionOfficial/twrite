/// A position within a text document.
///
/// The position is represented by a zero-based row and column.
/// The exact meaning of `column` depends on the document's coordinate
/// system, such as byte offset, character offset, or display column.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point {
    /// The zero-based row of the position.
    pub row: usize,

    /// The zero-based column of the position.
    pub column: usize,
}

impl Point {
    /// Creates a point at the given row and column.
    pub const fn new(row: usize, column: usize) -> Self {
        Self { row, column }
    }

    /// Creates a point at the beginning of the document.
    pub const fn zero() -> Self {
        Self { row: 0, column: 0 }
    }
}
