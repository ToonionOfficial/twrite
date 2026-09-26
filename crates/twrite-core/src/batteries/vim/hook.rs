use crate::EditorHook;

enum VimMode {
    Normal,
    Insert,
    Visual,
}

pub struct VimHook {
    mode: VimMode,
}

impl VimHook {}

impl EditorHook for VimHook {}
