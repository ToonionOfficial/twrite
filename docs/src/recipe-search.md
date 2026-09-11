# Recipe: Find & Replace

TWrite provides a complete, interactive Find & Replace toolbar as a drop-in hook. It includes regex pattern support, whole-word filtering, case sensitivity toggles, caret steppers, and single-undo batch replacements.

## 1. Drop-In Registration

To enable find and replace, register `SearchHook` on your editor:

```rust
use twrite::Editor;
use twrite::SearchHook;

let mut ed = Editor::new("Initial document content...\n", cx);

// Add SearchHook (register early so its shortcuts take priority):
ed.add_hook(SearchHook::new());
```

That is all that is needed. The editor will automatically render the interactive search toolbar when triggered.

## 2. Keyboard Controls

| Shortcut | Action |
| --- | --- |
| `Ctrl+F` | Open find bar (seeds query with any active selection). Focuses the find input. |
| `Ctrl+H` | Open/expand the replace row. Focuses the replace input. |
| `Enter` (Find input) | Advance to the next match. |
| `Enter` (Replace input) | Replace current match and advance to the next match. |
| `Tab` | Switch focus between Find and Replace input boxes. |
| `F3` / `Down` | Jump to next match. |
| `Shift+F3` / `Up` | Jump to previous match. |
| `Alt+Up` / `Alt+Down` | Cycle through previous search query history. |
| `Alt+C` | Toggle Match Case. |
| `Alt+W` | Toggle Whole Words. |
| `Alt+H` | Toggle Highlight All (canvas background wash). |
| `Alt+A` | Replace All matches as a single undoable transaction. |
| `Esc` | Close search toolbar and return focus to the editor buffer. |

## 3. Input Editing Shortcuts

Inside both the Find and Replace input boxes, standard desktop text editing shortcuts are fully supported:

- `Ctrl+Left` / `Ctrl+Right` (or `Alt+Left` / `Alt+Right`): Move cursor word-by-word.
- `Ctrl+Backspace` / `Alt+Backspace`: Delete previous word.
- `Ctrl+Delete` / `Alt+Delete`: Delete next word.
- `Ctrl+K`: Clear input from cursor to end of line.
- `Ctrl+U`: Clear input from cursor to start of line.
- `Home` / `End`: Jump to start or end of input.

## 4. Mouse Controls

The search bar renders an interactive UI with mouse support:

- **Expand Arrow (`▶` / `▼`)**: Expands or collapses the replace row.
- **Find and Replace Boxes**: Click either input box directly to focus it without closing or losing state.
- **Steppers (`^` and `v`)**: Click to navigate to the previous or next match.
- **Checkboxes**: Click `Highlight All`, `Match Case`, or `Whole Words` to toggle search criteria with immediate rescanning.
- **Action Buttons**: Click `Replace` to replace the current match, or `Replace All` to replace every match across the document.
- **Close Button (`✕`)**: Dismisses the search toolbar.

## 5. Headless Search Engine API

If you want to perform programmatic search or replace operations without opening the prompt toolbar, you can access the headless engine directly:

```rust
use twrite::search::{SearchQuery, find_matches, replace_all_query};

let text = ed.buffer.text();
let query = SearchQuery::literal("foo").case_sensitive(true);

// Collect all matching byte ranges:
let matches = find_matches(&text, &query);
println!("Found {} matches", matches.len());

// Programmatically replace all occurrences in a single undo step:
let count = replace_all_query(&mut ed.buffer, &query, "bar").unwrap();
println!("Replaced {} occurrences", count);
```
