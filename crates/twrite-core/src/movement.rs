use std::ops::Range;

use ropey::Rope;

/// Character classification used for word-boundary detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CharKind {
    /// Whitespace characters (spaces, tabs, newlines).
    Whitespace,
    /// Alphanumeric characters and underscore (`_`).
    Word,
    /// Punctuation and symbols (`.`, `,`, `(`, `)`, `;`, `+`, etc.).
    Punctuation,
}

/// Classifies a character into [`CharKind`].
pub fn classify_char(c: char) -> CharKind {
    if c.is_whitespace() {
        CharKind::Whitespace
    } else if c.is_alphanumeric() || c == '_' {
        CharKind::Word
    } else {
        CharKind::Punctuation
    }
}

/// Finds the start offset of the previous word (or punctuation token) moving backward from `cursor_byte`.
///
/// Behavior matches classic editor `Ctrl + Left`:
/// 1. If preceded by whitespace (non-newline), skips backward across the whitespace.
/// 2. If preceded by a newline, stops at that line boundary.
/// 3. Identifies whether the preceding token is a word or punctuation sequence.
/// 4. Moves backward across consecutive characters of that same kind.
/// 5. Returns the starting byte offset of that token.
pub fn find_prev_word_start(text: &Rope, cursor_byte: usize) -> usize {
    let cursor_byte = cursor_byte.min(text.len_bytes());
    if cursor_byte == 0 {
        return 0;
    }

    let mut char_idx = text.byte_to_char(cursor_byte);
    if char_idx == 0 {
        return 0;
    }

    let prev_c = text.char(char_idx - 1);
    if prev_c == '\n' {
        if char_idx >= 2 && text.char(char_idx - 2) == '\r' {
            return text.char_to_byte(char_idx - 2);
        }
        return text.char_to_byte(char_idx - 1);
    }

    while char_idx > 0 {
        let c = text.char(char_idx - 1);
        if c == '\n' || c == '\r' {
            return text.char_to_byte(char_idx);
        }
        if classify_char(c) != CharKind::Whitespace {
            break;
        }
        char_idx -= 1;
    }

    if char_idx == 0 {
        return 0;
    }

    let target_kind = classify_char(text.char(char_idx - 1));

    while char_idx > 0 {
        let c = text.char(char_idx - 1);
        if c == '\n' || c == '\r' || classify_char(c) != target_kind {
            break;
        }
        char_idx -= 1;
    }

    text.char_to_byte(char_idx)
}

/// Finds the end offset of the current word or next word moving forward from `cursor_byte`.
///
/// Behavior matches classic editor `Ctrl + Right`:
/// 1. If currently at a newline, advances past the newline.
/// 2. If currently at non-newline whitespace, skips forward across the whitespace.
/// 3. Identifies whether the current token is a word or punctuation sequence.
/// 4. Moves forward across consecutive characters of that same kind.
/// 5. Returns the ending byte offset of that token.
pub fn find_next_word_end(text: &Rope, cursor_byte: usize) -> usize {
    let total_bytes = text.len_bytes();
    let cursor_byte = cursor_byte.min(total_bytes);
    if cursor_byte >= total_bytes {
        return total_bytes;
    }

    let total_chars = text.len_chars();
    let mut char_idx = text.byte_to_char(cursor_byte);
    if char_idx >= total_chars {
        return total_bytes;
    }

    let c = text.char(char_idx);
    if c == '\r' {
        if char_idx + 1 < total_chars && text.char(char_idx + 1) == '\n' {
            return text.char_to_byte(char_idx + 2);
        }
        return text.char_to_byte(char_idx + 1);
    }
    if c == '\n' {
        return text.char_to_byte(char_idx + 1);
    }

    while char_idx < total_chars {
        let c = text.char(char_idx);
        if c == '\n' || c == '\r' {
            return text.char_to_byte(char_idx);
        }
        if classify_char(c) != CharKind::Whitespace {
            break;
        }
        char_idx += 1;
    }

    if char_idx >= total_chars {
        return total_bytes;
    }

    let target_kind = classify_char(text.char(char_idx));

    while char_idx < total_chars {
        let c = text.char(char_idx);
        if c == '\n' || c == '\r' || classify_char(c) != target_kind {
            break;
        }
        char_idx += 1;
    }

    text.char_to_byte(char_idx)
}

/// Returns the byte offset of the beginning of the line containing `cursor_byte`.
pub fn find_line_start(text: &Rope, cursor_byte: usize) -> usize {
    let cursor_byte = cursor_byte.min(text.len_bytes());
    let char_idx = text.byte_to_char(cursor_byte);
    let line_idx = text.char_to_line(char_idx);
    let line_start_char = text.line_to_char(line_idx);
    text.char_to_byte(line_start_char)
}

/// Returns the byte offset of the end of the line containing `cursor_byte`,
/// excluding trailing newline characters (`\r\n` or `\n`).
pub fn find_line_end(text: &Rope, cursor_byte: usize) -> usize {
    let cursor_byte = cursor_byte.min(text.len_bytes());
    let char_idx = text.byte_to_char(cursor_byte);
    let line_idx = text.char_to_line(char_idx);
    let line = text.line(line_idx);
    let mut line_len_chars = line.len_chars();

    if line_len_chars > 0 && line.char(line_len_chars - 1) == '\n' {
        line_len_chars -= 1;
        if line_len_chars > 0 && line.char(line_len_chars - 1) == '\r' {
            line_len_chars -= 1;
        }
    }

    let line_start_char = text.line_to_char(line_idx);
    text.char_to_byte(line_start_char + line_len_chars)
}

/// Finds the byte range of the word, punctuation token, or whitespace run containing `cursor_byte`.
///
/// If `cursor_byte` points to whitespace or a line break immediately following a word or
/// punctuation token on the same line, the preceding token is selected. Otherwise, the
/// continuous token of the same [`CharKind`] spanning `cursor_byte` is returned, never
/// crossing line boundaries.
pub fn find_word_range_at(text: &Rope, cursor_byte: usize) -> Range<usize> {
    let total_bytes = text.len_bytes();
    let cursor_byte = cursor_byte.min(total_bytes);
    if total_bytes == 0 {
        return 0..0;
    }

    let total_chars = text.len_chars();
    let char_idx = text.byte_to_char(cursor_byte);

    let target_idx = if char_idx >= total_chars {
        if char_idx > 0 {
            let prev = text.char(char_idx - 1);
            if prev != '\n' && prev != '\r' {
                char_idx - 1
            } else {
                return cursor_byte..cursor_byte;
            }
        } else {
            return cursor_byte..cursor_byte;
        }
    } else {
        let curr = text.char(char_idx);
        if curr == '\n' || curr == '\r' {
            if char_idx > 0 {
                let prev = text.char(char_idx - 1);
                if prev != '\n' && prev != '\r' {
                    char_idx - 1
                } else {
                    return cursor_byte..cursor_byte;
                }
            } else {
                return cursor_byte..cursor_byte;
            }
        } else if classify_char(curr) == CharKind::Whitespace && char_idx > 0 {
            let prev = text.char(char_idx - 1);
            if prev != '\n' && prev != '\r' && classify_char(prev) != CharKind::Whitespace {
                char_idx - 1
            } else {
                char_idx
            }
        } else {
            char_idx
        }
    };

    let target_char = text.char(target_idx);
    let target_kind = classify_char(target_char);

    let mut start_idx = target_idx;
    while start_idx > 0 {
        let prev = text.char(start_idx - 1);
        if prev == '\n' || prev == '\r' || classify_char(prev) != target_kind {
            break;
        }
        start_idx -= 1;
    }

    let mut end_idx = target_idx + 1;
    while end_idx < total_chars {
        let next = text.char(end_idx);
        if next == '\n' || next == '\r' || classify_char(next) != target_kind {
            break;
        }
        end_idx += 1;
    }

    let start_byte = text.char_to_byte(start_idx);
    let end_byte = text.char_to_byte(end_idx);
    start_byte..end_byte
}

/// Returns the byte range of the full line containing `cursor_byte`, including
/// any trailing line terminator (`\n` or `\r\n`).
pub fn find_line_range_at(text: &Rope, cursor_byte: usize) -> Range<usize> {
    let total_bytes = text.len_bytes();
    let cursor_byte = cursor_byte.min(total_bytes);
    if total_bytes == 0 {
        return 0..0;
    }

    let char_idx = text.byte_to_char(cursor_byte);
    let line_idx = text.char_to_line(char_idx);
    let line_start_char = text.line_to_char(line_idx);
    let start = text.char_to_byte(line_start_char);

    let end = if line_idx + 1 < text.len_lines() {
        text.line_to_byte(line_idx + 1)
    } else {
        total_bytes
    };

    start..end
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_char() {
        assert_eq!(classify_char('a'), CharKind::Word);
        assert_eq!(classify_char('Z'), CharKind::Word);
        assert_eq!(classify_char('0'), CharKind::Word);
        assert_eq!(classify_char('_'), CharKind::Word);
        assert_eq!(classify_char(' '), CharKind::Whitespace);
        assert_eq!(classify_char('\t'), CharKind::Whitespace);
        assert_eq!(classify_char('\n'), CharKind::Whitespace);
        assert_eq!(classify_char('.'), CharKind::Punctuation);
        assert_eq!(classify_char('('), CharKind::Punctuation);
        assert_eq!(classify_char(';'), CharKind::Punctuation);
    }

    #[test]
    fn test_find_prev_word_start() {
        let text = Rope::from_str("hello world, foo.bar();");

        assert_eq!(find_prev_word_start(&text, 23), 20);
        assert_eq!(find_prev_word_start(&text, 20), 17);
        assert_eq!(find_prev_word_start(&text, 17), 16);
        assert_eq!(find_prev_word_start(&text, 16), 13);
        assert_eq!(find_prev_word_start(&text, 13), 11);
        assert_eq!(find_prev_word_start(&text, 11), 6);
        assert_eq!(find_prev_word_start(&text, 6), 0);
        assert_eq!(find_prev_word_start(&text, 0), 0);
    }

    #[test]
    fn test_find_prev_word_multiple_spaces() {
        let text = Rope::from_str("hello    world");
        assert_eq!(find_prev_word_start(&text, 14), 9);
        assert_eq!(find_prev_word_start(&text, 9), 0);
    }

    #[test]
    fn test_find_prev_word_across_lines() {
        let text = Rope::from_str("hello\nworld");
        assert_eq!(find_prev_word_start(&text, 11), 6);
        assert_eq!(find_prev_word_start(&text, 6), 5);
        assert_eq!(find_prev_word_start(&text, 5), 0);
    }

    #[test]
    fn test_find_next_word_end() {
        let text = Rope::from_str("hello world, foo.bar();");

        assert_eq!(find_next_word_end(&text, 0), 5);
        assert_eq!(find_next_word_end(&text, 5), 11);
        assert_eq!(find_next_word_end(&text, 11), 12);
        assert_eq!(find_next_word_end(&text, 12), 16);
        assert_eq!(find_next_word_end(&text, 16), 17);
        assert_eq!(find_next_word_end(&text, 17), 20);
        assert_eq!(find_next_word_end(&text, 20), 23);
        assert_eq!(find_next_word_end(&text, 23), 23);
    }

    #[test]
    fn test_find_next_word_multiple_spaces() {
        let text = Rope::from_str("hello    world");
        assert_eq!(find_next_word_end(&text, 0), 5);
        assert_eq!(find_next_word_end(&text, 5), 14);
    }

    #[test]
    fn test_find_line_boundaries() {
        let text = Rope::from_str("first line\nsecond line\nthird");

        assert_eq!(find_line_start(&text, 5), 0);
        assert_eq!(find_line_end(&text, 5), 10);

        assert_eq!(find_line_start(&text, 15), 11);
        assert_eq!(find_line_end(&text, 15), 22);

        assert_eq!(find_line_start(&text, 25), 23);
        assert_eq!(find_line_end(&text, 25), 28);
    }

    #[test]
    fn test_find_word_range_at() {
        let text = Rope::from_str("hello world, foo.bar();\nsecond line");

        assert_eq!(find_word_range_at(&text, 0), 0..5);
        assert_eq!(find_word_range_at(&text, 2), 0..5);
        assert_eq!(find_word_range_at(&text, 5), 0..5);

        assert_eq!(find_word_range_at(&text, 6), 6..11);
        assert_eq!(find_word_range_at(&text, 10), 6..11);

        assert_eq!(find_word_range_at(&text, 11), 11..12);

        assert_eq!(find_word_range_at(&text, 12), 11..12);

        assert_eq!(find_word_range_at(&text, 13), 13..16);
        assert_eq!(find_word_range_at(&text, 16), 16..17);
        assert_eq!(find_word_range_at(&text, 17), 17..20);
        assert_eq!(find_word_range_at(&text, 20), 20..23);
        assert_eq!(find_word_range_at(&text, 21), 20..23);
        assert_eq!(find_word_range_at(&text, 22), 20..23);

        assert_eq!(find_word_range_at(&text, 23), 20..23);

        assert_eq!(find_word_range_at(&text, 24), 24..30);

        let empty = Rope::from_str("");
        assert_eq!(find_word_range_at(&empty, 0), 0..0);

        let unicode = Rope::from_str("مرحبا بالعالم");
        assert_eq!(find_word_range_at(&unicode, 0), 0..10);
    }

    #[test]
    fn test_find_word_range_multiple_spaces() {
        let text = Rope::from_str("hello   world");
        assert_eq!(find_word_range_at(&text, 5), 0..5);
        assert_eq!(find_word_range_at(&text, 6), 5..8);
        assert_eq!(find_word_range_at(&text, 7), 5..8);
        assert_eq!(find_word_range_at(&text, 8), 8..13);
    }

    #[test]
    fn test_find_line_range_at() {
        let text = Rope::from_str("first line\nsecond line\nthird");

        assert_eq!(find_line_range_at(&text, 0), 0..11);
        assert_eq!(find_line_range_at(&text, 5), 0..11);
        assert_eq!(find_line_range_at(&text, 10), 0..11);

        assert_eq!(find_line_range_at(&text, 11), 11..23);
        assert_eq!(find_line_range_at(&text, 15), 11..23);

        assert_eq!(find_line_range_at(&text, 23), 23..28);
        assert_eq!(find_line_range_at(&text, 27), 23..28);
        assert_eq!(find_line_range_at(&text, 28), 23..28);

        let crlf = Rope::from_str("first\r\nsecond\r\n");
        assert_eq!(find_line_range_at(&crlf, 2), 0..7);
        assert_eq!(find_line_range_at(&crlf, 8), 7..15);

        let empty = Rope::from_str("");
        assert_eq!(find_line_range_at(&empty, 0), 0..0);
    }
}
