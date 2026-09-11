# Custom Prompts

Companion file: `examples/prompt.rs`. Two tiny hooks show the two prompt
shapes; both run on `ctx.prompt` with zero frontend code.

## Shape 1: submit-and-interpret (goto-line)

Open on a shortcut, wait for submit, interpret the string:

```rust
// Open:
ctx.prompt.open(
    PromptSpec::new("goto-line", ":", "Line number", PromptPlacement::BottomBar, false),
    "",
);
// On Submitted(input): parse, jump, close (or set_message() and stay open
// so the user can retry). Esc cancels for free.
```

## Shape 2: live-filtered palette (commands)

Re-rank rows on every keystroke with `fuzzy_filter`; `Up`/`Down` move,
`Tab` completes, `Enter` runs the highlighted row:

```rust
// On every Editing event:
let ranked = fuzzy_filter(&all_items, ctx.prompt.input());
ctx.prompt.set_items(ranked.into_iter().map(|(i, _)| all[i].clone()).collect());
// On submit: act on ctx.prompt.selected_item(), then close.
```

## What you get for free

Typing and cursor motion, `Ctrl+W` / `Ctrl+U` line editing, `Up`/`Down`
history, `Tab` completion, `Esc` handling, and the `PromptBar` rendering:
bottom bar or floating `TopPalette` depending on your spec. App-level work
(save, quit, load) goes through `HookEffect`: push to `ctx.effects` and the
host drains the queue after the event.

Need find/replace rather than a custom box? Don't rebuild it; embed the
stock `SearchHook` (which is this same pattern) like the vim example does.
