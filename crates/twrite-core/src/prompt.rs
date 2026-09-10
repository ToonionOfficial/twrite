use crate::hook::KeyEvent;

/// Where a frontend should render an open prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PromptPlacement {
    /// Vim-style single line anchored to the bottom (`:`, `/`, `Ctrl+F`).
    #[default]
    BottomBar,
    /// Browser-`F1` style floating palette centered near the top.
    TopPalette,
}

/// One selectable row in a palette-style prompt (commands, files, matches).
///
/// Hooks own the meaning: they fill these, the frontend only draws them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptItem {
    /// The primary row text (inserted on Tab-completion).
    pub label: String,
    /// Secondary hint text (keybinding, path, match count).
    pub hint: Option<String>,
}

impl PromptItem {
    /// Creates an item with no hint.
    pub fn new(label: &str) -> Self {
        Self {
            label: label.to_string(),
            hint: None,
        }
    }

    /// Creates an item with hint text.
    pub fn with_hint(label: &str, hint: &str) -> Self {
        Self {
            label: label.to_string(),
            hint: Some(hint.to_string()),
        }
    }
}

/// How a prompt was opened. Semantics belong to hooks; this is only the
/// view-model a frontend renders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptSpec {
    /// Stable id so hooks recognize their own prompt (`"search"`, `"vim-ex"`).
    pub id: &'static str,
    /// Leading adornment (`"/"`, `":"`, `""`).
    pub prefix: String,
    /// Ghost text shown while the input is empty.
    pub placeholder: String,
    /// Where the frontend should anchor the box.
    pub placement: PromptPlacement,
    /// Hint that the hook refreshes live on every keystroke (search) rather
    /// than only on submit (ex-commands).
    pub live_update: bool,
}

impl PromptSpec {
    /// Creates a prompt spec.
    pub fn new(
        id: &'static str,
        prefix: &str,
        placeholder: &str,
        placement: PromptPlacement,
        live_update: bool,
    ) -> Self {
        Self {
            id,
            prefix: prefix.to_string(),
            placeholder: placeholder.to_string(),
            placement,
            live_update,
        }
    }
}

/// What [`PromptState::handle_key`] decided for a keystroke.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptAction {
    /// Consumed as prompt editing; the prompt stays open.
    Editing,
    /// `Enter`: the input was submitted (history recorded, prompt left open
    /// for the caller to inspect items/message and close explicitly).
    Submitted(String),
    /// `Escape`: the prompt was closed.
    Cancelled,
    /// Not a prompt key (closed prompt, or e.g. `Tab` with no items):
    /// the caller may pass it through to buffer handling.
    Ignored,
}

/// Maximum retained prompt history entries.
const HISTORY_LIMIT: usize = 100;

/// Headless single-line input + item list + history.
///
/// Owned by the host (the GPUI `Editor`, or a test harness) and shared with
/// hooks via `HookContext::prompt`, so any hook can open a bottom bar or
/// command palette with zero frontend code: `ctx.prompt.open(...)`, read
/// `ctx.prompt.input()` on submit, push a [`crate::HookEffect`] for
/// app-level work (save/quit). Frontends only render + forward keys.
#[derive(Debug, Clone, Default)]
pub struct PromptState {
    spec: Option<PromptSpec>,
    input: String,
    cursor: usize,
    items: Vec<PromptItem>,
    selected: usize,
    message: Option<String>,
    history: Vec<String>,
    history_idx: Option<usize>,
    draft: String,
}

impl PromptState {
    /// Creates a closed prompt.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether a prompt is currently open.
    pub fn is_open(&self) -> bool {
        self.spec.is_some()
    }

    /// Returns the active spec, if open.
    pub fn spec(&self) -> Option<&PromptSpec> {
        self.spec.as_ref()
    }

    /// Current input text.
    pub fn input(&self) -> &str {
        &self.input
    }

    /// Byte index of the input cursor (always a char boundary).
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Current item rows.
    pub fn items(&self) -> &[PromptItem] {
        &self.items
    }

    /// Selected item index.
    pub fn selected_index(&self) -> usize {
        self.selected
    }

    /// Selected item, if the list is non-empty.
    pub fn selected_item(&self) -> Option<&PromptItem> {
        self.items.get(self.selected)
    }

    /// Validation / error message to show under the box.
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    /// Submitted-input history, oldest first.
    pub fn history(&self) -> &[String] {
        &self.history
    }

    /// Opens a prompt, replacing any active one.
    pub fn open(&mut self, spec: PromptSpec, initial: &str) {
        self.spec = Some(spec);
        self.input = initial.to_string();
        self.cursor = self.input.len();
        self.items.clear();
        self.selected = 0;
        self.message = None;
        self.history_idx = None;
        self.draft.clear();
    }

    /// Closes the prompt, clearing input, items, and message (history kept).
    pub fn close(&mut self) {
        self.spec = None;
        self.input.clear();
        self.cursor = 0;
        self.items.clear();
        self.selected = 0;
        self.message = None;
        self.history_idx = None;
        self.draft.clear();
    }

    /// Sets the validation / error message.
    pub fn set_message(&mut self, message: &str) {
        self.message = Some(message.to_string());
    }

    /// Clears the validation / error message.
    pub fn clear_message(&mut self) {
        self.message = None;
    }

    /// Replaces the item list, clamping the selection into range.
    pub fn set_items(&mut self, items: Vec<PromptItem>) {
        self.items = items;
        self.selected = self.selected.min(self.items.len().saturating_sub(1));
    }

    /// Previous char boundary at or before `idx`.
    fn prev_boundary(&self, idx: usize) -> usize {
        let mut pos = idx.min(self.input.len());
        if pos == 0 {
            return 0;
        }
        pos -= 1;
        while pos > 0 && !self.input.is_char_boundary(pos) {
            pos -= 1;
        }
        pos
    }

    /// Next char boundary at or after `idx`.
    fn next_boundary(&self, idx: usize) -> usize {
        let mut pos = idx.min(self.input.len());
        if pos >= self.input.len() {
            return self.input.len();
        }
        pos += 1;
        while pos < self.input.len() && !self.input.is_char_boundary(pos) {
            pos += 1;
        }
        pos
    }

    /// Editing abandons history browsing.
    fn abandon_history(&mut self) {
        self.history_idx = None;
        self.draft.clear();
    }

    /// Removes the char ending at the cursor (cursor must be a boundary).
    fn backspace_one(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let prev = self.prev_boundary(self.cursor);
        self.input.drain(prev..self.cursor);
        self.cursor = prev;
    }

    /// Inserts text at the input cursor.
    pub fn insert(&mut self, text: &str) {
        self.abandon_history();
        self.input.insert_str(self.cursor, text);
        self.cursor += text.len();
    }

    /// Deletes the char before the input cursor (UTF-8 safe).
    pub fn backspace(&mut self) {
        self.abandon_history();
        self.backspace_one();
    }

    /// Deletes the char after the input cursor (UTF-8 safe).
    pub fn delete_after_cursor(&mut self) {
        self.abandon_history();
        if self.cursor < self.input.len() {
            let next = self.next_boundary(self.cursor);
            self.input.drain(self.cursor..next);
        }
    }

    /// Deletes back to the previous blank-separated word start (`Ctrl+W`).
    pub fn delete_word_before(&mut self) {
        self.abandon_history();
        // Trailing whitespace first, then the word itself.
        while self.cursor > 0
            && self.input[..self.cursor]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_whitespace())
        {
            self.backspace_one();
        }
        while self.cursor > 0
            && self.input[..self.cursor]
                .chars()
                .next_back()
                .is_some_and(|c| !c.is_whitespace())
        {
            self.backspace_one();
        }
    }

    /// Clears everything before the cursor (`Ctrl+U`).
    pub fn clear_to_start(&mut self) {
        self.abandon_history();
        self.input.drain(..self.cursor);
        self.cursor = 0;
    }

    /// Moves the input cursor one char left.
    pub fn move_left(&mut self) {
        self.cursor = self.prev_boundary(self.cursor);
    }

    /// Moves the input cursor one char right.
    pub fn move_right(&mut self) {
        self.cursor = self.next_boundary(self.cursor);
    }

    /// Moves the input cursor to the start.
    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    /// Moves the input cursor to the end.
    pub fn move_end(&mut self) {
        self.cursor = self.input.len();
    }

    /// Steps back through submitted-input history.
    pub fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let idx = match self.history_idx {
            None => {
                self.draft = self.input.clone();
                self.history.len() - 1
            }
            Some(0) => return,
            Some(i) => i - 1,
        };
        self.history_idx = Some(idx);
        self.input = self.history[idx].clone();
        self.cursor = self.input.len();
    }

    /// Steps forward through history, restoring the draft at the end.
    pub fn history_next(&mut self) {
        let idx = match self.history_idx {
            None => return,
            Some(i) => i,
        };
        if idx + 1 < self.history.len() {
            self.history_idx = Some(idx + 1);
            self.input = self.history[idx + 1].clone();
        } else {
            self.history_idx = None;
            self.input = std::mem::take(&mut self.draft);
        }
        self.cursor = self.input.len();
    }

    /// Moves the item selection forward, wrapping.
    pub fn select_next(&mut self) {
        if self.items.is_empty() {
            return;
        }
        self.selected = (self.selected + 1) % self.items.len();
    }

    /// Moves the item selection backward, wrapping.
    pub fn select_prev(&mut self) {
        if self.items.is_empty() {
            return;
        }
        self.selected = self.selected.checked_sub(1).unwrap_or(self.items.len() - 1);
    }

    /// Copies the selected item's label into the input (`Tab`).
    pub fn complete_selected(&mut self) {
        if let Some(item) = self.selected_item() {
            let label = item.label.clone();
            self.abandon_history();
            self.input = label;
            self.cursor = self.input.len();
        }
    }

    /// Records the input in history and returns it. Leaves the prompt open
    /// so the caller can inspect items/message before closing.
    pub fn submit(&mut self) -> String {
        let submitted = self.input.clone();
        if !submitted.is_empty() && self.history.last() != Some(&submitted) {
            self.history.push(submitted.clone());
            if self.history.len() > HISTORY_LIMIT {
                let excess = self.history.len() - HISTORY_LIMIT;
                self.history.drain(..excess);
            }
        }
        self.history_idx = None;
        self.draft.clear();
        submitted
    }

    /// Routes one keystroke when the prompt is open. Returns [`PromptAction::Ignored`]
    /// when closed (or for keys the prompt does not own, e.g. `Tab` with an
    /// empty item list) so callers can fall through to buffer handling.
    pub fn handle_key(&mut self, event: &KeyEvent) -> PromptAction {
        if self.spec.is_none() {
            return PromptAction::Ignored;
        }
        let mods = &event.modifiers;
        match event.key.as_str() {
            "escape" => {
                self.close();
                PromptAction::Cancelled
            }
            "enter" if !mods.ctrl && !mods.alt && !mods.meta => {
                PromptAction::Submitted(self.submit())
            }
            "tab" => {
                if self.selected_item().is_some() {
                    self.complete_selected();
                    PromptAction::Editing
                } else {
                    PromptAction::Ignored
                }
            }
            "arrowup" if !mods.ctrl && !mods.alt && !mods.meta => {
                if self.items.is_empty() {
                    self.history_prev();
                } else {
                    self.select_prev();
                }
                PromptAction::Editing
            }
            "arrowdown" if !mods.ctrl && !mods.alt && !mods.meta => {
                if self.items.is_empty() {
                    self.history_next();
                } else {
                    self.select_next();
                }
                PromptAction::Editing
            }
            "backspace" if !mods.ctrl && !mods.alt && !mods.meta => {
                self.backspace();
                PromptAction::Editing
            }
            "delete" if !mods.ctrl && !mods.alt && !mods.meta => {
                self.delete_after_cursor();
                PromptAction::Editing
            }
            "arrowleft" if !mods.ctrl && !mods.alt && !mods.meta => {
                self.move_left();
                PromptAction::Editing
            }
            "arrowright" if !mods.ctrl && !mods.alt && !mods.meta => {
                self.move_right();
                PromptAction::Editing
            }
            "home" => {
                self.move_home();
                PromptAction::Editing
            }
            "end" => {
                self.move_end();
                PromptAction::Editing
            }
            key if mods.ctrl && !mods.alt && !mods.meta => match key.to_lowercase().as_str() {
                "u" => {
                    self.clear_to_start();
                    PromptAction::Editing
                }
                "w" => {
                    self.delete_word_before();
                    PromptAction::Editing
                }
                "a" => {
                    self.move_home();
                    PromptAction::Editing
                }
                "e" => {
                    self.move_end();
                    PromptAction::Editing
                }
                _ => PromptAction::Ignored,
            },
            // Some frontends send the word "space"; normalize to " ".
            key if !mods.ctrl && !mods.alt && !mods.meta => {
                if key == "space" {
                    self.insert(" ");
                    PromptAction::Editing
                } else if key.chars().count() == 1 {
                    self.insert(key);
                    PromptAction::Editing
                } else {
                    PromptAction::Ignored
                }
            }
            _ => PromptAction::Ignored,
        }
    }
}

/// Scores `candidate` against `query` as a case-insensitive subsequence.
///
/// Higher is better; `None` when the query is not a subsequence. Empty
/// queries match everything with score `0` (palette shows all rows).
pub fn fuzzy_score(candidate: &str, query: &str) -> Option<i64> {
    if query.is_empty() {
        return Some(0);
    }
    let lowered: Vec<char> = candidate.to_lowercase().chars().collect();
    let wanted: Vec<char> = query.to_lowercase().chars().collect();
    let mut score: i64 = 0;
    let mut pos = 0;
    let mut prev: Option<usize> = None;
    for (qi, qc) in wanted.iter().enumerate() {
        let mut found = None;
        for (ci, cc) in lowered.iter().enumerate().skip(pos) {
            if cc == qc {
                found = Some(ci);
                break;
            }
        }
        let ci = found?;
        score += 10;
        if qi == 0 && ci == 0 {
            score += 20;
        }
        if prev.is_some_and(|p| p + 1 == ci) {
            score += 15;
        }
        prev = Some(ci);
        pos = ci + 1;
    }
    Some(score - lowered.len() as i64)
}

/// Filters item labels against `query`, returning `(index, score)` pairs
/// sorted by score descending, index ascending.
pub fn fuzzy_filter(candidates: &[PromptItem], query: &str) -> Vec<(usize, i64)> {
    let mut ranked: Vec<(usize, i64)> = candidates
        .iter()
        .enumerate()
        .filter_map(|(i, item)| fuzzy_score(&item.label, query).map(|s| (i, s)))
        .collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    ranked
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hook::Modifiers;

    fn search_spec() -> PromptSpec {
        PromptSpec::new("search", "/", "Search", PromptPlacement::BottomBar, true)
    }

    fn palette_spec() -> PromptSpec {
        PromptSpec::new(
            "commands",
            "",
            "Type a command",
            PromptPlacement::TopPalette,
            false,
        )
    }

    fn key(k: &str) -> KeyEvent {
        KeyEvent::plain(k)
    }

    fn ctrl(k: &str) -> KeyEvent {
        KeyEvent {
            key: k.to_string(),
            modifiers: Modifiers {
                ctrl: true,
                ..Default::default()
            },
        }
    }

    #[test]
    fn open_close_lifecycle() {
        let mut prompt = PromptState::new();
        assert!(!prompt.is_open());
        assert_eq!(prompt.handle_key(&key("a")), PromptAction::Ignored);

        prompt.open(search_spec(), "init");
        assert!(prompt.is_open());
        assert_eq!(prompt.spec().unwrap().id, "search");
        assert_eq!(prompt.input(), "init");
        assert_eq!(prompt.cursor(), 4);

        prompt.close();
        assert!(!prompt.is_open());
        assert_eq!(prompt.input(), "");
        assert_eq!(prompt.cursor(), 0);
        assert!(prompt.message().is_none());
    }

    #[test]
    fn typing_and_cursor_movement() {
        let mut prompt = PromptState::new();
        prompt.open(search_spec(), "");

        prompt.insert("hello");
        assert_eq!(prompt.input(), "hello");
        assert_eq!(prompt.cursor(), 5);

        prompt.move_left();
        prompt.move_left();
        prompt.insert("X");
        assert_eq!(prompt.input(), "helXlo");

        prompt.move_home();
        assert_eq!(prompt.cursor(), 0);
        prompt.move_end();
        assert_eq!(prompt.cursor(), 6);
    }

    #[test]
    fn editing_is_unicode_safe() {
        let mut prompt = PromptState::new();
        prompt.open(search_spec(), "");
        prompt.insert("héllo");
        assert_eq!(prompt.cursor(), "héllo".len());

        prompt.move_home();
        prompt.move_right();
        assert_eq!(prompt.cursor(), 1);
        prompt.move_right();
        // Stepped over the 2-byte "é" to the next boundary.
        assert_eq!(prompt.cursor(), 3);

        prompt.backspace();
        assert_eq!(prompt.input(), "hllo");
        assert!(prompt.input().is_char_boundary(prompt.cursor()));
    }

    #[test]
    fn backspace_delete_word_and_clear_line() {
        let mut prompt = PromptState::new();
        prompt.open(search_spec(), "");
        prompt.insert("foo bar baz");

        prompt.delete_word_before();
        assert_eq!(prompt.input(), "foo bar ");

        prompt.delete_word_before();
        assert_eq!(prompt.input(), "foo ");

        prompt.clear_to_start();
        assert_eq!(prompt.input(), "");
        assert_eq!(prompt.cursor(), 0);
    }

    #[test]
    fn enter_submits_and_records_history() {
        let mut prompt = PromptState::new();
        prompt.open(search_spec(), "");

        prompt.insert("first");
        assert_eq!(
            prompt.handle_key(&key("enter")),
            PromptAction::Submitted("first".to_string())
        );
        // Still open for the caller to close explicitly.
        assert!(prompt.is_open());
        assert_eq!(prompt.history(), &["first".to_string()]);
        prompt.close();

        prompt.open(search_spec(), "");
        prompt.insert("second");
        prompt.submit();
        prompt.close();
        assert_eq!(
            prompt.history(),
            &["first".to_string(), "second".to_string()]
        );

        prompt.open(search_spec(), "");
        prompt.history_prev();
        assert_eq!(prompt.input(), "second");
        prompt.history_prev();
        assert_eq!(prompt.input(), "first");
        prompt.history_next();
        assert_eq!(prompt.input(), "second");
        prompt.history_next();
        // Back past the end restores the pre-history draft (empty here).
        assert_eq!(prompt.input(), "");
    }

    #[test]
    fn escape_cancels_and_closes() {
        let mut prompt = PromptState::new();
        prompt.open(search_spec(), "");
        prompt.insert("abc");
        assert_eq!(prompt.handle_key(&key("escape")), PromptAction::Cancelled);
        assert!(!prompt.is_open());
        assert_eq!(prompt.input(), "");
    }

    #[test]
    fn handle_key_routes_editing_keys() {
        let mut prompt = PromptState::new();
        prompt.open(search_spec(), "");

        assert_eq!(prompt.handle_key(&key("a")), PromptAction::Editing);
        assert_eq!(prompt.input(), "a");
        assert_eq!(prompt.handle_key(&key("backspace")), PromptAction::Editing);
        assert_eq!(prompt.input(), "");
        assert_eq!(prompt.handle_key(&key("arrowleft")), PromptAction::Editing);

        // Plain "space" key name (as sent by some frontends) inserts a space.
        assert_eq!(prompt.handle_key(&key("space")), PromptAction::Editing);
        assert_eq!(prompt.input(), " ");
    }

    #[test]
    fn ctrl_shortcuts_edit_input() {
        let mut prompt = PromptState::new();
        prompt.open(search_spec(), "");
        prompt.insert("hello");

        assert_eq!(prompt.handle_key(&ctrl("a")), PromptAction::Editing);
        assert_eq!(prompt.cursor(), 0);
        assert_eq!(prompt.handle_key(&ctrl("e")), PromptAction::Editing);
        assert_eq!(prompt.cursor(), 5);
        assert_eq!(prompt.handle_key(&ctrl("u")), PromptAction::Editing);
        assert_eq!(prompt.input(), "");
    }

    #[test]
    fn items_selection_wraps_and_clamps() {
        let mut prompt = PromptState::new();
        prompt.open(palette_spec(), "");
        prompt.set_items(vec![
            PromptItem::new("save"),
            PromptItem::new("quit"),
            PromptItem::new("write"),
        ]);
        assert_eq!(prompt.selected_index(), 0);

        prompt.select_prev();
        assert_eq!(prompt.selected_index(), 2);
        prompt.select_next();
        assert_eq!(prompt.selected_index(), 0);
        assert_eq!(prompt.selected_item().unwrap().label, "save");

        prompt.set_items(vec![PromptItem::new("only")]);
        assert_eq!(prompt.selected_index(), 0);
        prompt.set_items(vec![]);
        assert!(prompt.selected_item().is_none());
    }

    #[test]
    fn tab_completes_selected_label() {
        let mut prompt = PromptState::new();
        prompt.open(palette_spec(), "");
        prompt.set_items(vec![PromptItem::with_hint("save-file", "Ctrl+S")]);
        assert_eq!(prompt.handle_key(&key("tab")), PromptAction::Editing);
        assert_eq!(prompt.input(), "save-file");
    }

    #[test]
    fn up_down_prefer_items_over_history() {
        let mut prompt = PromptState::new();
        prompt.open(palette_spec(), "");
        prompt.insert("x");
        prompt.submit();
        prompt.insert("y");

        // No items: arrows walk history.
        assert_eq!(prompt.handle_key(&key("arrowup")), PromptAction::Editing);
        assert_eq!(prompt.input(), "x");

        // With items: arrows walk the selection instead.
        prompt.set_items(vec![PromptItem::new("one"), PromptItem::new("two")]);
        assert_eq!(prompt.handle_key(&key("arrowdown")), PromptAction::Editing);
        assert_eq!(prompt.selected_index(), 1);
        assert_eq!(prompt.input(), "x");
    }

    #[test]
    fn message_roundtrip() {
        let mut prompt = PromptState::new();
        prompt.open(search_spec(), "");
        assert!(prompt.message().is_none());
        prompt.set_message("E486: Pattern not found");
        assert_eq!(prompt.message(), Some("E486: Pattern not found"));
        prompt.clear_message();
        assert!(prompt.message().is_none());
    }

    #[test]
    fn fuzzy_score_matches_subsequence_case_insensitively() {
        assert!(fuzzy_score("save-file", "satek").is_none());
        assert!(fuzzy_score("Save-File", "sf").is_some());
        assert!(fuzzy_score("quit", "sf").is_none());
        // Consecutive + prefix beats a scattered match.
        let strong = fuzzy_score("save", "sa").unwrap();
        let weak = fuzzy_score("xsave", "sa").unwrap();
        assert!(strong > weak);
        // Empty query matches everything neutrally.
        assert_eq!(fuzzy_score("anything", ""), Some(0));
    }

    #[test]
    fn fuzzy_filter_sorts_by_score_then_index() {
        let items = vec![
            PromptItem::new("quit"),
            PromptItem::new("save-file"),
            PromptItem::new("save-all"),
        ];
        let ranked = fuzzy_filter(&items, "save");
        let ids: Vec<usize> = ranked.iter().map(|(i, _)| *i).collect();
        // Non-matches excluded; shorter equal-quality match ranks first.
        assert_eq!(ids, vec![2, 1]);

        // Exact score ties fall back to index order.
        let tied = vec![PromptItem::new("abx"), PromptItem::new("aby")];
        let ranked = fuzzy_filter(&tied, "ab");
        let ids: Vec<usize> = ranked.iter().map(|(i, _)| *i).collect();
        assert_eq!(ids, vec![0, 1]);
    }
}
