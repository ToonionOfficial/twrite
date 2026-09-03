//! GPUI rendering, canvas text-shaping, and interactive editor component for twrite.

/// Canvas element handling prepaint, layout, and GPU quad rendering.
pub mod canvas;
/// Configuration settings for font size, line height, wrapping, and gutters.
pub mod config;
/// Main editor entity, keybindings, selections, and hook executions.
pub mod editor;
/// Translation helpers from GPUI key events to normalized twrite key events.
pub mod input;
/// Color palettes, Catppuccin themes, and syntax token style resolution.
pub mod theme;

pub use canvas::{EditorCanvas, LineMetrics, build_line_text_runs};
pub use config::EditorConfig;
pub use editor::Editor;
pub use theme::{EditorTheme, ResolvedTokenStyle, SyntaxTheme};
