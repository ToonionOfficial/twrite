use gpui::*;
use twrite_core::{
    HighlightTag, Point as BufferPoint, StyleSpan, StyleValue, UnderlineDecoration,
    split_line_intervals,
};

use crate::editor::Editor;
use crate::theme::EditorTheme;

/// Visual layout metrics and block-level decorations for a single rendered line.
#[derive(Debug, Clone)]
pub struct LineMetrics {
    /// Font size in pixels for this line.
    pub font_size: Pixels,
    /// Vertical line height in pixels for this line.
    pub line_height: Pixels,
    /// Whether this line is a blockquote.
    pub is_quote: bool,
    /// Whether this line is part of a fenced code block.
    pub is_code_block: bool,
}

impl LineMetrics {
    /// Calculates layout metrics and block-level decorations based on syntax spans and line text.
    pub fn for_line(
        line_text: &str,
        spans: &[StyleSpan],
        base_font_size: Pixels,
        base_line_height: Pixels,
    ) -> Self {
        let mut font_size = base_font_size;
        let mut line_height = base_line_height;

        let has_h1 = spans
            .iter()
            .any(|s| matches!(s.style, StyleValue::Tag(HighlightTag::Heading1)));
        let has_h2 = spans
            .iter()
            .any(|s| matches!(s.style, StyleValue::Tag(HighlightTag::Heading2)));
        let has_h3 = spans
            .iter()
            .any(|s| matches!(s.style, StyleValue::Tag(HighlightTag::Heading3)));
        let has_h4 = spans
            .iter()
            .any(|s| matches!(s.style, StyleValue::Tag(HighlightTag::Heading4)));
        let has_h5 = spans
            .iter()
            .any(|s| matches!(s.style, StyleValue::Tag(HighlightTag::Heading5)));
        let has_h6 = spans
            .iter()
            .any(|s| matches!(s.style, StyleValue::Tag(HighlightTag::Heading6)));

        if has_h1 {
            font_size = base_font_size * 2.0;
            line_height = base_line_height * 1.8;
        } else if has_h2 {
            font_size = base_font_size * 1.5;
            line_height = base_line_height * 1.4;
        } else if has_h3 {
            font_size = base_font_size * 1.25;
            line_height = base_line_height * 1.2;
        } else if has_h4 {
            font_size = base_font_size * 1.125;
            line_height = base_line_height * 1.1;
        } else if has_h5 {
            font_size = base_font_size * 1.0;
            line_height = base_line_height * 1.0;
        } else if has_h6 {
            font_size = base_font_size * 0.875;
            line_height = base_line_height * 0.95;
        }

        let trimmed = line_text.trim_start();
        let is_quote = trimmed.starts_with("> ") || trimmed == ">";

        let is_code_block = trimmed.starts_with("```")
            || trimmed.starts_with("~~~")
            || (!line_text.is_empty()
                && spans.iter().any(|s| {
                    s.range.len() == line_text.len()
                        && matches!(s.style, StyleValue::Tag(HighlightTag::Code))
                }));

        Self {
            font_size,
            line_height,
            is_quote,
            is_code_block,
        }
    }
}

/// Builds styled text runs for a single line by blending syntax spans and active selection.
pub fn build_line_text_runs(
    line_text: &str,
    spans: &[StyleSpan],
    selection_line_range: Option<(usize, usize)>,
    font: &Font,
    theme: &EditorTheme,
) -> Vec<TextRun> {
    if line_text.is_empty() {
        return Vec::new();
    }

    let segments = split_line_intervals(line_text.len(), spans, selection_line_range);
    let mut runs = Vec::with_capacity(segments.len());

    for segment in segments {
        let resolved = segment.style.map(|s| theme.resolve_style(s));

        let color = resolved
            .as_ref()
            .map(|r| r.color)
            .unwrap_or(theme.foreground);
        let background_color = if segment.is_selected {
            Some(theme.selection)
        } else {
            resolved.as_ref().and_then(|r| r.background)
        };

        let mut run_font = font.clone();
        if let Some(r) = resolved.as_ref() {
            if r.bold {
                run_font.weight = FontWeight::BOLD;
            }
            if r.italic {
                run_font.style = FontStyle::Italic;
            }
        }

        let underline = resolved
            .as_ref()
            .and_then(|r| r.underline)
            .map(|u| match u {
                UnderlineDecoration::Solid => UnderlineStyle {
                    color: Some(color),
                    thickness: px(1.0),
                    wavy: false,
                },
                UnderlineDecoration::Wavy => UnderlineStyle {
                    color: Some(theme.syntax.error),
                    thickness: px(1.2),
                    wavy: true,
                },
            });

        let strikethrough = resolved.as_ref().and_then(|r| {
            if r.strikethrough {
                Some(StrikethroughStyle {
                    color: Some(color),
                    thickness: px(1.0),
                })
            } else {
                None
            }
        });

        runs.push(TextRun {
            len: segment.range.end - segment.range.start,
            font: run_font,
            color,
            background_color,
            underline,
            strikethrough,
        });
    }

    let mut merged: Vec<TextRun> = Vec::with_capacity(runs.len());
    for run in runs {
        if let Some(last) = merged.last_mut()
            && last.font == run.font
            && last.color == run.color
            && last.background_color == run.background_color
            && last.underline == run.underline
            && last.strikethrough == run.strikethrough
        {
            last.len += run.len;
            continue;
        }
        merged.push(run);
    }

    merged
}

/// Data computed during prepaint for each visible line.
struct PreparedLine {
    gutter_num: Option<(Point<Pixels>, ShapedLine)>,
    text_origin: Point<Pixels>,
    text_line: WrappedLine,
    line_height: Pixels,
    quote_bar_quad: Option<PaintQuad>,
    code_block_bg_quad: Option<PaintQuad>,
    empty_selection_quad: Option<PaintQuad>,
    cursor_quad: Option<PaintQuad>,
}

/// The state struct (T) passed from `prepaint` to `paint`.
struct EditorCanvasPrepaint {
    background_quad: PaintQuad,
    lines: Vec<PreparedLine>,
}

/// The GPUI canvas element responsible for shaping and rendering text runs and quads.
#[derive(IntoElement)]
pub struct EditorCanvas {
    editor: Entity<Editor>,
}

impl EditorCanvas {
    /// Creates a new canvas element linked to the given editor entity.
    pub fn new(editor: Entity<Editor>) -> Self {
        EditorCanvas { editor }
    }
}

impl RenderOnce for EditorCanvas {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let editor_handle = self.editor.clone();

        canvas(
            move |bounds, window, cx| {
                editor_handle.update(cx, |editor, _| {
                    editor.last_bounds = Some(bounds);
                });
                let editor = editor_handle.read(cx);
                let theme = editor.theme.clone();
                let config = editor.config.clone();
                let font = window.text_style().font();

                let gutter_width = if config.line_numbers {
                    px(48.0)
                } else {
                    px(0.0)
                };
                let text_origin_x = bounds.left() + gutter_width + px(12.0);

                let wrap_width = if config.line_wrap {
                    let available = bounds.size.width - gutter_width - px(24.0);
                    Some(available.max(px(50.0)))
                } else {
                    None
                };

                let total_lines = editor.buffer.len_lines();
                let total_bytes = editor.buffer.len_bytes();
                let scroll_row = editor.scroll_row;

                let cursor_offset = editor.buffer.cursor_offset();
                let cursor_point = editor.buffer.cursor_point();
                let selection = editor.selection;

                let is_all_selected = selection.is_some_and(|s| {
                    let range = s.byte_range();
                    !s.is_empty() && range.start == 0 && range.end == total_bytes
                });

                let mut lines: Vec<PreparedLine> = Vec::new();
                let mut current_y = bounds.top();
                let mut computed_cursor_pixel = None;

                for row in scroll_row..total_lines {
                    if current_y >= bounds.bottom() {
                        break;
                    }

                    let raw_line = editor.buffer.line_to_string(row);
                    let line_text = raw_line.trim_end_matches(['\r', '\n']);
                    let line_start_byte = editor.buffer.point_to_offset(BufferPoint::new(row, 0));
                    let line_end_byte = line_start_byte + line_text.len();

                    let gutter_num = if config.line_numbers {
                        let is_cursor_row = cursor_point.row == row;
                        let line_num_str = format!("{:>3}", row + 1);
                        let num_color = if is_cursor_row {
                            theme.line_number_active
                        } else {
                            theme.line_number
                        };

                        let shaped = window.text_system().shape_line(
                            line_num_str.into(),
                            config.font_size,
                            &[TextRun {
                                len: 3,
                                font: font.clone(),
                                color: num_color,
                                background_color: None,
                                underline: None,
                                strikethrough: None,
                            }],
                            None,
                        );

                        Some((point(bounds.left() + px(8.0), current_y), shaped))
                    } else {
                        None
                    };

                    let spans = if let Some(ref highlighter) = editor.highlighter {
                        highlighter.highlight_line(&editor.buffer, row, line_text)
                    } else {
                        Vec::new()
                    };

                    let metrics = LineMetrics::for_line(
                        line_text,
                        &spans,
                        config.font_size,
                        config.line_height,
                    );

                    let selection_line_range = if let Some(sel) = selection {
                        let sel_range = sel.byte_range();
                        if sel_range.end > line_start_byte && sel_range.start < line_end_byte {
                            let sel_start = sel_range
                                .start
                                .saturating_sub(line_start_byte)
                                .min(line_text.len());
                            let sel_end = (sel_range.end - line_start_byte).min(line_text.len());
                            if sel_end > sel_start {
                                Some((sel_start, sel_end))
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    let runs = build_line_text_runs(
                        line_text,
                        &spans,
                        selection_line_range,
                        &font,
                        &theme,
                    );

                    let text_line = window
                        .text_system()
                        .shape_text(
                            line_text.to_string().into(),
                            metrics.font_size,
                            &runs,
                            wrap_width,
                            None,
                        )
                        .ok()
                        .and_then(|mut l| l.pop())
                        .unwrap_or_default();

                    let line_visual_lines = text_line.wrap_boundaries.len() + 1;
                    let line_total_height = metrics.line_height * line_visual_lines;

                    let quote_bar_quad = if metrics.is_quote {
                        Some(fill(
                            Bounds::new(
                                point(bounds.left() + gutter_width + px(4.0), current_y),
                                size(px(3.0), line_total_height),
                            ),
                            theme.syntax.comment,
                        ))
                    } else {
                        None
                    };

                    let code_block_bg_quad = if metrics.is_code_block {
                        let bg_width = (bounds.size.width - gutter_width - px(8.0)).max(px(0.0));
                        Some(fill(
                            Bounds::new(
                                point(bounds.left() + gutter_width + px(4.0), current_y),
                                size(bg_width, line_total_height),
                            ),
                            theme.syntax.code_bg,
                        ))
                    } else {
                        None
                    };

                    let cursor_quad = if cursor_point.row == row {
                        let col_in_line = cursor_offset
                            .saturating_sub(line_start_byte)
                            .min(line_text.len());
                        let pos = text_line
                            .position_for_index(col_in_line, metrics.line_height)
                            .unwrap_or(point(px(0.0), px(0.0)));

                        computed_cursor_pixel = Some(point(
                            text_origin_x + pos.x,
                            current_y + pos.y + metrics.line_height,
                        ));

                        if !is_all_selected {
                            let style = if config.block_cursor {
                                twrite_core::CursorStyle::Block
                            } else {
                                editor.cursor_style
                            };

                            match style {
                                twrite_core::CursorStyle::Hidden => None,
                                twrite_core::CursorStyle::Block => Some(fill(
                                    Bounds::new(
                                        point(text_origin_x + pos.x, current_y + pos.y),
                                        size(px(8.5), metrics.line_height),
                                    ),
                                    theme.cursor,
                                )),
                                twrite_core::CursorStyle::Underline => Some(fill(
                                    Bounds::new(
                                        point(
                                            text_origin_x + pos.x,
                                            current_y + pos.y + metrics.line_height - px(2.0),
                                        ),
                                        size(px(8.5), px(2.0)),
                                    ),
                                    theme.cursor,
                                )),
                                twrite_core::CursorStyle::Bar => Some(fill(
                                    Bounds::new(
                                        point(text_origin_x + pos.x, current_y + pos.y),
                                        size(px(2.0), metrics.line_height),
                                    ),
                                    theme.cursor,
                                )),
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    let empty_selection_quad = if line_text.is_empty() {
                        if let Some(sel) = selection {
                            let sel_range = sel.byte_range();
                            if sel_range.start <= line_start_byte && sel_range.end > line_start_byte
                            {
                                Some(fill(
                                    Bounds::new(
                                        point(text_origin_x, current_y),
                                        size(px(6.0), metrics.line_height),
                                    ),
                                    theme.selection,
                                ))
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    lines.push(PreparedLine {
                        gutter_num,
                        text_origin: point(text_origin_x, current_y),
                        text_line,
                        line_height: metrics.line_height,
                        quote_bar_quad,
                        code_block_bg_quad,
                        empty_selection_quad,
                        cursor_quad,
                    });

                    current_y += line_total_height;
                }

                editor_handle.update(cx, |editor, _| {
                    editor.last_cursor_pixel = computed_cursor_pixel;
                });

                EditorCanvasPrepaint {
                    background_quad: fill(bounds, theme.background),
                    lines,
                }
            },
            move |_bounds, prepaint, window, cx| {
                window.paint_quad(prepaint.background_quad);

                for line in prepaint.lines {
                    if let Some(code_bg) = line.code_block_bg_quad {
                        window.paint_quad(code_bg);
                    }
                    if let Some(quote_bar) = line.quote_bar_quad {
                        window.paint_quad(quote_bar);
                    }

                    if let Some((origin, shaped_num)) = line.gutter_num {
                        let _ = shaped_num.paint(
                            origin,
                            line.line_height,
                            TextAlign::Left,
                            None,
                            window,
                            cx,
                        );
                    }

                    let _ = line.text_line.paint_background(
                        line.text_origin,
                        line.line_height,
                        TextAlign::Left,
                        None,
                        window,
                        cx,
                    );
                    if let Some(empty_sel) = line.empty_selection_quad {
                        window.paint_quad(empty_sel);
                    }

                    if let Some(cursor_quad) = line.cursor_quad {
                        window.paint_quad(cursor_quad);
                    }

                    let _ = line.text_line.paint(
                        line.text_origin,
                        line.line_height,
                        TextAlign::Left,
                        None,
                        window,
                        cx,
                    );
                }
            },
        )
        .size_full()
    }
}
