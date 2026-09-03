use gpui::*;
use twrite_core::Point as BufferPoint;

use crate::editor::Editor;

/// Data computed during prepaint for each visible line
struct PreparedLine {
    gutter_num: Option<(Point<Pixels>, ShapedLine)>,
    text_origin: Point<Pixels>,
    text_line: ShapedLine,
    selection_quad: Option<PaintQuad>,
    cursor_quad: Option<PaintQuad>,
}

/// The state struct (T) passed from `prepaint` to `paint`.
struct EditorCanvasPrepaint {
    background_quad: PaintQuad,
    line_height: Pixels,
    lines: Vec<PreparedLine>,
}

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

                let total_lines = editor.buffer.len_lines();
                let scroll_row = editor.scroll_row;

                // Only process visible lines
                let visible_count = (bounds.size.height / line_height).ceil() as usize + 1;
                let start_row = scroll_row;
                let end_row = (start_row + visible_count).min(total_lines);

                let cursor_offset = editor.buffer.cursor_offset();
                let cursor_point = editor.buffer.cursor_point();
                let selection = editor.selection;

                let mut lines: Vec<PreparedLine> = Vec::with_capacity(end_row - start_row);

                for row in start_row..end_row {
                    let line_y = bounds.top() + (row - start_row) * line_height;
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

                        Some((point(bounds.left() + px(8.0), line_y), shaped))
                    } else {
                        None
                    };

                    // Shape line content
                    let runs = if line_text.is_empty() {
                        Vec::new()
                    } else {
                        vec![TextRun {
                            len: line_text.len(),
                            font: font.clone(),
                            color: theme.foreground,
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        }]
                    };

                    let text_line = window.text_system().shape_line(
                        line_text.to_string().into(),
                        font_size,
                        &runs,
                        None,
                    );

                    // Selection Quad
                    let selection_quad = selection.and_then(|sel| {
                        let selection_range = sel.byte_range();

                        if selection_range.end > line_start_byte
                            && selection_range.start < line_end_byte
                        {
                            let selection_line_start = selection_range
                                .start
                                .saturating_sub(line_start_byte)
                                .min(line_text.len());
                            let selection_line_end =
                                (selection_range.end - line_start_byte).min(line_text.len());

                            if selection_line_end > selection_line_start {
                                let selection_x_start = text_line.x_for_index(selection_line_start);
                                let selection_x_end = text_line.x_for_index(selection_line_end);

                                return Some(fill(
                                    Bounds::new(
                                        point(text_origin_x + selection_x_start, line_y),
                                        size(selection_x_end - selection_x_start, line_height),
                                    ),
                                    theme.selection,
                                ));
                            }
                        }
                        None
                    });

                    // Cursor Quad
                    let cursor_quad = if cursor_point.row == row {
                        let col_in_line = cursor_offset
                            .saturating_sub(line_start_byte)
                            .min(line_text.len());
                        let cursor_x = text_line.x_for_index(col_in_line);
                        let cursor_w = if config.block_cursor {
                            px(8.5)
                        } else {
                            px(2.0)
                        };

                        Some(fill(
                            Bounds::new(
                                point(text_origin_x + cursor_x, line_y),
                                size(cursor_w, line_height),
                            ),
                            theme.cursor,
                        ))
                    } else {
                        None
                    };

                    lines.push(PreparedLine {
                        gutter_num,
                        text_origin: point(text_origin_x, line_y),
                        text_line,
                        selection_quad,
                        cursor_quad,
                    });
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
                    if let Some(sel_quad) = line.selection_quad {
                        window.paint_quad(sel_quad);
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
