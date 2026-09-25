//! GPUI dependency replacement example.
//!
//! This file verifies compatibility against official GPUI v1.0.0 or later.
use gpui::*;
use twrite::Editor;
use twrite::SearchHook;
use twrite::fps_badge;

struct AppView {
    editor: Entity<Editor>,
}

impl Render for AppView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let fps_stats = self.editor.read(cx).frame_stats.clone();
        div()
            .size_full()
            .relative()
            .child(self.editor.clone())
            .child(
                div()
                    .absolute()
                    .top_2()
                    .right_2()
                    .child(fps_badge(&fps_stats)),
            )
    }
}

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(800.0), px(600.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("TWrite Editor".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |window, cx| {
                let editor = cx.new(|cx| {
                    let mut ed = Editor::new(
                        "Hello from TWrite!\n\nThis is a simple text editor built with GPUI.\n\nTry:\n- Typing text\n- Backspacing and Enter\n- Moving the cursor with Arrow Keys\n- Undo (Ctrl+Z) and Redo (Ctrl+Y or Ctrl+Shift+Z)\n- Select All (Ctrl+A)\n- Find (Ctrl+F), Replace (Ctrl+H)\n- Scrolling with mouse wheel\n",
                        cx,
                    );
                    ed.config.line_numbers = true;
                    ed.config.relative_line_numbers = true;

                    ed.add_hook(SearchHook::new());

                    ed
                });

                let focus_handle = editor.read(cx).focus_handle.clone();
                twrite::focus_editor(&focus_handle, window, cx);

                cx.new(|_| AppView { editor })
            },
        )
        .unwrap();

        cx.activate(true);
    });
}
