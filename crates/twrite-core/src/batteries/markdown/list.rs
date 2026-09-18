use std::collections::HashMap;
use std::ops::Range;

use crate::{EditorBuffer, HookContext, Point, Selection};

/// Marker style for a Markdown list item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListMarker {
    Bullet {
        bullet: char,
        task: Option<char>,
    },
    Ordered {
        number: usize,
        delimiter: char,
        task: Option<char>,
    },
}

impl ListMarker {
    pub fn number(&self) -> Option<usize> {
        match self {
            Self::Ordered { number, .. } => Some(*number),
            Self::Bullet { .. } => None,
        }
    }
}

/// Parsed structure of a single list item line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedListItem {
    pub indent_width: usize,
    pub indent_bytes: usize,
    pub marker: ListMarker,
    pub content_start_byte: usize,
}

/// Computes leading indentation column width and byte length.
pub fn parse_indent(line: &str) -> (usize, usize) {
    let mut width = 0;
    let mut bytes = 0;
    for (index, character) in line.char_indices() {
        match character {
            ' ' => {
                width += 1;
                bytes = index + 1;
            }
            '\t' => {
                width += 4 - width % 4;
                bytes = index + 1;
            }
            _ => break,
        }
    }
    (width, bytes)
}

/// Parses a line into list item metadata if it starts with a recognized list marker.
pub fn parse_list_item(line: &str) -> Option<ParsedListItem> {
    let (indent_width, indent_bytes) = parse_indent(line);
    let trimmed = line.get(indent_bytes..)?;
    if trimmed.is_empty() {
        return None;
    }

    let trimmed_bytes = trimmed.as_bytes();

    // Standalone single-character bullet at EOF or line end.
    if trimmed_bytes.len() == 1 && matches!(trimmed_bytes[0], b'-' | b'*' | b'+') {
        let bullet = trimmed_bytes[0] as char;
        return Some(ParsedListItem {
            indent_width,
            indent_bytes,
            marker: ListMarker::Bullet { bullet, task: None },
            content_start_byte: indent_bytes + 1,
        });
    }

    if trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ ") {
        let bullet = trimmed_bytes[0] as char;
        let rest = trimmed.get(2..).unwrap_or("");
        if rest.starts_with("[ ] ") {
            return Some(ParsedListItem {
                indent_width,
                indent_bytes,
                marker: ListMarker::Bullet {
                    bullet,
                    task: Some(' '),
                },
                content_start_byte: indent_bytes + 6,
            });
        }
        if rest.starts_with("[x] ") || rest.starts_with("[X] ") {
            let task_character = rest.as_bytes().get(1).copied().unwrap_or(b'x') as char;
            return Some(ParsedListItem {
                indent_width,
                indent_bytes,
                marker: ListMarker::Bullet {
                    bullet,
                    task: Some(task_character),
                },
                content_start_byte: indent_bytes + 6,
            });
        }
        return Some(ParsedListItem {
            indent_width,
            indent_bytes,
            marker: ListMarker::Bullet { bullet, task: None },
            content_start_byte: indent_bytes + 2,
        });
    }

    let digit_count = trimmed_bytes
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    if digit_count > 0 && digit_count < trimmed_bytes.len() {
        let delimiter_byte = trimmed_bytes[digit_count];
        if delimiter_byte == b'.' || delimiter_byte == b')' {
            let delimiter = delimiter_byte as char;
            let after_delimiter = trimmed.get(digit_count + 1..).unwrap_or("");
            if after_delimiter.is_empty()
                || after_delimiter.starts_with('\n')
                || after_delimiter.starts_with('\r')
            {
                let number = trimmed.get(..digit_count)?.parse::<usize>().ok()?;
                return Some(ParsedListItem {
                    indent_width,
                    indent_bytes,
                    marker: ListMarker::Ordered {
                        number,
                        delimiter,
                        task: None,
                    },
                    content_start_byte: indent_bytes + digit_count + 1,
                });
            }
            if after_delimiter.starts_with(' ') {
                let number = trimmed.get(..digit_count)?.parse::<usize>().ok()?;
                let rest = after_delimiter.get(1..).unwrap_or("");
                if rest.starts_with("[ ] ") {
                    return Some(ParsedListItem {
                        indent_width,
                        indent_bytes,
                        marker: ListMarker::Ordered {
                            number,
                            delimiter,
                            task: Some(' '),
                        },
                        content_start_byte: indent_bytes + digit_count + 2 + 4,
                    });
                }
                if rest.starts_with("[x] ") || rest.starts_with("[X] ") {
                    let task_character = rest.as_bytes().get(1).copied().unwrap_or(b'x') as char;
                    return Some(ParsedListItem {
                        indent_width,
                        indent_bytes,
                        marker: ListMarker::Ordered {
                            number,
                            delimiter,
                            task: Some(task_character),
                        },
                        content_start_byte: indent_bytes + digit_count + 2 + 4,
                    });
                }
                return Some(ParsedListItem {
                    indent_width,
                    indent_bytes,
                    marker: ListMarker::Ordered {
                        number,
                        delimiter,
                        task: None,
                    },
                    content_start_byte: indent_bytes + digit_count + 2,
                });
            }
        }
    }

    None
}

/// Formats the line prefix (leading indentation + marker + trailing space) for a list item.
pub fn format_list_prefix(indent_spaces: usize, marker: &ListMarker) -> String {
    let indentation = " ".repeat(indent_spaces);
    match marker {
        ListMarker::Bullet { bullet, task: None } => {
            format!("{indentation}{bullet} ")
        }
        ListMarker::Bullet {
            bullet,
            task: Some(task_character),
        } => {
            format!("{indentation}{bullet} [{task_character}] ")
        }
        ListMarker::Ordered {
            number,
            delimiter,
            task: None,
        } => {
            format!("{indentation}{number}{delimiter} ")
        }
        ListMarker::Ordered {
            number,
            delimiter,
            task: Some(task_character),
        } => {
            format!("{indentation}{number}{delimiter} [{task_character}] ")
        }
    }
}

/// Discovers the contiguous list item block surrounding the specified row range.
fn find_list_block_range(
    buffer: &EditorBuffer,
    start_row: usize,
    end_row: usize,
) -> (usize, usize) {
    let total_lines = buffer.len_lines();
    let mut block_start = start_row;
    while block_start > 0 {
        let previous_row = block_start - 1;
        let line = buffer.line_to_string(previous_row);
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.trim_start().is_empty() || trimmed.trim_start().starts_with('#') {
            break;
        }
        if parse_list_item(trimmed).is_some() {
            block_start = previous_row;
        } else {
            break;
        }
    }

    let mut block_end = end_row;
    while block_end + 1 < total_lines {
        let next_row = block_end + 1;
        let line = buffer.line_to_string(next_row);
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.trim_start().is_empty() || trimmed.trim_start().starts_with('#') {
            break;
        }
        if parse_list_item(trimmed).is_some() {
            block_end = next_row;
        } else {
            break;
        }
    }

    (block_start, block_end)
}

struct RowPlan {
    row_index: usize,
    parsed: ParsedListItem,
    target_indent: usize,
    target_marker: ListMarker,
}

/// Handles `Tab` (indent/nest) or `Shift+Tab` (outdent/unnest) inside lists.
/// Returns `true` if consumed, or `false` to pass through.
pub fn handle_list_tab(ctx: &mut HookContext, outdent: bool, indent_size: usize) -> bool {
    let (target_start_row, target_end_row) = if let Some(selection) = ctx.selection.as_ref() {
        let range = selection.byte_range();
        let start_point = ctx.buffer.offset_to_point(range.start);
        let end_point = ctx.buffer.offset_to_point(range.end);
        let adjusted_end_row = if end_point.row > start_point.row && end_point.column == 0 {
            end_point.row - 1
        } else {
            end_point.row
        };
        (start_point.row, adjusted_end_row)
    } else {
        let row = ctx.buffer.cursor_point().row;
        (row, row)
    };

    // Verify at least one row in the target range is a list item.
    let mut any_list_item = false;
    for row_index in target_start_row..=target_end_row {
        let line = ctx.buffer.line_to_string(row_index);
        if parse_list_item(&line).is_some() {
            any_list_item = true;
            break;
        }
    }
    if !any_list_item {
        return false;
    }

    let (block_start_row, block_end_row) =
        find_list_block_range(ctx.buffer, target_start_row, target_end_row);

    let mut row_plans: Vec<RowPlan> = Vec::new();
    for row_index in block_start_row..=block_end_row {
        let line = ctx.buffer.line_to_string(row_index);
        if let Some(parsed) = parse_list_item(&line) {
            let target_indent = if row_index >= target_start_row && row_index <= target_end_row {
                if outdent {
                    parsed.indent_bytes.saturating_sub(indent_size)
                } else {
                    parsed.indent_bytes + indent_size
                }
            } else {
                parsed.indent_bytes
            };
            let target_marker = parsed.marker.clone();
            row_plans.push(RowPlan {
                row_index,
                parsed,
                target_indent,
                target_marker,
            });
        }
    }

    if row_plans.is_empty() {
        return false;
    }

    // Renumber ordered list items consecutively at each indentation level.
    let mut active_sequences: HashMap<usize, usize> = HashMap::new();
    for plan in &mut row_plans {
        let current_indent = plan.target_indent;
        // Clear sequences for any sublists deeper than the current indentation level.
        active_sequences.retain(|&level, _| level <= current_indent);

        match &plan.parsed.marker {
            ListMarker::Bullet { .. } => {
                active_sequences.remove(&current_indent);
            }
            ListMarker::Ordered {
                delimiter, task, ..
            } => {
                let next_number = match active_sequences.get(&current_indent) {
                    Some(&previous_number) => previous_number + 1,
                    None => {
                        if plan.row_index >= target_start_row && plan.row_index <= target_end_row {
                            1
                        } else {
                            plan.parsed.marker.number().unwrap_or(1)
                        }
                    }
                };
                active_sequences.insert(current_indent, next_number);
                plan.target_marker = ListMarker::Ordered {
                    number: next_number,
                    delimiter: *delimiter,
                    task: *task,
                };
            }
        }
    }

    // Build replacement edits for rows where the prefix has changed.
    let mut replacements: Vec<(Range<usize>, String)> = Vec::new();
    let mut prefix_deltas: HashMap<usize, (usize, usize)> = HashMap::new();

    for plan in &row_plans {
        let new_prefix = format_list_prefix(plan.target_indent, &plan.target_marker);
        let line = ctx.buffer.line_to_string(plan.row_index);
        let old_prefix = line.get(..plan.parsed.content_start_byte).unwrap_or("");

        prefix_deltas.insert(
            plan.row_index,
            (plan.parsed.content_start_byte, new_prefix.len()),
        );

        if new_prefix != old_prefix {
            let line_start = ctx.buffer.point_to_offset(Point::new(plan.row_index, 0));
            let range = line_start..line_start + plan.parsed.content_start_byte;
            replacements.push((range, new_prefix));
        }
    }

    let initial_cursor = ctx.buffer.cursor_point();
    let initial_selection = *ctx.selection;

    if !replacements.is_empty() {
        ctx.buffer.replace_many(replacements);
    }

    // Update cursor and selection positions after prefix alterations.
    if let Some(selection) = initial_selection {
        let start_point = ctx.buffer.offset_to_point(selection.byte_range().start);
        let end_point = ctx.buffer.offset_to_point(selection.byte_range().end);

        let adjusted_start_col =
            if let Some(&(old_len, new_len)) = prefix_deltas.get(&start_point.row) {
                if start_point.column <= old_len {
                    new_len
                } else {
                    new_len + (start_point.column - old_len)
                }
            } else {
                start_point.column
            };

        let adjusted_end_col = if let Some(&(old_len, new_len)) = prefix_deltas.get(&end_point.row)
        {
            if end_point.column <= old_len {
                new_len
            } else {
                new_len + (end_point.column - old_len)
            }
        } else {
            end_point.column
        };

        let new_start_offset = ctx
            .buffer
            .point_to_offset(Point::new(start_point.row, adjusted_start_col));
        let new_end_offset = ctx
            .buffer
            .point_to_offset(Point::new(end_point.row, adjusted_end_col));

        *ctx.selection = Some(Selection::range(new_start_offset, new_end_offset));
        ctx.buffer.set_cursor_offset(new_end_offset);
    } else {
        let adjusted_col = if let Some(&(old_len, new_len)) = prefix_deltas.get(&initial_cursor.row)
        {
            if initial_cursor.column <= old_len {
                new_len
            } else {
                new_len + (initial_cursor.column - old_len)
            }
        } else {
            initial_cursor.column
        };
        let new_cursor_offset = ctx
            .buffer
            .point_to_offset(Point::new(initial_cursor.row, adjusted_col));
        ctx.buffer.set_cursor_offset(new_cursor_offset);
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_list_item_bullets() {
        let item = parse_list_item("- Simple item").unwrap();
        assert_eq!(item.indent_width, 0);
        assert_eq!(
            item.marker,
            ListMarker::Bullet {
                bullet: '-',
                task: None
            }
        );
        assert_eq!(item.content_start_byte, 2);

        let item2 = parse_list_item("  * [x] Task").unwrap();
        assert_eq!(item2.indent_width, 2);
        assert_eq!(
            item2.marker,
            ListMarker::Bullet {
                bullet: '*',
                task: Some('x')
            }
        );
        assert_eq!(item2.content_start_byte, 8);
    }

    #[test]
    fn test_parse_list_item_ordered() {
        let item = parse_list_item("1. First").unwrap();
        assert_eq!(item.indent_width, 0);
        assert_eq!(
            item.marker,
            ListMarker::Ordered {
                number: 1,
                delimiter: '.',
                task: None
            }
        );
        assert_eq!(item.content_start_byte, 3);

        let item2 = parse_list_item("    12) [ ] Task").unwrap();
        assert_eq!(item2.indent_width, 4);
        assert_eq!(
            item2.marker,
            ListMarker::Ordered {
                number: 12,
                delimiter: ')',
                task: Some(' ')
            }
        );
        assert_eq!(item2.content_start_byte, 12);
    }

    #[test]
    fn test_format_list_prefix() {
        assert_eq!(
            format_list_prefix(
                2,
                &ListMarker::Bullet {
                    bullet: '-',
                    task: None
                }
            ),
            "  - "
        );
        assert_eq!(
            format_list_prefix(
                0,
                &ListMarker::Ordered {
                    number: 3,
                    delimiter: '.',
                    task: Some('x')
                }
            ),
            "3. [x] "
        );
    }
}
