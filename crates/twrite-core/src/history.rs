use std::ops::Range;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    /// The byte range in the document before this edit was applied.
    pub bytes_range: Range<usize>,

    /// The text inserted at [`Self::bytes_range`].
    pub inserted_text: String,

    /// The text that was present at [`Self::bytes_range`] before the edit.
    pub deleted_text: String,
}

/// A group of edits that represents a single undoable operation.
///
/// A transaction may contain multiple edits that are undone and redone
/// together. The cursor positions record the state before and after the
/// transaction.
///
/// # Fields
///
/// * `edits` - The edits that make up this transaction.
/// * `previous_cursor` - The cursor position before the transaction.
/// * `resulting_cursor` - The cursor position after the transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transaction {
    /// The edits that make up this transaction.
    pub edits: Vec<Edit>,

    /// The cursor position before the transaction was applied.
    pub previous_cursor: usize,

    /// The cursor position after the transaction was applied.
    pub resulting_cursor: usize,
}

/// Tracks the undo and redo history of a document.
///
/// Transactions are stored in the undo stack when they are applied.
/// When a transaction is undone, it is moved to the redo stack. When
/// it is redone, it is moved back to the undo stack.
///
/// The most recent transaction is stored at the end of each stack.
///
/// Adding a new edit after undoing previous edits should clear the
/// redo stack, as the previous redo history is no longer applicable.
#[derive(Debug, Default)]
pub struct History {
    /// Transactions that can currently be undone.
    ///
    /// The most recent transaction is at the end of the vector.
    pub undo_stack: Vec<Transaction>,

    /// Transactions that can currently be redone.
    ///
    /// The most recently undone transaction is at the end of the vector.
    pub redo_stack: Vec<Transaction>,
}
