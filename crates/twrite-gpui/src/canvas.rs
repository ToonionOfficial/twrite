use gpui::*;
use twrite_core::Point as BufferPoint;

use crate::editor::Editor;

/// Data computed during prepaint for each visible line
struct PreparedLine {
    gutter_num: Option<(Point<Pixels>, ShapedLine)>,
    text_origin: Point<Pixels>,
    text_line: WrappedLine,
    empty_selection_quad: Option<PaintQuad>,
    cursor_quad: Option<PaintQuad>,
}

/// The state struct (T) passed from `prepaint` to `paint`.
struct EditorCanvasPrepaint {
    background_quad: PaintQuad,
    line_height: Pixels,
    lines: Vec<PreparedLine>,
}

#[derive(IntoElement)]
pub struct EditorCanvas {
    editor: Entity<Editor>,
}

impl EditorCanvas {
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
                let font_size = config.font_size;
                let line_height = config.line_height;

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
                let scroll_row = editor.scroll_row;

                let cursor_offset = editor.buffer.cursor_offset();
                let cursor_point = editor.buffer.cursor_point();
                let selection = editor.selection;

                let mut lines: Vec<PreparedLine> = Vec::new();
                let mut current_y = bounds.top();

                for row in scroll_row..total_lines {
                    if current_y >= bounds.bottom() {
                        break;
                    }

                    let raw_line = editor.buffer.line_to_string(row);
                    let line_text = raw_line.trim_end_matches(['\r', '\n']);
                    let line_start_byte = editor.buffer.point_to_offset(BufferPoint::new(row, 0));
                    let line_end_byte = line_start_byte + line_text.len();

                    // Line number
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
                            font_size,
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

                    // Shape line content with selection background
                    let mut runs = Vec::new();
                    if !line_text.is_empty() {
                        let mut selected_range_in_line = None;
                        if let Some(sel) = selection {
                            let selection_range = sel.byte_range();
                            if selection_range.end > line_start_byte
                                && selection_range.start < line_end_byte
                            {
                                let sel_start = selection_range
                                    .start
                                    .saturating_sub(line_start_byte)
                                    .min(line_text.len());
                                let sel_end =
                                    (selection_range.end - line_start_byte).min(line_text.len());
                                if sel_end > sel_start {
                                    selected_range_in_line = Some((sel_start, sel_end));
                                }
                            }
                        }

                        if let Some((sel_start, sel_end)) = selected_range_in_line {
                            if sel_start > 0 {
                                runs.push(TextRun {
                                    len: sel_start,
                                    font: font.clone(),
                                    color: theme.foreground,
                                    background_color: None,
                                    underline: None,
                                    strikethrough: None,
                                });
                            }
                            runs.push(TextRun {
                                len: sel_end - sel_start,
                                font: font.clone(),
                                color: theme.foreground,
                                background_color: Some(theme.selection),
                                underline: None,
                                strikethrough: None,
                            });
                            if sel_end < line_text.len() {
                                runs.push(TextRun {
                                    len: line_text.len() - sel_end,
                                    font: font.clone(),
                                    color: theme.foreground,
                                    background_color: None,
                                    underline: None,
                                    strikethrough: None,
                                });
                            }
                        } else {
                            runs.push(TextRun {
                                len: line_text.len(),
                                font: font.clone(),
                                color: theme.foreground,
                                background_color: None,
                                underline: None,
                                strikethrough: None,
                            });
                        }
                    }

                    let text_line = window
                        .text_system()
                        .shape_text(
                            line_text.to_string().into(),
                            font_size,
                            &runs,
                            wrap_width,
                            None,
                        )
                        .ok()
                        .and_then(|mut l| l.pop())
                        .unwrap_or_default();

                    let line_visual_lines = text_line.wrap_boundaries.len() + 1;
                    let line_total_height = line_height * line_visual_lines;

                    // Cursor Quad
                    let cursor_quad = if cursor_point.row == row {
                        let col_in_line = cursor_offset
                            .saturating_sub(line_start_byte)
                            .min(line_text.len());
                        let pos = text_line
                            .position_for_index(col_in_line, line_height)
                            .unwrap_or(point(px(0.0), px(0.0)));
                        let cursor_w = if config.block_cursor {
                            px(8.5)
                        } else {
                            px(2.0)
                        };

                        Some(fill(
                            Bounds::new(
                                point(text_origin_x + pos.x, current_y + pos.y),
                                size(cursor_w, line_height),
                            ),
                            theme.cursor,
                        ))
                    } else {
                        None
                    };

                    // Empty line selection highlight
                    let empty_selection_quad = if line_text.is_empty() {
                        if let Some(sel) = selection {
                            let sel_range = sel.byte_range();
                            if sel_range.start <= line_start_byte && sel_range.end > line_start_byte
                            {
                                Some(fill(
                                    Bounds::new(
                                        point(text_origin_x, current_y),
                                        size(px(6.0), line_height),
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
                        empty_selection_quad,
                        cursor_quad,
                    });

                    current_y += line_total_height;
                }

                EditorCanvasPrepaint {
                    background_quad: fill(bounds, theme.background),
                    line_height,
                    lines,
                }
            },
            move |_bounds, prepaint, window, cx| {
                // Background
                window.paint_quad(prepaint.background_quad);

                for line in prepaint.lines {
                    // Line number
                    if let Some((origin, shaped_num)) = line.gutter_num {
                        let _ = shaped_num.paint(
                            origin,
                            prepaint.line_height,
                            TextAlign::Left,
                            None,
                            window,
                            cx,
                        );
                    }

                    // Selection highlight (under text)
                    let _ = line.text_line.paint_background(
                        line.text_origin,
                        prepaint.line_height,
                        TextAlign::Left,
                        None,
                        window,
                        cx,
                    );
                    if let Some(empty_sel) = line.empty_selection_quad {
                        window.paint_quad(empty_sel);
                    }

                    // Cursor
                    if let Some(cursor_quad) = line.cursor_quad {
                        window.paint_quad(cursor_quad);
                    }

                    // Text
                    let _ = line.text_line.paint(
                        line.text_origin,
                        prepaint.line_height,
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
