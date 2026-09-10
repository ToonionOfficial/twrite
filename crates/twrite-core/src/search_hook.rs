use std::ops::Range;

use crate::{
    EditorError, EditorHook, HookContext, PromptAction, PromptPlacement, PromptSpec, SearchQuery,
    SearchState, Selection, replace_all_query, replace_one_query,
};

/// Prompt ids owned by [`SearchHook`].
pub const SEARCH_PROMPT_ID: &str = "search";
/// Prompt ids owned by [`SearchHook`].
pub const REPLACE_PROMPT_ID: &str = "replace";

fn search_spec() -> PromptSpec {
    PromptSpec::new(
        SEARCH_PROMPT_ID,
        "/",
        "Search",
        PromptPlacement::BottomBar,
        true,
    )
}

fn replace_spec() -> PromptSpec {
    PromptSpec::new(
        REPLACE_PROMPT_ID,
        "Replace",
        "Replace with",
        PromptPlacement::BottomBar,
        false,
    )
}

/// A stock hook connecting the headless search engine to the shared prompt.
///
/// Keymap (all headless, no frontend code):
/// - `Ctrl+F`: open search (selected text becomes the initial query).
/// - Typing: live match refresh.
/// - `Enter` / `F3`: next match, `Shift+F3`: previous match.
/// - `Ctrl+H`: open the replace field; `Enter` on it stores the replacement
///   and returns to the search prompt (`REPLACE` mode).
/// - `Ctrl+Enter`: replace the current match and advance.
/// - `Alt+A`: replace all matches (single undo step).
/// - `Escape`: close.
///
/// `Enter` always navigates, even in `REPLACE` mode. Register this hook
/// before mode hooks (e.g. vim) so its keys win while the prompt is open;
/// unconsumed keys pass through untouched when the prompt is closed.
#[derive(Debug, Clone, Default)]
pub struct SearchHook {
    state: SearchState,
    query_text: String,
    replacement: String,
    replace_mode: bool,
    active: bool,
    status_cache: String,
}

impl SearchHook {
    /// Creates an idle search hook.
    pub fn new() -> Self {
        Self {
            status_cache: "SEARCH".to_string(),
            ..Self::default()
        }
    }

    /// Whether the hook currently owns the open prompt.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Number of matches for the current query.
    pub fn match_count(&self) -> usize {
        self.state.match_count()
    }

    /// The current match range, if navigation has occurred.
    pub fn current_match(&self) -> Option<Range<usize>> {
        self.state.current_match()
    }

    /// The last submitted query text.
    pub fn query_text(&self) -> &str {
        &self.query_text
    }

    /// The stored replacement text.
    pub fn replacement(&self) -> &str {
        &self.replacement
    }

    /// Opens the search prompt with `initial` as the query.
    pub fn open_search(&mut self, ctx: &mut HookContext, initial: &str) {
        ctx.prompt.open(search_spec(), initial);
        self.query_text = initial.to_string();
        self.active = true;
        self.refresh_from_input(ctx);
    }

    /// Opens the replace prompt, stashing the current query.
    pub fn open_replace(&mut self, ctx: &mut HookContext) {
        if ctx.prompt.spec().is_some_and(|s| s.id == SEARCH_PROMPT_ID) {
            self.query_text = ctx.prompt.input().to_string();
        }
        let replacement = self.replacement.clone();
        ctx.prompt.open(replace_spec(), &replacement);
        self.active = true;
    }

    /// Closes a hook-owned prompt, leaving replace mode.
    pub fn close(&mut self, ctx: &mut HookContext) {
        ctx.prompt.close();
        self.active = false;
        self.replace_mode = false;
        self.update_status();
    }

    /// Re-scans from the prompt input (literal query).
    fn refresh_from_input(&mut self, ctx: &mut HookContext) {
        let input = ctx.prompt.input().to_string();
        self.query_text = input.clone();
        if input.is_empty() {
            self.state = SearchState::new();
        } else {
            self.state.set_query(SearchQuery::literal(&input));
            // Literal queries cannot fail to compile; ignore staleness errors.
            let _ = self.state.refresh(ctx.buffer);
        }
        self.update_status();
    }

    fn update_status(&mut self) {
        let scope = if self.replace_mode {
            "REPLACE"
        } else {
            "SEARCH"
        };
        let count = self.state.match_count();
        self.status_cache = if count == 0 {
            format!("{scope} — no matches")
        } else {
            match self.state.current_index() {
                Some(i) => format!("{scope} {}/{}", i + 1, count),
                None => format!("{scope} — {count} matches"),
            }
        };
    }

    /// Selects the next match at or after the cursor.
    ///
    /// Sitting exactly on the current match start steps past it, so repeated
    /// `Enter` / `F3` walks forward instead of re-selecting.
    pub fn navigate_next(&mut self, ctx: &mut HookContext, wrap: bool) -> bool {
        self.navigate_next_from(ctx, ctx.buffer.cursor_offset(), wrap)
    }

    /// Selects the next match at or after `from` (see [`Self::navigate_next`]).
    pub fn navigate_next_from(&mut self, ctx: &mut HookContext, from: usize, wrap: bool) -> bool {
        let from = match self.state.current_match() {
            Some(m) if m.start == from => from + 1,
            _ => from,
        };
        let found = self
            .state
            .next(ctx.buffer, from, wrap)
            .ok()
            .flatten()
            .map(|m| {
                ctx.buffer.set_cursor_offset(m.start);
                *ctx.selection = Some(Selection::range(m.start, m.end));
            })
            .is_some();
        self.update_status();
        found
    }

    /// Selects the previous match at or before the cursor.
    pub fn navigate_prev(&mut self, ctx: &mut HookContext, wrap: bool) -> bool {
        self.navigate_prev_from(ctx, ctx.buffer.cursor_offset(), wrap)
    }

    /// Selects the previous match at or before `from`.
    pub fn navigate_prev_from(&mut self, ctx: &mut HookContext, from: usize, wrap: bool) -> bool {
        let found = self
            .state
            .prev(ctx.buffer, from, wrap)
            .ok()
            .flatten()
            .map(|m| {
                ctx.buffer.set_cursor_offset(m.start);
                *ctx.selection = Some(Selection::range(m.start, m.end));
            })
            .is_some();
        self.update_status();
        found
    }

    /// Replaces the match containing the cursor (else the next match) and
    /// advances to the following match. Returns `Ok(false)` when there is
    /// nothing to replace.
    pub fn replace_current(&mut self, ctx: &mut HookContext) -> Result<bool, EditorError> {
        if self.query_text.is_empty() {
            return Ok(false);
        }
        // Refresh against external buffer edits first.
        let _ = self.state.refresh(ctx.buffer);
        let from = ctx.buffer.cursor_offset();
        let target = self
            .state
            .matches()
            .iter()
            .find(|m| m.start <= from && from <= m.end)
            .cloned()
            .or_else(|| {
                self.state
                    .matches()
                    .iter()
                    .find(|m| m.start >= from)
                    .cloned()
            });
        let Some(target) = target else {
            return Ok(false);
        };
        let query = SearchQuery::literal(&self.query_text);
        let replacement = self.replacement.clone();
        if !replace_one_query(ctx.buffer, &query, target, &replacement)? {
            return Ok(false);
        }
        let _ = self.state.refresh(ctx.buffer);
        self.navigate_next(ctx, true);
        Ok(true)
    }

    /// Replaces all matches as a single undoable transaction, reporting the
    /// count in the prompt message. Returns the number of replacements.
    pub fn replace_all(&mut self, ctx: &mut HookContext) -> Result<usize, EditorError> {
        if self.query_text.is_empty() {
            return Ok(0);
        }
        let query = SearchQuery::literal(&self.query_text);
        let replacement = self.replacement.clone();
        let n = replace_all_query(ctx.buffer, &query, &replacement)?;
        let _ = self.state.refresh(ctx.buffer);
        *ctx.selection = None;
        ctx.prompt.set_message(&format!(
            "Replaced {n} match{}",
            if n == 1 { "" } else { "es" }
        ));
        self.update_status();
        Ok(n)
    }

    /// Initial query for `Ctrl+F`: selected text, else the last query.
    fn ctrl_f_initial(&self, ctx: &HookContext) -> String {
        match ctx.selection.map(|s| s.byte_range()) {
            Some(range) if !range.is_empty() => {
                let end = range.end.min(ctx.buffer.len_bytes());
                let start = range.start.min(end);
                ctx.buffer.text().byte_slice(start..end).to_string()
            }
            _ => self.query_text.clone(),
        }
    }
}

impl EditorHook for SearchHook {
    fn on_key(&mut self, ctx: &mut HookContext, event: &crate::KeyEvent) -> crate::HookOutcome {
        use crate::HookOutcome;

        let key_lower = event.key.to_lowercase();
        let mods = &event.modifiers;
        let owns_prompt = ctx.prompt.is_open()
            && ctx
                .prompt
                .spec()
                .is_some_and(|s| s.id == SEARCH_PROMPT_ID || s.id == REPLACE_PROMPT_ID);

        if owns_prompt {
            // Handled before the prompt sees them (it would ignore them).
            if event.key == "enter" && mods.ctrl && !mods.alt && !mods.meta {
                let _ = self.replace_current(ctx);
                return HookOutcome::Consumed;
            }
            if key_lower == "a" && mods.alt && !mods.ctrl && !mods.meta {
                let _ = self.replace_all(ctx);
                return HookOutcome::Consumed;
            }
            if key_lower == "h" && mods.ctrl && !mods.alt && !mods.meta {
                self.open_replace(ctx);
                return HookOutcome::Consumed;
            }
            if event.key == "f3" && !mods.ctrl && !mods.alt && !mods.meta {
                if mods.shift {
                    self.navigate_prev(ctx, true);
                } else {
                    self.navigate_next(ctx, true);
                }
                return HookOutcome::Consumed;
            }
            let is_replace = ctx.prompt.spec().is_some_and(|s| s.id == REPLACE_PROMPT_ID);
            match ctx.prompt.handle_key(event) {
                PromptAction::Editing => {
                    if !is_replace {
                        self.refresh_from_input(ctx);
                    }
                    HookOutcome::Consumed
                }
                PromptAction::Submitted(input) => {
                    if is_replace {
                        self.replacement = input;
                        let query = self.query_text.clone();
                        ctx.prompt.open(search_spec(), &query);
                        self.replace_mode = true;
                        self.refresh_from_input(ctx);
                    } else {
                        self.query_text = input;
                        self.navigate_next(ctx, true);
                    }
                    HookOutcome::Consumed
                }
                PromptAction::Cancelled => {
                    self.active = false;
                    self.replace_mode = false;
                    self.update_status();
                    HookOutcome::Consumed
                }
                // Swallow anything else while our prompt is open so no
                // keystroke leaks to hooks behind us or the buffer.
                PromptAction::Ignored => HookOutcome::Consumed,
            }
        } else if !ctx.prompt.is_open() {
            if key_lower == "f" && mods.ctrl && !mods.alt && !mods.meta {
                let initial = self.ctrl_f_initial(ctx);
                self.replace_mode = false;
                self.open_search(ctx, &initial);
                return HookOutcome::Consumed;
            }
            if key_lower == "h" && mods.ctrl && !mods.alt && !mods.meta {
                if !self.active {
                    let initial = self.ctrl_f_initial(ctx);
                    self.open_search(ctx, &initial);
                }
                self.open_replace(ctx);
                return HookOutcome::Consumed;
            }
            if event.key == "f3"
                && !mods.ctrl
                && !mods.alt
                && !mods.meta
                && self.active
                && !self.query_text.is_empty()
            {
                if mods.shift {
                    self.navigate_prev(ctx, true);
                } else {
                    self.navigate_next(ctx, true);
                }
                return HookOutcome::Consumed;
            }
            HookOutcome::PassThrough
        } else {
            // A foreign prompt is open: stay out of the way.
            HookOutcome::PassThrough
        }
    }

    fn status_text(&self) -> Option<&str> {
        if self.active {
            Some(&self.status_cache)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CursorStyle, EditorBuffer, HookContext, HookOutcome, KeyEvent, PromptState};

    struct Harness {
        buffer: EditorBuffer,
        selection: Option<Selection>,
        cursor_style: CursorStyle,
        prompt: PromptState,
        effects: Vec<crate::HookEffect>,
        hook: SearchHook,
    }

    impl Harness {
        fn new(text: &str) -> Self {
            Self {
                buffer: EditorBuffer::new(text),
                selection: None,
                cursor_style: CursorStyle::Bar,
                prompt: PromptState::new(),
                effects: Vec::new(),
                hook: SearchHook::new(),
            }
        }

        fn key(&mut self, key: &str) -> HookOutcome {
            let event = KeyEvent::plain(key);
            let Harness {
                buffer,
                selection,
                cursor_style,
                prompt,
                effects,
                hook,
            } = self;
            let mut ctx = HookContext::new(buffer, selection, cursor_style, prompt, effects);
            hook.on_key(&mut ctx, &event)
        }

        fn key_mod(&mut self, key: &str, ctrl: bool, alt: bool, shift: bool) -> HookOutcome {
            let event = KeyEvent {
                key: key.to_string(),
                modifiers: crate::Modifiers {
                    ctrl,
                    alt,
                    shift,
                    meta: false,
                },
            };
            let Harness {
                buffer,
                selection,
                cursor_style,
                prompt,
                effects,
                hook,
            } = self;
            let mut ctx = HookContext::new(buffer, selection, cursor_style, prompt, effects);
            hook.on_key(&mut ctx, &event)
        }

        fn ctx_text(&self) -> String {
            self.buffer.text().to_string()
        }
    }

    #[test]
    fn idle_hook_passes_everything_through() {
        let mut h = Harness::new("hello");
        assert_eq!(h.key("a"), HookOutcome::PassThrough);
        assert_eq!(h.key("enter"), HookOutcome::PassThrough);
        assert!(h.hook.status_text().is_none());
    }

    #[test]
    fn ctrl_f_opens_search_with_selection_as_initial() {
        let mut h = Harness::new("hello world");
        h.selection = Some(Selection::range(0, 5));
        assert_eq!(h.key_mod("f", true, false, false), HookOutcome::Consumed);
        assert!(h.prompt.is_open());
        assert_eq!(h.prompt.input(), "hello");
        assert_eq!(h.hook.match_count(), 1);
        assert!(h.hook.status_text().is_some());
    }

    #[test]
    fn typing_live_refreshes_and_enter_navigates() {
        let mut h = Harness::new("foo bar foo");
        assert_eq!(h.key_mod("f", true, false, false), HookOutcome::Consumed);

        for k in ["f", "o", "o"] {
            assert_eq!(h.key(k), HookOutcome::Consumed);
        }
        assert_eq!(h.hook.match_count(), 2);

        assert_eq!(h.key("enter"), HookOutcome::Consumed);
        assert_eq!(h.selection.unwrap().byte_range(), 0..3);
        assert_eq!(h.hook.status_text(), Some("SEARCH 1/2"));

        assert_eq!(h.key("enter"), HookOutcome::Consumed);
        assert_eq!(h.selection.unwrap().byte_range(), 8..11);
        assert_eq!(h.hook.status_text(), Some("SEARCH 2/2"));
    }

    #[test]
    fn f3_keys_navigate_without_typing() {
        let mut h = Harness::new("aa aa");
        h.key_mod("f", true, false, false);
        h.key("a");
        h.key("a");
        h.key("enter");
        assert_eq!(h.selection.unwrap().byte_range(), 0..2);

        assert_eq!(h.key("f3"), HookOutcome::Consumed);
        assert_eq!(h.selection.unwrap().byte_range(), 3..5);

        assert_eq!(h.key_mod("f3", false, false, true), HookOutcome::Consumed);
        assert_eq!(h.selection.unwrap().byte_range(), 0..2);
    }

    #[test]
    fn escape_closes_and_resets_mode() {
        let mut h = Harness::new("foo foo");
        h.key_mod("f", true, false, false);
        assert!(h.prompt.is_open());
        assert_eq!(h.key("escape"), HookOutcome::Consumed);
        assert!(!h.prompt.is_open());
        assert!(h.hook.status_text().is_none());
        assert_eq!(h.key("a"), HookOutcome::PassThrough);
    }

    #[test]
    fn ctrl_h_replace_flow_replaces_current_and_all() {
        let mut h = Harness::new("foo bar foo");
        h.key_mod("f", true, false, false);
        for k in ["f", "o", "o"] {
            h.key(k);
        }

        // Ctrl+H stashes the query and asks for replacement text.
        assert_eq!(h.key_mod("h", true, false, false), HookOutcome::Consumed);
        assert_eq!(h.prompt.spec().unwrap().id, REPLACE_PROMPT_ID);
        for k in ["b", "a", "z"] {
            h.key(k);
        }
        assert_eq!(h.key("enter"), HookOutcome::Consumed);
        // Back on the search prompt with the query restored.
        assert_eq!(h.prompt.spec().unwrap().id, SEARCH_PROMPT_ID);
        assert_eq!(h.prompt.input(), "foo");
        assert_eq!(h.hook.replacement(), "baz");

        // Ctrl+Enter replaces the match at the cursor and advances.
        assert_eq!(
            h.key_mod("enter", true, false, false),
            HookOutcome::Consumed
        );
        assert_eq!(h.ctx_text(), "baz bar foo");
        assert_eq!(h.selection.unwrap().byte_range(), 8..11);

        // Alt+A replaces the rest in one undo step.
        assert_eq!(h.key_mod("a", false, true, false), HookOutcome::Consumed);
        assert_eq!(h.ctx_text(), "baz bar baz");
        assert_eq!(h.prompt.message(), Some("Replaced 1 match"));
        h.buffer.undo();
        assert_eq!(h.ctx_text(), "baz bar foo");
    }

    #[test]
    fn public_api_drives_vim_style_flows() {
        let mut h = Harness::new("foo bar foo");
        let Harness {
            buffer,
            selection,
            cursor_style,
            prompt,
            effects,
            hook,
        } = &mut h;
        let mut ctx = HookContext::new(buffer, selection, cursor_style, prompt, effects);

        hook.open_search(&mut ctx, "foo");
        assert_eq!(hook.match_count(), 2);
        assert!(hook.navigate_next(&mut ctx, true));
        assert_eq!(ctx.selection.unwrap().byte_range(), 0..3);
        assert!(hook.navigate_prev(&mut ctx, true));
        assert_eq!(ctx.selection.unwrap().byte_range(), 8..11);

        hook.close(&mut ctx);
        assert!(!ctx.prompt.is_open());
        assert!(!hook.is_active());
    }
}
