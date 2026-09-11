# Recipe: Simple Text Editor

This recipe shows how to configure a clean, lightweight plain text editor and interact with its text buffer from your GPUI application.

## Basic Setup

Initialize an `Editor` entity inside a GPUI window:

```rust
use gpui::*;
use twrite::Editor;

struct EditorApp {
    editor: Entity<Editor>,
}

impl EditorApp {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let editor = cx.new(|cx| {
            let mut ed = Editor::new("Initial document text...\n", cx);

            // Configure editor display settings:
            ed.config.line_numbers = true;
            ed.config.cursor_blink = true;
            ed.config.font_size = px(15.0);
            ed.config.line_height = px(24.0);

            // Optional: override monospace font family (auto-selected by default):
            // ed.config.font_family = Some("JetBrains Mono".into());

            ed
        });

        Self { editor }
    }
}
```

## Reading and Writing Buffer Content

The underlying text is held by `ed.buffer` (an `EditorBuffer` powered by a rope structure):

```rust
impl EditorApp {
    /// Reads the complete document text as a String.
    pub fn get_content(&self, cx: &App) -> String {
        self.editor.read(cx).buffer.text()
    }

    /// Replaces the document text cleanly (resets undo history).
    pub fn set_content(&mut self, text: &str, cx: &mut Context<Self>) {
        self.editor.update(cx, |ed, cx| {
            ed.buffer.set_text(text);
            cx.notify();
        });
    }

    /// Inspects document size without copying text.
    pub fn print_stats(&self, cx: &App) {
        let ed = self.editor.read(cx);
        println!("Lines: {}", ed.buffer.line_count());
        println!("Characters: {}", ed.buffer.len_chars());
        println!("Bytes: {}", ed.buffer.len_bytes());
        println!("Edit Version: {}", ed.buffer.version());
    }
}
```

## Adding a Status Bar

To build a professional editor layout with a status line displaying cursor row, column, and match count:

```rust
impl Render for EditorApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ed = self.editor.read(cx);
        let point = ed.buffer.cursor_point();
        let status = format!(
            "Ln {}, Col {} | {} lines",
            point.row + 1,
            point.col + 1,
            ed.buffer.line_count()
        );

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0x1e1e2e))
            // Main editor area:
            .child(div().flex_1().child(self.editor.clone()))
            // Bottom status bar:
            .child(
                div()
                    .h(px(24.0))
                    .bg(rgb(0x181825))
                    .border_t_1()
                    .border_color(rgb(0x313244))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_xs()
                    .text_color(rgb(0xa6adc8))
                    .child(status),
            )
    }
}
```

## Built-In Keyboard Shortcuts

The basic editor comes equipped with standard desktop shortcuts out of the box:

- **Arrow Keys**: Move cursor by character / line.
- **Ctrl+Left / Ctrl+Right** (or Alt+Left / Alt+Right): Jump word-by-word.
- **Home / End**: Jump to line start or line end.
- **Ctrl+Z**: Undo edit.
- **Ctrl+Y** (or Ctrl+Shift+Z): Redo edit.
- **Ctrl+A**: Select all.
- **Backspace / Delete**: Single-character deletion.
- **Ctrl+Backspace / Ctrl+Delete**: Delete previous or next word.
- **Double-click**: Select word under mouse cursor.
- **Triple-click**: Select entire line.
- **Click and Drag**: Selection expands with boundary snapping.
