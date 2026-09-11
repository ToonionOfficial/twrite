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
        "Find in page",
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
/// - `Enter` / `F3` / `Down`: next match, `Shift+F3` / `Up`: previous match.
/// - `Alt+Down` / `Alt+Up`: input history (Up/Down navigate matches here).
/// - `Alt+C` / `Alt+W` / `Alt+H`: flip Match Case / Whole Word / Highlight All
///   (also while active with the prompt closed).
/// - `Ctrl+H`: open the replace field; `Enter` on it stores the replacement
///   and returns to the search prompt (`REPLACE` mode).
/// - `Ctrl+Enter`: replace the current match and advance.
/// - `Alt+A`: replace all matches (single undo step).
/// - `Escape`: close.
///
/// `Enter` always navigates, even in `REPLACE` mode. Register this hook
/// before mode hooks (e.g. vim) so its keys win while the prompt is open;
/// unconsumed keys pass through untouched when the prompt is closed.
#[derive(Debug, Clone)]
pub struct SearchHook {
    state: SearchState,
    query_text: String,
    replacement: String,
    replace_mode: bool,
    prompt_is_replace: bool,
    active: bool,
    status_cache: String,
    case_sensitive: bool,
    whole_word: bool,
    highlight_all: bool,
}

impl Default for SearchHook {
    fn default() -> Self {
        Self {
            state: SearchState::new(),
            query_text: String::new(),
            replacement: String::new(),
            replace_mode: false,
            prompt_is_replace: false,
            active: false,
            status_cache: "SEARCH".to_string(),
            case_sensitive: true,
            whole_word: false,
            highlight_all: true,
        }
    }
}

impl SearchHook {
    /// Creates an idle search hook (case-sensitive, whole-word off,
    /// highlight-all on).
    pub fn new() -> Self {
        Self::default()
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

    /// Whether matching is case-sensitive.
    pub fn case_sensitive(&self) -> bool {
        self.case_sensitive
    }

    /// Whether matches must span whole words.
    pub fn whole_word(&self) -> bool {
        self.whole_word
    }

    /// Whether all matches wash the viewport (vs current match only).
    pub fn highlight_all(&self) -> bool {
        self.highlight_all
    }

    /// Whether replace mode is active.
    pub fn is_replace_mode(&self) -> bool {
        self.replace_mode
    }

    /// Whether the active input prompt is currently the replace field.
    pub fn is_replace_prompt(&self) -> bool {
        self.prompt_is_replace
    }

    /// Builds the active query from the text plus toggle flags.
    /// Returns `None` when the query text is empty.
    pub fn build_query(&self) -> Option<SearchQuery> {
        if self.query_text.is_empty() {
            return None;
        }
        Some(SearchQuery::new(
            &self.query_text,
            self.case_sensitive,
            self.whole_word,
            false,
        ))
    }

    /// Flips Match Case and re-scans.
    pub fn toggle_case(&mut self, ctx: &mut HookContext) {
        self.case_sensitive = !self.case_sensitive;
        self.rescan(ctx);
    }

    /// Flips Whole Word and re-scans.
    pub fn toggle_word(&mut self, ctx: &mut HookContext) {
        self.whole_word = !self.whole_word;
        self.rescan(ctx);
    }

    /// Flips the highlight-all wash (no re-scan needed).
    pub fn toggle_highlight(&mut self) {
        self.highlight_all = !self.highlight_all;
        self.update_status();
    }

    /// Opens the search prompt with `initial` as the query.
    pub fn open_search(&mut self, ctx: &mut HookContext, initial: &str) {
        ctx.prompt.open(search_spec(), initial);
        self.query_text = initial.to_string();
        self.active = true;
        self.prompt_is_replace = false;
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
        self.replace_mode = true;
        self.prompt_is_replace = true;
    }

    /// Closes a hook-owned prompt, leaving replace mode.
    pub fn close(&mut self, ctx: &mut HookContext) {
        ctx.prompt.close();
        self.active = false;
        self.replace_mode = false;
        self.prompt_is_replace = false;
        self.update_status();
    }

    /// Toggles replace mode on and off.
    pub fn toggle_replace(&mut self, ctx: &mut HookContext) {
        if self.replace_mode {
            self.replace_mode = false;
            if self.prompt_is_replace {
                self.focus_search(ctx);
            }
        } else {
            self.open_replace(ctx);
        }
    }

    /// Switches prompt focus to the search query field.
    pub fn focus_search(&mut self, ctx: &mut HookContext) {
        if self.prompt_is_replace {
            self.replacement = ctx.prompt.input().to_string();
            let query = self.query_text.clone();
            ctx.prompt.open(search_spec(), &query);
            self.prompt_is_replace = false;
            self.refresh_from_input(ctx);
        }
    }

    /// Switches prompt focus to the replacement text field.
    pub fn focus_replace(&mut self, ctx: &mut HookContext) {
        if !self.prompt_is_replace {
            self.open_replace(ctx);
        }
    }

    /// Re-scans from the prompt input with the current toggle flags.
    fn refresh_from_input(&mut self, ctx: &mut HookContext) {
        self.query_text = ctx.prompt.input().to_string();
        self.rescan(ctx);
    }

    /// Re-scans the stored query text with the current toggle flags.
    fn rescan(&mut self, ctx: &mut HookContext) {
        if self.query_text.is_empty() {
            self.state = SearchState::new();
        } else {
            self.state.set_query(SearchQuery::new(
                &self.query_text,
                self.case_sensitive,
                self.whole_word,
                false,
            ));
            // Flag combinations always compile; ignore staleness errors.
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
        let flags = format!(
            "[{}] [{}] [{}]",
            if self.case_sensitive { "Aa" } else { "aa" },
            if self.whole_word { "W" } else { "w" },
            if self.highlight_all { "H" } else { "h" },
        );
        let count = self.state.match_count();
        self.status_cache = if count == 0 {
            format!("{scope} — no matches {flags}")
        } else {
            match self.state.current_index() {
                Some(i) => format!("{scope} {}/{} {flags}", i + 1, count),
                None => format!("{scope} — {count} matches {flags}"),
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
        if ctx.prompt.spec().is_some_and(|s| s.id == REPLACE_PROMPT_ID) {
            self.replacement = ctx.prompt.input().to_string();
        }
        if self.query_text.is_empty() {
            return Ok(false);
        }
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
            })
            .or_else(|| self.state.matches().first().cloned());
        let Some(target) = target else {
            return Ok(false);
        };
        let Some(query) = self.build_query() else {
            return Ok(false);
        };
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
        if ctx.prompt.spec().is_some_and(|s| s.id == REPLACE_PROMPT_ID) {
            self.replacement = ctx.prompt.input().to_string();
        }
        let Some(query) = self.build_query() else {
            return Ok(0);
        };
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
            if key_lower == "f" && mods.ctrl && !mods.alt && !mods.meta {
                self.focus_search(ctx);
                return HookOutcome::Consumed;
            }
            if event.key == "tab" && !mods.ctrl && !mods.alt && !mods.meta && self.replace_mode {
                if self.prompt_is_replace {
                    self.focus_search(ctx);
                } else {
                    self.focus_replace(ctx);
                }
                return HookOutcome::Consumed;
            }
            if event.key == "enter" && mods.ctrl && !mods.alt && !mods.meta {
                let _ = self.replace_current(ctx);
                return HookOutcome::Consumed;
            }
            if key_lower == "a" && mods.alt && !mods.ctrl && !mods.meta {
                let _ = self.replace_all(ctx);
                return HookOutcome::Consumed;
            }
            if key_lower == "c" && mods.alt && !mods.ctrl && !mods.meta {
                self.toggle_case(ctx);
                return HookOutcome::Consumed;
            }
            if key_lower == "w" && mods.alt && !mods.ctrl && !mods.meta {
                self.toggle_word(ctx);
                return HookOutcome::Consumed;
            }
            if key_lower == "h" && mods.alt && !mods.ctrl && !mods.meta {
                self.toggle_highlight();
                return HookOutcome::Consumed;
            }
            if (event.key == "arrowup" || event.key == "up") && !mods.ctrl && !mods.meta {
                if mods.alt {
                    ctx.prompt.history_prev();
                } else {
                    self.navigate_prev(ctx, true);
                }
                return HookOutcome::Consumed;
            }
            if (event.key == "arrowdown" || event.key == "down") && !mods.ctrl && !mods.meta {
                if mods.alt {
                    ctx.prompt.history_next();
                } else {
                    self.navigate_next(ctx, true);
                }
                return HookOutcome::Consumed;
            }
            if key_lower == "h" && mods.ctrl && !mods.alt && !mods.meta {
                self.toggle_replace(ctx);
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
                    if is_replace {
                        self.replacement = ctx.prompt.input().to_string();
                    } else {
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
                        self.prompt_is_replace = false;
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
                    self.prompt_is_replace = false;
                    self.update_status();
                    HookOutcome::Consumed
                }
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
            if key_lower == "c" && mods.alt && !mods.ctrl && !mods.meta && self.active {
                self.toggle_case(ctx);
                return HookOutcome::Consumed;
            }
            if key_lower == "w" && mods.alt && !mods.ctrl && !mods.meta && self.active {
                self.toggle_word(ctx);
                return HookOutcome::Consumed;
            }
            if key_lower == "h" && mods.alt && !mods.ctrl && !mods.meta && self.active {
                self.toggle_highlight();
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

    fn search_snapshot(&self) -> Option<crate::SearchSnapshot> {
        if !self.active {
            return None;
        }
        Some(crate::SearchSnapshot {
            active: true,
            case_sensitive: self.case_sensitive,
            whole_word: self.whole_word,
            highlight_all: self.highlight_all,
            matches: self.state.matches().to_vec(),
            current: self.state.current_index(),
            replace_mode: self.replace_mode,
            is_replace_prompt: self.prompt_is_replace,
            query: self.query_text.clone(),
            replacement: self.replacement.clone(),
        })
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

        fn type_into_prompt(&mut self, text: &str) {
            for ch in text.chars() {
                self.key(&ch.to_string());
            }
        }

        fn with_ctx<R>(&mut self, f: impl FnOnce(&mut HookContext<'_>, &mut SearchHook) -> R) -> R {
            let Harness {
                buffer,
                selection,
                cursor_style,
                prompt,
                effects,
                hook,
            } = self;
            let mut ctx = HookContext::new(buffer, selection, cursor_style, prompt, effects);
            f(&mut ctx, hook)
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
        assert_eq!(h.hook.status_text(), Some("SEARCH 1/2 [Aa] [w] [H]"));

        assert_eq!(h.key("enter"), HookOutcome::Consumed);
        assert_eq!(h.selection.unwrap().byte_range(), 8..11);
        assert_eq!(h.hook.status_text(), Some("SEARCH 2/2 [Aa] [w] [H]"));
    }

    #[test]
    fn toggle_case_rescans_case_insensitively() {
        let mut h = Harness::new("Foo foo FOO");
        h.key_mod("f", true, false, false);
        for k in ["f", "o", "o"] {
            h.key(k);
        }
        assert_eq!(h.hook.match_count(), 1);
        assert!(h.hook.case_sensitive());

        h.with_ctx(|ctx, hook| hook.toggle_case(ctx));
        assert!(!h.hook.case_sensitive());
        assert_eq!(h.hook.match_count(), 3);
        assert_eq!(
            h.hook.status_text(),
            Some("SEARCH — 3 matches [aa] [w] [H]")
        );

        h.with_ctx(|ctx, hook| hook.toggle_case(ctx));
        assert_eq!(h.hook.match_count(), 1);
    }

    #[test]
    fn toggle_word_filters_substring_matches() {
        let mut h = Harness::new("foo foobar foo");
        h.key_mod("f", true, false, false);
        for k in ["f", "o", "o"] {
            h.key(k);
        }
        assert_eq!(h.hook.match_count(), 3);

        h.with_ctx(|ctx, hook| hook.toggle_word(ctx));
        assert!(h.hook.whole_word());
        assert_eq!(h.hook.match_count(), 2);
        assert_eq!(
            h.hook.status_text(),
            Some("SEARCH — 2 matches [Aa] [W] [H]")
        );
    }

    #[test]
    fn toggle_highlight_flips_without_rescanning() {
        let mut h = Harness::new("foo foo");
        h.key_mod("f", true, false, false);
        h.key("f");
        assert!(h.hook.highlight_all());

        h.with_ctx(|ctx, hook| {
            hook.toggle_highlight();
            let _ = ctx;
        });
        assert!(!h.hook.highlight_all());
        assert_eq!(h.hook.match_count(), 2);
        assert!(h.hook.status_text().unwrap().ends_with("[Aa] [w] [h]"));
    }

    #[test]
    fn replace_honors_toggle_flags() {
        let mut h = Harness::new("Foo foo");
        h.key_mod("f", true, false, false);
        for k in ["f", "o", "o"] {
            h.key(k);
        }
        h.with_ctx(|ctx, hook| hook.toggle_case(ctx));
        h.key_mod("h", true, false, false);
        for k in ["b", "a", "r"] {
            h.key(k);
        }
        h.key("enter");
        h.with_ctx(|ctx, hook| {
            assert_eq!(hook.replace_all(ctx).unwrap(), 2);
        });
        assert_eq!(h.ctx_text(), "bar bar");
    }

    #[test]
    fn build_query_is_none_when_empty() {
        let hook = SearchHook::new();
        assert!(hook.build_query().is_none());
    }

    #[test]
    fn alt_h_toggles_highlight_via_key() {
        let mut h = Harness::new("foo foo");
        h.key_mod("f", true, false, false);
        h.key("f");
        assert!(h.hook.highlight_all());
        assert_eq!(h.key_mod("h", false, true, false), HookOutcome::Consumed);
        assert!(!h.hook.highlight_all());
        // Prompt stays open; matching is untouched.
        assert!(h.prompt.is_open());
        assert_eq!(h.hook.match_count(), 2);
    }

    #[test]
    fn snapshot_reports_state_while_active() {
        let mut h = Harness::new("foo bar foo");
        assert!(h.hook.search_snapshot().is_none());
        h.key_mod("f", true, false, false);
        for k in ["f", "o", "o"] {
            h.key(k);
        }
        h.key("enter");
        let snap = h.hook.search_snapshot().expect("snapshot while active");
        assert!(snap.active);
        assert!(snap.case_sensitive);
        assert!(!snap.whole_word);
        assert!(snap.highlight_all);
        assert_eq!(snap.matches, vec![0..3, 8..11]);
        assert_eq!(snap.current, Some(0));
        h.key("escape");
        assert!(h.hook.search_snapshot().is_none());
    }

    #[test]
    fn alt_c_and_alt_w_toggle_via_keys() {
        let mut h = Harness::new("Foo foo foobar");
        h.key_mod("f", true, false, false);
        for k in ["f", "o", "o"] {
            h.key(k);
        }
        assert_eq!(h.hook.match_count(), 2);

        assert_eq!(h.key_mod("c", false, true, false), HookOutcome::Consumed);
        assert_eq!(h.hook.match_count(), 3);

        assert_eq!(h.key_mod("w", false, true, false), HookOutcome::Consumed);
        assert_eq!(h.hook.match_count(), 2);
        assert_eq!(
            h.hook.status_text(),
            Some("SEARCH — 2 matches [aa] [W] [H]")
        );
    }

    #[test]
    fn up_down_navigate_matches_and_alt_reaches_history() {
        let mut h = Harness::new("foo bar foo");
        h.key_mod("f", true, false, false);
        for k in ["f", "o", "o"] {
            h.key(k);
        }
        h.key("enter");
        assert_eq!(h.selection.unwrap().byte_range(), 0..3);

        // Down advances instead of walking history.
        assert_eq!(h.key("down"), HookOutcome::Consumed);
        assert_eq!(h.selection.unwrap().byte_range(), 8..11);
        assert_eq!(h.key("up"), HookOutcome::Consumed);
        assert_eq!(h.selection.unwrap().byte_range(), 0..3);

        // History moved to Alt+Up.
        h.key("enter");
        h.type_into_prompt("zzz");
        assert_eq!(
            h.key_mod("arrowup", false, true, false),
            HookOutcome::Consumed
        );
        assert_eq!(h.prompt.input(), "foo");
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

    #[test]
    fn replace_while_typing_in_replace_prompt_with_ctrl_enter_and_alt_a() {
        let mut h = Harness::new("alpha beta alpha");
        h.key_mod("f", true, false, false);
        for k in ["a", "l", "p", "h", "a"] {
            h.key(k);
        }
        assert_eq!(h.hook.match_count(), 2);

        assert_eq!(h.key_mod("h", true, false, false), HookOutcome::Consumed);
        assert_eq!(h.prompt.spec().unwrap().id, REPLACE_PROMPT_ID);

        for k in ["o", "m", "e", "g", "a"] {
            h.key(k);
        }
        assert_eq!(h.hook.replacement(), "omega");

        assert_eq!(
            h.key_mod("enter", true, false, false),
            HookOutcome::Consumed
        );
        assert_eq!(h.ctx_text(), "omega beta alpha");

        assert_eq!(h.key_mod("a", false, true, false), HookOutcome::Consumed);
        assert_eq!(h.ctx_text(), "omega beta omega");
    }

    #[test]
    fn tab_toggles_between_search_and_replace_in_replace_mode() {
        let mut h = Harness::new("one two one");
        h.key_mod("f", true, false, false);
        for k in ["o", "n", "e"] {
            h.key(k);
        }
        h.key_mod("h", true, false, false);
        assert!(h.hook.is_replace_mode());
        assert!(h.hook.is_replace_prompt());

        h.key("tab");
        assert!(!h.hook.is_replace_prompt());
        assert_eq!(h.prompt.spec().unwrap().id, SEARCH_PROMPT_ID);
        assert_eq!(h.prompt.input(), "one");

        h.key("tab");
        assert!(h.hook.is_replace_prompt());
        assert_eq!(h.prompt.spec().unwrap().id, REPLACE_PROMPT_ID);
    }

    #[test]
    fn search_snapshot_reflects_replace_state() {
        let mut h = Harness::new("test test");
        h.key_mod("f", true, false, false);
        h.type_into_prompt("test");
        let snap = h.hook.search_snapshot().unwrap();
        assert_eq!(snap.query, "test");
        assert!(!snap.replace_mode);
        assert!(!snap.is_replace_prompt);

        h.key_mod("h", true, false, false);
        h.type_into_prompt("passed");
        let snap = h.hook.search_snapshot().unwrap();
        assert_eq!(snap.query, "test");
        assert_eq!(snap.replacement, "passed");
        assert!(snap.replace_mode);
        assert!(snap.is_replace_prompt);
    }
}
