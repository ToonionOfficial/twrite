use gpui::KeyDownEvent;
use twrite_core::{KeyEvent, Modifiers};

/// Translates a GPUI [`KeyDownEvent`] into a platform-agnostic twrite [`KeyEvent`].
///
/// Arrow keys arrive under different names per platform/backend (`left` vs
/// `arrowleft`); they are normalized to the canonical `arrow*` form that the
/// core prompt and hook keymaps match on.
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
    let mods = &keystroke.modifiers;
    let key_str = match keystroke.key.as_str() {
        "space" => " ".to_string(),
        "left" => "arrowleft".to_string(),
        "right" => "arrowright".to_string(),
        "up" => "arrowup".to_string(),
        "down" => "arrowdown".to_string(),
        _ => match &keystroke.key_char {
            Some(text)
                if text.chars().count() == 1 && !mods.control && !mods.platform && !mods.alt =>
            {
                text.clone()
            }
            _ => keystroke.key.clone(),
        },
    };

    Some(KeyEvent {
        key: key_str,
        modifiers: Modifiers {
            ctrl: keystroke.modifiers.control,
            alt: keystroke.modifiers.alt,
            shift: keystroke.modifiers.shift,
            meta: keystroke.modifiers.platform,
        },
    })
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
        assert_eq!(translated.key, "A");
        assert!(translated.modifiers.shift);
    }

    #[test]
    fn plain_letter_passes_through() {
        let translated = translate_key_down(&plain("a", Some("a"))).unwrap();
        assert_eq!(translated.key, "a");
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
        assert_eq!(translate_key_down(&event).unwrap().key, "?");
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
        assert_eq!(translated.key, "b");
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
        assert_eq!(translated.key, "c");
        assert!(translated.modifiers.alt);
    }

    #[test]
    fn multichar_or_missing_key_char_falls_back_to_key() {
        // IME composition sequences are not text input yet.
        let translated = translate_key_down(&plain("a", Some("aeiou"))).unwrap();
        assert_eq!(translated.key, "a");
        // Modifier-only combos carry no character.
        let translated = translate_key_down(&plain("s", None)).unwrap();
        assert_eq!(translated.key, "s");
    }

    #[test]
    fn named_keys_and_aliases_are_untouched() {
        assert_eq!(
            translate_key_down(&plain("enter", None)).unwrap().key,
            "enter"
        );
        assert_eq!(
            translate_key_down(&plain("left", None)).unwrap().key,
            "arrowleft"
        );
        assert_eq!(translate_key_down(&plain("space", None)).unwrap().key, " ");
    }
}
