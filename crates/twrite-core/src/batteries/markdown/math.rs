use std::ops::Range;

use crate::EditorBuffer;

use super::inline::{cursor_inside_construct, overlaps_reserved};
use super::table::is_fenced_row;
use crate::syntax::{HighlightTag, StyleSpan};

/// Custom highlight tag name emitted for single-line `$...$` math.
pub const MATH_INLINE_TAG: &str = "markdown.math.inline";
/// Custom highlight tag name emitted for `$$...$$` display math, both
/// single-line pairs and multi-line fence blocks.
pub const MATH_BLOCK_TAG: &str = "markdown.math.block";

/// Reports whether a line opens or closes a display math block: trimmed text
/// starts with `$$`. Lines holding a complete `$$...$$` pair are
/// self-contained and never fence rows; the inline pass styles those.
pub fn is_math_fence_line(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("$$") && count_double_dollar(trimmed) < 2
}

/// Counts non-overlapping `$$` occurrences in a line.
fn count_double_dollar(line: &str) -> usize {
    let bytes = line.as_bytes();
    let mut count = 0;
    let mut index = 0;
    while index + 1 < bytes.len() {
        if bytes[index] == b'$' && bytes[index + 1] == b'$' {
            count += 1;
            index += 2;
        } else {
            index += 1;
        }
    }
    count
}

/// Pairs math fence rows into display blocks, skipping rows inside fenced
/// code blocks so code wins over math. Ranges use exclusive ends; a fence
/// row is `block.start` or `block.end - 1`, content rows lie strictly
/// between. A trailing opener with no closer covers only its own row so an
/// unclosed `$$` never swallows the rest of the document.
pub fn scan_math_blocks(buffer: &EditorBuffer, code_fences: &[usize]) -> Vec<Range<usize>> {
    let mut blocks = Vec::new();
    let mut opener: Option<usize> = None;
    for row in 0..buffer.len_lines() {
        if is_fenced_row(code_fences, row) {
            continue;
        }
        if !is_math_fence_line(&buffer.line_to_string(row)) {
            continue;
        }
        match opener {
            Some(start) => {
                blocks.push(start..row + 1);
                opener = None;
            }
            None => opener = Some(row),
        }
    }
    if let Some(start) = opener {
        blocks.push(start..start + 1);
    }
    blocks
}

/// Returns the math block containing `row`, if any.
pub fn math_block_at(blocks: &[Range<usize>], row: usize) -> Option<Range<usize>> {
    blocks.iter().find(|block| block.contains(&row)).cloned()
}

/// Scans `$...$` and single-line `$$...$$` pairs pulldown-cmark does not
/// parse and appends math spans with the same conceal and reveal behavior as
/// emphasis. Pairs overlapping an emitted code, link-conceal, or concealed
/// span stay literal, so `$` inside code spans and URLs never corrupts them.
pub(crate) fn highlight_math_spans(
    line_text: &str,
    cursor_offset: Option<usize>,
    delimiter_tag: Option<HighlightTag>,
    spans: &mut Vec<StyleSpan>,
) {
    let bytes = line_text.as_bytes();
    let mut block_ranges: Vec<Range<usize>> = Vec::new();
    let mut search_from = 0;
    while let Some(open) = find_double_dollar(bytes, search_from, true) {
        let content_start = open + 2;
        match find_double_dollar(bytes, content_start, false) {
            Some(close) if !overlaps_reserved(spans, open, close + 2) => {
                let end = close + 2;
                push_math_span(
                    spans,
                    open,
                    close,
                    end,
                    MATH_BLOCK_TAG,
                    cursor_offset,
                    delimiter_tag,
                );
                block_ranges.push(open..end);
                search_from = end;
            }
            Some(_) => search_from = open + 1,
            None => break,
        }
    }

    search_from = 0;
    while search_from < bytes.len() {
        let Some(open) = find_single_dollar(bytes, search_from, true) else {
            break;
        };
        let content_start = open + 1;
        match find_single_dollar(bytes, content_start, false) {
            Some(close)
                if !overlaps_reserved(spans, open, close + 1)
                    && !block_ranges
                        .iter()
                        .any(|range| range.start < close + 1 && open < range.end) =>
            {
                let end = close + 1;
                push_math_span(
                    spans,
                    open,
                    close,
                    end,
                    MATH_INLINE_TAG,
                    cursor_offset,
                    delimiter_tag,
                );
                search_from = end;
            }
            Some(_) => search_from = open + 1,
            None => break,
        }
    }
}

/// Pushes delimiter concealment plus a tagged content span for one math pair.
/// `open..end` covers the whole pair; the delimiter length follows from the
/// closing edge (`end - close` is 1 for `$`, 2 for `$$`).
fn push_math_span(
    spans: &mut Vec<StyleSpan>,
    open: usize,
    close: usize,
    end: usize,
    tag: &'static str,
    cursor_offset: Option<usize>,
    delimiter_tag: Option<HighlightTag>,
) {
    let content_start = open + (end - close);
    if !cursor_inside_construct(cursor_offset, open, end)
        && let Some(delimiter) = delimiter_tag
    {
        spans.push(StyleSpan::tag(open..content_start, delimiter));
        spans.push(StyleSpan::tag(
            content_start..close,
            HighlightTag::Custom(tag),
        ));
        spans.push(StyleSpan::tag(close..end, delimiter));
        return;
    }
    spans.push(StyleSpan::tag(open..end, HighlightTag::Custom(tag)));
}

/// Locates the next `$$` delimiter at or after `from`. Candidates inside a
/// longer `$` run stay literal so `$$$` never splits into a pair plus
/// garbage. Openers need content after them, closers need content before.
fn find_double_dollar(bytes: &[u8], from: usize, opening: bool) -> Option<usize> {
    let mut index = from;
    while index + 1 < bytes.len() {
        if bytes[index] == b'$' && bytes[index + 1] == b'$' {
            let in_run = index
                .checked_sub(1)
                .is_some_and(|prev| bytes.get(prev).is_some_and(|byte| *byte == b'$'))
                || bytes.get(index + 2).is_some_and(|byte| *byte == b'$');
            let edge_has_content = if opening {
                bytes
                    .get(index + 2)
                    .is_some_and(|byte| !byte.is_ascii_whitespace())
            } else {
                index > 0
                    && bytes
                        .get(index - 1)
                        .is_some_and(|byte| !byte.is_ascii_whitespace())
            };
            if !in_run && edge_has_content {
                return Some(index);
            }
            index += 1;
        } else {
            index += 1;
        }
    }
    None
}

/// Locates the next lone `$` at or after `from`: not escaped, not touching
/// another `$`, and edged with content on the pairing side. Openers need a
/// non-space successor, closers a non-space predecessor.
fn find_single_dollar(bytes: &[u8], from: usize, opening: bool) -> Option<usize> {
    let mut index = from;
    while index < bytes.len() {
        if bytes[index] == b'$'
            && !(index > 0
                && bytes
                    .get(index - 1)
                    .is_some_and(|byte| *byte == b'$' || *byte == b'\\'))
            && !bytes.get(index + 1).is_some_and(|byte| *byte == b'$')
        {
            let edge_has_content = if opening {
                bytes
                    .get(index + 1)
                    .is_some_and(|byte| !byte.is_ascii_whitespace())
            } else {
                index > 0
                    && bytes
                        .get(index - 1)
                        .is_some_and(|byte| !byte.is_ascii_whitespace())
            };
            if edge_has_content {
                return Some(index);
            }
        }
        index += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_math_fence_line_detection() {
        assert!(is_math_fence_line("$$"));
        assert!(is_math_fence_line("  $$  "));
        assert!(is_math_fence_line("$$x + y"));
        assert!(!is_math_fence_line("$x$"));
        assert!(!is_math_fence_line("text $$"));
        assert!(!is_math_fence_line("$$x$$"));
    }

    #[test]
    fn test_math_blocks_pair_across_lines() {
        let buffer = EditorBuffer::new("text\n$$\nx + y\n$$\nmore");
        let blocks = scan_math_blocks(&buffer, &[]);
        assert_eq!(blocks, vec![1..4]);
        assert_eq!(math_block_at(&blocks, 0), None);
        assert_eq!(math_block_at(&blocks, 2), Some(1..4));
    }

    #[test]
    fn test_math_unclosed_opener_covers_only_its_row() {
        let buffer = EditorBuffer::new("text\n$$\nx + y\nmore");
        let blocks = scan_math_blocks(&buffer, &[]);
        assert_eq!(blocks, vec![1..2]);
        assert_eq!(math_block_at(&blocks, 3), None);
    }

    #[test]
    fn test_math_blocks_skip_code_fences() {
        let buffer = EditorBuffer::new("```\n$$\n```\n$$\n");
        let blocks = scan_math_blocks(&buffer, &[0, 2]);
        assert_eq!(blocks, vec![3..4]);
    }
}
