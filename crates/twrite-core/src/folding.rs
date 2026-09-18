//! Collapsible fold ranges and their collapsed state.
//!
//! A [`FoldRange`] names a hidden-able row span whose first row (the header)
//! always stays visible; rows after it hide while collapsed. Range
//! computation is producer-specific (Markdown headings and lists live in the
//! battery); this module only tracks which starts are collapsed and answers
//! visibility queries against caller-supplied ranges sorted by start row.

use std::collections::BTreeSet;

/// A collapsible row span with inclusive bounds. The start row is the header
/// and is never hidden itself; hiding applies to `start_row + 1..=end_row`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FoldRange {
    /// First row of the fold; always visible.
    pub start_row: usize,
    /// Last hidden row while collapsed (inclusive).
    pub end_row: usize,
}

/// Which fold starts are collapsed. Owned by the editor view; row ranges come
/// from the highlighter per document version.
#[derive(Debug, Clone, Default)]
pub struct FoldState {
    collapsed: BTreeSet<usize>,
}

impl FoldState {
    /// Creates an empty fold state with nothing collapsed.
    pub fn new() -> Self {
        Self::default()
    }

    /// Toggles the fold starting at `start_row`, returning true when the fold
    /// ends up collapsed. Toggling a row that starts no range still flips
    /// membership so a later recompute revealing a fold there picks it up.
    pub fn toggle(&mut self, start_row: usize) -> bool {
        if self.collapsed.remove(&start_row) {
            false
        } else {
            self.collapsed.insert(start_row);
            true
        }
    }

    /// Reports whether the fold starting at `start_row` is collapsed.
    pub fn is_collapsed(&self, start_row: usize) -> bool {
        self.collapsed.contains(&start_row)
    }

    /// Drops collapsed starts absent from `ranges`, keeping the set aligned
    /// with structural edits that move or delete fold headers.
    pub fn retain(&mut self, ranges: &[FoldRange]) {
        self.collapsed
            .retain(|start| ranges.iter().any(|range| range.start_row == *start));
    }

    /// Returns the collapsed range hiding `row`, if any. Ranges must be
    /// sorted by start row; headers themselves are never hidden.
    pub fn hidden_range_at<'a>(
        &self,
        ranges: &'a [FoldRange],
        row: usize,
    ) -> Option<&'a FoldRange> {
        self.collapsed.iter().find_map(|start| {
            ranges
                .iter()
                .find(|range| range.start_row == *start && *start < row && row <= range.end_row)
        })
    }

    /// Reports whether `row` hides inside a collapsed range.
    pub fn is_row_hidden(&self, ranges: &[FoldRange], row: usize) -> bool {
        self.hidden_range_at(ranges, row).is_some()
    }

    /// Steps one visible row from `row`, skipping collapsed spans. Stays put
    /// when no visible row exists in that direction.
    pub fn step_visible_row(
        &self,
        ranges: &[FoldRange],
        row: usize,
        total_lines: usize,
        down: bool,
    ) -> usize {
        if total_lines == 0 {
            return 0;
        }
        let last = total_lines - 1;
        if down {
            let mut next = row.saturating_add(1);
            while next <= last {
                match self.hidden_range_at(ranges, next) {
                    Some(range) => next = range.end_row.saturating_add(1),
                    None => return next,
                }
            }
            row
        } else {
            if row == 0 {
                return 0;
            }
            let next = row - 1;
            loop {
                match self.hidden_range_at(ranges, next) {
                    // Headers stay visible, so their start row is the target.
                    Some(range) => return range.start_row,
                    None => return next,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ranges() -> Vec<FoldRange> {
        vec![
            FoldRange {
                start_row: 0,
                end_row: 5,
            },
            FoldRange {
                start_row: 1,
                end_row: 3,
            },
            FoldRange {
                start_row: 7,
                end_row: 9,
            },
        ]
    }

    #[test]
    fn toggle_flips_membership() {
        let mut state = FoldState::new();
        assert!(state.toggle(0));
        assert!(state.is_collapsed(0));
        assert!(!state.toggle(0));
        assert!(!state.is_collapsed(0));
    }

    #[test]
    fn headers_stay_visible() {
        let mut state = FoldState::new();
        state.toggle(0);
        assert!(!state.is_row_hidden(&ranges(), 0));
        assert!(state.is_row_hidden(&ranges(), 1));
        assert!(state.is_row_hidden(&ranges(), 5));
        assert!(!state.is_row_hidden(&ranges(), 6));
    }

    #[test]
    fn step_down_skips_collapsed_spans() {
        let mut state = FoldState::new();
        state.toggle(0);
        // From the header, Down jumps past the whole hidden span.
        assert_eq!(state.step_visible_row(&ranges(), 0, 10, true), 6);
        // From inside (possible after structural edits), same landing.
        assert_eq!(state.step_visible_row(&ranges(), 2, 10, true), 6);
        // Nothing visible below the last fold: stay put. Row 7 stays a
        // visible header, so Down from 6 still lands on it.
        state.toggle(7);
        assert_eq!(state.step_visible_row(&ranges(), 8, 10, true), 8);
        assert_eq!(state.step_visible_row(&ranges(), 6, 10, true), 7);
    }

    #[test]
    fn step_up_lands_on_headers() {
        let mut state = FoldState::new();
        state.toggle(0);
        // Up from below the fold lands on its header.
        assert_eq!(state.step_visible_row(&ranges(), 6, 10, false), 0);
        // Up from inside lands on the header too.
        assert_eq!(state.step_visible_row(&ranges(), 4, 10, false), 0);
        assert_eq!(state.step_visible_row(&ranges(), 0, 10, false), 0);
        // Non-zero headers work the same way.
        state.toggle(7);
        assert_eq!(state.step_visible_row(&ranges(), 9, 10, false), 7);
        assert_eq!(state.step_visible_row(&ranges(), 8, 10, true), 8);
        // No folds: plain stepping.
        let open = FoldState::new();
        assert_eq!(open.step_visible_row(&ranges(), 6, 10, false), 5);
        assert_eq!(open.step_visible_row(&ranges(), 6, 10, true), 7);
    }

    #[test]
    fn retain_drops_vanished_starts() {
        let mut state = FoldState::new();
        state.toggle(0);
        state.toggle(7);
        state.retain(&[FoldRange {
            start_row: 7,
            end_row: 9,
        }]);
        assert!(!state.is_collapsed(0));
        assert!(state.is_collapsed(7));
    }

    #[test]
    fn empty_state_hides_nothing() {
        let state = FoldState::new();
        for row in 0..10 {
            assert!(!state.is_row_hidden(&ranges(), row));
        }
    }
}
