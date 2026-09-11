# Vim and Ex Commands

Companion file: `examples/vim.rs`: the largest example, and deliberately so:
it proves a full modal system runs on hooks alone, with zero Vim code in the
engine.

## Reading order inside the file

1. `VimHook` state: `mode`, `pending_key` (multi-key ops like `dd`/`gg`),
   the owned `SearchHook`, the `search_backward` direction flag, and the
   one-shot `status_override` line.
2. `on_key` dispatch: shared prompt first (`/` search delegates to the owned
   hook, `:` ex-commands stay here), then `escape`, then the mode arms.
3. Motions and operators per mode, then `execute_ex` / `execute_substitute`.
4. The `AppView` shell and `main` wiring at the bottom.

## Ex command reference

| Command | Effect |
|---|---|
| `:w [path]`, `:q[!]`, `:wq`, `:e path` | `HookEffect` queue; the host drains it |
| `:<num>` | Jump to a line |
| `:s/old/new/[g][i]`, `:%s/...` | Regex substitute, line or file scope, `$1` captures, single undo |

An empty `old` reuses the last search query; unknown input reports
vim-style `E...` codes on the status line. Note the `:` prompt is just
another `ctx.prompt` client with its own `"vim-ex"` spec id.
