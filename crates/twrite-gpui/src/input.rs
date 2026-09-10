use gpui::KeyDownEvent;
use twrite_core::{KeyEvent, Modifiers};

/// Translates a GPUI [`KeyDownEvent`] into a platform-agnostic twrite [`KeyEvent`].
///
/// Arrow keys arrive under different names per platform/backend (`left` vs
/// `arrowleft`); they are normalized to the canonical `arrow*` form that the
/// core prompt and hook keymaps match on.
pub fn translate_key_down(event: &KeyDownEvent) -> Option<KeyEvent> {
    let keystroke = &event.keystroke;
    let key_str = match keystroke.key.as_str() {
        "space" => " ".to_string(),
        "left" => "arrowleft".to_string(),
        "right" => "arrowright".to_string(),
        "up" => "arrowup".to_string(),
        "down" => "arrowdown".to_string(),
        _ => keystroke.key.clone(),
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
