//! Fold range computation for Markdown headings and lists.
//!
//! Line-based structural scan: headings follow the same `#` rules as
//! highlighting (setext excluded) and lists nest by indent width, so no
//! tree-sitter is involved. Fenced code and frontmatter rows never open
//! folds; fences absorb by indent like any other content row.

use std::ops::Range;

use crate::folding::FoldRange;
use crate::{EditorBuffer, is_fenced_row};

/// Heading level (1-6) for a trimmed line, shared with highlighting so the
/// two never disagree on what counts as a heading.
pub(crate) fn heading_level(trimmed_start: &str) -> Option<u8> {
    if trimmed_start.starts_with("# ") {
        Some(1)
    } else if trimmed_start.starts_with("## ") {
        Some(2)
    } else if trimmed_start.starts_with("### ") {
        Some(3)
    } else if trimmed_start.starts_with("#### ") {
        Some(4)
    } else if trimmed_start.starts_with("##### ") {
        Some(5)
    } else if trimmed_start.starts_with("###### ") {
        Some(6)
    } else {
        None
    }
}

/// Leading whitespace width in columns (tabs advance to 4-column stops),
/// matching the default tab size hosts configure.
fn indent_width(line: &str) -> usize {
    let mut width = 0;
    for ch in line.chars() {
        match ch {
            ' ' => width += 1,
            '\t' => width += 4 - width % 4,
            _ => break,
        }
    }
    width
}

/// Reports whether a trimmed line opens a list item: `-`, `*`, `+`, or an
/// ordered marker, each followed by a space or the line end. Task markers
/// qualify through their leading `- `/`* `.
fn is_list_item(trimmed_start: &str) -> bool {
    let bytes = trimmed_start.as_bytes();
    if bytes.len() == 1 && matches!(bytes[0], b'-' | b'*' | b'+') {
        return true;
    }
    if trimmed_start.starts_with("- ")
        || trimmed_start.starts_with("* ")
        || trimmed_start.starts_with("+ ")
    {
        return true;
    }
    let digits = bytes
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    if digits == 0 || digits >= bytes.len() {
        return false;
    }
    (bytes[digits] == b'.' || bytes[digits] == b')')
        && bytes.get(digits + 1).is_none_or(|byte| *byte == b' ')
}

/// Per-row structure used by the fold scan.
struct StructureRow {
    blank: bool,
    /// Heading level when the row opens a section.
    heading: Option<u8>,
    /// Indent width when the row opens a list subtree.
    list_indent: Option<usize>,
    /// Indent width for absorb-or-terminate decisions.
    indent: usize,
}

/// Computes foldable ranges for `buffer`, sorted by start row. Heading
/// sections run to the next heading of equal or higher level; list subtrees
/// run over deeper-indented rows. Trailing blanks trim off; single-row spans
/// never emit. Rows inside `fences` or `frontmatter` never open folds.
pub(crate) fn markdown_fold_ranges(
    buffer: &EditorBuffer,
    fences: &[usize],
    frontmatter: Option<&Range<usize>>,
) -> Vec<FoldRange> {
    let total = buffer.len_lines();
    let mut rows = Vec::with_capacity(total);
    for row in 0..total {
        let raw = buffer.line_to_string(row);
        let text = raw.trim_end_matches(['\r', '\n']);
        let trimmed = text.trim_start();
        let opaque = is_fenced_row(fences, row)
            || frontmatter
                .as_ref()
                .is_some_and(|block| block.contains(&row));
        rows.push(StructureRow {
            blank: trimmed.is_empty(),
            heading: (!opaque).then(|| heading_level(trimmed)).flatten(),
            list_indent: (!opaque && is_list_item(trimmed))
                .then(|| text.len() - trimmed.len())
                .map(|bytes| indent_width(&text[..bytes])),
            indent: indent_width(text),
        });
    }

    let mut ranges = Vec::new();
    // Open folds as (start row, level-or-indent) stacks; headings and lists
    // track separately so a list never swallows a heading section.
    let mut open_headings: Vec<(usize, u8)> = Vec::new();
    let mut open_lists: Vec<(usize, usize)> = Vec::new();

    for (row, info) in rows.iter().enumerate() {
        if info.blank {
            continue;
        }
        if let Some(level) = info.heading {
            // Headings terminate every open list, then nest by level.
            close_lists(&mut open_lists, &rows, row, 0, false, &mut ranges);
            close_headings(&mut open_headings, &rows, row, level, &mut ranges);
            open_headings.push((row, level));
        } else if let Some(indent) = info.list_indent {
            close_lists(&mut open_lists, &rows, row, indent, false, &mut ranges);
            open_lists.push((row, indent));
        } else {
            // Continuation content dedents strictly: same-indent lazy lines
            // absorb instead of ending the item.
            close_lists(&mut open_lists, &rows, row, info.indent, true, &mut ranges);
        }
    }
    // End of document closes everything still open.
    let end = total;
    close_lists(&mut open_lists, &rows, end, 0, false, &mut ranges);
    close_headings(&mut open_headings, &rows, end, 1, &mut ranges);

    ranges.sort_by_key(|range| range.start_row);
    ranges
}

/// Closes stacked list folds at or above `floor` indent, trimming trailing
/// blanks. Strict floors keep same-indent folds (lazy continuation lines
/// absorb); non-strict floors close siblings too.
fn close_lists(
    stack: &mut Vec<(usize, usize)>,
    rows: &[StructureRow],
    row: usize,
    floor: usize,
    strict: bool,
    ranges: &mut Vec<FoldRange>,
) {
    while stack.last().is_some_and(|&(_, indent)| {
        if strict {
            indent > floor
        } else {
            indent >= floor
        }
    }) {
        let (start, _) = stack.pop().expect("stack not empty");
        let mut end = row.saturating_sub(1);
        while end > start && rows[end].blank {
            end -= 1;
        }
        if end > start {
            ranges.push(FoldRange {
                start_row: start,
                end_row: end,
            });
        }
    }
}

/// Closes stacked heading folds at or above `floor` level, trimming blanks.
fn close_headings(
    stack: &mut Vec<(usize, u8)>,
    rows: &[StructureRow],
    row: usize,
    floor: u8,
    ranges: &mut Vec<FoldRange>,
) {
    while stack.last().is_some_and(|&(_, level)| level >= floor) {
        let (start, _) = stack.pop().expect("stack not empty");
        let mut end = row.saturating_sub(1);
        while end > start && rows[end].blank {
            end -= 1;
        }
        if end > start {
            ranges.push(FoldRange {
                start_row: start,
                end_row: end,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fold(buffer_text: &str) -> Vec<FoldRange> {
        let buffer = EditorBuffer::new(buffer_text);
        markdown_fold_ranges(&buffer, &[], None)
    }

    #[test]
    fn heading_sections_nest_by_level() {
        let ranges = fold("# H1\na\n## H2\nb\n# H1b\n");
        assert!(ranges.contains(&FoldRange {
            start_row: 0,
            end_row: 3
        }));
        assert!(ranges.contains(&FoldRange {
            start_row: 2,
            end_row: 3
        }));
        assert!(!ranges.iter().any(|range| range.start_row == 4));
    }

    #[test]
    fn single_row_sections_emit_nothing() {
        assert!(fold("# Solo\n").is_empty());
        assert!(fold("# A\n# B\n").is_empty());
    }

    #[test]
    fn trailing_blanks_trim_off() {
        let ranges = fold("# H\ntext\n\n\n");
        assert_eq!(
            ranges,
            vec![FoldRange {
                start_row: 0,
                end_row: 1
            }]
        );
    }

    #[test]
    fn list_subtrees_nest_by_indent() {
        let ranges = fold("- a\n  - b\n    - c\n  - d\n- e\n");
        assert!(ranges.contains(&FoldRange {
            start_row: 0,
            end_row: 3
        }));
        assert!(ranges.contains(&FoldRange {
            start_row: 1,
            end_row: 2
        }));
        assert!(!ranges.iter().any(|range| range.start_row == 4));
    }

    #[test]
    fn ordered_and_task_items_fold() {
        // "1. one" stands alone; "2. two" plus its continuation folds.
        let ranges = fold("1. one\n2. two\n   continued\n3. three\n");
        assert!(ranges.contains(&FoldRange {
            start_row: 1,
            end_row: 2
        }));
        assert!(!ranges.iter().any(|range| range.start_row == 0));
        assert!(!ranges.iter().any(|range| range.start_row == 3));
        let tasks = fold("- [ ] a\n  detail\n- [ ] b\n");
        assert!(ranges_contains(&tasks, 0, 1));
    }

    #[test]
    fn lazy_continuation_absorbs() {
        // Same-indent non-list text continues the item instead of ending it.
        let ranges = fold("- a\ncontinued\n- b\n");
        assert!(ranges.contains(&FoldRange {
            start_row: 0,
            end_row: 1
        }));
    }

    #[test]
    fn headings_terminate_lists() {
        let ranges = fold("- a\n  - b\n# H\ntext\n");
        assert!(ranges.contains(&FoldRange {
            start_row: 0,
            end_row: 1
        }));
        assert!(ranges.contains(&FoldRange {
            start_row: 2,
            end_row: 3
        }));
    }

    #[test]
    fn fenced_rows_never_open_folds() {
        let buffer = EditorBuffer::new("# H\n```\n# not a heading\n- not a list\n```\ntext\n");
        let ranges = markdown_fold_ranges(&buffer, &[1, 4], None);
        assert_eq!(
            ranges,
            vec![FoldRange {
                start_row: 0,
                end_row: 5
            }]
        );
    }

    #[test]
    fn frontmatter_rows_never_open_folds() {
        let buffer = EditorBuffer::new("---\n# not a heading\n---\n# Real\ntext\n");
        let ranges = markdown_fold_ranges(&buffer, &[], Some(&(0..3)));
        assert_eq!(
            ranges,
            vec![FoldRange {
                start_row: 3,
                end_row: 4
            }]
        );
    }

    #[test]
    fn shared_heading_levels_match_highlighting() {
        assert_eq!(heading_level("# H"), Some(1));
        assert_eq!(heading_level("###### H"), Some(6));
        assert_eq!(heading_level("#nospace"), None);
        assert_eq!(heading_level("####### H"), None);
    }

    fn ranges_contains(ranges: &[FoldRange], start: usize, end: usize) -> bool {
        ranges.contains(&FoldRange {
            start_row: start,
            end_row: end,
        })
    }
}
