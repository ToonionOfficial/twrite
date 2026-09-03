use gpui::KeyDownEvent;
use twrite_core::{KeyEvent, Modifiers};

pub fn translate_key_down(event: &KeyDownEvent) -> Option<KeyEvent> {
    let keystroke = &event.keystroke;
    let key_str = match keystroke.key.as_str() {
        "space" => " ".to_string(),
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
