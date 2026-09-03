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
}
