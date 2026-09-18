use gpui::{Pixels, Point, TextRun, Window, point, px};
use twrite_core::{FoldRange, Selection};

use crate::canvas::{LineMetrics, RunFonts, build_line_text_runs};

use super::{Editor, FoldEpoch, VisibleLineLayout};

/// Finds the visible line containing vertical position `y` via binary search.
///
/// `lines` is sorted by `top` (constructed in paint order in prepaint).
pub(crate) fn find_visible_line(
    lines: &[VisibleLineLayout],
    y: Pixels,
) -> Option<&VisibleLineLayout> {
    let idx = lines.partition_point(|l| l.top <= y);
    let line = lines.get(idx.checked_sub(1)?)?;
    (y < line.bottom).then_some(line)
}

impl Editor {
    /// Scrolls the viewport upward by a given number of lines.
    pub fn scroll_up(&mut self, count: usize) {
        let target = self.scroll_row.saturating_sub(count);
        self.scroll_row = self.resolve_scroll_row(target);
    }

    /// Scrolls the viewport downward by a given number of lines.
    pub fn scroll_down(&mut self, count: usize) {
        let total_lines = self.buffer.len_lines();
        let target = (self.scroll_row + count).min(total_lines.saturating_sub(1));
        self.scroll_row = self.resolve_scroll_row(target);
    }

    /// Scrolls the viewport by `count` rows and clamps the cursor into the
    /// visible range (Vim `Ctrl+E` / `Ctrl+Y` style).
    ///
    /// Set `scroll_down` to `true` to scroll toward the end of the document,
    /// `false` to scroll toward the beginning.
    ///
    /// If the cursor row is still visible after the scroll it is not moved.
    /// If it scrolled above the viewport the cursor moves to the first visible
    /// row. If it scrolled below the viewport the cursor moves to the last
    /// visible row. The cursor column is preserved and clamped to the target
    /// line length by the buffer.
    ///
    /// Pass `select = true` when Shift is held to extend the selection instead
    /// of collapsing it.
    ///
    /// The caller is responsible for running `on_selection_change` hooks,
    /// `flush_effects`, `sync_search_state`, and `cx.notify()` afterwards,
    /// matching the post-processing every other cursor-moving key path runs.
    pub fn scroll_and_clamp_cursor(&mut self, count: usize, scroll_down: bool, select: bool) {
        if scroll_down {
            self.scroll_down(count);
        } else {
            self.scroll_up(count);
        }

        let total_lines = self.buffer.len_lines();
        if total_lines == 0 {
            return;
        }

        let visible_row_range =
            if !self.visible_lines.is_empty() && self.visible_lines[0].row == self.scroll_row {
                let first = self.visible_lines[0].row;
                let last = self.visible_lines.last().unwrap().row;
                Some(first..=last)
            } else if let Some(bounds) = self.last_bounds {
                let line_height = self.config.line_height;
                let viewport_height = bounds.size.height;
                let visible_row_count = if line_height > px(0.0) {
                    (viewport_height / line_height).floor() as usize
                } else {
                    1
                };
                let first = self.scroll_row;
                let last = (self.scroll_row + visible_row_count.saturating_sub(1))
                    .min(total_lines.saturating_sub(1));
                Some(first..=last)
            } else {
                None
            };

        let Some(visible_rows) = visible_row_range else {
            return;
        };

        let cursor_point = self.buffer.cursor_point();
        let cursor_col = cursor_point.column;

        let target_row = if cursor_point.row < *visible_rows.start() {
            *visible_rows.start()
        } else if cursor_point.row > *visible_rows.end() {
            *visible_rows.end()
        } else {
            return;
        };

        let target_offset = self
            .buffer
            .point_to_offset(twrite_core::Point::new(target_row, cursor_col));
        self.move_cursor_to(target_offset, select);
    }

    /// Moves the cursor to `new_offset`, expanding or creating a selection if `select` is true.
    pub fn move_cursor_to(&mut self, new_offset: usize, select: bool) {
        // Folded rows are never valid cursor rows; pull the target back to
        // its fold header while preserving the column.
        let point = self.buffer.offset_to_point(new_offset);
        let ranges = self.fold_ranges();
        let new_offset = match self.fold_state.hidden_range_at(&ranges, point.row) {
            Some(range) => {
                let current_point = self.buffer.cursor_point();
                let total_lines = self.buffer.len_lines();
                if current_point.row <= range.start_row {
                    let next_visible_row = self.fold_state.step_visible_row(
                        &ranges,
                        range.start_row,
                        total_lines,
                        true,
                    );
                    if next_visible_row > range.start_row {
                        self.buffer
                            .point_to_offset(twrite_core::Point::new(next_visible_row, 0))
                    } else {
                        let header_text = self.buffer.line_to_string(range.start_row);
                        let header_length = header_text.trim_end_matches(['\r', '\n']).len();
                        self.buffer.point_to_offset(twrite_core::Point::new(
                            range.start_row,
                            header_length,
                        ))
                    }
                } else {
                    let header_text = self.buffer.line_to_string(range.start_row);
                    let header_length = header_text.trim_end_matches(['\r', '\n']).len();
                    self.buffer
                        .point_to_offset(twrite_core::Point::new(range.start_row, header_length))
                }
            }
            None => new_offset,
        };
        if select {
            let anchor = self
                .selection
                .map(|s| s.anchor)
                .unwrap_or_else(|| self.buffer.cursor_offset());
            self.buffer.set_cursor_offset(new_offset);
            if anchor != new_offset {
                self.selection = Some(Selection::range(anchor, new_offset));
            } else {
                self.selection = None;
            }
        } else {
            self.buffer.set_cursor_offset(new_offset);
            self.selection = None;
        }
    }

    /// Returns the foldable ranges for the current document version,
    /// recomputing once per version and dropping collapsed starts whose
    /// headers vanished in structural edits.
    pub(crate) fn fold_ranges(&mut self) -> Vec<FoldRange> {
        let version = self.buffer.version();
        if let Some((cached_version, cached_rev, ref ranges)) = self.fold_epoch
            && cached_version == version
            && cached_rev == self.highlighter_rev
        {
            return ranges.clone();
        }
        self.rebuild_fold_epoch()
    }

    /// Recomputes fold ranges unconditionally, returning the fresh set.
    fn rebuild_fold_epoch(&mut self) -> Vec<FoldRange> {
        let ranges = self
            .highlighter
            .as_deref()
            .map(|highlighter| highlighter.foldable_ranges(&self.buffer))
            .unwrap_or_default();
        self.fold_state.retain(&ranges);
        self.fold_epoch = Some((self.buffer.version(), self.highlighter_rev, ranges.clone()));
        ranges
    }

    /// Takes the fold epoch out for paint, which holds no editor borrow
    /// across the frame. Restored via [`Self::restore_fold_epoch`].
    pub(crate) fn take_fold_epoch(&mut self) -> (Vec<FoldRange>, Option<FoldEpoch>) {
        let ranges = self.fold_ranges();
        (ranges, self.fold_epoch.take())
    }

    /// Restores the epoch taken by [`Self::take_fold_epoch`], keeping the
    /// entry only when no edit or highlighter swap landed mid-frame.
    pub(crate) fn restore_fold_epoch(&mut self, epoch: Option<FoldEpoch>) {
        match epoch {
            Some((version, rev, _))
                if version == self.buffer.version() && rev == self.highlighter_rev =>
            {
                self.fold_epoch = epoch;
            }
            _ => {
                self.fold_epoch = None;
            }
        }
    }

    /// Steps one visible row from `row`, skipping collapsed spans for
    /// Up/Down navigation. Stays put with nowhere to go.
    pub(crate) fn step_visible_row(&mut self, row: usize, down: bool) -> usize {
        let ranges = self.fold_ranges();
        self.fold_state
            .step_visible_row(&ranges, row, self.buffer.len_lines(), down)
    }

    /// Collapses the fold starting at the cursor row, if any.
    pub fn collapse_fold_at_cursor(&mut self) {
        let row = self.buffer.cursor_point().row;
        let ranges = self.fold_ranges();
        if ranges
            .iter()
            .any(|range| range.start_row == row && !self.fold_state.is_collapsed(row))
        {
            self.toggle_fold_at_row(row);
        }
    }

    /// Expands any fold starting at or hiding the cursor row, if collapsed.
    pub fn expand_fold_at_cursor(&mut self) {
        let row = self.buffer.cursor_point().row;
        if self.fold_state.is_collapsed(row) {
            self.toggle_fold_at_row(row);
            return;
        }
        let ranges = self.fold_ranges();
        if let Some(range) = self.fold_state.hidden_range_at(&ranges, row) {
            let start = range.start_row;
            self.toggle_fold_at_row(start);
        }
    }

    /// Toggles the fold starting at `header_row`, returning false when the
    /// row starts no foldable range. A swallowed cursor relocates to the
    /// header; a selection touching hidden rows clears so hidden rows never
    /// hold cursor or selection state.
    pub fn toggle_fold_at_row(&mut self, header_row: usize) -> bool {
        let ranges = self.fold_ranges();
        let Some(range) = ranges
            .iter()
            .find(|range| range.start_row == header_row)
            .copied()
        else {
            return false;
        };
        let now_collapsed = self.fold_state.toggle(header_row);
        let cursor_row = self.buffer.cursor_point().row;
        if now_collapsed && range.start_row < cursor_row && cursor_row <= range.end_row {
            let header_text = self.buffer.line_to_string(range.start_row);
            let header_length = header_text.trim_end_matches(['\r', '\n']).len();
            let offset = self
                .buffer
                .point_to_offset(twrite_core::Point::new(range.start_row, header_length));
            self.buffer.set_cursor_offset(offset);
        }
        if let Some(selection) = self.selection {
            let span = selection.byte_range();
            let first = self.buffer.offset_to_point(span.start).row;
            let last = self.buffer.offset_to_point(span.end).row;
            let ranges = self.fold_ranges();
            if (first..=last).any(|row| self.fold_state.is_row_hidden(&ranges, row)) {
                self.selection = None;
            }
        }
        true
    }

    /// Resolves the visible-row target for `requested` scroll row: folded
    /// rows can never anchor the viewport, so a hidden request lands on its
    /// fold header instead.
    pub(crate) fn resolve_scroll_row(&mut self, requested: usize) -> usize {
        let ranges = self.fold_ranges();
        match self.fold_state.hidden_range_at(&ranges, requested) {
            Some(range) => range.start_row,
            None => requested,
        }
    }

    /// Scrolls the viewport so that the cursor is visible.
    ///
    /// Ensures a 1-line margin above and below the cursor when possible.
    pub fn scroll_to_cursor(&mut self, window: Option<&Window>) {
        let total_lines = self.buffer.len_lines();
        if total_lines == 0 {
            self.scroll_row = 0;
            return;
        }

        let cursor_row = self
            .buffer
            .cursor_point()
            .row
            .min(total_lines.saturating_sub(1));

        let margin_lines = 1;
        if cursor_row < self.scroll_row + margin_lines {
            self.scroll_row = cursor_row.saturating_sub(margin_lines);
            return;
        }

        let bounds = match self.last_bounds {
            Some(b) => b,
            None => return,
        };

        let viewport_height = bounds.size.height;
        if viewport_height <= px(0.0) {
            return;
        }

        let line_height = self.config.line_height;
        let margin = line_height * margin_lines as f32;

        if self.config.line_wrap
            && let Some(win) = window
        {
            let gutter_width = if self.config.line_numbers | self.config.relative_line_numbers {
                px(48.0)
            } else {
                px(0.0)
            };
            let wrap_width = Some((bounds.size.width - gutter_width - px(24.0)).max(px(50.0)));
            let font = self.resolved_base_font(&win.text_style().font());

            let get_row_visual_lines = |row: usize| -> usize {
                let raw_line = self.buffer.line_to_string(row);
                let line_text = raw_line.trim_end_matches(['\r', '\n']);
                if line_text.is_empty() {
                    return 1;
                }
                let wraps = self
                    .highlighter
                    .as_deref()
                    .map(|h| h.should_wrap_line(&self.buffer, row))
                    .unwrap_or(true);
                if !wraps {
                    return 1;
                }
                let runs = [TextRun {
                    len: line_text.len(),
                    font: font.clone(),
                    color: self.theme.foreground,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                }];
                win.text_system()
                    .shape_text(
                        line_text.to_string().into(),
                        self.config.font_size,
                        &runs,
                        wrap_width,
                        None,
                    )
                    .ok()
                    .and_then(|mut l| l.pop())
                    .map(|l| l.wrap_boundaries.len() + 1)
                    .unwrap_or(1)
            };

            let mut accumulated = line_height * get_row_visual_lines(cursor_row) as f32;
            let mut new_scroll_row = cursor_row;

            while new_scroll_row > 0 {
                let prev_lines = get_row_visual_lines(new_scroll_row - 1);
                let prev_height = line_height * prev_lines as f32;
                if accumulated + prev_height + margin > viewport_height {
                    break;
                }
                accumulated += prev_height;
                new_scroll_row -= 1;
            }

            if new_scroll_row > self.scroll_row {
                self.scroll_row = new_scroll_row;
            }
        } else {
            let visible_lines = (viewport_height / line_height).floor() as usize;
            let effective_visible = visible_lines.saturating_sub(margin_lines).max(1);

            if cursor_row >= self.scroll_row + effective_visible {
                self.scroll_row = cursor_row.saturating_sub(effective_visible.saturating_sub(1));
            }
        }
    }

    /// Calculates the byte offset in the text buffer corresponding to a window pixel position.
    ///
    /// Shares [`crate::layout_cache::LayoutCache`] inputs with prepaint: the highlight/conceal work
    /// for the target row is a cache hit unless the buffer changed since paint.
    pub fn offset_for_position(&mut self, pos: Point<Pixels>, window: &Window) -> usize {
        let bounds = match self.last_bounds {
            Some(b) => b,
            None => return self.buffer.cursor_offset(),
        };

        let total_lines = self.buffer.len_lines();
        if total_lines == 0 {
            return 0;
        }

        if !self.visible_lines.is_empty() {
            if pos.y < self.visible_lines[0].top {
                return self.visible_lines[0].line_start_byte;
            }

            let (last_row, last_bottom, last_start_byte, last_len_bytes) = {
                let last = self.visible_lines.last().unwrap();
                (
                    last.row,
                    last.bottom,
                    last.line_start_byte,
                    last.line_len_bytes,
                )
            };
            if pos.y >= last_bottom {
                let ranges = self.fold_ranges();
                if let Some(range) = ranges.iter().find(|range| {
                    range.start_row == last_row && self.fold_state.is_collapsed(range.start_row)
                }) {
                    let next_row = range.end_row + 1;
                    if next_row < total_lines {
                        return self
                            .buffer
                            .point_to_offset(twrite_core::Point::new(next_row, 0));
                    }
                    let raw_line = self.buffer.line_to_string(last_row);
                    let line_text = raw_line.trim_end_matches(['\r', '\n']);
                    return last_start_byte + line_text.len();
                }
                return (last_start_byte + last_len_bytes).min(self.buffer.len_bytes());
            }

            let target_line = match find_visible_line(&self.visible_lines, pos.y) {
                Some(line) => line,
                None => self.visible_lines.last().unwrap(),
            };

            let row = target_line.row;
            let line_start_byte = target_line.line_start_byte;
            let task_state = target_line.task_state;
            let text_origin_x = target_line.text_origin_x;
            let line_top = target_line.top;
            let raw_line = self.buffer.line_to_string(row);
            let line_text = raw_line.trim_end_matches(['\r', '\n']);

            if line_text.is_empty() || pos.x <= text_origin_x {
                return line_start_byte;
            }

            let base_font_size = self.config.font_size;
            let base_line_height = self.config.line_height;
            let cursor_position = self.buffer.cursor_point();
            let highlighter_rev = self.highlighter_rev;
            let host_font = window.text_style().font();
            let font = self.resolved_base_font(&host_font);
            let code_font = self.resolved_code_font(&host_font);
            let cached = self.layout_cache.cached_input(
                &self.buffer,
                self.highlighter.as_deref(),
                highlighter_rev,
                cursor_position,
                row,
                line_text,
            );
            let concealed = &cached.concealed;

            // Mirror paint: headings shape at a scaled font size, so hit-test
            // must use the same metrics or clicks drift on concealed lines.
            let metrics = LineMetrics::for_line(
                line_text,
                &concealed.display_text,
                &cached.spans,
                base_font_size,
                base_line_height,
            );

            let is_checked_task =
                task_state == Some(true) && line_text.len() != concealed.display_text.len();

            let fonts = RunFonts {
                base: &font,
                code: &code_font,
            };
            let runs = build_line_text_runs(
                &concealed.display_text,
                &concealed.spans,
                None,
                &fonts,
                &self.theme,
                metrics.is_code_block,
                is_checked_task,
            );

            let wrap_width = if self.config.line_wrap && cached.allow_wrap {
                let available = bounds.size.width - (text_origin_x - bounds.left()) - px(12.0);
                Some(available.max(px(50.0)))
            } else {
                None
            };

            let text_line = window
                .text_system()
                .shape_text(
                    concealed.display_text.clone().into(),
                    metrics.font_size,
                    &runs,
                    wrap_width,
                    None,
                )
                .ok()
                .and_then(|mut l| l.pop())
                .unwrap_or_default();

            let line_rel_y = (pos.y - line_top).max(px(0.0));
            let line_rel_x = (pos.x - text_origin_x).max(px(0.0));
            let rel_pos = point(line_rel_x, line_rel_y);

            let col_display = text_line
                .closest_index_for_position(rel_pos, metrics.line_height)
                .unwrap_or_else(|idx| idx);

            let col_src = concealed.display_to_source(col_display);
            return line_start_byte + col_src.min(line_text.len());
        }

        0
    }

    /// Returns the window pixel coordinates (X, Y) at the bottom of the active cursor.
    ///
    /// This value is automatically computed and cached during each canvas render pass.
    /// Returns `None` if the editor has not yet been rendered, or if the cursor is scrolled
    /// outside the visible viewport.
    pub fn cursor_pixel_position(&self) -> Option<Point<Pixels>> {
        self.last_cursor_pixel
    }

    /// Returns the target URL if `pos` is over a hyperlink.
    pub fn link_at_position(&self, pos: Point<Pixels>) -> Option<String> {
        let bounds = self.last_bounds?;
        if !bounds.contains(&pos) {
            return None;
        }

        if let Some(line) = find_visible_line(&self.visible_lines, pos.y) {
            for link in &line.links {
                if link.bounds.contains(&pos) {
                    return Some(link.url.clone());
                }
            }
        }

        None
    }

    pub(crate) fn is_position_over_task_checkbox(
        &self,
        pos: Point<Pixels>,
        _window: &Window,
    ) -> bool {
        let bounds = match self.last_bounds {
            Some(b) => b,
            None => return false,
        };

        if !bounds.contains(&pos) {
            return false;
        }

        if let Some(line) = find_visible_line(&self.visible_lines, pos.y)
            && line.is_task_checkbox
        {
            return pos.x >= line.checkbox_box_x && pos.x <= line.checkbox_box_x + px(22.0);
        }

        false
    }

    /// Reports whether `pos` sits on a fold indicator in the gutter of a
    /// foldable row or on a collapsed row's inline badge. Used for click
    /// handling and hover cursor styling.
    pub(crate) fn fold_indicator_at_position(&mut self, pos: Point<Pixels>) -> Option<usize> {
        let bounds = self.last_bounds?;
        if !bounds.contains(&pos) {
            return None;
        }
        let (row, text_origin_x, indicator_bounds) = {
            let line = find_visible_line(&self.visible_lines, pos.y)?;
            (line.row, line.text_origin_x, line.fold_indicator_bounds)
        };
        // The indicator lives left of the text origin in the gutter column,
        // or as an inline indicator ("...") near the title when collapsed.
        if pos.x <= text_origin_x - px(4.0) {
            let ranges = self.fold_ranges();
            if ranges.iter().any(|range| range.start_row == row) {
                return Some(row);
            }
        }
        if let Some(indicator_bounds) = indicator_bounds
            && indicator_bounds.contains(&pos)
        {
            return Some(row);
        }
        None
    }

    /// Toggles the fold whose indicator sits under `pos`, returning true when
    /// a fold consumed the click. Visible-row repaint follows from notify.
    pub fn toggle_fold_at_position(&mut self, pos: Point<Pixels>) -> bool {
        match self.fold_indicator_at_position(pos) {
            Some(row) => self.toggle_fold_at_row(row),
            None => false,
        }
    }
}
