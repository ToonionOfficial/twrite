use gpui::{
    Context, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ScrollDelta, ScrollWheelEvent, Window,
};
use twrite_core::{HookContext, HookOutcome, Selection};

use super::geometry::find_visible_line;
use super::{Editor, SelectionGranularity};

impl Editor {
    pub(crate) fn handle_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_handle.focus(window, cx);
        self.reset_blink_cursor(cx);

        if event.click_count == 1
            && !event.modifiers.shift
            && let Some(url) = self.link_at_position(event.position)
        {
            cx.open_url(&url);
            return;
        }

        if event.click_count == 1 && !event.modifiers.shift {
            let clicked = find_visible_line(&self.visible_lines, event.position.y)
                .map(|l| (l.row, l.line_start_byte, l.line_len_bytes));

            if let Some((row, line_start_byte, line_len_bytes)) = clicked
                && self.is_position_over_task_checkbox(event.position, window)
            {
                let offset = self.offset_for_position(event.position, window);
                let col = offset.saturating_sub(line_start_byte).min(line_len_bytes);
                let initial_version = self.buffer.version();
                let mut consumed = false;
                let mut hook_idx = 0;
                while hook_idx < self.hooks.len() {
                    let mut ctx = HookContext::new(
                        &mut self.buffer,
                        &mut self.selection,
                        &mut self.cursor_style,
                        &mut self.prompt,
                        &mut self.pending_effects,
                    );
                    if self.hooks[hook_idx].on_click(&mut ctx, row, col) == HookOutcome::Consumed {
                        consumed = true;
                        break;
                    }
                    hook_idx += 1;
                }
                if consumed {
                    if self.buffer.version() != initial_version {
                        for hook in &mut self.hooks {
                            hook.after_edit(&mut self.buffer);
                        }
                    }
                    for hook in &mut self.hooks {
                        hook.on_selection_change(&self.buffer, self.selection.as_ref());
                    }
                    self.selection = None;
                    self.is_selecting = false;
                    self.flush_effects();
                    cx.notify();
                    return;
                }
            }
        }

        let offset = self.offset_for_position(event.position, window);
        self.is_selecting = true;

        if event.click_count == 2 {
            let word = self.buffer.word_range_at(offset);
            self.selection_granularity = SelectionGranularity::Word;
            self.drag_initial_range = Some(word.clone());
            self.buffer.set_cursor_offset(word.end);
            self.selection = Some(Selection::range(word.start, word.end));
        } else if event.click_count >= 3 {
            let line = self.buffer.line_range_at(offset);
            self.selection_granularity = SelectionGranularity::Line;
            self.drag_initial_range = Some(line.clone());
            self.buffer.set_cursor_offset(line.end);
            self.selection = Some(Selection::range(line.start, line.end));
        } else {
            self.selection_granularity = SelectionGranularity::Character;
            self.drag_initial_range = None;
            self.buffer.set_cursor_offset(offset);

            if event.modifiers.shift {
                if let Some(sel) = self.selection {
                    self.selection = Some(Selection::range(sel.anchor, offset));
                } else {
                    self.selection = Some(Selection::point(offset));
                }
            } else {
                self.selection = Some(Selection::point(offset));
            }
        }

        for hook in &mut self.hooks {
            hook.on_selection_change(&self.buffer, self.selection.as_ref());
        }

        self.scroll_to_cursor(Some(window));
        self.flush_effects();
        cx.notify();
    }

    pub(crate) fn handle_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_selecting {
            self.reset_blink_cursor(cx);
            let offset = self.offset_for_position(event.position, window);
            match self.selection_granularity {
                SelectionGranularity::Character => {
                    if self.buffer.cursor_offset() != offset || self.selection.is_none() {
                        self.buffer.set_cursor_offset(offset);

                        if let Some(sel) = self.selection {
                            self.selection = Some(Selection::range(sel.anchor, offset));
                        } else {
                            self.selection = Some(Selection::point(offset));
                        }

                        for hook in &mut self.hooks {
                            hook.on_selection_change(&self.buffer, self.selection.as_ref());
                        }
                        self.scroll_to_cursor(Some(window));
                        cx.notify();
                    }
                }
                SelectionGranularity::Word => {
                    let initial = self.drag_initial_range.clone().unwrap_or(offset..offset);
                    let word = self.buffer.word_range_at(offset);
                    let (anchor, head) = if offset >= initial.start {
                        (initial.start, word.end.max(initial.end))
                    } else {
                        (initial.end, word.start)
                    };
                    let new_sel = Selection::range(anchor, head);
                    if self.selection != Some(new_sel) {
                        self.selection = Some(new_sel);
                        self.buffer.set_cursor_offset(head);
                        for hook in &mut self.hooks {
                            hook.on_selection_change(&self.buffer, self.selection.as_ref());
                        }
                        self.scroll_to_cursor(Some(window));
                        cx.notify();
                    }
                }
                SelectionGranularity::Line => {
                    let initial = self.drag_initial_range.clone().unwrap_or(offset..offset);
                    let line = self.buffer.line_range_at(offset);
                    let (anchor, head) = if offset >= initial.start {
                        (initial.start, line.end.max(initial.end))
                    } else {
                        (initial.end, line.start)
                    };
                    let new_sel = Selection::range(anchor, head);
                    if self.selection != Some(new_sel) {
                        self.selection = Some(new_sel);
                        self.buffer.set_cursor_offset(head);
                        for hook in &mut self.hooks {
                            hook.on_selection_change(&self.buffer, self.selection.as_ref());
                        }
                        self.scroll_to_cursor(Some(window));
                        cx.notify();
                    }
                }
            }
        } else {
            let hovering = self.is_position_over_task_checkbox(event.position, window);
            let hovered_link = self.link_at_position(event.position);
            let link_changed = hovered_link != self.hovered_link;
            let task_changed = hovering != self.is_hovering_task;

            if task_changed || link_changed {
                self.is_hovering_task = hovering;
                self.hovered_link = hovered_link;
                cx.notify();
            }
        }
    }

    pub(crate) fn handle_mouse_up(
        &mut self,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.is_selecting = false;
        self.drag_initial_range = None;
        self.selection_granularity = SelectionGranularity::Character;
        if let Some(sel) = self.selection
            && sel.is_empty()
        {
            self.selection = None;
        }
        for hook in &mut self.hooks {
            hook.on_selection_change(&self.buffer, self.selection.as_ref());
        }
        let hovering = self.is_position_over_task_checkbox(event.position, window);
        let hovered_link = self.link_at_position(event.position);
        let link_changed = hovered_link != self.hovered_link;
        let task_changed = hovering != self.is_hovering_task;

        if task_changed || link_changed {
            self.is_hovering_task = hovering;
            self.hovered_link = hovered_link;
        }
        self.flush_effects();
        cx.notify();
    }

    pub(crate) fn handle_scroll_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.reset_blink_cursor(cx);
        let delta_lines = match event.delta {
            ScrollDelta::Lines(delta) => -delta.y,
            ScrollDelta::Pixels(delta) => -(delta.y / self.config.line_height),
        };

        let total_lines = self.buffer.len_lines();
        if delta_lines > 0.0 {
            let count = delta_lines.round() as usize;
            self.scroll_row = (self.scroll_row + count).min(total_lines.saturating_sub(1));
        } else if delta_lines < 0.0 {
            let count = (-delta_lines).round() as usize;
            self.scroll_row = self.scroll_row.saturating_sub(count);
        }

        cx.notify();
    }
}
