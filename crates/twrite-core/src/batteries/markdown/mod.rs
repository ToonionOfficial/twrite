//! CommonMark and GitHub Flavored Markdown battery: highlighter and interactive hook.
//!
//! The battery is split into focused modules (promoted from a single
//! `batteries/markdown.rs` once it outgrew one file, per the registry
//! contract in [`crate::batteries`]):
//!
//! - [`config`]: `MarkdownConfig` / `ConcealMode` shared settings.
//! - [`table`]: GFM pipe-table detection (blocks, pipes, delimiter rows).
//! - [`highlight`]: `MarkdownHighlighter` (`SyntaxHighlighter` impl).
//! - [`hook`]: `MarkdownHook` (`EditorHook` impl: shortcuts, lists, tables).
//! - [`links`]: single-line hyperlink extraction shared by highlighting and
//!   click handling.
//!
//! Everything here is built only on the public core API (`SyntaxHighlighter`,
//! `EditorHook`, `HighlightTag`, `ConcealedLine`, …) and emits only existing
//! `HighlightTag` variants plus `Custom("markdown.table.*")` names, so no
//! `twrite-gpui`-side code is required.

mod config;
mod highlight;
mod hook;
mod links;
mod table;

pub use config::{ConcealMode, MarkdownConfig};
pub use highlight::MarkdownHighlighter;
pub use hook::MarkdownHook;
pub use links::extract_markdown_links;
pub use table::{
    TABLE_CELL_TAG, TABLE_DELIMITER_TAG, TABLE_HEADER_TAG, TableAlignment, TableBlock, TableLayout,
    TableRowKind, find_unescaped_pipes, parse_delimiter_row, split_table_cells, table_block_at,
    table_layouts,
};
