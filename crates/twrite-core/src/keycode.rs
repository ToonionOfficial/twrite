//! Structured key identity for hooks, hints, and keymaps.
//!
//! GPUI hands out key *strings* ([`gpui::Keystroke`]); this module interprets
//! them once, at the translation boundary
//! (`twrite_gpui::translate_key_down`). Everything downstream — hook
//! matching, menu hints, vim-style sequences — speaks [`KeyCode`], so
//! inconsistent spellings (`"arrowup"` vs `"up"`, `"ctrl + l"` vs `"Ctrl+U"`)
//! are impossible by construction.

use std::fmt;

/// A single key, crossterm-style: one [`KeyCode::Char`] variant absorbs all
/// printable characters instead of per-letter variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyCode {
    /// Layout-aware produced character (`'a'`, `'A'`, `'?'`, `' '`).
    ///
    /// Space is a `Char(' ')`, not a named variant - there is no invisible
    /// `" "` string to confuse with emptiness.
    Char(char),
    /// The Enter / Return key.
    Enter,
    /// The Tab key.
    Tab,
    /// The Escape key.
    Escape,
    /// The Backspace key.
    Backspace,
    /// The Delete / Forward-delete key.
    Delete,
    /// The Insert key.
    Insert,
    /// Arrow up.
    Up,
    /// Arrow down.
    Down,
    /// Arrow left.
    Left,
    /// Arrow right.
    Right,
    /// The Home key.
    Home,
    /// The End key.
    End,
    /// The Page Up key.
    PageUp,
    /// The Page Down key.
    PageDown,
    /// A function key (`F(1)` is F1).
    F(u8),
    /// A platform key with no mapping. Matches must pass it through, never
    /// consume it: it carries no actionable meaning.
    Unidentified,
}

impl KeyCode {
    /// Canonical display form for kbd chips and logs: arrows as glyphs,
    /// well-known names as words, `F(n)` uppercased, characters verbatim,
    /// unidentified keys as `?`.
    pub fn display(&self) -> String {
        match self {
            KeyCode::Char(c) => c.to_string(),
            KeyCode::Enter => "Enter".to_string(),
            KeyCode::Tab => "Tab".to_string(),
            KeyCode::Escape => "Esc".to_string(),
            KeyCode::Backspace => "Backspace".to_string(),
            KeyCode::Delete => "Delete".to_string(),
            KeyCode::Insert => "Insert".to_string(),
            KeyCode::Up => "↑".to_string(),
            KeyCode::Down => "↓".to_string(),
            KeyCode::Left => "←".to_string(),
            KeyCode::Right => "→".to_string(),
            KeyCode::Home => "Home".to_string(),
            KeyCode::End => "End".to_string(),
            KeyCode::PageUp => "PgUp".to_string(),
            KeyCode::PageDown => "PgDn".to_string(),
            KeyCode::F(n) => format!("F{n}"),
            KeyCode::Unidentified => "?".to_string(),
        }
    }
}

impl fmt::Display for KeyCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_covers_named_keys() {
        assert_eq!(KeyCode::Enter.to_string(), "Enter");
        assert_eq!(KeyCode::Escape.to_string(), "Esc");
        assert_eq!(KeyCode::Backspace.to_string(), "Backspace");
        assert_eq!(KeyCode::Delete.to_string(), "Delete");
        assert_eq!(KeyCode::Up.to_string(), "↑");
        assert_eq!(KeyCode::Left.to_string(), "←");
        assert_eq!(KeyCode::F(5).to_string(), "F5");
        assert_eq!(KeyCode::F(12).to_string(), "F12");
    }

    #[test]
    fn display_chars_verbatim() {
        assert_eq!(KeyCode::Char('a').to_string(), "a");
        assert_eq!(KeyCode::Char(' ').to_string(), " ");
        assert_eq!(KeyCode::Char('?').to_string(), "?");
    }

    #[test]
    fn display_unidentified_is_placeholder() {
        assert_eq!(KeyCode::Unidentified.to_string(), "?");
    }
}
