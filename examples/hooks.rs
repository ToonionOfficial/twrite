use gpui::*;
use gpui_platform::application;
use twrite::{
    AutoPairsHook, EditorHook, HookContext, HookOutcome, KeyEvent, Selection, editor::Editor,
};

/// A custom hook providing formatting shortcuts and smart list continuation.
#[derive(Default)]
struct MarkdownShortcutsHook;

impl EditorHook for MarkdownShortcutsHook {
    fn on_key(&mut self, ctx: &mut HookContext, event: &KeyEvent) -> HookOutcome {
        if event.modifiers.ctrl || event.modifiers.meta {
            match event.key.to_lowercase().as_str() {
                "b" => {
                    if let Some(sel) = ctx.selection.take() {
                        let range = sel.byte_range();
                        let text = ctx.buffer.text().byte_slice(range.clone()).to_string();
                        let wrapped = format!("**{}**", text);
                        ctx.buffer.replace_range(range.clone(), &wrapped);
                        *ctx.selection = Some(Selection::range(range.start + 2, range.end + 2));
                    } else {
                        ctx.buffer.insert("****");
                        ctx.buffer.move_cursor_left();
                        ctx.buffer.move_cursor_left();
                    }
                    return HookOutcome::Consumed;
                }
                "i" => {
                    if let Some(sel) = ctx.selection.take() {
                        let range = sel.byte_range();
                        let text = ctx.buffer.text().byte_slice(range.clone()).to_string();
                        let wrapped = format!("*{}*", text);
                        ctx.buffer.replace_range(range.clone(), &wrapped);
                        *ctx.selection = Some(Selection::range(range.start + 1, range.end + 1));
                    } else {
                        ctx.buffer.insert("**");
                        ctx.buffer.move_cursor_left();
                    }
                    return HookOutcome::Consumed;
                }
                _ => {}
            }
        }

        if event.key == "enter" && !event.modifiers.shift {
            let cursor = ctx.buffer.cursor_offset();
            let row = ctx.buffer.cursor_point().row;
            let line = ctx.buffer.line_to_string(row);
            let trimmed = line.trim_start();
            let indent_len = line.len() - trimmed.len();
            let indent = &line[..indent_len];

            if trimmed.starts_with("- ") {
                if trimmed == "- \n" || trimmed == "- \r\n" || trimmed == "-" || trimmed == "- " {
                    let line_start = ctx.buffer.point_to_offset(twrite::Point::new(row, 0));
                    ctx.buffer.delete_range(line_start..cursor);
                    return HookOutcome::Consumed;
                }

                ctx.buffer.insert(&format!("\n{}- ", indent));
                return HookOutcome::Consumed;
            }
        }

        HookOutcome::PassThrough
    }

    fn status_text(&self) -> Option<&str> {
        Some("MARKDOWN HOOKS ACTIVE")
    }
}

struct AppView {
    editor: Entity<Editor>,
}

impl Render for AppView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let editor_read = self.editor.read(cx);
        let cursor_point = editor_read.buffer.cursor_point();
        let status = editor_read.status_text().unwrap_or("READY");

        let status_line = format!(
            "Ln {}, Col {} | {}",
            cursor_point.row + 1,
            cursor_point.column + 1,
            status
        );

        div()
            .size_full()
            .flex()
            .flex_col()
            .child(div().flex_1().child(self.editor.clone()))
            .child(
                div()
                    .h(px(26.0))
                    .bg(rgb(0x11111b))
                    .text_color(rgb(0xa6adc8))
                    .text_size(px(12.0))
                    .px(px(12.0))
                    .flex()
                    .items_center()
                    .child(status_line),
            )
    }
}

fn main() {
    application().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(800.0), px(600.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("TWrite - Versatile Hooks Demo".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |window, cx| {
                let editor = cx.new(|cx| {
                    let mut ed = Editor::new(
                        "# Versatile Hooks Demo\n\nThis editor has multiple hooks running concurrently:\n\n1. AutoPairsHook:\n   - Type '(' or '[' or '{' or '\"' -> auto-inserts pair\n   - Select text and type '\"' -> wraps the selected text\n   - Press backspace inside empty '()' -> deletes both\n\n2. MarkdownShortcutsHook:\n   - Press Ctrl+B to bold (wraps selection or inserts ****)\n   - Press Ctrl+I to italic (wraps selection or inserts **)\n   - Type '- First item' and press Enter to auto-continue list\n\nTry it below:\n- Item 1\n",
                        cx,
                    );
                    ed.config.line_numbers = true;
                    ed.add_hook(AutoPairsHook::new());
                    ed.add_hook(MarkdownShortcutsHook);
                    ed
                });

                let focus_handle = editor.read(cx).focus_handle.clone();
                focus_handle.focus(window, cx);

                cx.new(|_| AppView { editor })
            },
        )
        .unwrap();

        cx.activate(true);
    });
}
