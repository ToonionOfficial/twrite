use crate::{EditorBuffer, Selection};
use crate::{HookEffect, PromptState};

/// Keyboard modifier keys state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifiers {
    /// Whether the Control key is pressed.
    pub ctrl: bool,
    /// Whether the Alt key is pressed.
    pub alt: bool,
    /// Whether the Shift key is pressed.
    pub shift: bool,
    /// Whether the Meta / Command / Windows key is pressed.
    pub meta: bool,
}

impl Modifiers {
    /// Creates an empty modifier set with all modifiers disabled.
    pub const fn empty() -> Self {
        Self {
            ctrl: false,
            alt: false,
            shift: false,
            meta: false,
        }
    }
}

/// A normalized keyboard event passed to editor hooks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyEvent {
    /// The normalized key string (e.g. "a", "enter", "backspace", "escape").
    pub key: String,
    /// The active keyboard modifiers during the key event.
    pub modifiers: Modifiers,
}

impl KeyEvent {
    /// Creates a key event without modifiers.
    pub fn plain(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            modifiers: Modifiers::empty(),
        }
    }
}

/// The visual style of the text cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CursorStyle {
    /// Vertical bar / I-beam cursor.
    #[default]
    Bar,
    /// Solid full-character block cursor.
    Block,
    /// Horizontal underline cursor.
    Underline,
    /// Invisible cursor.
    Hidden,
}

/// The mutable editing context passed to editor hooks during events.
///
/// `prompt` is the shared headless input box (bottom bar / command palette):
/// any hook can `open` it, read submitted input, and fill item rows, with no
/// frontend code. `effects` collects app-level requests (save/load/quit)
/// that the host drains after each event.
pub struct HookContext<'a> {
    /// Mutable access to the underlying text buffer.
    pub buffer: &'a mut EditorBuffer,
    /// Mutable access to the active selection range, if any.
    pub selection: &'a mut Option<Selection>,
    /// Mutable access to the cursor visual style.
    pub cursor_style: &'a mut CursorStyle,
    /// Shared headless prompt / input-box state.
    pub prompt: &'a mut PromptState,
    /// App-level requests for the host to drain after the event.
    pub effects: &'a mut Vec<HookEffect>,
}

impl<'a> HookContext<'a> {
    /// Creates a new hook context.
    pub fn new(
        buffer: &'a mut EditorBuffer,
        selection: &'a mut Option<Selection>,
        cursor_style: &'a mut CursorStyle,
        prompt: &'a mut PromptState,
        effects: &'a mut Vec<HookEffect>,
    ) -> Self {
        Self {
            buffer,
            selection,
            cursor_style,
            prompt,
            effects,
        }
    }
}

/// The outcome of an editor hook handling an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookOutcome {
    /// The hook handled the event; halt propagation to subsequent hooks and default editor handlers.
    Consumed,
    /// The hook did not consume the event; continue propagation to the next hook or default editor handler.
    PassThrough,
}

/// Trait for intercepting input events, mutating buffer state, and extending editor behaviors.
pub trait EditorHook: 'static {
    /// Intercepts a key press before standard editor processing.
    fn on_key(&mut self, _ctx: &mut HookContext, _event: &KeyEvent) -> HookOutcome {
        HookOutcome::PassThrough
    }

    /// Intercepts text insertion before it is written to the buffer.
    fn before_insert(&mut self, _ctx: &mut HookContext, _text: &str) -> HookOutcome {
        HookOutcome::PassThrough
    }

    /// Called immediately after any buffer mutation (typing, deletions, paste, undo/redo).
    fn after_edit(&mut self, _buffer: &mut EditorBuffer) {}

    /// Called whenever the cursor offset or selection range changes.
    fn on_selection_change(&mut self, _buffer: &EditorBuffer, _selection: Option<&Selection>) {}

    /// Intercepts a mouse click at a given buffer line and source byte column.
    ///
    /// `row` is the zero-based buffer row. `col` is the source byte offset within
    /// that line's text (pre-concealment coordinates, clamped to the line length).
    /// Hooks handling interactive elements (e.g. Markdown task checkboxes) should
    /// operate on `row` directly rather than the current cursor row, preserve the
    /// cursor offset when appropriate, and return `Consumed` to halt propagation.
    fn on_click(&mut self, _ctx: &mut HookContext, _row: usize, _col: usize) -> HookOutcome {
        HookOutcome::PassThrough
    }

    /// Returns a human-readable status or active mode name, if any.
    fn status_text(&self) -> Option<&str> {
        None
    }
}

/// Built-in hook that automatically inserts closing quotes, brackets, and braces, wraps selected text, and steps over closing pairs.
#[derive(Debug, Clone, Default)]
pub struct AutoPairsHook;

impl AutoPairsHook {
    /// Creates a new auto-pairs hook.
    pub fn new() -> Self {
        Self
    }

    fn matching_close(c: char) -> Option<char> {
        match c {
            '(' => Some(')'),
            '[' => Some(']'),
            '{' => Some('}'),
            '"' => Some('"'),
            '\'' => Some('\''),
            '`' => Some('`'),
            _ => None,
        }
    }

    fn is_pair(open: char, close: char) -> bool {
        Self::matching_close(open) == Some(close)
    }
}

impl EditorHook for AutoPairsHook {
    fn on_key(&mut self, ctx: &mut HookContext, event: &KeyEvent) -> HookOutcome {
        if event.modifiers.ctrl || event.modifiers.alt || event.modifiers.meta {
            return HookOutcome::PassThrough;
        }

        if event.key == "backspace" && ctx.selection.is_none() {
            let cursor = ctx.buffer.cursor_offset();
            if cursor > 0 && cursor < ctx.buffer.len_bytes() {
                let text = ctx.buffer.text();
                let prev_char = text.char_at_byte_offset(cursor - 1);
                let next_char = text.char_at_byte_offset(cursor);
                if let (Some(p), Some(n)) = (prev_char, next_char)
                    && Self::is_pair(p, n)
                {
                    ctx.buffer.delete();
                    ctx.buffer.backspace();
                    return HookOutcome::Consumed;
                }
            }
            return HookOutcome::PassThrough;
        }

        if event.key.chars().count() == 1 {
            let ch = event.key.chars().next().unwrap();

            if let Some(close) = Self::matching_close(ch) {
                if let Some(sel) = ctx.selection.take() {
                    let range = sel.byte_range();
                    let selected_text = ctx.buffer.text().byte_slice(range.clone()).to_string();
                    let wrapped = format!("{}{}{}", ch, selected_text, close);
                    ctx.buffer.replace_range(range.clone(), &wrapped);
                    *ctx.selection = Some(Selection::range(range.start + 1, range.end + 1));
                    return HookOutcome::Consumed;
                }

                let cursor = ctx.buffer.cursor_offset();
                let text = ctx.buffer.text();
                let next_char = text.char_at_byte_offset(cursor);

                if (ch == '"' || ch == '\'' || ch == '`') && next_char == Some(ch) {
                    ctx.buffer.move_cursor_right();
                    return HookOutcome::Consumed;
                }

                ctx.buffer.insert(&format!("{}{}", ch, close));
                ctx.buffer.move_cursor_left();
                return HookOutcome::Consumed;
            }

            if ch == ')' || ch == ']' || ch == '}' {
                let cursor = ctx.buffer.cursor_offset();
                let text = ctx.buffer.text();
                let next_char = text.char_at_byte_offset(cursor);
                if next_char == Some(ch) {
                    ctx.buffer.move_cursor_right();
                    return HookOutcome::Consumed;
                }
            }
        }

        HookOutcome::PassThrough
    }
}

trait CharAtByteOffset {
    fn char_at_byte_offset(&self, offset: usize) -> Option<char>;
}

impl CharAtByteOffset for ropey::Rope {
    fn char_at_byte_offset(&self, offset: usize) -> Option<char> {
        if offset >= self.len_bytes() {
            return None;
        }
        let char_idx = self.byte_to_char(offset);
        Some(self.char(char_idx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PromptState;

    struct MockModalHook {
        mode: String,
    }

    impl EditorHook for MockModalHook {
        fn on_key(&mut self, ctx: &mut HookContext, event: &KeyEvent) -> HookOutcome {
            if event.key == "escape" {
                self.mode = "NORMAL".into();
                *ctx.cursor_style = CursorStyle::Block;
                *ctx.selection = None;
                return HookOutcome::Consumed;
            }

            if self.mode == "NORMAL" {
                match event.key.as_str() {
                    "i" => {
                        self.mode = "INSERT".into();
                        *ctx.cursor_style = CursorStyle::Bar;
                        HookOutcome::Consumed
                    }
                    "v" => {
                        self.mode = "VISUAL".into();
                        *ctx.selection = Some(Selection::point(ctx.buffer.cursor_offset()));
                        HookOutcome::Consumed
                    }
                    "x" => {
                        ctx.buffer.delete();
                        HookOutcome::Consumed
                    }
                    _ => HookOutcome::Consumed,
                }
            } else {
                HookOutcome::PassThrough
            }
        }

        fn status_text(&self) -> Option<&str> {
            Some(&self.mode)
        }
    }

    #[test]
    fn test_modal_hook_transitions() {
        let mut buffer = EditorBuffer::new("hello");
        let mut selection = None;
        let mut cursor_style = CursorStyle::Block;
        let mut prompt = PromptState::new();
        let mut effects = Vec::new();
        let mut hook = MockModalHook {
            mode: "NORMAL".into(),
        };

        let mut ctx = HookContext::new(
            &mut buffer,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
        );

        assert_eq!(hook.status_text(), Some("NORMAL"));

        let outcome = hook.on_key(&mut ctx, &KeyEvent::plain("i"));
        assert_eq!(outcome, HookOutcome::Consumed);
        assert_eq!(hook.status_text(), Some("INSERT"));
        assert_eq!(*ctx.cursor_style, CursorStyle::Bar);

        let outcome = hook.on_key(&mut ctx, &KeyEvent::plain("a"));
        assert_eq!(outcome, HookOutcome::PassThrough);

        let outcome = hook.on_key(&mut ctx, &KeyEvent::plain("escape"));
        assert_eq!(outcome, HookOutcome::Consumed);
        assert_eq!(hook.status_text(), Some("NORMAL"));
        assert_eq!(*ctx.cursor_style, CursorStyle::Block);

        let outcome = hook.on_key(&mut ctx, &KeyEvent::plain("v"));
        assert_eq!(outcome, HookOutcome::Consumed);
        assert_eq!(hook.status_text(), Some("VISUAL"));
        assert!(ctx.selection.is_some());
    }

    #[test]
    fn test_autopairs_insert_and_wrap() {
        let mut buffer = EditorBuffer::new("");
        let mut selection = None;
        let mut cursor_style = CursorStyle::Bar;
        let mut prompt = PromptState::new();
        let mut effects = Vec::new();
        let mut autopairs = AutoPairsHook::new();

        let mut ctx = HookContext::new(
            &mut buffer,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
        );

        let outcome = autopairs.on_key(&mut ctx, &KeyEvent::plain("("));
        assert_eq!(outcome, HookOutcome::Consumed);
        assert_eq!(ctx.buffer.text().to_string(), "()");
        assert_eq!(ctx.buffer.cursor_offset(), 1);

        let outcome = autopairs.on_key(&mut ctx, &KeyEvent::plain(")"));
        assert_eq!(outcome, HookOutcome::Consumed);
        assert_eq!(ctx.buffer.text().to_string(), "()");
        assert_eq!(ctx.buffer.cursor_offset(), 2);

        ctx.buffer.set_cursor_offset(1);
        let outcome = autopairs.on_key(&mut ctx, &KeyEvent::plain("backspace"));
        assert_eq!(outcome, HookOutcome::Consumed);
        assert_eq!(ctx.buffer.text().to_string(), "");
        assert_eq!(ctx.buffer.cursor_offset(), 0);

        ctx.buffer.insert("word");
        *ctx.selection = Some(Selection::range(0, 4));
        let outcome = autopairs.on_key(&mut ctx, &KeyEvent::plain("\""));
        assert_eq!(outcome, HookOutcome::Consumed);
        assert_eq!(ctx.buffer.text().to_string(), "\"word\"");
    }

    /// Proof that a full vim-style search + ex-command loop runs on hooks +
    /// core alone: no frontend, no GPUI. `/` opens a live search prompt,
    /// `Enter` jumps to the match; `:` opens an ex prompt, `w`/`q` push
    /// app-level effects for the host to drain.
    struct MiniVim {
        search: crate::SearchState,
    }

    impl EditorHook for MiniVim {
        fn on_key(&mut self, ctx: &mut HookContext, event: &KeyEvent) -> HookOutcome {
            use crate::{HookEffect, PromptAction, PromptPlacement, PromptSpec, SearchQuery};

            if ctx.prompt.is_open() {
                let is_search = ctx.prompt.spec().is_some_and(|s| s.id == "search");
                match ctx.prompt.handle_key(event) {
                    PromptAction::Editing => {
                        if is_search && !ctx.prompt.input().is_empty() {
                            let input = ctx.prompt.input().to_string();
                            self.search.set_query(SearchQuery::literal(&input));
                            let _ = self.search.refresh(ctx.buffer);
                        }
                        return HookOutcome::Consumed;
                    }
                    PromptAction::Submitted(input) => {
                        if is_search {
                            let from = ctx.buffer.cursor_offset();
                            if let Ok(Some(m)) = self.search.next(ctx.buffer, from, true) {
                                ctx.buffer.set_cursor_offset(m.start);
                                *ctx.selection = Some(Selection::range(m.start, m.end));
                            }
                        } else {
                            match input.as_str() {
                                "w" => ctx.effects.push(HookEffect::Save { path: None }),
                                "q" => ctx.effects.push(HookEffect::Quit { force: false }),
                                "q!" => ctx.effects.push(HookEffect::Quit { force: true }),
                                other => ctx.effects.push(HookEffect::Message(format!(
                                    "E492: Not an editor command: {other}"
                                ))),
                            }
                        }
                        ctx.prompt.close();
                        return HookOutcome::Consumed;
                    }
                    PromptAction::Cancelled | PromptAction::Ignored => {
                        return HookOutcome::Consumed;
                    }
                }
            }
            match event.key.as_str() {
                "/" => {
                    ctx.prompt.open(
                        PromptSpec::new("search", "/", "Search", PromptPlacement::BottomBar, true),
                        "",
                    );
                    HookOutcome::Consumed
                }
                ":" => {
                    ctx.prompt.open(
                        PromptSpec::new("vim-ex", ":", "", PromptPlacement::BottomBar, false),
                        "",
                    );
                    HookOutcome::Consumed
                }
                _ => HookOutcome::PassThrough,
            }
        }
    }

    #[test]
    fn test_hook_only_vim_search_and_ex_commands() {
        use crate::{HookEffect, PromptState};

        let mut buffer = EditorBuffer::new("foo bar foo");
        let mut selection = None;
        let mut cursor_style = CursorStyle::Bar;
        let mut prompt = PromptState::new();
        let mut effects = Vec::new();
        let mut vim = MiniVim {
            search: crate::SearchState::new(),
        };
        let mut ctx = HookContext::new(
            &mut buffer,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
        );

        // `/foo` + Enter jumps to the first match and selects it.
        assert_eq!(
            vim.on_key(&mut ctx, &KeyEvent::plain("/")),
            HookOutcome::Consumed
        );
        assert!(ctx.prompt.is_open());
        for k in ["f", "o", "o"] {
            assert_eq!(
                vim.on_key(&mut ctx, &KeyEvent::plain(k)),
                HookOutcome::Consumed
            );
        }
        assert_eq!(vim.search.match_count(), 2);
        assert_eq!(
            vim.on_key(&mut ctx, &KeyEvent::plain("enter")),
            HookOutcome::Consumed
        );
        assert!(!ctx.prompt.is_open());
        assert_eq!(ctx.selection.unwrap().byte_range(), 0..3);

        // `:w` queues a save effect for the host.
        assert_eq!(
            vim.on_key(&mut ctx, &KeyEvent::plain(":")),
            HookOutcome::Consumed
        );
        assert_eq!(
            vim.on_key(&mut ctx, &KeyEvent::plain("w")),
            HookOutcome::Consumed
        );
        assert_eq!(
            vim.on_key(&mut ctx, &KeyEvent::plain("enter")),
            HookOutcome::Consumed
        );
        assert_eq!(ctx.effects.as_slice(), &[HookEffect::Save { path: None }]);

        // `:q` queues a quit effect.
        assert_eq!(
            vim.on_key(&mut ctx, &KeyEvent::plain(":")),
            HookOutcome::Consumed
        );
        assert_eq!(
            vim.on_key(&mut ctx, &KeyEvent::plain("q")),
            HookOutcome::Consumed
        );
        assert_eq!(
            vim.on_key(&mut ctx, &KeyEvent::plain("enter")),
            HookOutcome::Consumed
        );
        assert_eq!(
            ctx.effects.as_slice(),
            &[
                HookEffect::Save { path: None },
                HookEffect::Quit { force: false },
            ]
        );
    }
}
