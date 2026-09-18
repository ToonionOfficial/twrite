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
    /// Whether `Tab` / `Shift+Tab` indent and unindent list items and renumber ordered lists.
    pub list_indentation: bool,
    /// Whether `Alt+Up` / `Alt+Down` move list items and renumber ordered lists.
    pub list_reordering: bool,
    /// Number of spaces per list indent level (defaults to 2).
    pub list_indent_size: usize,
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
            list_indentation: true,
            list_reordering: true,
            list_indent_size: 2,
        }
    }
}

/// Display mode for markdown syntax delimiters (like `# `, `**`, `*`, `~~`, `` ` ``).
///
/// Block markers reveal when the cursor is anywhere on the line; inline
/// markers reveal only while the cursor sits inside the formatted word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConcealMode {
    /// Markdown markers are always visible with normal syntax coloring.
    Off,
    /// Markdown markers on inactive lines are rendered with faint opacity.
    #[default]
    Dimmed,
    /// Markdown markers are completely hidden (invisible) until the cursor
    /// enters their word or marker; quote prefixes reveal row-wide.
    Hidden,
}
