//! Getting-started example 2 of 6: writing your own hook.
//!
//! Read after `simple`. Demonstrates a custom [`EditorHook`]: intercepting
//! keys (`on_key`), reacting to edits (`after_edit`), and reporting a status
//! line (`status_text`). Deliberately battery-neutral: formatting shortcuts
//! (Ctrl+B and friends) live in the markdown battery, see `markdown`
//! (`Editor::enable_markdown`) instead of being reimplemented here.
//! Run with: `cargo run --example hooks`
//! Next: `syntax` (highlighting) or `prompt` (input boxes).
use gpui::*;
use gpui_platform::application;
use twrite::{AutoPairsHook, Editor, EditorBuffer, EditorHook, HookContext, HookOutcome, KeyEvent};

/// A custom hook showing the three most-used hook methods.
///
/// `on_key` duplicates the current line on `Ctrl+D` and continues `- ` lists
/// on `Enter` (batteries extend the list idea for tasks and tables);
/// `after_edit` recounts words after every buffer change; `status_text` feeds
/// the count to the status bar below.
#[derive(Default)]
struct ScratchHook {
    words: usize,
    status_line: String,
}

impl ScratchHook {
    fn new(initial_text: &str) -> Self {
        let words = initial_text.split_whitespace().count();
        Self {
            words,
            status_line: format!("{words} WORDS"),
        }
    }

    fn recount(&mut self, buffer: &EditorBuffer) {
        self.words = buffer.text().to_string().split_whitespace().count();
        self.status_line = format!("{} WORDS", self.words);
    }
}

impl EditorHook for ScratchHook {
    fn on_key(&mut self, ctx: &mut HookContext, event: &KeyEvent) -> HookOutcome {
        if (event.modifiers.ctrl || event.modifiers.meta)
            && event.key.to_lowercase().as_str() == "d"
        {
            let row = ctx.buffer.cursor_point().row;
            let line_start = ctx.buffer.point_to_offset(twrite::Point::new(row, 0));
            let stripped = ctx.buffer.line_to_string(row);
            let stripped = stripped.trim_end_matches(['\r', '\n']);
            ctx.buffer
                .replace_range(line_start..line_start, &format!("{stripped}\n"));
            return HookOutcome::Consumed;
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
        Some(&self.status_line)
    }

    fn after_edit(&mut self, buffer: &mut EditorBuffer) {
        self.recount(buffer);
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
                    let initial_text = "# Versatile Hooks Demo\n\nThis editor has multiple hooks running concurrently:\n\n1. AutoPairsHook:\n   - Type '(' or '[' or '{' or '\"' -> auto-inserts pair\n   - Select text and type '\"' -> wraps the selected text\n   - Press backspace inside empty '()' -> deletes both\n\n2. ScratchHook (custom, see top of this file):\n   - Press Ctrl+D to duplicate the current line\n   - Type '- First item' and press Enter to auto-continue list\n   - Status bar counts words live via after_edit\n\nTry it below:\n- Item 1\n";
                    let mut ed = Editor::new(initial_text, cx);
                    ed.config.line_numbers = true;
                    // No explicit family: the editor auto-selects the first
                    // platform monospace with bold + italic faces (see
                    // `Editor::face_availability`). Set `ed.config.font_family
                    // explicitly to override (e.g. Menlo, Consolas).
                    ed.add_hook(AutoPairsHook::new());
                    ed.add_hook(ScratchHook::new(initial_text));
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
