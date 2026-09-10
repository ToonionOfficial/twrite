//! Getting-started example 4 of 6: custom input boxes on `ctx.prompt`.
//!
//! Read after `hooks`. Demonstrates the shared prompt primitive with two tiny
//! hooks and zero frontend code: `GotoLineHook` (a submit-and-interpret bottom
//! bar) and `PaletteHook` (a live-filtered top palette). The `PromptBar`
//! renderer draws whatever prompt is open; hooks own the meaning.
//! Run with: `cargo run --example prompt`
//! Next: `markdown` (batteries) or `vim` (a full system on hooks).
use gpui::*;
use gpui_platform::application;
use twrite::{
    Editor, EditorHook, HookContext, HookOutcome, KeyEvent, Point, PromptAction, PromptItem,
    PromptPlacement, PromptSpec, Selection, fps_badge, fuzzy_filter,
};

/// Jump to a line number: the submit-and-interpret shape.
///
/// Open with `Ctrl+G`, type a number, `Enter` jumps. Bad input shows an error
/// in the box and stays open so you can retry; `Esc` cancels.
struct GotoLineHook;

impl EditorHook for GotoLineHook {
    fn on_key(&mut self, ctx: &mut HookContext, event: &KeyEvent) -> HookOutcome {
        if !ctx.prompt.is_open() {
            if event.key == "g" && event.modifiers.ctrl && !event.modifiers.alt {
                ctx.prompt.open(
                    PromptSpec::new(
                        "goto-line",
                        ":",
                        "Line number",
                        PromptPlacement::BottomBar,
                        false,
                    ),
                    "",
                );
                return HookOutcome::Consumed;
            }
            return HookOutcome::PassThrough;
        }
        if !ctx.prompt.spec().is_some_and(|s| s.id == "goto-line") {
            return HookOutcome::PassThrough;
        }

        match ctx.prompt.handle_key(event) {
            PromptAction::Editing => HookOutcome::Consumed,
            PromptAction::Submitted(input) => {
                match input.trim().parse::<usize>() {
                    Ok(num) if num >= 1 && num <= ctx.buffer.len_lines() => {
                        let target = ctx.buffer.point_to_offset(Point::new(num - 1, 0));
                        ctx.buffer.set_cursor_offset(target);
                        *ctx.selection = None;
                        ctx.prompt.close();
                    }
                    _ => ctx.prompt.set_message("Not a line number"),
                }
                HookOutcome::Consumed
            }
            PromptAction::Cancelled => HookOutcome::Consumed,
            PromptAction::Ignored => HookOutcome::Consumed,
        }
    }
}

/// One palette command: a label, a hint, and a buffer action.
#[derive(Clone, Copy)]
enum Command {
    Uppercase,
    Lowercase,
    DuplicateLine,
    SelectAll,
}

impl Command {
    fn all() -> [Command; 4] {
        use Command::*;
        [Uppercase, Lowercase, DuplicateLine, SelectAll]
    }

    fn label(self) -> &'static str {
        match self {
            Command::Uppercase => "Uppercase selection",
            Command::Lowercase => "Lowercase selection",
            Command::DuplicateLine => "Duplicate line",
            Command::SelectAll => "Select all",
        }
    }

    fn hint(self) -> &'static str {
        match self {
            Command::Uppercase => "UPPER the selection",
            Command::Lowercase => "lower the selection",
            Command::DuplicateLine => "copy line above",
            Command::SelectAll => "Ctrl+A by palette",
        }
    }
}

/// A live-filtered command palette: the item-list shape.
///
/// Open with `Ctrl+P`. Every keystroke re-ranks the rows with `fuzzy_filter`;
/// `Up`/`Down` move, `Tab` completes the highlighted row into the input,
/// `Enter` runs it. Reuses the same `ctx.prompt` primitive as the goto box.
struct PaletteHook;

impl PaletteHook {
    fn refresh_items(ctx: &mut HookContext) {
        let all: Vec<PromptItem> = Command::all()
            .iter()
            .map(|c| PromptItem::with_hint(c.label(), c.hint()))
            .collect();
        let ranked = fuzzy_filter(&all, ctx.prompt.input());
        ctx.prompt
            .set_items(ranked.into_iter().map(|(i, _)| all[i].clone()).collect());
    }

    fn run_selected(ctx: &mut HookContext) {
        let Some(picked) = ctx.prompt.selected_item().map(|item| item.label.clone()) else {
            ctx.prompt.set_message("No match");
            return;
        };
        let command = Command::all().into_iter().find(|c| c.label() == picked);
        match command {
            Some(Command::Uppercase) | Some(Command::Lowercase) => {
                let upper = matches!(command, Some(Command::Uppercase));
                let range = ctx.selection.map(|s| s.byte_range());
                match range {
                    Some(range) if !range.is_empty() => {
                        let text = ctx.buffer.text().byte_slice(range.clone()).to_string();
                        let converted = if upper {
                            text.to_uppercase()
                        } else {
                            text.to_lowercase()
                        };
                        ctx.buffer.replace_range(range.clone(), &converted);
                        *ctx.selection =
                            Some(Selection::range(range.start, range.start + converted.len()));
                        ctx.prompt.close();
                    }
                    _ => ctx.prompt.set_message("Select text first"),
                }
            }
            Some(Command::DuplicateLine) => {
                let row = ctx.buffer.cursor_point().row;
                let line_start = ctx.buffer.point_to_offset(Point::new(row, 0));
                let stripped = ctx.buffer.line_to_string(row);
                let stripped = stripped.trim_end_matches(['\r', '\n']);
                ctx.buffer
                    .replace_range(line_start..line_start, &format!("{stripped}\n"));
                ctx.prompt.close();
            }
            Some(Command::SelectAll) => {
                *ctx.selection = Some(Selection::range(0, ctx.buffer.len_bytes()));
                ctx.prompt.close();
            }
            None => ctx.prompt.set_message("No match"),
        }
    }
}

impl EditorHook for PaletteHook {
    fn on_key(&mut self, ctx: &mut HookContext, event: &KeyEvent) -> HookOutcome {
        if !ctx.prompt.is_open() {
            if event.key == "p" && event.modifiers.ctrl && !event.modifiers.alt {
                ctx.prompt.open(
                    PromptSpec::new(
                        "commands",
                        ">",
                        "Type a command",
                        PromptPlacement::TopPalette,
                        true,
                    ),
                    "",
                );
                Self::refresh_items(ctx);
                return HookOutcome::Consumed;
            }
            return HookOutcome::PassThrough;
        }
        if !ctx.prompt.spec().is_some_and(|s| s.id == "commands") {
            return HookOutcome::PassThrough;
        }

        match ctx.prompt.handle_key(event) {
            PromptAction::Editing => {
                Self::refresh_items(ctx);
                HookOutcome::Consumed
            }
            PromptAction::Submitted(input) => {
                if input.trim().is_empty() {
                    ctx.prompt.close();
                } else {
                    Self::run_selected(ctx);
                }
                HookOutcome::Consumed
            }
            PromptAction::Cancelled => HookOutcome::Consumed,
            PromptAction::Ignored => HookOutcome::Consumed,
        }
    }
}

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
    application().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(800.0), px(600.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("TWrite - Prompt Input Demo".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |window, cx| {
                let editor = cx.new(|cx| {
                    let mut ed = Editor::new(
                        "# Prompt Input Demo\n\nTwo tiny hooks, zero frontend code: everything below runs on ctx.prompt.\n\n- Ctrl+G: go to line (bottom bar, retries on bad input)\n- Ctrl+P: command palette (type to filter, Up/Down to move, Tab to complete, Enter to run)\n- Esc closes; Up/Down walks history in the goto box\n\nTry selecting this line and running Uppercase selection from the palette.\n",
                        cx,
                    );
                    ed.config.line_numbers = true;
                    // No explicit family: the editor auto-selects the first
                    // platform monospace with bold + italic faces (see
                    // `Editor::face_availability`). Set `ed.config.font_family
                    // explicitly to override (e.g. Menlo, Consolas).
                    ed.add_hook(GotoLineHook);
                    ed.add_hook(PaletteHook);
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
