# Recipe: Markdown Note Editor

TWrite includes a built-in Markdown battery that provides live syntax highlighting, concealed markup tokens, interactive task lists, and clickable hyperlinks.

## 1. Enable the `markdown` Feature

In your `Cargo.toml`, enable the optional `markdown` feature flag:

```toml
[dependencies]
twrite = { version = "0.10", features = ["markdown"] }
```

## 2. One-Line Initialization

Call `ed.enable_markdown()` when creating the editor:

```rust
use gpui::*;
use twrite::Editor;

let editor = cx.new(|cx| {
    let mut ed = Editor::new("# Welcome to TWrite\n\n- [ ] Task item\n", cx);

    // Activates markdown syntax highlighting, task lists, and shortcuts:
    ed.enable_markdown();

    ed.config.line_numbers = true;
    ed
});
```

## 3. Features Included Out of the Box

When `ed.enable_markdown()` is called, the following behaviors are activated automatically:

### Interactive Task Checkboxes
Lines starting with `- [ ]` or `- [x]` (or numbered `1. [ ]`) render as clickable checkboxes. Single-clicking the checkbox with the mouse toggles its state between checked and unchecked without having to type.

### Clickable Hyperlinks
Inline links formatted as `[Link text](https://example.com)` are detected. Single-clicking a link opens the URL in the system default browser. Double-clicking still selects the text for editing.

### Wikilinks
`[[Target]]`, `[[Target|Label]]`, and `[[Target#Fragment]]` render as links with Obsidian style concealment (inactive rows show the label only). Clicking one or pressing `Enter` on it reports a `HookEffect::FollowLink` carrying `target`, `fragment`, and `label`; the host resolves that against whatever storage it uses (files, database, memory) by draining `editor.take_effects()`. The core never touches storage:

```rust
use twrite::HookEffect;

// ... ed.add_hook(MarkdownHook::new()) ...

// After input, drain and resolve against your own store:
let pending = editor.update(cx, |editor, _| editor.take_effects());
for effect in pending {
    if let HookEffect::FollowLink {
        target,
        fragment,
        label,
    } = effect
    {
        open_target_in_your_store(&target, fragment.as_deref(), label.as_deref());
    }
}
```

Typing `[[` opens an inline completion popup near the cursor. Provide candidates through a provider callback (plain strings in, storage stays yours); the hook filters, navigates (`Up`/`Down`), accepts (`Enter`/`Tab`, preserving any `|alias` suffix), and dismisses (`Esc`, typing `]]`, or moving the cursor away):

```rust
use std::sync::Arc;
use twrite::{MarkdownHook, PromptItem};

let mut hooks = MarkdownHook::new();
hooks.set_completion_provider(Arc::new(|query: &str| {
    list_names_in_your_store()
        .into_iter()
        .filter(|name| name.contains(query))
        .map(|name| PromptItem::new(&name))
        .collect()
}));
```

### Text Highlight
Wrapping text in double equals signs (`==important==`) renders it with a tinted background wash. The markers conceal and reveal following the active conceal mode, word by word like other inline formatting.

### Callouts
A quote block opening with `[!KIND]` (`> [!NOTE] Title`) renders as a callout: the bar and body wash take the kind accent (NOTE, TIP, WARNING, CAUTION, IMPORTANT, others fall back to NOTE), the marker conceals like other delimiters, the title renders bold, and body text dims to quote gray under inline formatting. The `>` prefix conceals word-level like other markers. Collapsing bodies is not supported yet.

### Smart List Continuation
Pressing `Enter` at the end of a list item automatically inserts the next bullet point (`- ` or `* `) or increments ordered list numbers (`1. `, `2. `). Pressing `Enter` on an empty list bullet removes the prefix.

### List Indentation and Sibling Renumbering
Pressing `Tab` on a list item indents and nests the item (adjusting leading whitespace by `list_indent_size`, default 2 spaces). Pressing `Shift+Tab` outdents and unnests it. Sibling ordered lists are automatically renumbered consecutively at each indentation level.

### Move Lines and List Items
Pressing `Alt+Up` or `Alt+Down` moves the current line or selected block of lines up or down, carrying list markers along and keeping the cursor column aligned where possible. Sibling ordered lists are automatically renumbered consecutively across the affected list block. Configurable via `MarkdownConfig::list_reordering`.

### Formatting Shortcuts
Selecting text and pressing formatting keys wraps the selection automatically:
- `Ctrl+B`: Toggles bold (`**text**`)
- `Ctrl+I`: Toggles italic (`*text*`)
- Backtick (`` ` ``): Wraps selection in code span (`` `text` ``)
- Auto-pairs: Typing `(`, `[`, `{`, `"`, `'` wraps the selection or inserts matching pairs.

## 4. Conceal Modes

TWrite supports three conceal levels for Markdown syntax markers (such as `**`, `#`, and backticks):

| Mode | Behavior |
| --- | --- |
| `ConcealMode::Dimmed` (default) | Syntax markers are drawn in a subtle, dimmed color to keep content legible while keeping raw characters visible. |
| `ConcealMode::Hidden` | Syntax markers are hidden completely. Inline markers, the heading `#` prefix, and the task marker un-conceal while the cursor sits on them; quote prefixes un-conceal while the cursor is anywhere on the line. |
| `ConcealMode::Off` | All characters are rendered at full normal opacity. |

To change or toggle conceal mode dynamically:

```rust
use twrite::markdown::{ConcealMode, MarkdownHighlighter};

ed.update(cx, |ed, cx| {
    ed.config.markdown.conceal_mode = ConcealMode::Hidden;
    ed.set_highlighter(MarkdownHighlighter::with_config(ed.config.markdown));
    cx.notify();
});
```

## 5. Complete Markdown Example

Here is a full view component demonstrating Markdown setup with an active status badge:

```rust
use gpui::*;
use twrite::Editor;
use twrite::SearchHook;

struct MarkdownEditorApp {
    editor: Entity<Editor>,
}

impl MarkdownEditorApp {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let editor = cx.new(|cx| {
            let initial_content = r#"# Project Notes

## Features
- [x] GPU-accelerated canvas
- [ ] Customizable theme colors
- [x] Interactive task lists

Visit the [TWrite GitHub](https://github.com/ToonionOfficial/twrite) for updates.
"#;
            let mut ed = Editor::new(initial_content, cx);
            ed.enable_markdown();
            ed.add_hook(SearchHook::new());
            ed
        });

        Self { editor }
    }
}

impl Render for MarkdownEditorApp {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(rgb(0x1e1e2e))
            .child(self.editor.clone())
    }
}
```
