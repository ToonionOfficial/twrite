use std::sync::Arc;

use crate::{
    CompletionSnapshot, EditorBuffer, EditorHook, HookContext, HookEffect, HookOutcome, KeyCode,
    KeyEvent, Point, PromptItem, Selection, fuzzy_filter,
};

use super::config::MarkdownConfig;
use super::links::parse_wikilinks;
use super::list::{handle_list_move, handle_list_tab};
use super::table::{
    TableRowKind, clean_table_line, find_unescaped_pipes, split_table_cells, table_block_at,
};

/// Supplies wikilink completion candidates for a query string.
///
/// Hosts map the raw query (the target fragment between `[[` and the
/// cursor, never alias text after `|`) to candidate rows however they store
/// data (files, database, memory). The hook ranks rows with `fuzzy_filter`
/// and caps visible rows, so providers can return generous lists; pre-cap
/// for very large vaults.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
/// use twrite_core::PromptItem;
/// use twrite_core::markdown::MarkdownHook;
///
/// let mut hook = MarkdownHook::new();
/// hook.set_completion_provider(Arc::new(|query: &str| {
///     ["Notebook", "Note"]
///         .into_iter()
///         .filter(|name| name.contains(query))
///         .map(PromptItem::new)
///         .collect()
/// }));
/// assert!(hook.has_completion_provider());
/// ```
pub type WikilinkCompletionProvider = Arc<dyn Fn(&str) -> Vec<PromptItem> + Send + Sync>;

/// Maximum rows kept per completion refresh.
const MAX_COMPLETION_ITEMS: usize = 8;

/// An in-progress `[[` completion: anchor plus ranked rows.
#[derive(Debug, Clone)]
struct CompletionSession {
    /// Buffer byte offset of the opening `[[`.
    trigger_start: usize,
    /// Buffer row of the anchor (sessions never span lines).
    trigger_row: usize,
    /// Ranked candidate rows.
    items: Vec<PromptItem>,
    /// Selected row index.
    selected: usize,
}

/// An editor hook providing Markdown shortcuts (Ctrl+B, Ctrl+I, Ctrl+K), smart list continuation, and task list toggles.
///
/// It also recognizes `[[Target]]`, `[[Target|Label]]`, and
/// `[[Target#Fragment]]` wikilinks: click or `Enter` reports a `FollowLink` effect for the host to resolve, and typing `[[`
/// opens inline completion when a provider is set (see
/// [`Self::set_completion_provider`]).
#[derive(Clone)]
pub struct MarkdownHook {
    interactive_tasks: bool,
    table_navigation: bool,
    list_indentation: bool,
    list_reordering: bool,
    list_indent_size: usize,
    completion_provider: Option<WikilinkCompletionProvider>,
    completion: Option<CompletionSession>,
}

impl std::fmt::Debug for MarkdownHook {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MarkdownHook")
            .field("interactive_tasks", &self.interactive_tasks)
            .field("table_navigation", &self.table_navigation)
            .field("list_indentation", &self.list_indentation)
            .field("list_reordering", &self.list_reordering)
            .field("list_indent_size", &self.list_indent_size)
            .field(
                "has_completion_provider",
                &self.completion_provider.is_some(),
            )
            .field("completion_active", &self.completion.is_some())
            .finish()
    }
}

impl Default for MarkdownHook {
    fn default() -> Self {
        Self::new()
    }
}

impl MarkdownHook {
    /// Creates a new Markdown editing hook.
    pub fn new() -> Self {
        Self {
            interactive_tasks: true,
            table_navigation: true,
            list_indentation: true,
            list_reordering: true,
            list_indent_size: 2,
            completion_provider: None,
            completion: None,
        }
    }

    /// Creates a hook honoring the given Markdown configuration.
    pub fn with_config(config: MarkdownConfig) -> Self {
        Self {
            interactive_tasks: config.interactive_tasks,
            table_navigation: config.table_navigation,
            list_indentation: config.list_indentation,
            list_reordering: config.list_reordering,
            list_indent_size: config.list_indent_size,
            completion_provider: None,
            completion: None,
        }
    }

    /// Updates whether mouse clicks toggle task checkboxes.
    pub fn set_interactive_tasks(&mut self, interactive: bool) {
        self.interactive_tasks = interactive;
    }

    /// Returns whether mouse clicks toggle task checkboxes.
    pub fn interactive_tasks(&self) -> bool {
        self.interactive_tasks
    }

    /// Updates whether `Tab` / `Shift+Tab` move between table cells.
    pub fn set_table_navigation(&mut self, enabled: bool) {
        self.table_navigation = enabled;
    }

    /// Returns whether table cell navigation is enabled.
    pub fn table_navigation(&self) -> bool {
        self.table_navigation
    }

    /// Updates whether `Tab` / `Shift+Tab` indent and unindent list items.
    pub fn set_list_indentation(&mut self, enabled: bool) {
        self.list_indentation = enabled;
    }

    /// Returns whether list indentation is enabled.
    pub fn list_indentation(&self) -> bool {
        self.list_indentation
    }

    /// Updates whether `Alt+Up` / `Alt+Down` move list items.
    pub fn set_list_reordering(&mut self, enabled: bool) {
        self.list_reordering = enabled;
    }

    /// Returns whether list reordering is enabled.
    pub fn list_reordering(&self) -> bool {
        self.list_reordering
    }

    /// Sets the number of spaces per list indent level.
    pub fn set_list_indent_size(&mut self, size: usize) {
        self.list_indent_size = size;
    }

    /// Returns the number of spaces per list indent level.
    pub fn list_indent_size(&self) -> usize {
        self.list_indent_size
    }

    /// Reports the wikilink covering `col` on `row` as a `FollowLink`
    /// effect for the host to resolve against its own storage. Returns
    /// whether a wikilink was hit.
    fn follow_wikilink_at(ctx: &mut HookContext, row: usize, col: usize) -> bool {
        if row >= ctx.buffer.len_lines() {
            return false;
        }
        let line = ctx.buffer.line_to_string(row);
        let Some(target) = parse_wikilinks(&line)
            .into_iter()
            .find(|target| target.full_range.contains(&col))
        else {
            return false;
        };
        ctx.effects.push(HookEffect::FollowLink {
            target: target.note,
            fragment: target.heading,
            label: target.alias,
        });
        true
    }

    /// Sets the callback supplying wikilink completion candidates.
    ///
    /// Typing the second `[` of `[[` then opens an inline popup: typing
    /// filters rows, `Up`/`Down` move, `Enter`/`Tab` accept (preserving any
    /// `|alias` suffix and ensuring closing `]]`), and `Esc`, typing `]]`,
    /// or moving the cursor away dismisses. Accepts and popup clicks insert
    /// the selected label over the query. See the "Inline Completion"
    /// recipe for the session contract in full.
    pub fn set_completion_provider(&mut self, provider: WikilinkCompletionProvider) {
        self.completion_provider = Some(provider);
    }

    /// Returns whether a completion provider is configured.
    pub fn has_completion_provider(&self) -> bool {
        self.completion_provider.is_some()
    }

    /// Reads the live query between the session anchor and the cursor.
    /// Returns `None` when edits broke the anchor (closed or unclosed
    /// brackets, row change); callers dismiss the session then.
    fn completion_query(buffer: &EditorBuffer, session: &CompletionSession) -> Option<String> {
        let cursor = buffer.cursor_offset();
        if cursor < session.trigger_start + 2 {
            return None;
        }
        if buffer.cursor_point().row != session.trigger_row {
            return None;
        }
        let text = buffer.text();
        if !text
            .get_byte_slice(session.trigger_start..session.trigger_start + 2)
            .is_some_and(|slice| slice == "[[")
        {
            return None;
        }
        let query = text.get_byte_slice(session.trigger_start + 2..cursor)?;
        Some(query.chars().collect())
    }

    /// Re-ranks session rows for `query`, capping visible rows.
    fn refresh_completion(&mut self, query: &str) {
        let (candidates, ranked) =
            match (self.completion.as_mut(), self.completion_provider.clone()) {
                (Some(_), Some(provider)) => {
                    let candidates = provider(query);
                    let ranked = fuzzy_filter(&candidates, query);
                    (candidates, ranked)
                }
                _ => return,
            };
        if let Some(session) = self.completion.as_mut() {
            session.items = ranked
                .into_iter()
                .take(MAX_COMPLETION_ITEMS)
                .map(|(index, _)| candidates[index].clone())
                .collect();
            session.selected = session.selected.min(session.items.len().saturating_sub(1));
        }
    }

    /// Opens a session at the just-typed second `[` (cursor sits after it).
    fn open_completion(&mut self, trigger_start: usize, trigger_row: usize, query: &str) {
        self.completion = Some(CompletionSession {
            trigger_start,
            trigger_row,
            items: Vec::new(),
            selected: 0,
        });
        self.refresh_completion(query);
    }

    /// Inserts the selected row's label over the query fragment, preserving
    /// any `|alias` suffix and ensuring closing `]]`. Returns whether a row
    /// was accepted.
    fn accept_completion_at(&mut self, ctx: &mut HookContext, index: usize) -> bool {
        let (trigger_start, label) = match self.completion.as_ref() {
            Some(session) if index < session.items.len() => {
                (session.trigger_start, session.items[index].label.clone())
            }
            _ => return false,
        };
        let cursor = ctx.buffer.cursor_offset();
        let query_start = trigger_start + 2;
        if cursor < query_start {
            self.completion = None;
            return false;
        }
        let query_text = match ctx
            .buffer
            .text()
            .get_byte_slice(query_start..cursor)
            .map(|slice| slice.chars().collect::<String>())
        {
            Some(query_text) => query_text,
            None => {
                self.completion = None;
                return false;
            }
        };
        let replace_end = match query_text.find('|') {
            Some(pipe) => query_start + pipe,
            None => cursor,
        };
        let suffix_len = cursor.saturating_sub(replace_end);
        ctx.buffer.replace_range(query_start..replace_end, &label);
        let label_end = query_start + label.len();
        let suffix_end = label_end + suffix_len;
        let needs_close = ctx
            .buffer
            .text()
            .get_byte_slice(suffix_end..suffix_end + 2)
            .is_none_or(|slice| slice != "]]");
        if needs_close {
            ctx.buffer.set_cursor_offset(suffix_end);
            ctx.buffer.insert("]]");
        }
        ctx.buffer.set_cursor_offset(label_end);
        *ctx.selection = None;
        self.completion = None;
        true
    }

    /// Routes one keystroke through an active completion session.
    /// Returns `None` when the key falls through to normal handling (either
    /// the session just dismissed itself or the key is not a completion key).
    fn handle_completion_key(
        &mut self,
        ctx: &mut HookContext,
        event: &KeyEvent,
    ) -> Option<HookOutcome> {
        let anchor_live = self
            .completion
            .as_ref()
            .is_some_and(|session| Self::completion_query(ctx.buffer, session).is_some());
        if !anchor_live {
            self.completion = None;
            return None;
        }
        match &event.code {
            KeyCode::Escape => {
                self.completion = None;
                Some(HookOutcome::Consumed)
            }
            KeyCode::Up
                if !event.modifiers.ctrl && !event.modifiers.alt && !event.modifiers.meta =>
            {
                if let Some(session) = self.completion.as_mut()
                    && !session.items.is_empty()
                {
                    session.selected = session
                        .selected
                        .checked_sub(1)
                        .unwrap_or(session.items.len() - 1);
                }
                Some(HookOutcome::Consumed)
            }
            KeyCode::Down
                if !event.modifiers.ctrl && !event.modifiers.alt && !event.modifiers.meta =>
            {
                if let Some(session) = self.completion.as_mut()
                    && !session.items.is_empty()
                {
                    session.selected = (session.selected + 1) % session.items.len();
                }
                Some(HookOutcome::Consumed)
            }
            KeyCode::Enter if !event.modifiers.shift && plain_modifiers(&event.modifiers) => {
                let index = self.completion.as_ref().map(|session| session.selected);
                match index {
                    Some(index)
                        if self
                            .completion
                            .as_ref()
                            .is_some_and(|session| !session.items.is_empty()) =>
                    {
                        self.accept_completion_at(ctx, index);
                        Some(HookOutcome::Consumed)
                    }
                    _ => {
                        self.completion = None;
                        None
                    }
                }
            }
            KeyCode::Tab if plain_modifiers(&event.modifiers) => {
                let index = self.completion.as_ref().map(|session| session.selected);
                match index {
                    Some(index)
                        if self
                            .completion
                            .as_ref()
                            .is_some_and(|session| !session.items.is_empty()) =>
                    {
                        self.accept_completion_at(ctx, index);
                        Some(HookOutcome::Consumed)
                    }
                    _ => None,
                }
            }
            KeyCode::Backspace if plain_modifiers(&event.modifiers) => {
                let trigger_start = self.completion.as_ref().map(|s| s.trigger_start);
                match trigger_start {
                    Some(trigger_start) if ctx.buffer.cursor_offset() > trigger_start + 2 => {
                        ctx.buffer.backspace();
                        let cursor = ctx.buffer.cursor_offset();
                        let query = ctx
                            .buffer
                            .text()
                            .get_byte_slice(trigger_start + 2..cursor)
                            .map(|slice| slice.chars().collect::<String>())
                            .unwrap_or_default();
                        self.refresh_completion(completion_fragment(&query));
                        Some(HookOutcome::Consumed)
                    }
                    _ => {
                        self.completion = None;
                        None
                    }
                }
            }
            KeyCode::Char(current) if plain_modifiers(&event.modifiers) => {
                if *current == ']' {
                    self.completion = None;
                    return None;
                }
                let mut text = [0u8; 4];
                ctx.buffer.insert(current.encode_utf8(&mut text));
                let trigger_start = self
                    .completion
                    .as_ref()
                    .map(|session| session.trigger_start)
                    .unwrap_or(0);
                let cursor = ctx.buffer.cursor_offset();
                let query = ctx
                    .buffer
                    .text()
                    .get_byte_slice(trigger_start + 2..cursor)
                    .map(|slice| slice.chars().collect::<String>())
                    .unwrap_or_default();
                self.refresh_completion(completion_fragment(&query));
                Some(HookOutcome::Consumed)
            }
            _ => None,
        }
    }

    fn toggle_marker_at_row(ctx: &mut HookContext, row: usize) -> bool {
        if row >= ctx.buffer.len_lines() {
            return false;
        }
        let line = ctx.buffer.line_to_string(row);
        let line_start = ctx.buffer.point_to_offset(Point::new(row, 0));
        let old_cursor = ctx.buffer.cursor_offset();

        // Unchecked -> checked (lowercase x, matching Ctrl+Enter behavior).
        for (empty, checked) in [("- [ ] ", "- [x] "), ("* [ ] ", "* [x] ")] {
            if let Some(idx) = line.find(empty) {
                let s = line_start + idx;
                ctx.buffer.replace_range(s..s + 6, checked);
                ctx.buffer.set_cursor_offset(old_cursor);
                return true;
            }
        }
        // Checked (x or X) -> unchecked.
        for (checked, empty) in [
            ("- [x] ", "- [ ] "),
            ("- [X] ", "- [ ] "),
            ("* [x] ", "- [ ] "),
            ("* [X] ", "- [ ] "),
        ] {
            if let Some(idx) = line.find(checked) {
                let s = line_start + idx;
                ctx.buffer.replace_range(s..s + 6, empty);
                ctx.buffer.set_cursor_offset(old_cursor);
                return true;
            }
        }
        false
    }

    fn toggle_checkbox(ctx: &mut HookContext) -> bool {
        let row = ctx.buffer.cursor_point().row;
        let line = ctx.buffer.line_to_string(row);
        let line_start = ctx.buffer.point_to_offset(Point::new(row, 0));

        if let Some(idx) = line.find("- [ ] ") {
            let target_start = line_start + idx;
            ctx.buffer
                .replace_range(target_start..target_start + 6, "- [x] ");
            return true;
        } else if let Some(idx) = line.find("- [x] ") {
            let target_start = line_start + idx;
            ctx.buffer
                .replace_range(target_start..target_start + 6, "- [ ] ");
            return true;
        } else if let Some(idx) = line.find("* [ ] ") {
            let target_start = line_start + idx;
            ctx.buffer
                .replace_range(target_start..target_start + 6, "* [x] ");
            return true;
        } else if let Some(idx) = line.find("* [x] ") {
            let target_start = line_start + idx;
            ctx.buffer
                .replace_range(target_start..target_start + 6, "* [ ] ");
            return true;
        }
        false
    }

    /// Cell-content starts for a stripped table line: byte offset just after
    /// each separator pipe (skipping one run of padding spaces), plus offset
    /// `0` when the line does not open with a pipe.
    fn table_cell_starts(stripped: &str) -> Vec<usize> {
        let bytes = stripped.as_bytes();
        let mut starts = Vec::new();
        if !stripped.trim_start().starts_with('|') {
            starts.push(0);
        }
        for p in find_unescaped_pipes(stripped) {
            let mut s = (p + 1).min(stripped.len());
            while s < stripped.len() && (bytes[s] == b' ' || bytes[s] == b'\t') {
                s += 1;
            }
            starts.push(s);
        }
        starts
    }

    /// Computes the cursor target for `Tab` (forward) / `Shift+Tab` (backward)
    /// inside GFM table header/body rows, appending a skeleton row when
    /// tabbing past the last cell. Returns `None` to fall through to the
    /// default handler (e.g. outside tables, on delimiter rows).
    fn table_tab_target(ctx: &mut HookContext, backwards: bool) -> Option<usize> {
        let row = ctx.buffer.cursor_point().row;
        let block = table_block_at(ctx.buffer, row)?;
        if !matches!(
            block.kind_at(row)?,
            TableRowKind::Header | TableRowKind::Body
        ) {
            return None;
        }
        let line_start = ctx.buffer.point_to_offset(Point::new(row, 0));
        let stripped = clean_table_line(&ctx.buffer.line_to_string(row)).to_string();
        let cursor_col = ctx
            .buffer
            .cursor_offset()
            .saturating_sub(line_start)
            .min(stripped.len());
        let starts = Self::table_cell_starts(&stripped);

        if !backwards {
            if let Some(&s) = starts.iter().find(|&&s| s > cursor_col) {
                return Some(line_start + s);
            }
            // Last cell: move into the next data row, else append a skeleton.
            for r in row + 1..=block.end_row {
                if matches!(
                    block.kind_at(r),
                    Some(TableRowKind::Header) | Some(TableRowKind::Body)
                ) {
                    let next_start = ctx.buffer.point_to_offset(Point::new(r, 0));
                    let next_stripped = clean_table_line(&ctx.buffer.line_to_string(r)).to_string();
                    let next_cells = Self::table_cell_starts(&next_stripped);
                    return Some(next_start + next_cells.first().copied().unwrap_or(0));
                }
            }
            let indent_len = stripped.len() - stripped.trim_start().len();
            let indent = &stripped[..indent_len];
            let skeleton = format!("{}|{}", indent, " |".repeat(block.col_count));
            let line_end = line_start + stripped.len();
            ctx.buffer.set_cursor_offset(line_end);
            ctx.buffer.insert(&format!("\n{skeleton}"));
            return Some(line_end + 1 + indent_len + 2);
        }

        if let Some(&s) = starts.iter().rev().find(|&&s| s < cursor_col) {
            return Some(line_start + s);
        }
        // First cell: move into the previous data row's last cell.
        for r in (block.header_row..row).rev() {
            if matches!(
                block.kind_at(r),
                Some(TableRowKind::Header) | Some(TableRowKind::Body)
            ) {
                let prev_start = ctx.buffer.point_to_offset(Point::new(r, 0));
                let prev_stripped = clean_table_line(&ctx.buffer.line_to_string(r)).to_string();
                let prev_cells = Self::table_cell_starts(&prev_stripped);
                return Some(prev_start + prev_cells.last().copied().unwrap_or(0));
            }
        }
        None
    }
}

/// Reports keys with no active modifiers (Shift may be held for capitals).
fn plain_modifiers(modifiers: &crate::Modifiers) -> bool {
    !modifiers.ctrl && !modifiers.alt && !modifiers.meta
}

/// Returns the completable target fragment of a completion query: text up
/// to any `|alias` separator. Alias text is preserved verbatim on accept
/// and never sent to the provider, keeping providers trivial.
fn completion_fragment(query: &str) -> &str {
    query.split('|').next().unwrap_or(query)
}

/// Returns the offset of the first `[` when the cursor sits right after one,
/// meaning the next typed `[` opens a `[[` completion.
fn completion_trigger_at(ctx: &HookContext) -> Option<usize> {
    let cursor = ctx.buffer.cursor_offset();
    if cursor == 0 {
        return None;
    }
    let before = ctx
        .buffer
        .text()
        .get_byte_slice(cursor - 1..cursor)
        .map(|slice| slice == "[")
        .unwrap_or(false);
    before.then_some(cursor - 1)
}

impl EditorHook for MarkdownHook {
    fn on_key(&mut self, ctx: &mut HookContext, event: &KeyEvent) -> HookOutcome {
        // An open completion session owns its keys; anything it releases
        // falls through to normal handling below.
        if self.completion.is_some() {
            if let Some(outcome) = self.handle_completion_key(ctx, event) {
                return outcome;
            }
        } else if let KeyCode::Char('[') = event.code
            && plain_modifiers(&event.modifiers)
            && self.completion_provider.is_some()
            && let Some(trigger_start) = completion_trigger_at(ctx)
        {
            ctx.buffer.insert("[");
            let trigger_row = ctx.buffer.offset_to_point(trigger_start).row;
            self.open_completion(trigger_start, trigger_row, "");
            return HookOutcome::Consumed;
        }

        if event.modifiers.ctrl || event.modifiers.meta {
            match &event.code {
                KeyCode::Char('b') => {
                    if let Some(sel) = ctx.selection.take() {
                        let range = sel.byte_range();
                        let text = ctx.buffer.text().byte_slice(range.clone()).to_string();
                        let wrapped = format!("**{}**", text);
                        ctx.buffer.replace_range(range.clone(), &wrapped);
                        *ctx.selection = Some(Selection::range(range.start + 2, range.end + 2));
                    } else {
                        ctx.buffer.insert("****");
                        ctx.buffer.move_cursor_left();
                        ctx.buffer.move_cursor_left();
                    }
                    return HookOutcome::Consumed;
                }
                KeyCode::Char('i') => {
                    if let Some(sel) = ctx.selection.take() {
                        let range = sel.byte_range();
                        let text = ctx.buffer.text().byte_slice(range.clone()).to_string();
                        let wrapped = format!("*{}*", text);
                        ctx.buffer.replace_range(range.clone(), &wrapped);
                        *ctx.selection = Some(Selection::range(range.start + 1, range.end + 1));
                    } else {
                        ctx.buffer.insert("**");
                        ctx.buffer.move_cursor_left();
                    }
                    return HookOutcome::Consumed;
                }
                KeyCode::Char('k') => {
                    if let Some(sel) = ctx.selection.take() {
                        let range = sel.byte_range();
                        let text = ctx.buffer.text().byte_slice(range.clone()).to_string();
                        let wrapped = format!("[{}](url)", text);
                        ctx.buffer.replace_range(range.clone(), &wrapped);
                        let url_start = range.start + 1 + text.len() + 2;
                        *ctx.selection = Some(Selection::range(url_start, url_start + 3));
                    } else {
                        ctx.buffer.insert("[](url)");
                        ctx.buffer.move_cursor_left();
                        ctx.buffer.move_cursor_left();
                        ctx.buffer.move_cursor_left();
                        ctx.buffer.move_cursor_left();
                        ctx.buffer.move_cursor_left();
                        ctx.buffer.move_cursor_left();
                    }
                    return HookOutcome::Consumed;
                }
                KeyCode::Enter if Self::toggle_checkbox(ctx) => {
                    return HookOutcome::Consumed;
                }
                _ => {}
            }
        }

        if event.code == KeyCode::Enter && !event.modifiers.shift {
            let cursor = ctx.buffer.cursor_offset();
            let row = ctx.buffer.cursor_point().row;
            let line = ctx.buffer.line_to_string(row);
            let trimmed = line.trim_start();
            let indent_len = line.len() - trimmed.len();
            let indent = &line[..indent_len];

            // GFM table row continuation (before list handling: table rows
            // start with `|`, never with a list marker).
            if self.table_navigation
                && let Some(block) = table_block_at(ctx.buffer, row)
                && let Some(kind) = block.kind_at(row)
                && matches!(kind, TableRowKind::Header | TableRowKind::Body)
            {
                let stripped = clean_table_line(&line).to_string();
                let (_, cells) = split_table_cells(&stripped);
                let all_empty = cells.iter().all(|c| {
                    stripped
                        .get(c.clone())
                        .map(|s| s.trim().is_empty())
                        .unwrap_or(true)
                });
                if all_empty {
                    // Empty row exits the table, mirroring empty list items.
                    let line_start = ctx.buffer.point_to_offset(Point::new(row, 0));
                    ctx.buffer.delete_range(line_start..cursor);
                    return HookOutcome::Consumed;
                }
                let table_indent_len = stripped.len() - stripped.trim_start().len();
                let table_indent = &stripped[..table_indent_len];
                let skeleton = format!("{}|{}", table_indent, " |".repeat(block.col_count));
                ctx.buffer.insert(&format!("\n{skeleton}"));
                return HookOutcome::Consumed;
            }

            if trimmed.starts_with("- [ ] ") || trimmed.starts_with("- [x] ") {
                if trimmed == "- [ ] \n"
                    || trimmed == "- [ ] \r\n"
                    || trimmed == "- [ ] "
                    || trimmed == "- [x] \n"
                    || trimmed == "- [x] \r\n"
                    || trimmed == "- [x] "
                {
                    let line_start = ctx.buffer.point_to_offset(Point::new(row, 0));
                    ctx.buffer.delete_range(line_start..cursor);
                    return HookOutcome::Consumed;
                }
                ctx.buffer.insert(&format!("\n{}- [ ] ", indent));
                return HookOutcome::Consumed;
            }

            if trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ ") {
                let bullet = &trimmed[..2];
                if trimmed == "- \n"
                    || trimmed == "- \r\n"
                    || trimmed == "- "
                    || trimmed == "* \n"
                    || trimmed == "* \r\n"
                    || trimmed == "* "
                    || trimmed == "+ \n"
                    || trimmed == "+ \r\n"
                    || trimmed == "+ "
                {
                    let line_start = ctx.buffer.point_to_offset(Point::new(row, 0));
                    ctx.buffer.delete_range(line_start..cursor);
                    return HookOutcome::Consumed;
                }
                ctx.buffer.insert(&format!("\n{}{}", indent, bullet));
                return HookOutcome::Consumed;
            }

            if let Some(dot_idx) = trimmed.find(". ") {
                let num_str = &trimmed[..dot_idx];
                if let Ok(num) = num_str.parse::<usize>() {
                    let rest = &trimmed[dot_idx + 2..];
                    if rest == "\n" || rest == "\r\n" || rest.is_empty() {
                        let line_start = ctx.buffer.point_to_offset(Point::new(row, 0));
                        ctx.buffer.delete_range(line_start..cursor);
                        return HookOutcome::Consumed;
                    }
                    ctx.buffer.insert(&format!("\n{}{}. ", indent, num + 1));
                    return HookOutcome::Consumed;
                }
            }

            // Plain `Enter` on a wikilink follows it. List and table
            // continuation above take precedence so items containing links
            // keep editing behavior.
            {
                let line_start = ctx.buffer.point_to_offset(Point::new(row, 0));
                let col = cursor.saturating_sub(line_start);
                if Self::follow_wikilink_at(ctx, row, col) {
                    return HookOutcome::Consumed;
                }
            }
        }

        if event.code == KeyCode::Tab
            && !event.modifiers.ctrl
            && !event.modifiers.meta
            && !event.modifiers.alt
        {
            if self.table_navigation
                && let Some(target) = Self::table_tab_target(ctx, event.modifiers.shift)
            {
                ctx.buffer.set_cursor_offset(target);
                *ctx.selection = None;
                return HookOutcome::Consumed;
            }

            if self.list_indentation
                && handle_list_tab(ctx, event.modifiers.shift, self.list_indent_size)
            {
                return HookOutcome::Consumed;
            }
        }

        if matches!(event.code, KeyCode::Up | KeyCode::Down)
            && event.modifiers.alt
            && !event.modifiers.ctrl
            && !event.modifiers.meta
            && self.list_reordering
            && handle_list_move(ctx, event.code == KeyCode::Up)
        {
            return HookOutcome::Consumed;
        }

        HookOutcome::PassThrough
    }

    fn on_click(&mut self, ctx: &mut HookContext, row: usize, col: usize) -> HookOutcome {
        if self.interactive_tasks && Self::toggle_marker_at_row(ctx, row) {
            return HookOutcome::Consumed;
        }
        if Self::follow_wikilink_at(ctx, row, col) {
            return HookOutcome::Consumed;
        }
        HookOutcome::PassThrough
    }

    fn on_selection_change(&mut self, buffer: &EditorBuffer, _selection: Option<&Selection>) {
        let dismiss = match self.completion.as_ref() {
            Some(session) => {
                let cursor = buffer.cursor_offset();
                if buffer.cursor_point().row != session.trigger_row
                    || cursor < session.trigger_start
                {
                    true
                } else {
                    buffer
                        .text()
                        .get_byte_slice(session.trigger_start..cursor)
                        .map(|slice| {
                            let text: String = slice.chars().collect();
                            text.contains("]]") || text.contains('\n')
                        })
                        .unwrap_or(true)
                }
            }
            None => false,
        };
        if dismiss {
            self.completion = None;
        }
    }

    fn completion_snapshot(&self) -> Option<CompletionSnapshot> {
        self.completion.as_ref().map(|session| CompletionSnapshot {
            items: session.items.clone(),
            selected: session.selected,
        })
    }

    fn on_completion_select(&mut self, ctx: &mut HookContext, index: usize) -> HookOutcome {
        if self.accept_completion_at(ctx, index) {
            HookOutcome::Consumed
        } else {
            HookOutcome::PassThrough
        }
    }

    fn dismiss_completion(&mut self) {
        self.completion = None;
    }

    fn status_text(&self) -> Option<&str> {
        Some("MARKDOWN")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        EditorBuffer, HookContext, HookEffect, HookOutcome, KeyEvent, PromptState, Selection,
    };

    fn hook_with_notes() -> MarkdownHook {
        let mut hook = MarkdownHook::new();
        hook.set_completion_provider(Arc::new(|query: &str| {
            ["Notebook", "Note", "Blog"]
                .into_iter()
                .filter(|name| name.contains(query))
                .map(PromptItem::new)
                .collect()
        }));
        hook
    }

    /// Drives one key through the hook the way the editor does: `on_key`
    /// first, then the selection-change broadcast that follows every input.
    fn press_completion_key(
        hook: &mut MarkdownHook,
        buffer: &mut EditorBuffer,
        selection: &mut Option<Selection>,
        cursor_style: &mut crate::CursorStyle,
        prompt: &mut PromptState,
        effects: &mut Vec<HookEffect>,
        event: KeyEvent,
    ) -> HookOutcome {
        let outcome = {
            let mut ctx = HookContext::new(buffer, selection, cursor_style, prompt, effects);
            hook.on_key(&mut ctx, &event)
        };
        hook.on_selection_change(buffer, selection.as_ref());
        outcome
    }

    fn plain_char_key(code: KeyCode) -> KeyEvent {
        KeyEvent::plain(code)
    }

    #[test]
    fn test_markdown_hook_bold_wrapping() {
        let mut buffer = EditorBuffer::new("hello world");
        let mut selection = Some(Selection::range(0, 5));
        let mut cursor_style = crate::CursorStyle::Bar;
        let mut prompt = PromptState::new();
        let mut effects = Vec::new();
        let mut hook = MarkdownHook::new();

        let mut ctx = HookContext::new(
            &mut buffer,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
        );
        let event = KeyEvent {
            code: KeyCode::Char('b'),
            modifiers: crate::Modifiers {
                ctrl: true,
                ..Default::default()
            },
        };

        let outcome = hook.on_key(&mut ctx, &event);
        assert_eq!(outcome, HookOutcome::Consumed);
        assert_eq!(ctx.buffer.text().to_string(), "**hello** world");
        assert_eq!(ctx.selection.unwrap().byte_range(), 2..7);
    }

    #[test]
    fn test_markdown_hook_checkbox_toggle() {
        let mut buffer = EditorBuffer::new("- [ ] Task item");
        let mut selection = None;
        let mut cursor_style = crate::CursorStyle::Bar;
        let mut prompt = PromptState::new();
        let mut effects = Vec::new();
        let mut hook = MarkdownHook::new();

        let mut ctx = HookContext::new(
            &mut buffer,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
        );
        let event = KeyEvent {
            code: KeyCode::Enter,
            modifiers: crate::Modifiers {
                ctrl: true,
                ..Default::default()
            },
        };

        let outcome = hook.on_key(&mut ctx, &event);
        assert_eq!(outcome, HookOutcome::Consumed);
        assert_eq!(ctx.buffer.text().to_string(), "- [x] Task item");

        let outcome2 = hook.on_key(&mut ctx, &event);
        assert_eq!(outcome2, HookOutcome::Consumed);
        assert_eq!(ctx.buffer.text().to_string(), "- [ ] Task item");
    }

    #[test]
    fn test_markdown_hook_numbered_list_continuation() {
        let mut buffer = EditorBuffer::new("1. First item");
        buffer.set_cursor_offset(13);
        let mut selection = None;
        let mut cursor_style = crate::CursorStyle::Bar;
        let mut prompt = PromptState::new();
        let mut effects = Vec::new();
        let mut hook = MarkdownHook::new();

        let mut ctx = HookContext::new(
            &mut buffer,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
        );
        let event = KeyEvent::plain(KeyCode::Enter);

        let outcome = hook.on_key(&mut ctx, &event);
        assert_eq!(outcome, HookOutcome::Consumed);
        assert_eq!(ctx.buffer.text().to_string(), "1. First item\n2. ");
    }

    #[test]
    fn test_markdown_hook_on_click_toggles_task() {
        let mut buffer = EditorBuffer::new("- [ ] Task one\n- [x] Task two");
        let mut selection = None;
        let mut cursor_style = crate::CursorStyle::Bar;
        let mut prompt = PromptState::new();
        let mut effects = Vec::new();
        let mut hook = MarkdownHook::new();

        // Click row 1 (checked -> unchecked), cursor stays put.
        buffer.set_cursor_offset(0);
        let mut ctx = HookContext::new(
            &mut buffer,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
        );
        assert_eq!(hook.on_click(&mut ctx, 1, 0), HookOutcome::Consumed);
        assert_eq!(
            ctx.buffer.text().to_string(),
            "- [ ] Task one\n- [ ] Task two"
        );
        assert_eq!(ctx.buffer.cursor_offset(), 0);

        // Click row 0 (unchecked -> checked).
        let mut ctx = HookContext::new(
            &mut buffer,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
        );
        assert_eq!(hook.on_click(&mut ctx, 0, 2), HookOutcome::Consumed);
        assert_eq!(
            ctx.buffer.text().to_string(),
            "- [x] Task one\n- [ ] Task two"
        );

        // Uppercase [X] also toggles.
        ctx.buffer.replace_range(0..14, "- [X] Task one");
        let mut ctx = HookContext::new(
            &mut buffer,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
        );
        assert_eq!(hook.on_click(&mut ctx, 0, 3), HookOutcome::Consumed);
        assert_eq!(
            ctx.buffer.text().to_string(),
            "- [ ] Task one\n- [ ] Task two"
        );

        // Plain line passes through.
        let mut plain = EditorBuffer::new("hello");
        let mut ctx = HookContext::new(
            &mut plain,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
        );
        assert_eq!(hook.on_click(&mut ctx, 0, 0), HookOutcome::PassThrough);
    }

    #[test]
    fn test_markdown_hook_on_click_respects_config() {
        let mut buffer = EditorBuffer::new("- [ ] Task");
        let mut selection = None;
        let mut cursor_style = crate::CursorStyle::Bar;
        let mut prompt = PromptState::new();
        let mut effects = Vec::new();
        let mut hook = MarkdownHook::with_config(MarkdownConfig {
            interactive_tasks: false,
            ..Default::default()
        });
        let mut ctx = HookContext::new(
            &mut buffer,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
        );
        assert_eq!(hook.on_click(&mut ctx, 0, 0), HookOutcome::PassThrough);
        assert_eq!(ctx.buffer.text().to_string(), "- [ ] Task");
    }

    #[test]
    fn test_table_hook_tab_moves_between_cells() {
        let mut buffer = EditorBuffer::new("| a | b |\n| --- | --- |\n| c | d |");
        let mut selection = None;
        let mut cursor_style = crate::CursorStyle::Bar;
        let mut prompt = PromptState::new();
        let mut effects = Vec::new();
        let mut hook = MarkdownHook::new();
        buffer.set_cursor_offset(0);

        let mut ctx = HookContext::new(
            &mut buffer,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
        );
        assert_eq!(
            hook.on_key(&mut ctx, &KeyEvent::plain(KeyCode::Tab)),
            HookOutcome::Consumed
        );
        assert_eq!(ctx.buffer.cursor_offset(), 2); // start of `a`

        let mut ctx = HookContext::new(
            &mut buffer,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
        );
        assert_eq!(
            hook.on_key(&mut ctx, &KeyEvent::plain(KeyCode::Tab)),
            HookOutcome::Consumed
        );
        assert_eq!(ctx.buffer.cursor_offset(), 6); // start of `b`

        // Shift+Tab goes back.
        let back = KeyEvent {
            code: KeyCode::Tab,
            modifiers: crate::Modifiers {
                shift: true,
                ..Default::default()
            },
        };
        let mut ctx = HookContext::new(
            &mut buffer,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
        );
        assert_eq!(hook.on_key(&mut ctx, &back), HookOutcome::Consumed);
        assert_eq!(ctx.buffer.cursor_offset(), 2);
    }

    #[test]
    fn test_table_hook_tab_appends_row_at_end() {
        let mut buffer = EditorBuffer::new("| a |\n| --- |\n| b |");
        let mut selection = None;
        let mut cursor_style = crate::CursorStyle::Bar;
        let mut prompt = PromptState::new();
        let mut effects = Vec::new();
        let mut hook = MarkdownHook::new();
        buffer.set_cursor_offset(buffer.len_bytes());

        let mut ctx = HookContext::new(
            &mut buffer,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
        );
        assert_eq!(
            hook.on_key(&mut ctx, &KeyEvent::plain(KeyCode::Tab)),
            HookOutcome::Consumed
        );
        assert_eq!(ctx.buffer.text().to_string(), "| a |\n| --- |\n| b |\n| |");
    }

    #[test]
    fn test_table_hook_tab_passthrough_outside_tables() {
        let mut buffer = EditorBuffer::new("plain text");
        let mut selection = None;
        let mut cursor_style = crate::CursorStyle::Bar;
        let mut prompt = PromptState::new();
        let mut effects = Vec::new();
        let mut hook = MarkdownHook::new();
        buffer.set_cursor_offset(3);

        let mut ctx = HookContext::new(
            &mut buffer,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
        );
        assert_eq!(
            hook.on_key(&mut ctx, &KeyEvent::plain(KeyCode::Tab)),
            HookOutcome::PassThrough
        );

        let mut disabled = EditorBuffer::new("| a |\n| --- |\n| b |");
        disabled.set_cursor_offset(0);
        let mut hook_off = MarkdownHook::with_config(MarkdownConfig {
            table_navigation: false,
            ..Default::default()
        });
        let mut ctx = HookContext::new(
            &mut disabled,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
        );
        assert_eq!(
            hook_off.on_key(&mut ctx, &KeyEvent::plain(KeyCode::Tab)),
            HookOutcome::PassThrough
        );
    }

    #[test]
    fn test_table_hook_enter_continues_and_exits() {
        // Continuation inserts a skeleton row at the cursor.
        let mut buffer = EditorBuffer::new("| a | b |\n| --- | --- |\n| c | d |");
        buffer.set_cursor_offset(buffer.len_bytes());
        let mut selection = None;
        let mut cursor_style = crate::CursorStyle::Bar;
        let mut prompt = PromptState::new();
        let mut effects = Vec::new();
        let mut hook = MarkdownHook::new();
        let mut ctx = HookContext::new(
            &mut buffer,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
        );
        assert_eq!(
            hook.on_key(&mut ctx, &KeyEvent::plain(KeyCode::Enter)),
            HookOutcome::Consumed
        );
        assert_eq!(
            ctx.buffer.text().to_string(),
            "| a | b |\n| --- | --- |\n| c | d |\n| | |"
        );

        // An all-blank row exits the table like empty list items do.
        let mut empty = EditorBuffer::new("| a |\n| --- |\n| |");
        empty.set_cursor_offset(empty.len_bytes());
        let mut ctx = HookContext::new(
            &mut empty,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
        );
        assert_eq!(
            hook.on_key(&mut ctx, &KeyEvent::plain(KeyCode::Enter)),
            HookOutcome::Consumed
        );
        assert_eq!(ctx.buffer.text().to_string(), "| a |\n| --- |\n");
    }

    #[test]
    fn test_markdown_hook_list_tab_indent_and_outdent_bullet() {
        let mut buffer = EditorBuffer::new("- Parent\n- Child");
        let second_line_offset = buffer.point_to_offset(Point::new(1, 2));
        buffer.set_cursor_offset(second_line_offset);
        let mut selection = None;
        let mut cursor_style = crate::CursorStyle::Bar;
        let mut prompt = PromptState::new();
        let mut effects = Vec::new();
        let mut hook = MarkdownHook::new();
        let mut ctx = HookContext::new(
            &mut buffer,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
        );

        let tab_event = KeyEvent::plain(KeyCode::Tab);
        let outcome = hook.on_key(&mut ctx, &tab_event);
        assert_eq!(outcome, HookOutcome::Consumed);
        assert_eq!(ctx.buffer.text().to_string(), "- Parent\n  - Child");

        let shift_tab_event = KeyEvent {
            code: KeyCode::Tab,
            modifiers: crate::Modifiers {
                shift: true,
                ..Default::default()
            },
        };
        let outcome2 = hook.on_key(&mut ctx, &shift_tab_event);
        assert_eq!(outcome2, HookOutcome::Consumed);
        assert_eq!(ctx.buffer.text().to_string(), "- Parent\n- Child");
    }

    #[test]
    fn test_markdown_hook_list_tab_ordered_nesting_and_renumbering() {
        let mut buffer = EditorBuffer::new("1. One\n2. Two\n3. Three");
        let second_line_offset = buffer.point_to_offset(Point::new(1, 3));
        buffer.set_cursor_offset(second_line_offset);
        let mut selection = None;
        let mut cursor_style = crate::CursorStyle::Bar;
        let mut prompt = PromptState::new();
        let mut effects = Vec::new();
        let mut hook = MarkdownHook::new();
        let mut ctx = HookContext::new(
            &mut buffer,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
        );

        let tab_event = KeyEvent::plain(KeyCode::Tab);
        let outcome = hook.on_key(&mut ctx, &tab_event);
        assert_eq!(outcome, HookOutcome::Consumed);
        assert_eq!(ctx.buffer.text().to_string(), "1. One\n  1. Two\n2. Three");

        let shift_tab_event = KeyEvent {
            code: KeyCode::Tab,
            modifiers: crate::Modifiers {
                shift: true,
                ..Default::default()
            },
        };
        let outcome2 = hook.on_key(&mut ctx, &shift_tab_event);
        assert_eq!(outcome2, HookOutcome::Consumed);
        assert_eq!(ctx.buffer.text().to_string(), "1. One\n2. Two\n3. Three");
    }

    #[test]
    fn test_markdown_hook_list_tab_tasks() {
        let mut buffer = EditorBuffer::new("- [ ] Task 1\n- [x] Task 2");
        let second_line_offset = buffer.point_to_offset(Point::new(1, 4));
        buffer.set_cursor_offset(second_line_offset);
        let mut selection = None;
        let mut cursor_style = crate::CursorStyle::Bar;
        let mut prompt = PromptState::new();
        let mut effects = Vec::new();
        let mut hook = MarkdownHook::new();
        let mut ctx = HookContext::new(
            &mut buffer,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
        );

        let tab_event = KeyEvent::plain(KeyCode::Tab);
        let outcome = hook.on_key(&mut ctx, &tab_event);
        assert_eq!(outcome, HookOutcome::Consumed);
        assert_eq!(
            ctx.buffer.text().to_string(),
            "- [ ] Task 1\n  - [x] Task 2"
        );
    }

    #[test]
    fn test_markdown_hook_list_tab_multiline_selection() {
        let mut buffer = EditorBuffer::new("- A\n- B\n- C\n- D");
        let start_offset = buffer.point_to_offset(Point::new(1, 0));
        let end_offset = buffer.point_to_offset(Point::new(2, 3));
        let mut selection = Some(Selection::range(start_offset, end_offset));
        let mut cursor_style = crate::CursorStyle::Bar;
        let mut prompt = PromptState::new();
        let mut effects = Vec::new();
        let mut hook = MarkdownHook::new();
        let mut ctx = HookContext::new(
            &mut buffer,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
        );

        let tab_event = KeyEvent::plain(KeyCode::Tab);
        let outcome = hook.on_key(&mut ctx, &tab_event);
        assert_eq!(outcome, HookOutcome::Consumed);
        assert_eq!(ctx.buffer.text().to_string(), "- A\n  - B\n  - C\n- D");

        let shift_tab_event = KeyEvent {
            code: KeyCode::Tab,
            modifiers: crate::Modifiers {
                shift: true,
                ..Default::default()
            },
        };
        let outcome2 = hook.on_key(&mut ctx, &shift_tab_event);
        assert_eq!(outcome2, HookOutcome::Consumed);
        assert_eq!(ctx.buffer.text().to_string(), "- A\n- B\n- C\n- D");
    }

    #[test]
    fn test_markdown_hook_list_tab_non_list_passes_through() {
        let mut buffer = EditorBuffer::new("Paragraph text here");
        buffer.set_cursor_offset(5);
        let mut selection = None;
        let mut cursor_style = crate::CursorStyle::Bar;
        let mut prompt = PromptState::new();
        let mut effects = Vec::new();
        let mut hook = MarkdownHook::new();
        let mut ctx = HookContext::new(
            &mut buffer,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
        );

        let tab_event = KeyEvent::plain(KeyCode::Tab);
        let outcome = hook.on_key(&mut ctx, &tab_event);
        assert_eq!(outcome, HookOutcome::PassThrough);
    }

    #[test]
    fn test_markdown_hook_list_tab_undo_redo() {
        let mut buffer = EditorBuffer::new("1. One\n2. Two\n3. Three");
        let second_line_offset = buffer.point_to_offset(Point::new(1, 3));
        buffer.set_cursor_offset(second_line_offset);
        let mut selection = None;
        let mut cursor_style = crate::CursorStyle::Bar;
        let mut prompt = PromptState::new();
        let mut effects = Vec::new();
        let mut hook = MarkdownHook::new();
        let mut ctx = HookContext::new(
            &mut buffer,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
        );

        let tab_event = KeyEvent::plain(KeyCode::Tab);
        assert_eq!(hook.on_key(&mut ctx, &tab_event), HookOutcome::Consumed);
        assert_eq!(ctx.buffer.text().to_string(), "1. One\n  1. Two\n2. Three");

        // Single undo reverts all lines and restores the cursor offset
        ctx.buffer.undo();
        assert_eq!(ctx.buffer.text().to_string(), "1. One\n2. Two\n3. Three");
        assert_eq!(ctx.buffer.cursor_offset(), second_line_offset);

        // Redo re-applies the indentation
        ctx.buffer.redo();
        assert_eq!(ctx.buffer.text().to_string(), "1. One\n  1. Two\n2. Three");
    }

    #[test]
    fn test_markdown_hook_list_tab_outdent_at_root() {
        let mut buffer = EditorBuffer::new("- One\n- Two");
        let second_line_offset = buffer.point_to_offset(Point::new(1, 2));
        buffer.set_cursor_offset(second_line_offset);
        let mut selection = None;
        let mut cursor_style = crate::CursorStyle::Bar;
        let mut prompt = PromptState::new();
        let mut effects = Vec::new();
        let mut hook = MarkdownHook::new();
        let mut ctx = HookContext::new(
            &mut buffer,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
        );

        let shift_tab_event = KeyEvent {
            code: KeyCode::Tab,
            modifiers: crate::Modifiers {
                shift: true,
                ..Default::default()
            },
        };
        assert_eq!(
            hook.on_key(&mut ctx, &shift_tab_event),
            HookOutcome::Consumed
        );
        assert_eq!(ctx.buffer.text().to_string(), "- One\n- Two");
    }

    #[test]
    fn test_markdown_hook_list_tab_custom_indent_size() {
        let mut buffer = EditorBuffer::new("- Parent\n- Child");
        let second_line_offset = buffer.point_to_offset(Point::new(1, 2));
        buffer.set_cursor_offset(second_line_offset);
        let mut selection = None;
        let mut cursor_style = crate::CursorStyle::Bar;
        let mut prompt = PromptState::new();
        let mut effects = Vec::new();
        let mut hook = MarkdownHook::new();
        hook.set_list_indent_size(4);
        assert_eq!(hook.list_indent_size(), 4);

        let mut ctx = HookContext::new(
            &mut buffer,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
        );

        let tab_event = KeyEvent::plain(KeyCode::Tab);
        assert_eq!(hook.on_key(&mut ctx, &tab_event), HookOutcome::Consumed);
        assert_eq!(ctx.buffer.text().to_string(), "- Parent\n    - Child");
    }

    #[test]
    fn test_markdown_hook_list_tab_parenthesis_delimiter() {
        let mut buffer = EditorBuffer::new("1) One\n2) Two\n3) Three");
        let second_line_offset = buffer.point_to_offset(Point::new(1, 3));
        buffer.set_cursor_offset(second_line_offset);
        let mut selection = None;
        let mut cursor_style = crate::CursorStyle::Bar;
        let mut prompt = PromptState::new();
        let mut effects = Vec::new();
        let mut hook = MarkdownHook::new();
        let mut ctx = HookContext::new(
            &mut buffer,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
        );

        let tab_event = KeyEvent::plain(KeyCode::Tab);
        assert_eq!(hook.on_key(&mut ctx, &tab_event), HookOutcome::Consumed);
        assert_eq!(ctx.buffer.text().to_string(), "1) One\n  1) Two\n2) Three");
    }

    #[test]
    fn test_markdown_hook_list_tab_disabled() {
        let mut buffer = EditorBuffer::new("- Parent\n- Child");
        let second_line_offset = buffer.point_to_offset(Point::new(1, 2));
        buffer.set_cursor_offset(second_line_offset);
        let mut selection = None;
        let mut cursor_style = crate::CursorStyle::Bar;
        let mut prompt = PromptState::new();
        let mut effects = Vec::new();
        let mut hook = MarkdownHook::new();
        hook.set_list_indentation(false);
        assert!(!hook.list_indentation());

        let mut ctx = HookContext::new(
            &mut buffer,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
        );

        let tab_event = KeyEvent::plain(KeyCode::Tab);
        assert_eq!(hook.on_key(&mut ctx, &tab_event), HookOutcome::PassThrough);
    }

    #[test]
    fn test_markdown_hook_alt_up_ordered_list_renumbers() {
        let mut buffer = EditorBuffer::new("1. One\n2. Two\n3. Three");
        let second_line_offset = buffer.point_to_offset(Point::new(1, 3));
        buffer.set_cursor_offset(second_line_offset);
        let mut selection = None;
        let mut cursor_style = crate::CursorStyle::Bar;
        let mut prompt = PromptState::new();
        let mut effects = Vec::new();
        let mut hook = MarkdownHook::new();
        let mut ctx = HookContext::new(
            &mut buffer,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
        );

        let event = KeyEvent {
            code: KeyCode::Up,
            modifiers: crate::Modifiers {
                alt: true,
                ctrl: false,
                shift: false,
                meta: false,
            },
        };
        assert_eq!(hook.on_key(&mut ctx, &event), HookOutcome::Consumed);
        assert_eq!(ctx.buffer.text().to_string(), "1. Two\n2. One\n3. Three");
        assert_eq!(ctx.buffer.cursor_point().row, 0);
    }

    #[test]
    fn test_markdown_hook_alt_down_ordered_list_renumbers() {
        let mut buffer = EditorBuffer::new("1. One\n2. Two\n3. Three");
        let mut selection = None;
        let mut cursor_style = crate::CursorStyle::Bar;
        let mut prompt = PromptState::new();
        let mut effects = Vec::new();
        let mut hook = MarkdownHook::new();
        let mut ctx = HookContext::new(
            &mut buffer,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
        );

        let event = KeyEvent {
            code: KeyCode::Down,
            modifiers: crate::Modifiers {
                alt: true,
                ctrl: false,
                shift: false,
                meta: false,
            },
        };
        assert_eq!(hook.on_key(&mut ctx, &event), HookOutcome::Consumed);
        assert_eq!(ctx.buffer.text().to_string(), "1. Two\n2. One\n3. Three");
        assert_eq!(ctx.buffer.cursor_point().row, 1);
    }

    #[test]
    fn test_markdown_hook_alt_up_bullets_and_tasks() {
        let mut buffer = EditorBuffer::new("- [ ] Task 1\n- [x] Task 2");
        let second_line_offset = buffer.point_to_offset(Point::new(1, 6));
        buffer.set_cursor_offset(second_line_offset);
        let mut selection = None;
        let mut cursor_style = crate::CursorStyle::Bar;
        let mut prompt = PromptState::new();
        let mut effects = Vec::new();
        let mut hook = MarkdownHook::new();
        let mut ctx = HookContext::new(
            &mut buffer,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
        );

        let event = KeyEvent {
            code: KeyCode::Up,
            modifiers: crate::Modifiers {
                alt: true,
                ctrl: false,
                shift: false,
                meta: false,
            },
        };
        assert_eq!(hook.on_key(&mut ctx, &event), HookOutcome::Consumed);
        assert_eq!(ctx.buffer.text().to_string(), "- [x] Task 2\n- [ ] Task 1");
    }

    #[test]
    fn test_markdown_hook_alt_up_reordering_disabled() {
        let mut buffer = EditorBuffer::new("- Item 1\n- Item 2");
        let second_line_offset = buffer.point_to_offset(Point::new(1, 2));
        buffer.set_cursor_offset(second_line_offset);
        let mut selection = None;
        let mut cursor_style = crate::CursorStyle::Bar;
        let mut prompt = PromptState::new();
        let mut effects = Vec::new();
        let mut hook = MarkdownHook::new();
        hook.set_list_reordering(false);
        assert!(!hook.list_reordering());

        let mut ctx = HookContext::new(
            &mut buffer,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
        );

        let event = KeyEvent {
            code: KeyCode::Up,
            modifiers: crate::Modifiers {
                alt: true,
                ctrl: false,
                shift: false,
                meta: false,
            },
        };
        assert_eq!(hook.on_key(&mut ctx, &event), HookOutcome::PassThrough);
    }

    #[test]
    fn test_markdown_hook_click_follows_wikilink() {
        let mut buffer = EditorBuffer::new("See [[Note]] here");
        let mut selection = None;
        let mut cursor_style = crate::CursorStyle::Bar;
        let mut prompt = PromptState::new();
        let mut effects = Vec::new();
        let mut hook = MarkdownHook::new();
        let mut ctx = HookContext::new(
            &mut buffer,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
        );
        assert_eq!(hook.on_click(&mut ctx, 0, 6), HookOutcome::Consumed);
        assert_eq!(
            ctx.effects.as_slice(),
            &[HookEffect::FollowLink {
                target: "Note".to_string(),
                fragment: None,
                label: None,
            }]
        );
    }

    #[test]
    fn test_markdown_hook_click_reports_full_target() {
        let mut buffer = EditorBuffer::new("See [[Note#Heading|Alias]] here");
        let mut selection = None;
        let mut cursor_style = crate::CursorStyle::Bar;
        let mut prompt = PromptState::new();
        let mut effects = Vec::new();
        let mut hook = MarkdownHook::new();
        let mut ctx = HookContext::new(
            &mut buffer,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
        );
        assert_eq!(hook.on_click(&mut ctx, 0, 8), HookOutcome::Consumed);
        assert_eq!(
            ctx.effects.as_slice(),
            &[HookEffect::FollowLink {
                target: "Note".to_string(),
                fragment: Some("Heading".to_string()),
                label: Some("Alias".to_string()),
            }]
        );
    }

    #[test]
    fn test_markdown_hook_click_outside_wikilink_passes_through() {
        let mut buffer = EditorBuffer::new("See [[Note]] here");
        let mut selection = None;
        let mut cursor_style = crate::CursorStyle::Bar;
        let mut prompt = PromptState::new();
        let mut effects = Vec::new();
        let mut hook = MarkdownHook::new();
        let mut ctx = HookContext::new(
            &mut buffer,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
        );
        assert_eq!(hook.on_click(&mut ctx, 0, 0), HookOutcome::PassThrough);
        assert!(ctx.effects.is_empty());
    }

    #[test]
    fn test_markdown_hook_enter_follows_wikilink() {
        let mut buffer = EditorBuffer::new("See [[Note]] here");
        buffer.set_cursor_offset(6);
        let mut selection = None;
        let mut cursor_style = crate::CursorStyle::Bar;
        let mut prompt = PromptState::new();
        let mut effects = Vec::new();
        let mut hook = MarkdownHook::new();
        let mut ctx = HookContext::new(
            &mut buffer,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
        );
        assert_eq!(
            hook.on_key(&mut ctx, &KeyEvent::plain(KeyCode::Enter)),
            HookOutcome::Consumed
        );
        assert_eq!(
            ctx.effects.as_slice(),
            &[HookEffect::FollowLink {
                target: "Note".to_string(),
                fragment: None,
                label: None,
            }]
        );
    }

    #[test]
    fn test_completion_opens_on_second_bracket() {
        let mut buffer = EditorBuffer::new("See [");
        buffer.set_cursor_offset(5);
        let mut selection = None;
        let mut cursor_style = crate::CursorStyle::Bar;
        let mut prompt = PromptState::new();
        let mut effects = Vec::new();
        let mut hook = hook_with_notes();
        assert!(hook.has_completion_provider());
        let outcome = press_completion_key(
            &mut hook,
            &mut buffer,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
            plain_char_key(KeyCode::Char('[')),
        );
        assert_eq!(outcome, HookOutcome::Consumed);
        assert_eq!(buffer.text().to_string(), "See [[");
        let snapshot = hook.completion_snapshot().expect("session must be open");
        assert_eq!(snapshot.items.len(), 3);
        assert_eq!(snapshot.selected, 0);
    }

    #[test]
    fn test_completion_ignores_single_bracket() {
        let mut buffer = EditorBuffer::new("See ");
        buffer.set_cursor_offset(4);
        let mut selection = None;
        let mut cursor_style = crate::CursorStyle::Bar;
        let mut prompt = PromptState::new();
        let mut effects = Vec::new();
        let mut hook = hook_with_notes();
        let outcome = press_completion_key(
            &mut hook,
            &mut buffer,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
            plain_char_key(KeyCode::Char('[')),
        );
        assert_eq!(outcome, HookOutcome::PassThrough);
        assert!(hook.completion_snapshot().is_none());
    }

    #[test]
    fn test_completion_filters_as_you_type() {
        let mut buffer = EditorBuffer::new("See [");
        buffer.set_cursor_offset(5);
        let mut selection = None;
        let mut cursor_style = crate::CursorStyle::Bar;
        let mut prompt = PromptState::new();
        let mut effects = Vec::new();
        let mut hook = hook_with_notes();
        let open = plain_char_key(KeyCode::Char('['));
        assert_eq!(
            press_completion_key(
                &mut hook,
                &mut buffer,
                &mut selection,
                &mut cursor_style,
                &mut prompt,
                &mut effects,
                open
            ),
            HookOutcome::Consumed
        );
        for key in ['N', 'o', 't', 'e'] {
            let outcome = press_completion_key(
                &mut hook,
                &mut buffer,
                &mut selection,
                &mut cursor_style,
                &mut prompt,
                &mut effects,
                plain_char_key(KeyCode::Char(key)),
            );
            assert_eq!(outcome, HookOutcome::Consumed);
        }
        assert_eq!(buffer.text().to_string(), "See [[Note");
        let snapshot = hook.completion_snapshot().expect("session must be open");
        assert_eq!(snapshot.items.len(), 2);
        assert!(
            snapshot
                .items
                .iter()
                .all(|item| item.label.contains("Note"))
        );
    }

    #[test]
    fn test_completion_enter_accepts_selected_label() {
        let mut buffer = EditorBuffer::new("See [");
        buffer.set_cursor_offset(5);
        let mut selection = None;
        let mut cursor_style = crate::CursorStyle::Bar;
        let mut prompt = PromptState::new();
        let mut effects = Vec::new();
        let mut hook = hook_with_notes();
        for key in ['[', 'B', 'l', 'o', 'g'] {
            press_completion_key(
                &mut hook,
                &mut buffer,
                &mut selection,
                &mut cursor_style,
                &mut prompt,
                &mut effects,
                plain_char_key(KeyCode::Char(key)),
            );
        }
        let outcome = press_completion_key(
            &mut hook,
            &mut buffer,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
            KeyEvent::plain(KeyCode::Enter),
        );
        assert_eq!(outcome, HookOutcome::Consumed);
        assert_eq!(buffer.text().to_string(), "See [[Blog]]");
        assert_eq!(buffer.cursor_offset(), 10);
        assert!(hook.completion_snapshot().is_none());
    }

    #[test]
    fn test_completion_navigates_and_tabs_to_accept() {
        let mut buffer = EditorBuffer::new("[");
        buffer.set_cursor_offset(1);
        let mut selection = None;
        let mut cursor_style = crate::CursorStyle::Bar;
        let mut prompt = PromptState::new();
        let mut effects = Vec::new();
        let mut hook = hook_with_notes();
        press_completion_key(
            &mut hook,
            &mut buffer,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
            plain_char_key(KeyCode::Char('[')),
        );
        press_completion_key(
            &mut hook,
            &mut buffer,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
            KeyEvent::plain(KeyCode::Down),
        );
        let snapshot = hook.completion_snapshot().expect("session must be open");
        assert_eq!(snapshot.selected, 1);
        press_completion_key(
            &mut hook,
            &mut buffer,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
            KeyEvent::plain(KeyCode::Up),
        );
        let snapshot = hook.completion_snapshot().expect("session must be open");
        assert_eq!(snapshot.selected, 0);
        let outcome = press_completion_key(
            &mut hook,
            &mut buffer,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
            KeyEvent::plain(KeyCode::Tab),
        );
        assert_eq!(outcome, HookOutcome::Consumed);
        assert_eq!(buffer.text().to_string(), "[[Notebook]]");
        assert!(hook.completion_snapshot().is_none());
    }

    #[test]
    fn test_completion_escape_keeps_typed_text() {
        let mut buffer = EditorBuffer::new("[");
        buffer.set_cursor_offset(1);
        let mut selection = None;
        let mut cursor_style = crate::CursorStyle::Bar;
        let mut prompt = PromptState::new();
        let mut effects = Vec::new();
        let mut hook = hook_with_notes();
        press_completion_key(
            &mut hook,
            &mut buffer,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
            plain_char_key(KeyCode::Char('[')),
        );
        press_completion_key(
            &mut hook,
            &mut buffer,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
            plain_char_key(KeyCode::Char('x')),
        );
        let outcome = press_completion_key(
            &mut hook,
            &mut buffer,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
            KeyEvent::plain(KeyCode::Escape),
        );
        assert_eq!(outcome, HookOutcome::Consumed);
        assert_eq!(buffer.text().to_string(), "[[x");
        assert!(hook.completion_snapshot().is_none());
    }

    #[test]
    fn test_completion_backspace_to_anchor_dismisses() {
        let mut buffer = EditorBuffer::new("[");
        buffer.set_cursor_offset(1);
        let mut selection = None;
        let mut cursor_style = crate::CursorStyle::Bar;
        let mut prompt = PromptState::new();
        let mut effects = Vec::new();
        let mut hook = hook_with_notes();
        press_completion_key(
            &mut hook,
            &mut buffer,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
            plain_char_key(KeyCode::Char('[')),
        );
        assert!(hook.completion_snapshot().is_some());
        let outcome = press_completion_key(
            &mut hook,
            &mut buffer,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
            KeyEvent::plain(KeyCode::Backspace),
        );
        assert_eq!(outcome, HookOutcome::PassThrough);
        assert!(hook.completion_snapshot().is_none());
    }

    #[test]
    fn test_completion_cursor_leave_dismisses() {
        let mut buffer = EditorBuffer::new("See [");
        buffer.set_cursor_offset(5);
        let mut selection = None;
        let mut cursor_style = crate::CursorStyle::Bar;
        let mut prompt = PromptState::new();
        let mut effects = Vec::new();
        let mut hook = hook_with_notes();
        press_completion_key(
            &mut hook,
            &mut buffer,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
            plain_char_key(KeyCode::Char('[')),
        );
        assert!(hook.completion_snapshot().is_some());
        buffer.set_cursor_offset(0);
        hook.on_selection_change(&buffer, selection.as_ref());
        assert!(hook.completion_snapshot().is_none());
    }

    #[test]
    fn test_completion_mouse_select_accepts_row() {
        let mut buffer = EditorBuffer::new("[");
        buffer.set_cursor_offset(1);
        let mut selection = None;
        let mut cursor_style = crate::CursorStyle::Bar;
        let mut prompt = PromptState::new();
        let mut effects = Vec::new();
        let mut hook = hook_with_notes();
        press_completion_key(
            &mut hook,
            &mut buffer,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
            plain_char_key(KeyCode::Char('[')),
        );
        let mut ctx = HookContext::new(
            &mut buffer,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
        );
        assert_eq!(
            hook.on_completion_select(&mut ctx, 1),
            HookOutcome::Consumed
        );
        assert_eq!(ctx.buffer.text().to_string(), "[[Note]]");
        assert!(hook.completion_snapshot().is_none());
    }

    #[test]
    fn test_completion_preserves_alias_suffix_on_accept() {
        let mut buffer = EditorBuffer::new("[");
        buffer.set_cursor_offset(1);
        let mut selection = None;
        let mut cursor_style = crate::CursorStyle::Bar;
        let mut prompt = PromptState::new();
        let mut effects = Vec::new();
        let mut hook = hook_with_notes();
        for key in ['[', 'N', 'o', 't', 'e', 'b', '|', 'm', 'y'] {
            press_completion_key(
                &mut hook,
                &mut buffer,
                &mut selection,
                &mut cursor_style,
                &mut prompt,
                &mut effects,
                plain_char_key(KeyCode::Char(key)),
            );
        }
        assert_eq!(buffer.text().to_string(), "[[Noteb|my");
        let outcome = press_completion_key(
            &mut hook,
            &mut buffer,
            &mut selection,
            &mut cursor_style,
            &mut prompt,
            &mut effects,
            KeyEvent::plain(KeyCode::Enter),
        );
        assert_eq!(outcome, HookOutcome::Consumed);
        assert_eq!(buffer.text().to_string(), "[[Notebook|my]]");
        assert!(hook.completion_snapshot().is_none());
    }
}
