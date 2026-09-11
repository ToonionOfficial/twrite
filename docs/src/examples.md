# Examples Tour

The examples in `examples/` are the recommended reading order: the
numbered 1–6 sequence plus one supplemental demo. Each file header repeats
its number and prerequisites:

1. **`simple`**: bare window plus stock find. Start here.
2. **`hooks`**: a custom hook (`Ctrl+D` duplicate-line, list continuation,
   live word-count status) without overlapping battery territory.
3. **`syntax`**: a hand-written `SyntaxHighlighter` with custom theme tags.
4. **`prompt`**: goto-line bottom bar plus a live-filtered command palette,
   both on `ctx.prompt` with no frontend code.
5. **`markdown`**: the battery pattern: one `enable_markdown()` call plus a
   stock `SearchHook` (needs `--features markdown`).
6. **`vim`**: the full modal system on hooks alone; read last.
7. **`context_menu`** (supplemental, after `hooks`): the expandable right-click menu — built-in edit rows plus hook-contributed UPPERCASE/separator actions.

Run any of them with `cargo run --example <name>` (add `--features
markdown` for the markdown demo).
