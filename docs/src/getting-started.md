# Getting Started

This guide walks you through embedding TWrite into a GPUI application from scratch.

## 1. System Prerequisites

GPUI renders natively using Vulkan or Metal, with Wayland/X11 on Linux. On Linux systems, install the windowing and font development packages:

```sh
sudo apt-get install -y pkg-config libfontconfig1-dev libwayland-dev \
  libx11-xcb-dev libxkbcommon-x11-dev libxkbcommon-dev
```

## 2. Add Dependencies to `Cargo.toml`

Add `twrite` and GPUI to your application manifest:

```toml
[package]
name = "my-editor-app"
version = "0.1.0"
edition = "2024"

[dependencies]
gpui = { git = "https://github.com/zed-industries/zed", rev = "d85eade7ab475de6d11a193beccf1951be2ff371" }
gpui_platform = { git = "https://github.com/zed-industries/zed", rev = "d85eade7ab475de6d11a193beccf1951be2ff371", features = ["wayland", "x11", "font-kit"] }
twrite = "0.6"

# Optional: To enable the full Markdown battery, use:
# twrite = { version = "0.6", features = ["markdown"] }
```

## 3. Your First Editor Application

Create `src/main.rs` with the following minimal app:

```rust
use gpui::*;
use gpui_platform::application;
use twrite::Editor;

struct AppView {
    editor: Entity<Editor>,
}

impl Render for AppView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(rgb(0x1e1e2e))
            .child(self.editor.clone())
    }
}

fn main() {
    application().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(800.0), px(600.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("My First TWrite Editor".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |_window, cx| {
                let editor = cx.new(|cx| {
                    let mut ed = Editor::new("Hello from TWrite!\nStart typing here...\n", cx);
                    ed.config.line_numbers = true;
                    ed
                });
                cx.new(|_cx| AppView { editor })
            },
        )
        .unwrap();
    });
}
```

Run your application:

```sh
cargo run
```

You now have a fully functional text editor window with smooth scrolling, text selection, undo/redo (`Ctrl+Z` / `Ctrl+Y`), and line numbers.

## 4. Next Steps

Check out the step-by-step recipes:

- **[Simple Text Editor](recipe-simple.md)**: Configure fonts, line numbers, cursor blinking, and read/write buffer content.
- **[Markdown Note Editor](recipe-markdown.md)**: Add syntax highlighting, task checkboxes, and conceal formatting.
- **[Find & Replace](recipe-search.md)**: Drop in the interactive search and replace toolbar with one line of code.
- **[Custom Hooks & Shortcuts](recipe-hooks.md)**: Bind custom keyboard shortcuts and status bar info.
