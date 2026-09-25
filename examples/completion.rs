//! Supplemental example: inline completion on the hook system alone.
//!
//! Not part of the numbered 1-6 sequence; read after `hooks`. Needs no
//! feature flags. A single custom [`EditorHook`] implements an `@mention`
//! popup end to end: it opens a session when `@` is typed, filters a word
//! list as you type, navigates with `Up`/`Down`, accepts with `Enter`/`Tab`,
//! and dismisses with `Esc`, `Space`, or a cursor move. The GPUI layer
//! draws the popup from `completion_snapshot` with zero battery code,
//! proving the completion surface is generic core machinery.
//! Run with: `cargo run --example completion`
use gpui::*;
use twrite::{
    CompletionSnapshot, Editor, EditorBuffer, EditorHook, HookContext, HookOutcome, KeyCode,
    KeyEvent, Modifiers, PromptItem, Selection, fuzzy_filter,
};

/// Words offered as mentions; any host list works here.
const WORDS: &[&str] = &[
    "ada",
    "alan",
    "alonzo",
    "barbara",
    "dijkstra",
    "grace",
    "hedy",
    "katherine",
    "margaret",
    "radia",
];

/// Live `@` session: anchor plus ranked rows.
struct MentionSession {
    start: usize,
    row: usize,
    items: Vec<PromptItem>,
    selected: usize,
}

/// Demo hook driving the whole interaction with no battery involved.
struct MentionHook {
    session: Option<MentionSession>,
}

impl MentionHook {
    fn query(buffer: &EditorBuffer, session: &MentionSession) -> Option<String> {
        let cursor = buffer.cursor_offset();
        if cursor <= session.start || buffer.cursor_point().row != session.row {
            return None;
        }
        let text = buffer.text();
        if !text
            .get_byte_slice(session.start..session.start + 1)
            .is_some_and(|slice| slice == "@")
        {
            return None;
        }
        text.get_byte_slice(session.start + 1..cursor)
            .map(|slice| slice.chars().collect())
    }

    fn refresh(&mut self, query: &str) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let candidates: Vec<PromptItem> = WORDS
            .iter()
            .filter(|word| word.contains(query))
            .map(|word| PromptItem::new(word))
            .collect();
        session.items = fuzzy_filter(&candidates, query)
            .into_iter()
            .take(8)
            .map(|(index, _)| candidates[index].clone())
            .collect();
        session.selected = session.selected.min(session.items.len().saturating_sub(1));
    }

    fn accept_at(&mut self, ctx: &mut HookContext, index: usize) -> bool {
        let (start, label) = match self.session.as_ref() {
            Some(session) if index < session.items.len() => {
                (session.start, session.items[index].label.clone())
            }
            _ => return false,
        };
        let cursor = ctx.buffer.cursor_offset();
        if cursor <= start {
            self.session = None;
            return false;
        }
        ctx.buffer.replace_range(start + 1..cursor, &label);
        let end = start + 1 + label.len();
        ctx.buffer.set_cursor_offset(end);
        ctx.buffer.insert(" ");
        *ctx.selection = None;
        self.session = None;
        true
    }
}

fn plain(modifiers: &Modifiers) -> bool {
    !modifiers.ctrl && !modifiers.alt && !modifiers.meta
}

impl EditorHook for MentionHook {
    fn on_key(&mut self, ctx: &mut HookContext, event: &KeyEvent) -> HookOutcome {
        if self.session.is_some() {
            let live = self
                .session
                .as_ref()
                .is_some_and(|session| Self::query(ctx.buffer, session).is_some());
            if !live {
                self.session = None;
            } else {
                match &event.code {
                    KeyCode::Escape => {
                        self.session = None;
                        return HookOutcome::Consumed;
                    }
                    KeyCode::Up if plain(&event.modifiers) => {
                        if let Some(session) = self.session.as_mut()
                            && !session.items.is_empty()
                        {
                            session.selected = session
                                .selected
                                .checked_sub(1)
                                .unwrap_or(session.items.len() - 1);
                        }
                        return HookOutcome::Consumed;
                    }
                    KeyCode::Down if plain(&event.modifiers) => {
                        if let Some(session) = self.session.as_mut()
                            && !session.items.is_empty()
                        {
                            session.selected = (session.selected + 1) % session.items.len();
                        }
                        return HookOutcome::Consumed;
                    }
                    KeyCode::Enter | KeyCode::Tab if plain(&event.modifiers) => {
                        let index = self.session.as_ref().map(|s| s.selected);
                        match index {
                            Some(index)
                                if self.session.as_ref().is_some_and(|s| !s.items.is_empty()) =>
                            {
                                self.accept_at(ctx, index);
                                return HookOutcome::Consumed;
                            }
                            _ => self.session = None,
                        }
                    }
                    KeyCode::Backspace if plain(&event.modifiers) => {
                        let at_anchor = self
                            .session
                            .as_ref()
                            .is_some_and(|s| ctx.buffer.cursor_offset() <= s.start + 1);
                        if at_anchor {
                            self.session = None;
                            return HookOutcome::PassThrough;
                        }
                        ctx.buffer.backspace();
                        let cursor = ctx.buffer.cursor_offset();
                        let start = self.session.as_ref().map(|s| s.start).unwrap_or(0);
                        let query = ctx
                            .buffer
                            .text()
                            .get_byte_slice(start + 1..cursor)
                            .map(|slice| slice.chars().collect::<String>())
                            .unwrap_or_default();
                        self.refresh(&query);
                        return HookOutcome::Consumed;
                    }
                    KeyCode::Char(' ') => {
                        self.session = None;
                        return HookOutcome::PassThrough;
                    }
                    KeyCode::Char(current) if plain(&event.modifiers) => {
                        let mut text = [0u8; 4];
                        ctx.buffer.insert(current.encode_utf8(&mut text));
                        let cursor = ctx.buffer.cursor_offset();
                        let start = self.session.as_ref().map(|s| s.start).unwrap_or(0);
                        let query = ctx
                            .buffer
                            .text()
                            .get_byte_slice(start + 1..cursor)
                            .map(|slice| slice.chars().collect::<String>())
                            .unwrap_or_default();
                        self.refresh(&query);
                        return HookOutcome::Consumed;
                    }
                    _ => {}
                }
            }
        }
        if let KeyCode::Char('@') = event.code
            && plain(&event.modifiers)
        {
            ctx.buffer.insert("@");
            let start = ctx.buffer.cursor_offset() - 1;
            let row = ctx.buffer.offset_to_point(start).row;
            self.session = Some(MentionSession {
                start,
                row,
                items: Vec::new(),
                selected: 0,
            });
            self.refresh("");
            return HookOutcome::Consumed;
        }
        HookOutcome::PassThrough
    }

    fn on_selection_change(&mut self, buffer: &EditorBuffer, _selection: Option<&Selection>) {
        let live = self
            .session
            .as_ref()
            .is_some_and(|session| Self::query(buffer, session).is_some());
        if !live {
            self.session = None;
        }
    }

    fn completion_snapshot(&self) -> Option<CompletionSnapshot> {
        self.session.as_ref().map(|session| CompletionSnapshot {
            items: session.items.clone(),
            selected: session.selected,
        })
    }

    fn on_completion_select(&mut self, ctx: &mut HookContext, index: usize) -> HookOutcome {
        if self.accept_at(ctx, index) {
            HookOutcome::Consumed
        } else {
            HookOutcome::PassThrough
        }
    }

    fn dismiss_completion(&mut self) {
        self.session = None;
    }

    fn status_text(&self) -> Option<&str> {
        Some("TYPE @ FOR MENTIONS")
    }
}

struct CompletionApp {
    editor: Entity<Editor>,
}

impl Render for CompletionApp {
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
                    title: Some("TWrite - Completion Demo".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |window, cx| {
                let editor = cx.new(|cx| {
                    let initial_text = "Type @ to mention someone.\n\nTry @gr then Enter, or @a then Down + Tab.\nEsc, Space, or moving the cursor dismisses the popup.\n";
                    let mut ed = Editor::new(initial_text, cx);
                    ed.config.line_numbers = true;
                    ed.add_hook(MentionHook { session: None });
                    ed
                });

                let focus_handle = editor.read(cx).focus_handle.clone();
                twrite::focus_editor(&focus_handle, window, cx);

                cx.new(|_| CompletionApp { editor })
            },
        )
        .unwrap();

        cx.activate(true);
    });
}
