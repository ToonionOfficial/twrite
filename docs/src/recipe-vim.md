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
    fn on_key(&mut self, ctx: &mut HookContext, event: &KeyEvent) -> HookOutcome {
        match self.mode {
            VimMode::Normal => {
                match &event.code {
                    KeyCode::Char('i') => {
                        self.mode = VimMode::Insert;
                        *ctx.cursor_style = CursorStyle::Bar;
                        return HookOutcome::Consumed;
                    }
                    KeyCode::Char('h') | KeyCode::Left => {
                        ctx.buffer.move_cursor_left();
                        return HookOutcome::Consumed;
                    }
                    KeyCode::Char('l') | KeyCode::Right => {
                        ctx.buffer.move_cursor_right();
                        return HookOutcome::Consumed;
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        ctx.buffer.move_cursor_up();
                        return HookOutcome::Consumed;
                    }
                    KeyCode::Char('j') | KeyCode::Down => {
                        ctx.buffer.move_cursor_down();
                        return HookOutcome::Consumed;
                    }
                    KeyCode::Char('x') => {
                        ctx.buffer.delete();
                        return HookOutcome::Consumed;
                    }
                    KeyCode::Char('u') => {
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
                if event.code == KeyCode::Escape {
                    self.mode = VimMode::Normal;
                    *ctx.cursor_style = CursorStyle::Block;
                    return HookOutcome::Consumed;
                }
                // Allow characters to type naturally into buffer:
                HookOutcome::PassThrough
            }
            VimMode::Visual => {
                if event.code == KeyCode::Escape {
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

## 4. Search Submit and `n` / `N`

The stock `SearchHook` is a persistent toolbar: `Enter` navigates but leaves the prompt open. For vim-modal `/` / `?`, track a `search_modal` flag when opening the search and, on plain `Enter`, delegate to the hook and then close just the prompt (keeping the query, matches, and highlight wash for `n` / `N`). The shared `Ctrl+F` toolbar path leaves the flag unset, so it keeps its stay-open behavior. `*` / `#` jump immediately and dismiss at once, so a later `Enter` can't skip a match. Since submit leaves the hook active, `Escape` (with the prompt closed) deactivates it again — clearing the highlight wash and the `SEARCH` status.

## 5. Full Vim Implementation Reference

For a complete implementation that includes operators (`d`, `c`, `y`), motions, multi-key sequences (`gg`, `dd`), count prefixes (`3w`), ex-commands (`:w`, `:q`), `/` search forwarding, and linewise Visual mode (`V`, `ggVG`), inspect the repository example:

```sh
cargo run --example vim
```

Located in `examples/vim.rs`.
