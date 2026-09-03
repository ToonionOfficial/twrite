use crate::EditorBuffer;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
}

impl Modifiers {
    pub const fn empty() -> Self {
        Self {
            ctrl: false,
            alt: false,
            shift: false,
            meta: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyEvent {
    pub key: String,
    pub modifiers: Modifiers,
}

impl KeyEvent {
    pub fn plain(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            modifiers: Modifiers::empty(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookOutcome {
    /// The hook handled the event; halt propagation
    Consumed,

    /// The hook ignored or passed on the event; continue to next hook or default typing
    PassThrough,
}

pub trait EditorHook: 'static {
    /// Intercepts a key press before it touches the editor buffer
    fn on_key(&mut self, buffer: &mut EditorBuffer, event: &KeyEvent) -> HookOutcome;

    /// Called immediately after any buffer mutation (eg. typing, paste, undo)
    fn after_edit(&mut self, _buffer: &mut EditorBuffer) {}
}
