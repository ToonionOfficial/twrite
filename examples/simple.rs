use gpui::*;
use gpui_platform::application;
use twrite::Editor;

struct AppView {
    editor: Entity<Editor>,
}

impl Render for AppView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.editor.clone())
    }
}

fn main() {
    application().run(|cx: &mut App| {
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
                        "Hello from TWrite!\n\nThis is a simple text editor built with GPUI.\n\nTry:\n- Typing text\n- Backspacing and Enter\n- Moving the cursor with Arrow Keys\n- Undo (Ctrl+Z) and Redo (Ctrl+Y or Ctrl+Shift+Z)\n- Select All (Ctrl+A)\n- Scrolling with mouse wheel\n",
                        cx,
                    );
                    ed.config.line_numbers = true;
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
