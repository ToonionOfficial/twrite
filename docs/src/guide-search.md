# Search Integration

You have two integration points depending on how much control you need.

## Drop-in: `SearchHook`

Register it (first, so its keys win) and you get the full find/replace UX
with no further code:

```rust
ed.add_hook(SearchHook::new());
```

- `Ctrl+F` find (selection seeds the query), live refresh, `Enter` / `F3` /
  `Down` next, `Shift+F3` / `Up` previous, `Alt+Up` / `Alt+Down` history.
- `Ctrl+H` replace field, `Ctrl+Enter` replace-one, `Alt+A` single-undo
  replace-all, `Esc` close.
- `Alt+C` / `Alt+W` / `Alt+H` flip Match Case / Whole Word / Highlight All;
  the status line shows live state (`SEARCH 2/7 [Aa] [w] [H]`).
- The bar renders clickable `Aa` / `W` / `All` chips plus `↑` / `↓` steppers
  through the same key path, and the canvas washes every match.

## Composed: own it inside your hook

The vim example embeds `SearchHook` as a field: `/` and `?` open it,
`n` / `N` follow the direction flag, `*` / `#` seed it from the word under
the cursor, and `search_snapshot()` is forwarded so chips and highlight keep
working. Steal that shape whenever search is one mode among several.

## Headless: engine only

`find_matches` / `find_next` / `find_prev`, the version-cached
`SearchState`, and `replace_all_query` / `replace_one_query` /
`collect_replacements` (the last enables line-scoped `:s`) compose without
any prompt at all. All replacements funnel into
`EditorBuffer::replace_many`, so replace-all undoes in one step.
