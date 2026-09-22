# Recipe: Inline Completion

Typing `[[` in a Markdown buffer opens a suggestion popup under the cursor; typing `@` in the `completion` example does the same for mentions. Both run on one generic mechanism: a hook owns a live session, the editor draws it. This recipe shows how to drive that mechanism from your own hook, with no battery and no feature flags.

The full working version lives in `examples/completion.rs` (`cargo run --example completion`). What follows explains each part.

## 1. The Three Moving Parts

| Piece | Owner | Role |
|---|---|---|
| Session state (anchor, query, rows, selection) | Your hook | Opens on a trigger, updates per keystroke, closes on accept/dismiss |
| `completion_snapshot()` | Your hook | Returns the current rows + selected index; the editor polls it every frame |
| Popup drawing, row clicks, outside-click dismiss | The editor | Renders under the cursor, routes row clicks to `on_completion_select` |

Three `EditorHook` methods form the whole surface (all defaulted, so existing hooks are unaffected):

- `completion_snapshot(&self) -> Option<CompletionSnapshot>` — `None` means no popup. `Some` carries `items: Vec<PromptItem>` and `selected: usize`.
- `on_completion_select(&mut self, ctx, index) -> HookOutcome` — popup row clicks arrive here by row index. (Context-menu routing uses `&'static str` ids, which cannot name dynamic candidates, hence indices.)
- `dismiss_completion(&mut self)` — the editor calls this on outside/right clicks. Your session also dismisses itself on cursor moves via `on_selection_change`.

Rows reuse `PromptItem` (`label` inserted on accept, `hint` shown dimmed), and ranking reuses `fuzzy_filter` — the same helper behind command palettes.

## 2. Opening a Session

Watch for your trigger in `on_key`, which runs before the editor's default insertion. When the trigger character arrives, insert it yourself, record the anchor, and consume the key:

```rust
use twrite::{EditorHook, HookContext, HookOutcome, KeyCode};

impl EditorHook for MentionHook {
    fn on_key(&mut self, ctx: &mut HookContext, event: &KeyEvent) -> HookOutcome {
        if let KeyCode::Char('@') = event.code
            && !event.modifiers.ctrl && !event.modifiers.meta && !event.modifiers.alt
        {
            ctx.buffer.insert("@");
            let start = ctx.buffer.cursor_offset() - 1;
            let row = ctx.buffer.offset_to_point(start).row;
            self.session = Some(MentionSession { start, row, .. });
            self.refresh("");
            return HookOutcome::Consumed;
        }
        // ... normal handling ...
    }
}
```

The anchor is a buffer byte offset plus its row. Everything else derives from those two numbers, so the session survives typing but never goes stale: recompute, never cache.

## 3. Tracking the Query

On every keystroke while the session is live, re-derive the query from the buffer (text between anchor and cursor) instead of storing what was typed. If the anchor broke — brackets deleted, row changed, undo rewound past it — close the session and let the key fall through to normal handling:

```rust
fn query(buffer: &EditorBuffer, session: &MentionSession) -> Option<String> {
    let cursor = buffer.cursor_offset();
    if cursor <= session.start || buffer.cursor_point().row != session.row {
        return None;
    }
    let text = buffer.text();
    if !text
        .get_byte_slice(session.start..session.start + 1)
        .is_some_and(|slice| slice == "@")
    {
        return None;
    }
    text.get_byte_slice(session.start + 1..cursor)
        .map(|slice| slice.chars().collect())
}
```

`get_byte_slice` (not `byte_slice`) keeps torn-down state from panicking: `None` means dismiss, always. This is the one rule that keeps every edge (undo, paste over the anchor, multi-line edits) safe.

## 4. Handling Session Keys

With a live session, your hook owns these keys and releases everything else to normal handling:

| Key | Action |
|---|---|
| Printable char | Insert it into the buffer yourself, re-filter rows, consume |
| `Backspace` | At the anchor: close and pass through (default deletes the trigger). Otherwise delete, re-filter, consume |
| `Up` / `Down` (no modifiers) | Move the selection, wrapping; consume |
| `Enter` / `Tab` | Rows present: accept the selected row. No rows: close and pass through (so `Enter` still edits, `Tab` still indents) |
| `Esc` | Close, keep the typed text, consume |
| Anything else (`Ctrl+B`, arrows, …) | Pass through; `on_selection_change` below dismisses if the cursor left |

Refresh immediately after mutating (don't wait for a later callback): call your rank step right after `insert`/`backspace` with the freshly recomputed query.

## 5. Accepting a Row

Replace the query range with the label, then place the cursor. Mentions append a trailing space; wikilinks (see `MarkdownHook`) ensure closing `]]` and preserve any `|alias` suffix instead:

```rust
ctx.buffer.replace_range(start + 1..cursor, &label);
let end = start + 1 + label.len();
ctx.buffer.set_cursor_offset(end);
ctx.buffer.insert(" ");
*ctx.selection = None;
self.session = None;
```

Always set the cursor explicitly after `replace_range` and clear the selection, then close the session. The same function serves keyboard accept and `on_completion_select` alike.

## 6. Dismissing on Cursor Moves

Keys you pass through (arrows, clicks) move the cursor behind your back. `on_selection_change` is your cleanup hook — dismiss when the cursor leaves the session range:

```rust
fn on_selection_change(&mut self, buffer: &EditorBuffer, _selection: Option<&Selection>) {
    let live = self
        .session
        .as_ref()
        .is_some_and(|session| Self::query(buffer, session).is_some());
    if !live {
        self.session = None;
    }
}
```

Note the editor also calls `dismiss_completion()` on outside and right clicks, so popups never linger under menus.

## 7. Candidates Stay Yours

Row sources are plain functions from query string to labels — files, a database, an in-memory map, a static list. The Markdown battery formalizes this as `set_completion_provider`; a hand-rolled hook can inline the same closure. One convention to keep: filter generously, then rank with `fuzzy_filter` and cap visible rows (8 is the battery's cap), so popups stay scannable:

```rust
let candidates: Vec<PromptItem> = WORDS
    .iter()
    .filter(|word| word.contains(query))
    .map(|word| PromptItem::new(word))
    .collect();
session.items = fuzzy_filter(&candidates, query)
    .into_iter()
    .take(8)
    .map(|(index, _)| candidates[index].clone())
    .collect();
```

## 8. Testing Without a Window

The whole session is headless: drive `on_key` on a bare `EditorBuffer`, then mirror what the editor does after every input by calling `on_selection_change`. The `MarkdownHook` tests use a small `press_completion_key` helper doing exactly that — copy the pattern and assert on `completion_snapshot()` (rows, selection) and buffer text after each key. No GPUI, no timers, no screenshots.

## Checklist

- Trigger inserts its own character and consumes the key; nothing opens without candidates configured.
- Every mutation path re-filters before returning; every anchor break closes instead of panicking.
- Empty rows render nothing but keep session keys; `Enter`/`Tab` with no rows fall through.
- Accept sets cursor and selection explicitly and works identically from keyboard and mouse.
- `on_selection_change` dismisses on any cursor leave; outside clicks are covered by `dismiss_completion`.
