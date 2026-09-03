#![recursion_limit = "512"]
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
pub use editor::{Editor, VisibleLineLayout};
pub use theme::{EditorTheme, ResolvedTokenStyle, SyntaxTheme};

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Font, px};
    use twrite_core::{HighlightTag, StyleSpan};

    #[test]
    fn test_line_metrics_quote_detection_when_concealed() {
        let spans = vec![];
        let metrics = LineMetrics::for_line("> Quote", "Quote", &spans, px(16.0), px(22.0));
        assert!(metrics.is_quote);
        assert!(!metrics.is_code_block);
        assert_eq!(metrics.line_height, px(22.0));
    }

    #[test]
    fn test_line_metrics_code_fence() {
        let active_spans = vec![StyleSpan::tag(0..7, HighlightTag::Code)];
        let metrics =
            LineMetrics::for_line("```rust", "```rust", &active_spans, px(16.0), px(22.0));
        assert_eq!(metrics.line_height, px(22.0));
        assert!(metrics.is_code_block);
    }

    #[test]
    fn test_line_metrics_code_empty_line_inside_block() {
        let spans = vec![StyleSpan::tag(0..0, HighlightTag::Code)];
        let metrics = LineMetrics::for_line("", "", &spans, px(16.0), px(22.0));
        assert!(metrics.is_code_block);
        assert_eq!(metrics.line_height, px(22.0));
    }

    #[test]
    fn test_build_line_text_runs_code_block_disables_text_bg() {
        let theme = EditorTheme::default();
        let font = Font::default();
        let spans = vec![StyleSpan::tag(0..4, HighlightTag::Code)];

        let runs_block = build_line_text_runs("test", &spans, None, &font, &theme, true, false);
        assert_eq!(runs_block.len(), 1);
        assert!(runs_block[0].background_color.is_none());

        let runs_inline = build_line_text_runs("test", &spans, None, &font, &theme, false, false);
        assert_eq!(runs_inline.len(), 1);
        assert_eq!(runs_inline[0].background_color, Some(theme.syntax.code_bg));

        let runs_task_checked = build_line_text_runs("test", &spans, None, &font, &theme, false, true);
        assert_eq!(runs_task_checked.len(), 1);
        assert!(runs_task_checked[0].strikethrough.is_some());
    }

    #[test]
    fn test_line_metrics_task_state_detection() {
        let spans = vec![];
        let unchecked = LineMetrics::for_line("- [ ] Todo", "Todo", &spans, px(16.0), px(22.0));
        assert_eq!(unchecked.task_state, Some(false));

        let checked = LineMetrics::for_line("- [x] Done", "Done", &spans, px(16.0), px(22.0));
        assert_eq!(checked.task_state, Some(true));

        let plain = LineMetrics::for_line("Plain text", "Plain text", &spans, px(16.0), px(22.0));
        assert_eq!(plain.task_state, None);
    }
}
