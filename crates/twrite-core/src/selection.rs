use std::ops::Range;

/// Represents a text selection using an anchor and a head.
///
/// The anchor is the position where the selection started, while the head
/// is the current cursor position. When they differ, the selection extends
/// between these two positions.
///
/// Both positions are byte offsets into the document.
///
/// # Examples
///
/// A collapsed selection, where the cursor is at byte offset 10:
///
/// ```
/// # use twrite_core::Selection;
/// let selection = Selection::point(10);
/// assert!(selection.is_empty());
/// ```
///
/// A selection from byte offset 10 to 20:
///
/// ```
/// # use twrite_core::Selection;
/// let selection = Selection::range(10, 20);
/// assert_eq!(selection.byte_range(), 10..20);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    /// The byte offset where the selection started.
    ///
    /// This remains fixed while the selection is extended or contracted
    /// by moving the head.
    pub anchor: usize,

    /// The current cursor position, represented as a byte offset.
    ///
    /// Moving the head changes the extent and direction of the selection
    /// while the anchor remains fixed.
    pub head: usize,
}

impl Selection {
    /// Creates a collapsed selection with the cursor at `offset`.
    ///
    /// Both the anchor and head are initialized to the same position.
    pub const fn point(offset: usize) -> Self {
        Self {
            anchor: offset,
            head: offset,
        }
    }

    /// Creates a selection from an anchor position to a head position.
    ///
    /// The positions are not reordered, so `anchor` may be greater than
    /// `head`. Use [`Self::byte_range`] to obtain the normalized range.
    pub const fn range(anchor: usize, head: usize) -> Self {
        Self { anchor, head }
    }

    /// Returns `true` if the selection is collapsed.
    ///
    /// A collapsed selection has the anchor and head at the same position.
    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    /// Returns the normalized byte range covered by the selection.
    ///
    /// The returned range always starts at the smaller of the anchor and
    /// head and ends at the larger, regardless of the direction in which
    /// the selection was made.
    pub fn byte_range(&self) -> Range<usize> {
        self.anchor.min(self.head)..self.anchor.max(self.head)
    }
}
