use gpui::*;
use gpui_platform::application;
use twrite::Editor;

struct MarkdownApp {
    editor: Entity<Editor>,
}

impl Render for MarkdownApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (cursor_pos, pixel_pos, status_text) = {
            let ed = self.editor.read(cx);
            let pt = ed.buffer.cursor_point();
            let pixel = ed
                .cursor_pixel_position()
                .map(|p| format!("X: {:.0}, Y: {:.0}", p.x, p.y))
                .unwrap_or_else(|| "--".to_string());
            let status = ed
                .status_text()
                .map(|s| s.to_string())
                .unwrap_or_else(|| "READY".to_string());
            (
                format!("Ln {}, Col {}", pt.row + 1, pt.column + 1),
                pixel,
                status,
            )
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0x181825))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_4()
                    .py_2()
                    .bg(rgb(0x1e1e2e))
                    .border_b_1()
                    .border_color(rgb(0x313244))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgb(0xcba6f7))
                                    .child("TWrite Markdown Live Demo"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .px_2()
                                    .py_0p5()
                                    .rounded_md()
                                    .bg(rgb(0x313244))
                                    .text_color(rgb(0x89dceb))
                                    .child(status_text),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(0x6c7086))
                            .child("Ctrl+B: Bold  |  Ctrl+I: Italic  |  Ctrl+K: Link  |  Ctrl+Enter: Toggle Checkbox"),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .p_4()
                    .child(self.editor.clone()),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_4()
                    .py_1p5()
                    .bg(rgb(0x11111b))
                    .border_t_1()
                    .border_color(rgb(0x313244))
                    .text_xs()
                    .child(
                        div()
                            .text_color(rgb(0xa6adc8))
                            .child("CommonMark + GFM Mode Active"),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_4()
                            .child(
                                div()
                                    .text_color(rgb(0x89dceb))
                                    .child(format!("Cursor Pixel: {}", pixel_pos)),
                            )
                            .child(
                                div()
                                    .text_color(rgb(0xa6e3a1))
                                    .child(cursor_pos),
                            ),
                    ),
            )
    }
}

fn main() {
    application().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(840.0), px(600.0)), cx);

        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("TWrite - Markdown Editor".into()),
                    appears_transparent: false,
                    traffic_light_position: None,
                }),
                window_min_size: Some(size(px(500.0), px(350.0))),
                ..Default::default()
            },
            |window, cx| {
                let initial_text = "\
# Markdown Live Demo

Welcome to the **TWrite Markdown** editor engine!

## Features Out of the Box
- **Bold text** with `Ctrl + B`
- *Italic text* with `Ctrl + I`
- Links like [TWrite GitHub](https://github.com/ToonionOfficial/twrite) with `Ctrl + K`
- Inline code spans with `code background chips`

### Interactive Task Lists (Press Ctrl+Enter to toggle)
- [x] Fast rope-backed text buffer
- [x] Extensible hook architecture
- [ ] Try toggling this task with Ctrl+Enter!
- [ ] Press Enter on this item to auto-continue the list

### Numbered Lists (Smart auto-increment)
1. First item (press Enter at the end of this line)
2. Second item

### Code Blocks
```rust
fn main() {
    println!(\"Hello from TWrite WYSIWYG Markdown!\");
}
```

> Blockquotes are supported and rendered with distinct styling.
";

                let editor = cx.new(|cx| {
                    let mut ed = Editor::new(initial_text, cx);
                    ed.config.line_numbers = true;
                    ed.enable_markdown();
                    ed
                });

                let focus_handle = editor.read(cx).focus_handle.clone();
                focus_handle.focus(window, cx);

                cx.new(|_| MarkdownApp { editor })
            },
        )
        .unwrap();

        cx.activate(true);
    });
}
