/// Configuration settings for Markdown editing, highlighting, and WYSIWYG rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarkdownConfig {
    /// How syntax delimiters (`#`, `**`, `*`, `~~`, `` ` ``) are displayed on inactive lines.
    pub conceal_mode: ConcealMode,
    /// Whether horizontal rules (`---`, `***`, `___`) are rendered as visual divider quads.
    pub visual_thematic_breaks: bool,
    /// Whether clicking on task checkboxes (`- [ ]` / `- [x]`) toggles their state.
    pub interactive_tasks: bool,
    /// Whether GFM table pipes, header rows, and delimiter rows get structural styling.
    pub visual_tables: bool,
    /// Whether GFM table columns are padded to equal display widths (and
    /// excluded from soft-wrapping) so pipes form straight columns.
    pub table_alignment: bool,
    /// Whether `Tab` / `Shift+Tab` move between table cells and `Enter` continues table rows.
    pub table_navigation: bool,
}

impl Default for MarkdownConfig {
    fn default() -> Self {
        Self {
            conceal_mode: ConcealMode::Dimmed,
            visual_thematic_breaks: true,
            interactive_tasks: true,
            visual_tables: true,
            table_alignment: true,
            table_navigation: true,
        }
    }
}

/// Display mode for markdown syntax delimiters (like `# `, `**`, `*`, `~~`, `` ` ``) on inactive lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConcealMode {
    /// Markdown markers are always visible with normal syntax coloring.
    Off,
    /// Markdown markers on inactive lines are rendered with faint opacity.
    #[default]
    Dimmed,
    /// Markdown markers on inactive lines are completely hidden (invisible).
    Hidden,
}
