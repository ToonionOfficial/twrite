# Recipe: Markdown Note Editor

TWrite includes a built-in Markdown battery that provides live syntax highlighting, concealed markup tokens, interactive task lists, and clickable hyperlinks.

## 1. Enable the `markdown` Feature

In your `Cargo.toml`, enable the optional `markdown` feature flag:

```toml
[dependencies]
twrite = { version = "0.8", features = ["markdown"] }
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

### Smart List Continuation
Pressing `Enter` at the end of a list item automatically inserts the next bullet point (`- ` or `* `) or increments ordered list numbers (`1. `, `2. `). Pressing `Enter` on an empty list bullet removes the prefix.

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
| `ConcealMode::Hidden` | Syntax markers are hidden completely. When the cursor enters a line, markers un-conceal so you can edit them directly. |
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
