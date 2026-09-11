# Your First Hook

Companion file: `examples/hooks.rs`. This guide walks its shape; read the
file alongside.

## The skeleton

Every hook is a struct implementing `EditorHook`. The scratch hook in the
example shows the three methods you'll use most:

```rust
struct ScratchHook { words: usize, status_line: String }

impl EditorHook for ScratchHook {
    fn on_key(&mut self, ctx: &mut HookContext, event: &KeyEvent) -> HookOutcome {
        // Ctrl+D duplicates the current line via ctx.buffer ...
        // Enter continues "- " lists ...
        HookOutcome::PassThrough
    }

    fn after_edit(&mut self, buffer: &mut EditorBuffer) {
        // recount words, refresh the cached status line
    }

    fn status_text(&self) -> Option<&str> {
        Some(&self.status_line)
    }
}
```

Register hooks in order: the chain is first-`Consumed`-wins:

```rust
ed.add_hook(AutoPairsHook::new());
ed.add_hook(ScratchHook::new(initial_text));
```

## Rules that save debugging time

- **Check your spec id.** When several hooks share the prompt, only handle
  keys while *your* spec is open; pass everything else through.
- **Swallow while yours is open.** An `Ignored` key with your prompt open
  should still return `Consumed`, or the keystroke leaks to hooks behind you
  (and, with no prompt open, to the buffer).
- **Keep `status_text` cheap.** It runs on every render; cache strings like
  the example does instead of formatting per call.
- **Don't reimplement batteries.** Formatting shortcuts, lists, tables, and
  tasks already exist behind `Editor::enable_markdown()`; reach for hooks
  for behavior no battery provides.

Next: [Custom Prompts](guide-prompt.md).
