//! Supplemental example: the expandable right-click context menu.
//!
//! Not part of the numbered 1-6 sequence; read after `hooks`. Demonstrates
//! a custom [`EditorHook`] contributing menu rows: right-click shows the
//! built-in edit actions (Undo/Redo/Cut/Copy/Paste/Delete/Select All)
//! followed by hook rows. `MenuDemoHook` adds "UPPERCASE selection"
//! (enabled only with a non-empty selection) and "Insert separator".
//! Run with: `cargo run --example context_menu`
use gpui::*;
use twrite::{
    ContextMenuContext, ContextMenuItem, Editor, EditorHook, HookContext, HookOutcome, KeyCode,
    KeyHint,
};

/// Demo hook contributing two context menu rows.
#[derive(Default)]
struct MenuDemoHook;

impl EditorHook for MenuDemoHook {
    fn context_menu_items(&self, ctx: &ContextMenuContext) -> Vec<ContextMenuItem> {
        let has_selection = ctx.selection.is_some_and(|s| !s.byte_range().is_empty());
        let mut uppercase = ContextMenuItem::with_hint(
            "demo.uppercase",
            "UPPERCASE selection",
            KeyHint::ctrl(KeyCode::Char('U')),
        );
        uppercase.enabled = has_selection;
        vec![
            uppercase,
            ContextMenuItem::new("demo.insert-separator", "Insert separator"),
        ]
    }

    fn on_context_menu_action(&mut self, ctx: &mut HookContext, id: &str) -> HookOutcome {
        match id {
            "demo.uppercase" => {
                if let Some(sel) = ctx.selection.take() {
                    let range = sel.byte_range();
                    if !range.is_empty() {
                        let text = ctx.buffer.text().byte_slice(range.clone()).to_string();
                        ctx.buffer.replace_range(range, &text.to_uppercase());
                    }
                }
                HookOutcome::Consumed
            }
            "demo.insert-separator" => {
                ctx.buffer.insert("\n---\n");
                HookOutcome::Consumed
            }
            _ => HookOutcome::PassThrough,
        }
    }

    fn status_text(&self) -> Option<&str> {
        Some("RIGHT-CLICK FOR MENU")
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
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(800.0), px(600.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("TWrite - Context Menu Demo".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |window, cx| {
                let editor = cx.new(|cx| {
                    let initial_text = "Right-click anywhere in this editor.\n\nThe menu leads with built-in edit rows (Undo, Redo, Cut, Copy, Paste, Delete, Select All), then hook rows:\n\n- Select some text and pick \"UPPERCASE selection\"\n- Pick \"Insert separator\" to drop in a rule\n- Disabled rows (e.g. Copy with no selection) render dimmed\n";
                    let mut ed = Editor::new(initial_text, cx);
                    ed.config.line_numbers = true;
                    // No explicit family: the editor auto-selects the first
                    // platform monospace with bold + italic faces (see
                    // `Editor::face_availability`). Set `ed.config.font_family
                    // explicitly to override (e.g. Menlo, Consolas).
                    ed.add_hook(MenuDemoHook);
                    ed
                });

                let focus_handle = editor.read(cx).focus_handle.clone();
                focus_handle.focus(window);

                cx.new(|_| AppView { editor })
            },
        )
        .unwrap();

        cx.activate(true);
    });
}
