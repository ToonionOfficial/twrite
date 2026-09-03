use gpui::*;
use gpui_platform::application;
use twrite::{
    CursorStyle, Editor, EditorHook, HookContext, HookOutcome, KeyEvent, Point, Selection,
};

/// The operating mode of the Vim state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum VimMode {
    #[default]
    Normal,
    Insert,
    Visual,
}

/// An editor hook implementing modal Vim keybindings (Normal, Insert, and Visual modes).
#[derive(Default)]
struct VimHook {
    mode: VimMode,
    pending_key: Option<char>,
}

impl VimHook {
    fn new() -> Self {
        Self {
            mode: VimMode::Normal,
            pending_key: None,
        }
    }

    fn enter_normal_mode(&mut self, ctx: &mut HookContext) {
        self.mode = VimMode::Normal;
        self.pending_key = None;
        *ctx.cursor_style = CursorStyle::Block;
        *ctx.selection = None;
    }

    fn enter_insert_mode(&mut self, ctx: &mut HookContext) {
        self.mode = VimMode::Insert;
        self.pending_key = None;
        *ctx.cursor_style = CursorStyle::Bar;
        *ctx.selection = None;
    }

    fn enter_visual_mode(&mut self, ctx: &mut HookContext) {
        self.mode = VimMode::Visual;
        self.pending_key = None;
        *ctx.cursor_style = CursorStyle::Block;
        let cursor = ctx.buffer.cursor_offset();
        *ctx.selection = Some(Selection::range(
            cursor,
            (cursor + 1).min(ctx.buffer.len_bytes()),
        ));
    }

    fn move_visual(&self, ctx: &mut HookContext, new_head: usize) {
        if let Some(sel) = ctx.selection.take() {
            ctx.buffer.set_cursor_offset(new_head);
            *ctx.selection = Some(Selection::range(sel.anchor, new_head));
        }
    }
}

impl EditorHook for VimHook {
    fn on_key(&mut self, ctx: &mut HookContext, event: &KeyEvent) -> HookOutcome {
        if event.key == "escape" || (event.modifiers.ctrl && event.key == "[") {
            self.enter_normal_mode(ctx);
            return HookOutcome::Consumed;
        }

        match self.mode {
            VimMode::Insert => HookOutcome::PassThrough,

            VimMode::Visual => {
                let cursor = ctx.buffer.cursor_offset();

                match event.key.as_str() {
                    "h" | "arrowleft" => {
                        let target = cursor.saturating_sub(1);
                        self.move_visual(ctx, target);
                        HookOutcome::Consumed
                    }
                    "l" | "arrowright" => {
                        let target = (cursor + 1).min(ctx.buffer.len_bytes());
                        self.move_visual(ctx, target);
                        HookOutcome::Consumed
                    }
                    "k" | "arrowup" => {
                        ctx.buffer.move_cursor_up();
                        let target = ctx.buffer.cursor_offset();
                        self.move_visual(ctx, target);
                        HookOutcome::Consumed
                    }
                    "j" | "arrowdown" => {
                        ctx.buffer.move_cursor_down();
                        let target = ctx.buffer.cursor_offset();
                        self.move_visual(ctx, target);
                        HookOutcome::Consumed
                    }
                    "w" => {
                        let target = ctx.buffer.next_word_offset();
                        self.move_visual(ctx, target);
                        HookOutcome::Consumed
                    }
                    "b" => {
                        let target = ctx.buffer.prev_word_offset();
                        self.move_visual(ctx, target);
                        HookOutcome::Consumed
                    }
                    "0" => {
                        let target = ctx.buffer.line_start_offset();
                        self.move_visual(ctx, target);
                        HookOutcome::Consumed
                    }
                    "$" => {
                        let target = ctx.buffer.line_end_offset();
                        self.move_visual(ctx, target);
                        HookOutcome::Consumed
                    }
                    "d" | "x" => {
                        if let Some(sel) = ctx.selection.take() {
                            let range = sel.byte_range();
                            if !range.is_empty() {
                                ctx.buffer.delete_range(range);
                            }
                        }
                        self.enter_normal_mode(ctx);
                        HookOutcome::Consumed
                    }
                    "y" => {
                        self.enter_normal_mode(ctx);
                        HookOutcome::Consumed
                    }
                    _ => HookOutcome::Consumed,
                }
            }

            VimMode::Normal => {
                if event.modifiers.ctrl {
                    match event.key.as_str() {
                        "r" => {
                            ctx.buffer.redo();
                            return HookOutcome::Consumed;
                        }
                        _ => return HookOutcome::PassThrough,
                    }
                }

                if let Some(pending) = self.pending_key.take() {
                    match (pending, event.key.as_str()) {
                        ('d', "d") => {
                            let row = ctx.buffer.cursor_point().row;
                            let line_start = ctx.buffer.point_to_offset(Point::new(row, 0));
                            let line_end = if row + 1 < ctx.buffer.len_lines() {
                                ctx.buffer.point_to_offset(Point::new(row + 1, 0))
                            } else {
                                ctx.buffer.len_bytes()
                            };
                            ctx.buffer.delete_range(line_start..line_end);
                            return HookOutcome::Consumed;
                        }
                        ('d', "w") => {
                            ctx.buffer.delete_next_word();
                            return HookOutcome::Consumed;
                        }
                        ('g', "g") => {
                            ctx.buffer.set_cursor_offset(0);
                            return HookOutcome::Consumed;
                        }
                        _ => return HookOutcome::Consumed,
                    }
                }

                match event.key.as_str() {
                    "h" | "arrowleft" => {
                        ctx.buffer.move_cursor_left();
                        HookOutcome::Consumed
                    }
                    "l" | "arrowright" => {
                        ctx.buffer.move_cursor_right();
                        HookOutcome::Consumed
                    }
                    "k" | "arrowup" => {
                        ctx.buffer.move_cursor_up();
                        HookOutcome::Consumed
                    }
                    "j" | "arrowdown" => {
                        ctx.buffer.move_cursor_down();
                        HookOutcome::Consumed
                    }
                    "w" => {
                        let target = ctx.buffer.next_word_offset();
                        ctx.buffer.set_cursor_offset(target);
                        HookOutcome::Consumed
                    }
                    "b" => {
                        let target = ctx.buffer.prev_word_offset();
                        ctx.buffer.set_cursor_offset(target);
                        HookOutcome::Consumed
                    }
                    "0" => {
                        let target = ctx.buffer.line_start_offset();
                        ctx.buffer.set_cursor_offset(target);
                        HookOutcome::Consumed
                    }
                    "$" => {
                        let target = ctx.buffer.line_end_offset();
                        ctx.buffer.set_cursor_offset(target);
                        HookOutcome::Consumed
                    }
                    "G" => {
                        let target = ctx.buffer.len_bytes();
                        ctx.buffer.set_cursor_offset(target);
                        HookOutcome::Consumed
                    }
                    "i" => {
                        self.enter_insert_mode(ctx);
                        HookOutcome::Consumed
                    }
                    "a" => {
                        ctx.buffer.move_cursor_right();
                        self.enter_insert_mode(ctx);
                        HookOutcome::Consumed
                    }
                    "o" => {
                        let end = ctx.buffer.line_end_offset();
                        ctx.buffer.set_cursor_offset(end);
                        ctx.buffer.insert("\n");
                        self.enter_insert_mode(ctx);
                        HookOutcome::Consumed
                    }
                    "O" => {
                        let start = ctx.buffer.line_start_offset();
                        ctx.buffer.set_cursor_offset(start);
                        ctx.buffer.insert("\n");
                        ctx.buffer.set_cursor_offset(start);
                        self.enter_insert_mode(ctx);
                        HookOutcome::Consumed
                    }
                    "x" => {
                        ctx.buffer.delete();
                        HookOutcome::Consumed
                    }
                    "u" => {
                        ctx.buffer.undo();
                        HookOutcome::Consumed
                    }
                    "v" => {
                        self.enter_visual_mode(ctx);
                        HookOutcome::Consumed
                    }
                    "d" => {
                        self.pending_key = Some('d');
                        HookOutcome::Consumed
                    }
                    "g" => {
                        self.pending_key = Some('g');
                        HookOutcome::Consumed
                    }
                    _ => HookOutcome::Consumed,
                }
            }
        }
    }

    fn status_text(&self) -> Option<&str> {
        match (self.mode, self.pending_key) {
            (VimMode::Normal, Some('d')) => Some("-- NORMAL (d) --"),
            (VimMode::Normal, Some('g')) => Some("-- NORMAL (g) --"),
            (VimMode::Normal, _) => Some("-- NORMAL --"),
            (VimMode::Insert, _) => Some("-- INSERT --"),
            (VimMode::Visual, _) => Some("-- VISUAL --"),
        }
    }
}

struct AppView {
    editor: Entity<Editor>,
}

impl Render for AppView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let editor_read = self.editor.read(cx);
        let cursor_point = editor_read.buffer.cursor_point();
        let status = editor_read.status_text().unwrap_or("-- NORMAL --");

        let status_badge_color = match status {
            s if s.starts_with("-- INSERT") => rgb(0x89b4fa),
            s if s.starts_with("-- VISUAL") => rgb(0xcba6f7),
            _ => rgb(0xa6e3a1),
        };

        div()
            .size_full()
            .flex()
            .flex_col()
            .child(div().flex_1().child(self.editor.clone()))
            .child(
                div()
                    .h(px(28.0))
                    .bg(rgb(0x11111b))
                    .px(px(12.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(12.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(status_badge_color)
                            .child(status.to_string()),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(rgb(0x6c7086))
                            .child(format!(
                                "Ln {}, Col {}",
                                cursor_point.row + 1,
                                cursor_point.column + 1
                            )),
                    ),
            )
    }
}

fn main() {
    application().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(850.0), px(620.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("TWrite - Vim Mode Example".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |window, cx| {
                let editor = cx.new(|cx| {
                    let mut ed = Editor::new(
                        "# Vim Mode in TWrite\n\nThis entire Vim modal editing system is powered by an EditorHook.\nZero lines of Vim code exist in the core twrite engine!\n\nKeybindings supported:\n- Normal Mode (Block cursor):\n  * h, j, k, l : move cursor\n  * w, b : next / previous word\n  * 0, $ : line start / line end\n  * G, gg : document bottom / document top\n  * x : delete character\n  * dd : delete current line\n  * dw : delete word\n  * u : undo, Ctrl+r : redo\n  * i, a : enter Insert mode\n  * o, O : open line below / above and enter Insert mode\n  * v : enter Visual mode\n- Visual Mode:\n  * Expand selection with h/j/k/l, w, b\n  * d or x : delete selection and return to Normal\n  * Escape : cancel selection\n- Insert Mode (Bar cursor):\n  * Type normally\n  * Escape : return to Normal mode\n",
                        cx,
                    );
                    ed.config.line_numbers = true;
                    // No explicit family: the editor auto-selects the first
                    // platform monospace with bold + italic faces (see
                    // `Editor::face_availability`). Set `ed.config.font_family
                    // explicitly to override (e.g. Menlo, Consolas).
                    let mut vim = VimHook::new();
                    let mut ctx = HookContext::new(&mut ed.buffer, &mut ed.selection, &mut ed.cursor_style);
                    vim.enter_normal_mode(&mut ctx);
                    ed.add_hook(vim);
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
