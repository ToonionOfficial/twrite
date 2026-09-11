use crate::{EditorBuffer, Selection};

/// Well-known context menu item ids for the built-in edit actions.
///
/// Hooks may return an item with one of these ids to override the
/// corresponding default (label, hint, enabled state); the hook's
/// version wins and keeps the default's position.
pub const CUT_ID: &str = "cut";
/// Well-known id for the built-in Copy action.
pub const COPY_ID: &str = "copy";
/// Well-known id for the built-in Paste action.
pub const PASTE_ID: &str = "paste";
/// Well-known id for the built-in Select All action.
pub const SELECT_ALL_ID: &str = "select_all";
/// Well-known id for the built-in Undo action.
pub const UNDO_ID: &str = "undo";
/// Well-known id for the built-in Redo action.
pub const REDO_ID: &str = "redo";
/// Well-known id for the built-in Delete action.
pub const DELETE_ID: &str = "delete";

/// Frontend-supplied capabilities the core cannot observe headlessly.
///
/// The OS clipboard lives outside `twrite-core`; GPUI hosts read it once
/// per right-click and pass the result in. Everything else derives from
/// the buffer + selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ContextMenuCaps {
    /// Whether a non-empty selection exists.
    pub has_selection: bool,
    /// Whether the OS clipboard currently holds text (supplied by the host).
    pub clipboard_has_text: bool,
    /// Whether an undo transaction is available.
    pub can_undo: bool,
    /// Whether a redo transaction is available.
    pub can_redo: bool,
    /// Whether the selection already spans the whole document.
    pub is_full_doc_selected: bool,
}

impl ContextMenuCaps {
    /// Derives buffer-observable caps; the host ORs in `clipboard_has_text`.
    pub fn from_buffer(
        buffer: &EditorBuffer,
        selection: Option<&Selection>,
        clipboard_has_text: bool,
    ) -> Self {
        let has_selection = selection.is_some_and(|s| !s.byte_range().is_empty());
        let is_full_doc_selected = selection.is_some_and(|s| {
            let range = s.byte_range();
            range.start == 0 && range.end == buffer.len_bytes() && !range.is_empty()
        });
        Self {
            has_selection,
            clipboard_has_text,
            can_undo: buffer.can_undo(),
            can_redo: buffer.can_redo(),
            is_full_doc_selected,
        }
    }
}

/// One row in the right-click context menu.
///
/// Hooks own the meaning of custom ids; frontends only draw rows and
/// route clicks back through `on_context_menu_action` / built-in dispatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextMenuItem {
    /// Stable id (`"cut"`, `"copy"`, ... or hook-defined e.g. `"md.toggle-task"`).
    pub id: &'static str,
    /// Primary row text.
    pub label: String,
    /// Secondary hint text (keybinding shown right-aligned).
    pub hint: Option<String>,
    /// Whether the row is clickable. Disabled rows render dimmed.
    pub enabled: bool,
    /// Whether a separator renders directly below this row.
    pub divider_after: bool,
}

impl ContextMenuItem {
    /// Creates an enabled item with no hint.
    pub fn new(id: &'static str, label: &str) -> Self {
        Self {
            id,
            label: label.to_string(),
            hint: None,
            enabled: true,
            divider_after: false,
        }
    }

    /// Creates an enabled item with hint text.
    pub fn with_hint(id: &'static str, label: &str, hint: &str) -> Self {
        Self {
            id,
            label: label.to_string(),
            hint: Some(hint.to_string()),
            enabled: true,
            divider_after: false,
        }
    }

    /// Marks the item as disabled (renders dimmed, ignores clicks).
    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    /// Requests a separator directly below this row.
    pub fn with_divider(mut self) -> Self {
        self.divider_after = true;
        self
    }
}

/// Read-only snapshot passed to hooks contributing menu items.
///
/// `clicked_row` / `clicked_col` are buffer coordinates of the right-click
/// (row = zero-based buffer row, col = source byte offset within that
/// line's text, clamped to the line length) so hooks can offer
/// position-aware actions (e.g. "Toggle task checkbox" only on task lines).
pub struct ContextMenuContext<'a> {
    /// The underlying text buffer (read-only for item contribution).
    pub buffer: &'a EditorBuffer,
    /// The active selection range, if any.
    pub selection: Option<&'a Selection>,
    /// Current cursor byte offset.
    pub cursor_offset: usize,
    /// Zero-based buffer row that was right-clicked.
    pub clicked_row: usize,
    /// Source byte offset within the clicked line that was right-clicked.
    pub clicked_col: usize,
    /// Frontend-supplied capabilities (clipboard, undo/redo availability).
    pub caps: ContextMenuCaps,
}

/// Headless open/closed menu state + resolved item list.
///
/// Owned by the host (the GPUI `Editor`) mirroring `PromptState`: hooks
/// contribute items via `EditorHook::context_menu_items`, the host merges
/// with [`default_context_items`] via [`collect_context_items`], and the
/// frontend only draws rows. Click position (pixels) stays in the GPUI
/// layer; this type never touches screen coordinates.
#[derive(Debug, Clone, Default)]
pub struct ContextMenuState {
    items: Vec<ContextMenuItem>,
    open: bool,
}

impl ContextMenuState {
    /// Creates a closed menu.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether a menu is currently open.
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Current item rows.
    pub fn items(&self) -> &[ContextMenuItem] {
        &self.items
    }

    /// Opens the menu with a pre-merged item list.
    pub fn open(&mut self, items: Vec<ContextMenuItem>) {
        self.items = items;
        self.open = true;
    }

    /// Closes the menu, clearing the item list.
    pub fn close(&mut self) {
        self.items.clear();
        self.open = false;
    }
}

/// Builds the built-in edit rows for the given capabilities.
///
/// Order: Undo, Redo, Cut, Copy, Paste, Delete, Select All. The last
/// default carries `divider_after = true` so renderers draw a separator
/// before hook-contributed rows (removed by [`collect_context_items`]
/// when no hook rows follow).
pub fn default_context_items(caps: ContextMenuCaps) -> Vec<ContextMenuItem> {
    let mut items = vec![
        with_enabled(
            ContextMenuItem::with_hint(UNDO_ID, "Undo", "Ctrl+Z"),
            caps.can_undo,
        ),
        with_enabled(
            ContextMenuItem::with_hint(REDO_ID, "Redo", "Ctrl+Y"),
            caps.can_redo,
        ),
        with_enabled(
            ContextMenuItem::with_hint(CUT_ID, "Cut", "Ctrl+X"),
            caps.has_selection,
        ),
        with_enabled(
            ContextMenuItem::with_hint(COPY_ID, "Copy", "Ctrl+C"),
            caps.has_selection,
        ),
        with_enabled(
            ContextMenuItem::with_hint(PASTE_ID, "Paste", "Ctrl+V"),
            caps.clipboard_has_text,
        ),
        with_enabled(
            ContextMenuItem::new(DELETE_ID, "Delete"),
            caps.has_selection,
        ),
    ];
    let select_all = if caps.is_full_doc_selected {
        ContextMenuItem::with_hint(SELECT_ALL_ID, "Select All", "Ctrl+A").disabled()
    } else {
        ContextMenuItem::with_hint(SELECT_ALL_ID, "Select All", "Ctrl+A")
    };
    items.push(select_all.with_divider());
    items
}

fn with_enabled(mut item: ContextMenuItem, enabled: bool) -> ContextMenuItem {
    item.enabled = enabled;
    item
}

/// Merges defaults with hook-contributed rows.
///
/// * When `include_defaults` is false, only hook rows are used.
/// * Hook rows append in order after the defaults.
/// * A hook row whose `id` matches a default (or an earlier hook row)
///   replaces it in place, keeping the original position.
/// * The trailing divider on the defaults block is dropped when no hook
///   rows follow, so a defaults-only menu has no stray separator.
pub fn collect_context_items(
    include_defaults: bool,
    caps: ContextMenuCaps,
    hook_rows: Vec<Vec<ContextMenuItem>>,
) -> Vec<ContextMenuItem> {
    let mut merged: Vec<ContextMenuItem> = if include_defaults {
        default_context_items(caps)
    } else {
        Vec::new()
    };
    let mut hook_count = 0;
    for rows in hook_rows {
        for row in rows {
            if let Some(pos) = merged.iter().position(|m| m.id == row.id) {
                merged[pos] = row;
            } else {
                merged.push(row);
                hook_count += 1;
            }
        }
    }
    if hook_count == 0
        && let Some(last) = merged.last_mut()
    {
        last.divider_after = false;
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caps_all() -> ContextMenuCaps {
        ContextMenuCaps {
            has_selection: true,
            clipboard_has_text: true,
            can_undo: true,
            can_redo: true,
            is_full_doc_selected: false,
        }
    }

    #[test]
    fn defaults_enablement_matrix() {
        let items = default_context_items(ContextMenuCaps::default());
        // Nothing available: only Select All enabled.
        for item in &items {
            if item.id == SELECT_ALL_ID {
                assert!(item.enabled);
            } else {
                assert!(!item.enabled, "{} should be disabled", item.id);
            }
        }
        // Last default carries the hook-block separator.
        assert!(items.last().unwrap().divider_after);

        let items = default_context_items(caps_all());
        assert!(items.iter().all(|i| i.enabled));
    }

    #[test]
    fn select_all_disabled_when_full_doc_selected() {
        let caps = ContextMenuCaps {
            is_full_doc_selected: true,
            ..caps_all()
        };
        let select = default_context_items(caps)
            .into_iter()
            .find(|i| i.id == SELECT_ALL_ID)
            .unwrap();
        assert!(!select.enabled);
    }

    #[test]
    fn state_open_close_lifecycle() {
        let mut state = ContextMenuState::new();
        assert!(!state.is_open());
        state.open(default_context_items(caps_all()));
        assert!(state.is_open());
        assert_eq!(state.items().len(), 7);
        state.close();
        assert!(!state.is_open());
        assert!(state.items().is_empty());
    }

    #[test]
    fn collect_merges_hooks_after_defaults() {
        let merged = collect_context_items(
            true,
            caps_all(),
            vec![vec![ContextMenuItem::new("md.toggle-task", "Toggle task")]],
        );
        assert_eq!(merged.len(), 8);
        assert_eq!(merged[7].id, "md.toggle-task");
        // Hook rows present: defaults-block divider retained.
        assert!(merged[6].divider_after);
    }

    #[test]
    fn collect_drops_trailing_divider_without_hooks() {
        let merged = collect_context_items(true, caps_all(), vec![]);
        assert!(!merged.last().unwrap().divider_after);
    }

    #[test]
    fn collect_hook_overrides_default_by_id() {
        let merged = collect_context_items(
            true,
            ContextMenuCaps::default(),
            vec![vec![ContextMenuItem::new(COPY_ID, "Copy link")]],
        );
        let copy = merged.iter().find(|i| i.id == COPY_ID).unwrap();
        assert_eq!(copy.label, "Copy link");
        assert!(copy.enabled);
        // Override replaces in place: no extra row.
        assert_eq!(merged.len(), 7);
    }

    #[test]
    fn collect_without_defaults_uses_hooks_only() {
        let merged = collect_context_items(
            false,
            ContextMenuCaps::default(),
            vec![vec![ContextMenuItem::new("custom", "Custom")]],
        );
        assert_eq!(merged.len(), 1);
    }

    #[test]
    fn caps_from_buffer_derives_selection_and_history() {
        let mut buffer = EditorBuffer::new("hello world");
        buffer.insert("!");
        let sel = Selection::range(0, 5);
        let caps = ContextMenuCaps::from_buffer(&buffer, Some(&sel), true);
        assert!(caps.has_selection);
        assert!(caps.clipboard_has_text);
        assert!(caps.can_undo);
        assert!(!caps.can_redo);
        assert!(!caps.is_full_doc_selected);

        let full = Selection::range(0, buffer.len_bytes());
        let caps = ContextMenuCaps::from_buffer(&buffer, Some(&full), false);
        assert!(caps.is_full_doc_selected);

        let caps = ContextMenuCaps::from_buffer(&buffer, None, false);
        assert!(!caps.has_selection);
    }
}
