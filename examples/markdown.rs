use gpui::*;
use gpui_platform::application;
use twrite::Editor;
use twrite::markdown::{
    ConcealMode, MarkdownHighlighter, TABLE_CELL_TAG, TABLE_DELIMITER_TAG, TABLE_HEADER_TAG,
};

struct MarkdownApp {
    editor: Entity<Editor>,
}

impl MarkdownApp {
    fn cycle_conceal_mode(&mut self, cx: &mut Context<Self>) {
        self.editor.update(cx, |ed, _| {
            let next = match ed.config.markdown.conceal_mode {
                ConcealMode::Dimmed => ConcealMode::Hidden,
                ConcealMode::Hidden => ConcealMode::Off,
                ConcealMode::Off => ConcealMode::Dimmed,
            };
            ed.config.markdown.conceal_mode = next;
            ed.set_highlighter(MarkdownHighlighter::with_config(ed.config.markdown));
        });
        cx.notify();
    }
}

impl Render for MarkdownApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (cursor_pos, pixel_pos, status_text, conceal_mode, faces_text) = {
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
            let conceal = ed.config.markdown.conceal_mode;
            let family = ed
                .selected_font_family
                .as_ref()
                .map(|f| f.to_string())
                .or_else(|| ed.config.font_family.as_ref().map(|f| f.to_string()))
                .unwrap_or_else(|| "(host default)".to_string());
            let faces = match ed.face_availability {
                Some(f) if f.bold && f.italic => format!("Faces: {family} B+I ok"),
                Some(f) => format!(
                    "Faces: {family} {} missing",
                    match (f.bold, f.italic) {
                        (false, false) => "bold+italic",
                        (false, true) => "bold",
                        _ => "italic",
                    }
                ),
                None => format!("Faces: {family} …"),
            };
            (
                format!("Ln {}, Col {}", pt.row + 1, pt.column + 1),
                pixel,
                status,
                conceal,
                faces,
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
                            )
                            .child(
                                div()
                                    .id("conceal-toggle")
                                    .cursor_pointer()
                                    .text_xs()
                                    .px_2()
                                    .py_0p5()
                                    .rounded_md()
                                    .bg(rgb(0x45475a))
                                    .text_color(rgb(0xf9e2af))
                                    .child(format!("Conceal: {:?} (Click to toggle)", conceal_mode))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, _, _, cx| {
                                            this.cycle_conceal_mode(cx);
                                        }),
                                    ),
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
                                    .text_color(rgb(0xf9e2af))
                                    .child(faces_text),
                            )
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

### Interactive Task Lists (Click checkbox or press Ctrl+Enter)
- [x] Fast rope-backed text buffer
- [x] Focus-aware marker dimming
- [ ] Click this checkbox directly with your mouse!
- [ ] Try toggling with Ctrl+Enter!

---

### Code Blocks
```rust
fn main() {
    println!(\"Hello from TWrite WYSIWYG Markdown!\");
}
```

> Blockquotes are rendered with a sleek vertical accent bar.

### Tables (Tab / Shift+Tab moves between cells, Enter adds a row)
| Name | Age | City |
| --- | ---: | :---: |
| Ada | 36 | London |
| `Grace` | 85 | **New York** |
";

                let editor = cx.new(|cx| {
                    let mut ed = Editor::new(initial_text, cx);
                    ed.config.line_numbers = true;
                    // Table custom tags fall back to the foreground unless the
                    // host registers colors for them (battery-only extension).
                    ed.theme
                        .syntax
                        .set_custom_tag_color(TABLE_HEADER_TAG, gpui::rgb(0xf9e2af).into());
                    ed.theme
                        .syntax
                        .set_custom_tag_color(TABLE_CELL_TAG, gpui::rgb(0xcdd6f4).into());
                    ed.theme
                        .syntax
                        .set_custom_tag_color(TABLE_DELIMITER_TAG, gpui::rgb(0x6c7086).into());
                    // No explicit family: the editor auto-selects the first
                    // platform monospace with bold + italic faces (see
                    // `Editor::face_availability`). Set `ed.config.font_family
                    // explicitly to override (e.g. Menlo, Consolas).
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
