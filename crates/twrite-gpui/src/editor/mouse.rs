use gpui::{
    Context, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ScrollDelta, ScrollWheelEvent, Window,
};
use twrite_core::{HookContext, HookOutcome, Selection};

use super::geometry::find_visible_line;
use super::{Editor, SelectionGranularity};

/// What a click on a hyperlink does when no hook claimed it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LinkClickFallback {
    OpenInBrowser,
    PlaceCursor,
}

/// Decides the fallback for a click on `url` that no hook consumed.
/// Callers dispatch to hooks first and only consult this afterwards.
/// Internal links stay in the editor for cursor placement while anything
/// else opens in the browser. The literal prefix mirrors
/// `twrite_core::WIKILINK_SCHEME` without requiring the markdown feature.
fn resolve_unclaimed_link_click(url: &str) -> LinkClickFallback {
    if url.starts_with("wikilink:") {
        LinkClickFallback::PlaceCursor
    } else {
        LinkClickFallback::OpenInBrowser
    }
}

impl Editor {
    pub(crate) fn handle_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A left-click outside the menu dismisses it, then proceeds
        // normally so the click still moves the cursor.
        self.dismiss_context_menu(cx);

        #[cfg(not(feature = "gpui-latest-api"))]
        {
            self.focus_handle.focus(window);
        }
        #[cfg(feature = "gpui-latest-api")]
        {
            self.focus_handle.focus(window, cx);
        }

        self.reset_blink_cursor(cx);

        // An open completion popup owns its clicks: rows activate,
        // anything else dismisses the session and falls through so the
        // click still moves the cursor.
        if let Some(rect) = self.completion_popup_rect() {
            if rect.contains(&event.position) {
                let snapshot = self
                    .hooks
                    .iter()
                    .find_map(|hook| hook.completion_snapshot());
                let count = snapshot.as_ref().map(|s| s.items.len()).unwrap_or(0);
                if let Some(index) = super::completion::completion_row_at_position(
                    event.position,
                    rect.origin,
                    count,
                ) {
                    self.dispatch_completion_select(index, Some(window), cx);
                    return;
                }
            } else {
                for hook in &mut self.hooks {
                    hook.dismiss_completion();
                }
            }
        }

        if event.click_count == 1
            && !event.modifiers.shift
            && let Some(url) = self.link_at_position(event.position)
        {
            // Link clicks belong to hooks first, whatever the scheme: a
            // custom language claims its ranges here instead of borrowing
            // another scheme's prefix. Unclaimed destinations keep their
            // previous path via the fallback below.
            let clicked = find_visible_line(&self.visible_lines, event.position.y)
                .map(|visible| (visible.row, visible.line_start_byte, visible.line_len_bytes));
            if let Some((row, line_start_byte, line_len_bytes)) = clicked {
                let offset = self.offset_for_position(event.position, window);
                let column = offset.saturating_sub(line_start_byte).min(line_len_bytes);
                let initial_version = self.buffer.version();
                let mut consumed = false;
                let mut hook_index = 0;
                while hook_index < self.hooks.len() {
                    let mut ctx = HookContext::new(
                        &mut self.buffer,
                        &mut self.selection,
                        &mut self.cursor_style,
                        &mut self.prompt,
                        &mut self.pending_effects,
                    );
                    if self.hooks[hook_index].on_click(&mut ctx, row, column)
                        == HookOutcome::Consumed
                    {
                        consumed = true;
                        break;
                    }
                    hook_index += 1;
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
            match resolve_unclaimed_link_click(&url) {
                LinkClickFallback::OpenInBrowser => {
                    cx.open_url(&url);
                    return;
                }
                LinkClickFallback::PlaceCursor => {}
            }
        }

        if event.click_count == 1 && !event.modifiers.shift {
            // Fold disclosure toggles before cursor placement so gutter
            // clicks never move the cursor into a hidden row.
            if self.toggle_fold_at_position(event.position) {
                self.selection = None;
                self.is_selecting = false;
                self.is_hovering_fold = self.fold_indicator_at_position(event.position).is_some();
                self.flush_effects();
                cx.notify();
                return;
            }

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
            let hovering_fold = self.fold_indicator_at_position(event.position).is_some();
            let link_changed = hovered_link != self.hovered_link;
            let task_changed = hovering != self.is_hovering_task;
            let fold_changed = hovering_fold != self.is_hovering_fold;

            if task_changed || link_changed || fold_changed {
                self.is_hovering_task = hovering;
                self.hovered_link = hovered_link;
                self.is_hovering_fold = hovering_fold;
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
        let hovering_fold = self.fold_indicator_at_position(event.position).is_some();
        let link_changed = hovered_link != self.hovered_link;
        let task_changed = hovering != self.is_hovering_task;
        let fold_changed = hovering_fold != self.is_hovering_fold;

        if task_changed || link_changed || fold_changed {
            self.is_hovering_task = hovering;
            self.hovered_link = hovered_link;
            self.is_hovering_fold = hovering_fold;
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
        self.dismiss_context_menu(cx);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    use gpui::{Bounds, Modifiers, MouseButton, Pixels, Point, TestAppContext, point, px, size};
    use twrite_core::{EditorHook, HookContext, HookEffect, HookOutcome};

    use crate::editor::{VisibleLineLayout, VisibleLink};

    #[test]
    fn unclaimed_plain_link_opens_browser() {
        assert_eq!(
            resolve_unclaimed_link_click("https://example.com"),
            LinkClickFallback::OpenInBrowser
        );
    }

    #[test]
    fn unclaimed_custom_scheme_link_opens_browser() {
        assert_eq!(
            resolve_unclaimed_link_click("story:inspect"),
            LinkClickFallback::OpenInBrowser
        );
    }

    #[test]
    fn unclaimed_internal_link_places_cursor() {
        assert_eq!(
            resolve_unclaimed_link_click("wikilink:Note"),
            LinkClickFallback::PlaceCursor
        );
    }

    struct RecordingHook {
        clicks: Rc<RefCell<Vec<(usize, usize)>>>,
        consume: bool,
    }

    impl EditorHook for RecordingHook {
        fn on_click(&mut self, ctx: &mut HookContext, row: usize, col: usize) -> HookOutcome {
            self.clicks.borrow_mut().push((row, col));
            if self.consume {
                ctx.effects.push(HookEffect::FollowLink {
                    target: "inspect".into(),
                    fragment: None,
                    label: None,
                });
                HookOutcome::Consumed
            } else {
                HookOutcome::PassThrough
            }
        }
    }

    fn visible_layout_with_link(url: &str) -> (Bounds<Pixels>, Vec<VisibleLineLayout>) {
        let window_bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(800.0), px(600.0)));
        let link_bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(40.0), px(22.0)));
        let lines = vec![VisibleLineLayout {
            row: 0,
            top: px(0.0),
            bottom: px(22.0),
            line_start_byte: 0,
            line_len_bytes: 20,
            text_origin_x: px(50.0),
            line_height: px(22.0),
            is_task_checkbox: false,
            checkbox_box_x: px(0.0),
            task_state: None,
            links: vec![VisibleLink {
                bounds: link_bounds,
                url: url.into(),
            }],
            fold_indicator_bounds: None,
        }];
        (window_bounds, lines)
    }

    fn click_at(position: Point<Pixels>) -> MouseDownEvent {
        MouseDownEvent {
            button: MouseButton::Left,
            position,
            modifiers: Modifiers::default(),
            click_count: 1,
            ..Default::default()
        }
    }

    #[gpui::test]
    async fn custom_scheme_click_reaches_hooks_before_browser(cx: &mut TestAppContext) {
        let recorded: Rc<RefCell<Vec<(usize, usize)>>> = Rc::default();
        let hook = RecordingHook {
            clicks: Rc::clone(&recorded),
            consume: true,
        };
        let (window_bounds, lines) = visible_layout_with_link("story:inspect");
        let (editor, test_cx) = cx.add_window_view(|_, cx| Editor::new("! Inspect the crates", cx));
        test_cx.update(|_window, app| {
            editor.update(app, |editor, _| {
                editor.hooks.push(Box::new(hook));
                editor.last_bounds = Some(window_bounds);
                editor.visible_lines = lines;
            })
        });
        let event = click_at(point(px(10.0), px(11.0)));
        editor.update_in(&mut *test_cx, |editor, window, cx| {
            editor.handle_mouse_down(&event, window, cx);
        });
        assert_eq!(*recorded.borrow(), vec![(0, 0)]);
        assert_eq!(test_cx.opened_url(), None);
        let effects =
            test_cx.update(|_window, app| editor.update(app, |editor, _| editor.take_effects()));
        assert_eq!(
            effects,
            vec![HookEffect::FollowLink {
                target: "inspect".into(),
                fragment: None,
                label: None,
            }]
        );
    }

    #[gpui::test]
    async fn unclaimed_plain_click_still_opens_browser(cx: &mut TestAppContext) {
        let recorded: Rc<RefCell<Vec<(usize, usize)>>> = Rc::default();
        let hook = RecordingHook {
            clicks: Rc::clone(&recorded),
            consume: false,
        };
        let (window_bounds, lines) = visible_layout_with_link("https://example.com");
        let (editor, test_cx) = cx.add_window_view(|_, cx| Editor::new("! Inspect the crates", cx));
        test_cx.update(|_window, app| {
            editor.update(app, |editor, _| {
                editor.hooks.push(Box::new(hook));
                editor.last_bounds = Some(window_bounds);
                editor.visible_lines = lines;
            })
        });
        let event = click_at(point(px(10.0), px(11.0)));
        editor.update_in(&mut *test_cx, |editor, window, cx| {
            editor.handle_mouse_down(&event, window, cx);
        });
        assert_eq!(*recorded.borrow(), vec![(0, 0)]);
        assert_eq!(
            test_cx.opened_url(),
            Some("https://example.com".to_string())
        );
    }
}
