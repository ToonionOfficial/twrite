# Concepts

Four ideas compose the whole crate. Learn these and every example reads
like prose.

## Editor

`Editor` (in `twrite-gpui`) is the GPUI view and controller: it owns the
`EditorBuffer`, the theme and config, the hook chain, and (since the prompt
work) the shared `PromptState` plus the pending `HookEffect` queue. Its key
handling has a fixed order:

1. Hooks run first via `on_key`; the first `Consumed` wins.
2. While a prompt is open, unconsumed keys never reach the buffer.
3. Otherwise the built-in editing keys apply (typing, undo, selection, ...).

## Hooks

`EditorHook` (in `twrite-core`) is the only extension point, and it's
GPUI-free. The methods you'll touch most:

- `on_key`: intercept input; return `Consumed` to halt the chain.
- `before_insert`: filter or rewrite typed text.
- `after_edit`: react to any buffer mutation.
- `on_selection_change`: track cursor/selection.
- `status_text`: feed the status bar.
- `search_snapshot`: expose search state to renderers (only search hooks).

`HookContext` hands every hook the same five handles: `buffer`, `selection`,
`cursor_style`, `prompt`, and `effects`. If you can express it through those,
it works on every frontend.

## Prompt

`PromptState` is the headless input box behind the bottom bar and the `F1`
palette. Open one with a `PromptSpec` (your own id, prefix, placeholder,
placement), route keys through `handle_key`, and read the `Submitted` input:

```rust
ctx.prompt.open(
    PromptSpec::new("goto-line", ":", "Line number", PromptPlacement::BottomBar, false),
    "",
);
// later, inside on_key while your spec is open:
match ctx.prompt.handle_key(event) {
    PromptAction::Editing => { /* live-update here */ }
    PromptAction::Submitted(input) => { /* interpret input, then close */ }
    PromptAction::Cancelled => { /* Esc already closed it */ }
    PromptAction::Ignored => { /* swallow while yours is open */ }
}
```

Typing, history, `Tab` completion, and `Esc` come free; `PromptBar` draws
whatever is open with no per-client rendering code. See
[Command Palettes & Prompts](recipe-prompt.md) for the full pattern.

## Batteries

Batteries are optional feature-gated packs (`twrite_core::batteries`) built
**only** on the public core API: the same surface you get. Markdown is the
reference battery: `Editor::enable_markdown()` wires its highlighter plus
hooks in one call. [Writing a Battery](battery.md) documents the contract.
