# Recipe: Custom Hooks & Shortcuts

Hooks (`EditorHook`) are the primary extension mechanism in TWrite. They are pure Rust traits with no direct GPUI dependencies, meaning your editor logic remains testable, decoupled, and reusable across frontends.

## 1. The `EditorHook` Trait

Implement `EditorHook` to tap into editor lifecycle events:

```rust
use twrite::{EditorHook, HookContext, HookOutcome, KeyCode, KeyEvent};

pub struct MyCustomHook;

impl EditorHook for MyCustomHook {
    /// Intercepts keyboard input before the editor buffer processes it.
    fn on_key(&mut self, ctx: &mut HookContext, event: &KeyEvent) -> HookOutcome {
        if event.code == KeyCode::Char('s') && event.modifiers.ctrl {
            println!("Custom Ctrl+S intercepted! Buffer has {} bytes", ctx.buffer.len_bytes());
            return HookOutcome::Consumed;
        }
        HookOutcome::PassThrough
    }

    /// Feeds custom status text to the editor status bar.
    fn status_text(&self) -> Option<&str> {
        Some("MY-HOOK ACTIVE")
    }
}
```

Register the hook on your editor instance:

```rust
ed.add_hook(MyCustomHook);
```

## 2. Hook Context Capabilities

`HookContext` provides mutable access to the editor core without exposing raw GPUI internals:

- `ctx.buffer`: Read or edit text, inspect lines, get/set cursor position.
- `ctx.selection`: Inspect or modify active text selection range.
- `ctx.cursor_style`: Change the cursor shape (`CursorStyle::Bar`, `Block`, `Underline`, `Hidden`).
- `ctx.prompt`: Open or manage headless single-line inputs and command palettes.
- `ctx.effects`: Push application-level events (`HookEffect::Save`, `HookEffect::Message`, etc.) for the GPUI host to handle.

## 3. Practical Example: Word Counter and Auto-Save Trigger

Here is a practical hook that calculates word count on edits and dispatches a save effect on `Ctrl+S`:

```rust
use twrite::{EditorHook, HookContext, HookEffect, HookOutcome, KeyEvent};

pub struct WordCountAndSaveHook {
    word_count: usize,
    status_display: String,
}

impl WordCountAndSaveHook {
    pub fn new() -> Self {
        Self {
            word_count: 0,
            status_display: "0 words".to_string(),
        }
    }

    fn recalculate(&mut self, text: &str) {
        self.word_count = text.split_whitespace().count();
        self.status_display = format!("{} words", self.word_count);
    }
}

impl EditorHook for WordCountAndSaveHook {
    fn on_key(&mut self, ctx: &mut HookContext, event: &KeyEvent) -> HookOutcome {
        if event.code == KeyCode::Char('s') && event.modifiers.ctrl && !event.modifiers.alt {
            ctx.effects.push(HookEffect::Save);
            ctx.effects.push(HookEffect::Message("Document saved".to_string()));
            return HookOutcome::Consumed;
        }
        HookOutcome::PassThrough
    }

    fn after_edit(&mut self, ctx: &mut HookContext) {
        let text = ctx.buffer.text();
        self.recalculate(&text);
    }

    fn status_text(&self) -> Option<&str> {
        Some(&self.status_display)
    }
}
```

## 4. Context Menu Items

Right-click opens a menu of built-in edit rows (Undo/Redo/Cut/Copy/Paste/Delete/Select All) followed by hook rows. Contribute rows with `context_menu_items` and handle clicks with `on_context_menu_action` (run before built-in dispatch; return `Consumed` to halt, including to suppress a default by reusing its id like `"copy"`):

```rust
use twrite::{ContextMenuContext, ContextMenuItem, EditorHook, HookContext, HookOutcome, KeyCode, KeyHint};

impl EditorHook for MyCustomHook {
    fn context_menu_items(&self, _ctx: &ContextMenuContext) -> Vec<ContextMenuItem> {
        vec![ContextMenuItem::with_hint(
            "my-action",
            "Do thing",
            KeyHint::ctrl(KeyCode::Char('D')),
        )]
    }

    fn on_context_menu_action(&mut self, ctx: &mut HookContext, id: &str) -> HookOutcome {
        if id == "my-action" {
            // ... mutate ctx.buffer / ctx.selection ...
            return HookOutcome::Consumed;
        }
        HookOutcome::PassThrough
    }
}
```

Toggle the menu with `EditorConfig.context_menu` and the built-in rows with `EditorConfig.show_default_menu_items`.

## 5. Hook Execution Order

Hooks run in the order they were added to `Editor`:

1. The first hook whose `on_key` returns `HookOutcome::Consumed` halts the key pipeline.
2. If no hook consumes the key, the key falls through to the active prompt (if open), and finally to the default buffer text editing handlers.
3. Therefore, register high-priority modal hooks (like search or Vim emulation) first.
