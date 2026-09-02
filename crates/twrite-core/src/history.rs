use std::ops::Range;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub bytes_range: Range<usize>,
    pub inserted_text: String,
    pub deleted_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transaction {
    pub edits: Vec<Edit>,
    pub previous_cursor: usize,
    pub resulting_cursor: usize,
}

#[derive(Debug, Default)]
pub struct History {
    pub undo_stack: Vec<Transaction>,
    pub redo_stack: Vec<Transaction>,
}
