# Recipe: Command Palettes & Prompts

TWrite provides a headless prompt system (`PromptState`) capable of driving bottom-line inputs (such as `:goto-line`) and floating top command palettes (such as an `F1` or `Ctrl+P` quick picker).

## 1. Two Prompt Shapes

Prompts are configured using `PromptPlacement`:

- `PromptPlacement::BottomBar`: Single line anchored to the bottom border with optional leading prefix (for example, `:` or `/`).
- `PromptPlacement::TopPalette`: Centered floating modal overlay with item list and fuzzy search filtering.

## 2. Opening and Routing a Prompt

A hook opens a prompt via `ctx.prompt.open(...)`, and routes incoming keystrokes through `ctx.prompt.handle_key(event)`:

```rust
use twrite::{
    EditorHook, HookContext, HookOutcome, KeyCode, KeyEvent, PromptAction, PromptPlacement,
    PromptSpec,
};

pub struct GotoLineHook;

impl EditorHook for GotoLineHook {
    fn on_key(&mut self, ctx: &mut HookContext, event: &KeyEvent) -> HookOutcome {
        // Open on Ctrl+G:
        if event.code == KeyCode::Char('g') && event.modifiers.ctrl {
            ctx.prompt.open(
                PromptSpec::new("goto-line", ":", "Enter line number", PromptPlacement::BottomBar, false),
                "",
            );
            return HookOutcome::Consumed;
        }

        // When our prompt is open, route keys to it:
        if ctx.prompt.spec().is_some_and(|s| s.id == "goto-line") {
            match ctx.prompt.handle_key(event) {
                PromptAction::Submitted(input) => {
                    if let Ok(line_num) = input.trim().parse::<usize>() {
                        let target_row = line_num.saturating_sub(1);
                        ctx.buffer.set_cursor_point(twrite::Point::new(target_row, 0));
                        ctx.prompt.close();
                    } else {
                        ctx.prompt.set_message("Invalid line number".to_string());
                    }
                    return HookOutcome::Consumed;
                }
                PromptAction::Cancelled => {
                    return HookOutcome::Consumed;
                }
                PromptAction::Editing => {
                    return HookOutcome::Consumed;
                }
                PromptAction::Ignored => {}
            }
        }

        HookOutcome::PassThrough
    }
}
```

## 3. Fuzzy-Filtered Command Palette

To build a floating `F1` command palette with live fuzzy filtering, populate item rows whenever the input changes:

```rust
use twrite::prompt::{PromptItem, fuzzy_filter};
use twrite::{
    EditorHook, HookContext, HookOutcome, KeyCode, KeyEvent, PromptAction, PromptPlacement,
    PromptSpec,
};

pub struct CommandPaletteHook {
    available_commands: Vec<PromptItem>,
}

impl CommandPaletteHook {
    pub fn new() -> Self {
        Self {
            available_commands: vec![
                PromptItem::with_hint("Toggle Line Numbers", "Alt+L"),
                PromptItem::with_hint("Enable Markdown", "Alt+M"),
                PromptItem::with_hint("Save File", "Ctrl+S"),
                PromptItem::with_hint("Find in Page", "Ctrl+F"),
            ],
        }
    }

    fn update_filtered_items(&self, ctx: &mut HookContext) {
        let query = ctx.prompt.input();
        let ranked = fuzzy_filter(&self.available_commands, query);
        let items: Vec<PromptItem> = ranked
            .into_iter()
            .map(|(idx, _score)| self.available_commands[idx].clone())
            .collect();
        ctx.prompt.set_items(items);
    }
}

impl EditorHook for CommandPaletteHook {
    fn on_key(&mut self, ctx: &mut HookContext, event: &KeyEvent) -> HookOutcome {
        if event.code == KeyCode::F(1)
            || (event.code == KeyCode::Char('p') && event.modifiers.ctrl)
        {
            ctx.prompt.open(
                PromptSpec::new(
                    "command-palette",
                    ">",
                    "Type a command...",
                    PromptPlacement::TopPalette,
                    true,
                ),
                "",
            );
            self.update_filtered_items(ctx);
            return HookOutcome::Consumed;
        }

        if ctx.prompt.spec().is_some_and(|s| s.id == "command-palette") {
            match ctx.prompt.handle_key(event) {
                PromptAction::Editing => {
                    self.update_filtered_items(ctx);
                    return HookOutcome::Consumed;
                }
                PromptAction::Submitted(_input) => {
                    if let Some(selected) = ctx.prompt.selected_item() {
                        println!("Executing command: {}", selected.label);
                    }
                    ctx.prompt.close();
                    return HookOutcome::Consumed;
                }
                PromptAction::Cancelled => return HookOutcome::Consumed,
                PromptAction::Ignored => {}
            }
        }

        HookOutcome::PassThrough
    }
}
```

## 4. Built-in Features of All Prompts

Every open prompt automatically includes:
- **Vertical bar cursor**: Thin cursor with full UTF-8 character boundary safety.
- **Word navigation & deletion**: `Ctrl+Left` / `Ctrl+Right`, `Ctrl+Backspace` / `Ctrl+Delete`, `Ctrl+K`.
- **History navigation**: `Up` and `Down` recall previously submitted inputs (unless item rows are visible).
- **Tab completion**: Pressing `Tab` fills the input with the highlighted row label.
- **Escape**: Automatically cancels and closes the prompt.
