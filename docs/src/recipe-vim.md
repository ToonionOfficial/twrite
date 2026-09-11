# Recipe: Modal & Vim Editing

Because TWrite routes all user input through `EditorHook` before keys reach the text buffer, you can implement complete modal editing systems (like Vim or Kakoune) purely in Rust without modifying the GPUI renderer.

## 1. The Modal State Pattern

A modal editor maintains an active state enum:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VimMode {
    Normal,
    Insert,
    Visual,
}
```

## 2. Cursor Style Synchronization

Set `ctx.cursor_style` dynamically to reflect the current mode visually:

```rust
use twrite::{CursorStyle, HookContext};

fn sync_cursor(mode: VimMode, ctx: &mut HookContext) {
    match mode {
        VimMode::Normal => *ctx.cursor_style = CursorStyle::Block,
        VimMode::Insert => *ctx.cursor_style = CursorStyle::Bar,
        VimMode::Visual => *ctx.cursor_style = CursorStyle::Underline,
    }
}
```

## 3. Handling Keystrokes by Mode

In `on_key`:
- In **Normal Mode**: consume keys to execute commands (`h`, `j`, `k`, `l`, `w`, `b`, `x`, `u`). When `i` or `a` is pressed, switch to `Insert`.
- In **Insert Mode**: pass keys through so characters are typed into the buffer. When `Escape` is pressed, consume it and switch back to `Normal`.
- In **Visual Mode**: move selection range endpoints. When `Escape` or `y` is pressed, return to `Normal`. The full example adds a linewise variant: `V` selects whole lines (`-- VISUAL LINE --`), motions grow by line, and `G` / `gg` extend to the document bottom / top — so `ggVG` selects everything.

Here is a minimal demonstration:

```rust
use twrite::{
    CursorStyle, EditorHook, HookContext, HookOutcome, KeyEvent, Selection,
};

pub struct MinimalVimHook {
    mode: VimMode,
}

impl MinimalVimHook {
    pub fn new() -> Self {
        Self {
            mode: VimMode::Normal,
        }
    }
}

impl EditorHook for MinimalVimHook {
    fn on_key(&mut self, event: &KeyEvent, ctx: &mut HookContext) -> HookOutcome {
        match self.mode {
            VimMode::Normal => {
                match event.key.as_str() {
                    "i" => {
                        self.mode = VimMode::Insert;
                        *ctx.cursor_style = CursorStyle::Bar;
                        return HookOutcome::Consumed;
                    }
                    "h" | "arrowleft" => {
                        ctx.buffer.move_cursor_left();
                        return HookOutcome::Consumed;
                    }
                    "l" | "arrowright" => {
                        ctx.buffer.move_cursor_right();
                        return HookOutcome::Consumed;
                    }
                    "k" | "arrowup" => {
                        ctx.buffer.move_cursor_up();
                        return HookOutcome::Consumed;
                    }
                    "j" | "arrowdown" => {
                        ctx.buffer.move_cursor_down();
                        return HookOutcome::Consumed;
                    }
                    "x" => {
                        ctx.buffer.delete();
                        return HookOutcome::Consumed;
                    }
                    "u" => {
                        ctx.buffer.undo();
                        return HookOutcome::Consumed;
                    }
                    _ => {
                        // In normal mode, consume unhandled keys to prevent typing:
                        return HookOutcome::Consumed;
                    }
                }
            }
            VimMode::Insert => {
                if event.key == "escape" {
                    self.mode = VimMode::Normal;
                    *ctx.cursor_style = CursorStyle::Block;
                    return HookOutcome::Consumed;
                }
                // Allow characters to type naturally into buffer:
                HookOutcome::PassThrough
            }
            VimMode::Visual => {
                if event.key == "escape" {
                    self.mode = VimMode::Normal;
                    *ctx.selection = None;
                    *ctx.cursor_style = CursorStyle::Block;
                    return HookOutcome::Consumed;
                }
                HookOutcome::Consumed
            }
        }
    }

    fn status_text(&self) -> Option<&str> {
        match self.mode {
            VimMode::Normal => Some("-- NORMAL --"),
            VimMode::Insert => Some("-- INSERT --"),
            VimMode::Visual => Some("-- VISUAL --"),
        }
    }
}
```

## 4. Full Vim Implementation Reference

For a complete implementation that includes operators (`d`, `c`, `y`), motions, multi-key sequences (`gg`, `dd`), count prefixes (`3w`), ex-commands (`:w`, `:q`), and `/` search forwarding, inspect the repository example:

```sh
cargo run --example vim
```

Located in `examples/vim.rs`.
