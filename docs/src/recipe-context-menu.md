# Recipe: Right-Click Context Menu

Right-click opens an expandable context menu. The editor supplies built-in edit rows; any hook can append its own rows via `EditorHook::context_menu_items` and handle them via `EditorHook::on_context_menu_action`. Like `PromptState`, the state is headless (`twrite_core::ContextMenuState`) and the GPUI layer only draws rows.

## 1. Built-in Rows

With `EditorConfig.show_default_menu_items` (default `true`), the menu leads with:

| id | Label | Enabled when |
|---|---|---|
| `undo` | Undo (`Ctrl+Z`) | undo history is non-empty |
| `redo` | Redo (`Ctrl+Y`) | redo history is non-empty |
| `cut` | Cut (`Ctrl+X`) | a non-empty selection exists |
| `copy` | Copy (`Ctrl+C`) | a non-empty selection exists |
| `paste` | Paste (`Ctrl+V`) | the OS clipboard holds text |
| `delete` | Delete | a non-empty selection exists |
| `select_all` | Select All (`Ctrl+A`) | always (disabled when the whole document is already selected) |

Disabled rows render dimmed and ignore clicks. The clipboard check happens once per right-click in the GPUI host and is passed into the core as `ContextMenuCaps`, so item logic stays headless-testable.

## 2. Contributing Rows

Return rows from `context_menu_items`. You get a read-only snapshot: the click's buffer coordinates (`clicked_row`, `clicked_col`), the selection, and the caps above, so rows can be position-aware (e.g. only offer "Toggle task" on a task line):

```rust
use twrite::{ContextMenuContext, ContextMenuItem, EditorHook};

pub struct MyMenuHook;

impl EditorHook for MyMenuHook {
    fn context_menu_items(&self, ctx: &ContextMenuContext) -> Vec<ContextMenuItem> {
        let has_selection = ctx.selection.is_some_and(|s| !s.byte_range().is_empty());
        let mut shout = ContextMenuItem::with_hint("my.uppercase", "UPPERCASE selection", "Ctrl+U");
        shout.enabled = has_selection;
        vec![shout]
    }
}
```

Rows append after the built-ins (and after earlier hooks' rows) in hook registration order. A separator divides the built-in block from hook rows automatically. Returning an item with a well-known id such as `"copy"` overrides that default in place — same position, your label and handler.

## 3. Handling Actions

Activations run hooks first, then fall through to the built-in edit dispatch. Return `Consumed` to halt (this also suppresses a same-id default):

```rust
use twrite::{EditorHook, HookContext, HookOutcome};

impl EditorHook for MyMenuHook {
    fn on_context_menu_action(&mut self, ctx: &mut HookContext, id: &str) -> HookOutcome {
        if id == "my.uppercase" {
            if let Some(sel) = ctx.selection.take() {
                let range = sel.byte_range();
                if !range.is_empty() {
                    let text = ctx.buffer.text().byte_slice(range.clone()).to_string();
                    ctx.buffer.replace_range(range, &text.to_uppercase());
                }
            }
            return HookOutcome::Consumed;
        }
        HookOutcome::PassThrough
    }
}
```

`HookContext` gives the same mutable access as key handling (`buffer`, `selection`, `cursor_style`, `prompt`, `effects`), and the editor runs the usual post-processing (`after_edit`, selection callbacks, scrolling, effect flushing) after your action.

## 4. Interaction Contract

- **Selection policy:** a right-click inside the active selection keeps it (so Cut/Copy act on it); otherwise the cursor moves to the click and the selection collapses.
- **Dismissal:** left-click anywhere, mouse-wheel scroll, `Escape`, or picking a row closes the menu. Typing any other key closes it and the keystroke still lands in the buffer.
- **Keyboard:** `Up`/`Down` move across enabled rows, `Enter` activates the highlighted row.
- **Positioning:** the popup anchors at the click and clamps into the viewport.

## 5. Configuration

- `EditorConfig.context_menu` (`true`): master switch for right-click menus.
- `EditorConfig.show_default_menu_items` (`true`): include the built-in edit block; hooks-only menus set this to `false`.
- `EditorTheme.menu_bg`, `menu_border`, `menu_hover`, `menu_fg`, `menu_hint`: popup styling.

## 6. Try It

Run the dedicated demo (custom UPPERCASE + separator actions on top of the defaults):

```sh
cargo run --example context_menu
```
