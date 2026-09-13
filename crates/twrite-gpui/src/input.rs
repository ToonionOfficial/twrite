use gpui::KeyDownEvent;
use twrite_core::{KeyCode, KeyEvent, Modifiers};

/// Translates a GPUI [`KeyDownEvent`] into a platform-agnostic twrite [`KeyEvent`].
///
/// This is the single boundary where GPUI key *strings* are interpreted;
/// everything downstream matches on [`KeyCode`]. Arrow keys arrive under
/// different names per platform/backend (`left` vs `arrowleft`) and converge
/// on one variant here.
///
/// Printable characters come from [`Keystroke::key_char`] — the character
/// that would actually be typed (`"A"` with Shift held, layout-aware) —
///
/// rather than [`Keystroke::key`], which only names the physical key (`"a"`).
/// Using `key` would make capitals, shifted symbols, and non-US layouts
/// untypeable. `key_char` is honored only for genuine text input: command
/// combos (Ctrl/Cmd held) keep physical key names so `Ctrl+B`-style bindings
/// keep matching, and Option/Alt-modified keys keep theirs too.
///
/// NOTE (gpui 0.2.2): the git-era `KeyDownEvent::prefer_character_input`
/// signal (AltGr, macOS Option accents) does not exist in the 0.2.2 API, so
/// Alt-modified keys always keep physical names here. Revisit when upgrading
/// past 0.2.2 if that signal returns.
pub fn translate_key_down(event: &KeyDownEvent) -> Option<KeyEvent> {
    let keystroke = &event.keystroke;
    Some(KeyEvent {
        code: code_for(keystroke),
        modifiers: Modifiers {
            ctrl: keystroke.modifiers.control,
            alt: keystroke.modifiers.alt,
            shift: keystroke.modifiers.shift,
            meta: keystroke.modifiers.platform,
        },
    })
}

/// Maps one GPUI keystroke to a [`KeyCode`], preserving the historical
/// precedence: named physical keys first, then produced characters for
/// genuine text input, then single-char physical names (Ctrl/Cmd combos),
/// then the named table, else [`KeyCode::Unidentified`].
fn code_for(keystroke: &gpui::Keystroke) -> KeyCode {
    let mods = &keystroke.modifiers;
    match keystroke.key.as_str() {
        "space" => return KeyCode::Char(' '),
        "left" | "arrowleft" => return KeyCode::Left,
        "right" | "arrowright" => return KeyCode::Right,
        "up" | "arrowup" => return KeyCode::Up,
        "down" | "arrowdown" => return KeyCode::Down,
        _ => {}
    }
    if let Some(text) = &keystroke.key_char
        && text.chars().count() == 1
        && !mods.control
        && !mods.platform
        && !mods.alt
    {
        return KeyCode::Char(text.chars().next().unwrap());
    }
    if keystroke.key.chars().count() == 1 {
        return KeyCode::Char(keystroke.key.chars().next().unwrap().to_ascii_lowercase());
    }
    match keystroke.key.as_str() {
        "enter" => KeyCode::Enter,
        "tab" => KeyCode::Tab,
        "escape" => KeyCode::Escape,
        "backspace" => KeyCode::Backspace,
        "delete" => KeyCode::Delete,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        "insert" => KeyCode::Insert,
        name => parse_function_key(name).unwrap_or(KeyCode::Unidentified),
    }
}

/// Parses `F1`–`F24` in any case (`"f3"` is what GPUI emits on Linux).
fn parse_function_key(name: &str) -> Option<KeyCode> {
    let lower = name.to_ascii_lowercase();
    let digits = lower.strip_prefix('f')?;
    if !(1..=2).contains(&digits.len()) || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let n: u8 = digits.parse().ok()?;
    (1..=24).contains(&n).then_some(KeyCode::F(n))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keystroke(key: &str, key_char: Option<&str>, modifiers: gpui::Modifiers) -> KeyDownEvent {
        KeyDownEvent {
            keystroke: gpui::Keystroke {
                key: key.to_string(),
                key_char: key_char.map(|s| s.to_string()),
                modifiers,
            },
            is_held: false,
        }
    }

    fn plain(key: &str, key_char: Option<&str>) -> KeyDownEvent {
        keystroke(key, key_char, gpui::Modifiers::default())
    }

    #[test]
    fn shift_letter_yields_capital_with_shift_held() {
        let event = keystroke(
            "a",
            Some("A"),
            gpui::Modifiers {
                shift: true,
                ..Default::default()
            },
        );
        let translated = translate_key_down(&event).unwrap();
        assert_eq!(translated.code, KeyCode::Char('A'));
        assert!(translated.modifiers.shift);
    }

    #[test]
    fn plain_letter_passes_through() {
        let translated = translate_key_down(&plain("a", Some("a"))).unwrap();
        assert_eq!(translated.code, KeyCode::Char('a'));
        assert!(!translated.modifiers.shift);
    }

    #[test]
    fn shifted_symbol_uses_typed_character() {
        let event = keystroke(
            "/",
            Some("?"),
            gpui::Modifiers {
                shift: true,
                ..Default::default()
            },
        );
        assert_eq!(translate_key_down(&event).unwrap().code, KeyCode::Char('?'));
    }

    #[test]
    fn command_combos_keep_physical_key_names() {
        let event = keystroke(
            "b",
            Some("b"),
            gpui::Modifiers {
                control: true,
                ..Default::default()
            },
        );
        let translated = translate_key_down(&event).unwrap();
        assert_eq!(translated.code, KeyCode::Char('b'));
        assert!(translated.modifiers.ctrl);
    }

    #[test]
    fn alt_combos_keep_key_names() {
        // gpui 0.2.2 has no prefer_character_input signal, so Alt+C stays
        // a toggle binding (physical key name).
        let alt = gpui::Modifiers {
            alt: true,
            ..Default::default()
        };
        let translated = translate_key_down(&keystroke("c", Some("ç"), alt)).unwrap();
        assert_eq!(translated.code, KeyCode::Char('c'));
        assert!(translated.modifiers.alt);
    }

    #[test]
    fn multichar_or_missing_key_char_falls_back_to_key() {
        // IME composition sequences are not text input yet.
        let translated = translate_key_down(&plain("a", Some("aeiou"))).unwrap();
        assert_eq!(translated.code, KeyCode::Char('a'));
        // Modifier-only combos carry no character.
        let translated = translate_key_down(&plain("s", None)).unwrap();
        assert_eq!(translated.code, KeyCode::Char('s'));
    }

    #[test]
    fn named_keys_and_aliases_map_to_variants() {
        assert_eq!(
            translate_key_down(&plain("enter", None)).unwrap().code,
            KeyCode::Enter
        );
        assert_eq!(
            translate_key_down(&plain("left", None)).unwrap().code,
            KeyCode::Left
        );
        assert_eq!(
            translate_key_down(&plain("space", None)).unwrap().code,
            KeyCode::Char(' ')
        );
        assert_eq!(
            translate_key_down(&plain("f3", None)).unwrap().code,
            KeyCode::F(3)
        );
        assert_eq!(
            translate_key_down(&plain("back", None)).unwrap().code,
            KeyCode::Unidentified
        );
    }
}
