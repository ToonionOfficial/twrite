use gpui::prelude::*;
use gpui::{Context, Div, MouseButton, MouseDownEvent, Pixels, Point, Stateful, Window, div, px};
use twrite_core::{
    ContextMenuCaps, ContextMenuContext, ContextMenuItem, HookContext, HookOutcome, Selection,
    collect_context_items,
};

use super::Editor;

impl Editor {
    /// Opens the expandable right-click menu at the click position.
    ///
    /// Selection policy (VS Code style): a click inside the active
    /// selection keeps it; otherwise the cursor moves to the click and
    /// any selection collapses. Items merge built-in edit rows with
    /// hook-contributed rows (see [`collect_context_items`]).
    pub(crate) fn handle_right_click(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.config.context_menu {
            return;
        }
        self.focus_handle.focus(window, cx);
        self.reset_blink_cursor(cx);

        let offset = self.offset_for_position(event.position, window);
        let keep_selection = self
            .selection
            .is_some_and(|sel| !sel.byte_range().is_empty() && sel.byte_range().contains(&offset));
        if !keep_selection {
            self.buffer.set_cursor_offset(offset);
            self.selection = None;
            for hook in &mut self.hooks {
                hook.on_selection_change(&self.buffer, self.selection.as_ref());
            }
        }

        let point = self.buffer.offset_to_point(offset);
        let line_start = self
            .buffer
            .point_to_offset(twrite_core::Point::new(point.row, 0));
        let clicked_col = offset.saturating_sub(line_start);

        let clipboard_has_text = cx
            .read_from_clipboard()
            .and_then(|item| item.text())
            .is_some_and(|text| !text.is_empty());
        let caps =
            ContextMenuCaps::from_buffer(&self.buffer, self.selection.as_ref(), clipboard_has_text);

        let mut hook_rows = Vec::with_capacity(self.hooks.len());
        for hook in self.hooks.iter() {
            let menu_ctx = ContextMenuContext {
                buffer: &self.buffer,
                selection: self.selection.as_ref(),
                cursor_offset: self.buffer.cursor_offset(),
                clicked_row: point.row,
                clicked_col,
                caps,
            };
            hook_rows.push(hook.context_menu_items(&menu_ctx));
        }
        let items = collect_context_items(self.config.show_default_menu_items, caps, hook_rows);
        if items.is_empty() {
            self.dismiss_context_menu(cx);
            return;
        }

        self.context_menu.open(items);
        self.context_menu_anchor = Some(event.position);
        self.context_menu_selected = None;
        self.flush_effects();
        cx.notify();
    }

    /// Runs a menu row activation: hooks first, then built-in edit actions.
    pub(crate) fn dispatch_context_menu_action(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.reset_blink_cursor(cx);
        let enabled = self
            .context_menu
            .items()
            .iter()
            .find(|item| item.id == id)
            .is_some_and(|item| item.enabled);
        if !enabled {
            return;
        }

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
            if self.hooks[hook_idx].on_context_menu_action(&mut ctx, id) == HookOutcome::Consumed {
                consumed = true;
                break;
            }
            hook_idx += 1;
        }

        let mut edited = false;
        if !consumed {
            edited = self.run_builtin_menu_action(id, cx);
        } else if self.buffer.version() != initial_version {
            edited = true;
        }

        if edited {
            for hook in &mut self.hooks {
                hook.after_edit(&mut self.buffer);
            }
        }
        for hook in &mut self.hooks {
            hook.on_selection_change(&self.buffer, self.selection.as_ref());
        }

        self.context_menu.close();
        self.context_menu_anchor = None;
        self.context_menu_selected = None;
        self.scroll_to_cursor(Some(window));
        self.flush_effects();
        self.sync_search_state();
        cx.notify();
    }

    /// Executes a well-known edit id. Returns whether the buffer changed.
    /// Unknown ids are ignored (a hook was expected to consume them).
    fn run_builtin_menu_action(&mut self, id: &str, cx: &mut Context<Self>) -> bool {
        match id {
            twrite_core::UNDO_ID => {
                if self.buffer.can_undo() {
                    self.buffer.undo();
                    self.selection = None;
                    true
                } else {
                    false
                }
            }
            twrite_core::REDO_ID => {
                if self.buffer.can_redo() {
                    self.buffer.redo();
                    self.selection = None;
                    true
                } else {
                    false
                }
            }
            twrite_core::CUT_ID => self.cut(cx),
            twrite_core::COPY_ID => {
                self.copy(cx);
                false
            }
            twrite_core::PASTE_ID => self.paste(cx),
            twrite_core::DELETE_ID => {
                if self.delete_selection() {
                    self.selection = None;
                    true
                } else {
                    false
                }
            }
            twrite_core::SELECT_ALL_ID => {
                self.selection = Some(Selection::range(0, self.buffer.len_bytes()));
                false
            }
            _ => false,
        }
    }

    /// Closes the menu without running an action.
    pub(crate) fn dismiss_context_menu(&mut self, cx: &mut Context<Self>) {
        if self.context_menu.is_open() {
            self.context_menu.close();
            self.context_menu_anchor = None;
            self.context_menu_selected = None;
            cx.notify();
        }
    }

    /// Moves keyboard selection to the next / previous enabled row.
    pub(crate) fn move_context_menu_selection(&mut self, forward: bool, cx: &mut Context<Self>) {
        let items = self.context_menu.items();
        if items.is_empty() {
            return;
        }
        let len = items.len();
        let mut idx = self
            .context_menu_selected
            .unwrap_or(if forward { len - 1 } else { 0 });
        for _ in 0..len {
            idx = if forward {
                (idx + 1) % len
            } else {
                idx.checked_sub(1).unwrap_or(len - 1)
            };
            if items[idx].enabled {
                self.context_menu_selected = Some(idx);
                cx.notify();
                return;
            }
        }
    }

    /// Activates the keyboard-selected row, if any.
    pub(crate) fn activate_context_menu_selected(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(idx) = self.context_menu_selected
            && let Some(item) = self.context_menu.items().get(idx).cloned()
        {
            let id: &'static str = item.id;
            self.dispatch_context_menu_action(id, window, cx);
        }
    }

    /// Renders the open menu as an absolutely-positioned overlay.
    ///
    /// The anchor is clamped into `last_bounds` using the same fixed
    /// metrics the rows are drawn with, so the popup never overflows
    /// the viewport. Renders nothing when closed.
    pub(crate) fn render_context_menu(&self, cx: &mut Context<Self>) -> Div {
        if !self.context_menu.is_open() {
            return div();
        }
        const MENU_WIDTH: f32 = 230.0;
        const ROW_HEIGHT: f32 = 28.0;
        const DIVIDER_HEIGHT: f32 = 9.0;

        let items = self.context_menu.items();
        let mut height: f32 = 8.0;
        for item in items {
            height += ROW_HEIGHT;
            if item.divider_after {
                height += DIVIDER_HEIGHT;
            }
        }
        height += 8.0;

        let anchor = self
            .context_menu_anchor
            .unwrap_or(gpui::point(px(0.0), px(0.0)));
        let pos = match self.last_bounds {
            Some(bounds) => clamp_menu_anchor(anchor, bounds, MENU_WIDTH, height),
            None => anchor,
        };

        let mut list = div().flex().flex_col().py(px(4.0));
        for (idx, item) in items.iter().enumerate() {
            list = list.child(self.render_menu_row(
                item,
                idx,
                Some(idx) == self.context_menu_selected,
                cx,
            ));
            if item.divider_after {
                list = list.child(
                    div()
                        .mx(px(8.0))
                        .my(px(4.0))
                        .h(px(1.0))
                        .bg(self.theme.menu_border),
                );
            }
        }

        div()
            .absolute()
            .left(pos.x)
            .top(pos.y)
            .w(px(MENU_WIDTH))
            .rounded_md()
            .border_1()
            .border_color(self.theme.menu_border)
            .bg(self.theme.menu_bg)
            .shadow_lg()
            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                cx.stop_propagation();
            })
            .child(list)
    }

    fn render_menu_row(
        &self,
        item: &ContextMenuItem,
        idx: usize,
        highlighted: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let id: &'static str = item.id;
        let enabled = item.enabled;
        let (fg, hint_color) = if enabled {
            (self.theme.menu_fg, self.theme.menu_hint)
        } else {
            (self.theme.menu_hint, self.theme.menu_hint)
        };
        let mut row = div()
            .id(("context-menu-item", idx))
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .h(px(28.0))
            .px(px(12.0))
            .gap(px(16.0))
            .text_sm()
            .text_color(fg);

        if enabled {
            row = row.cursor_pointer();
            if highlighted {
                row = row.bg(self.theme.menu_hover);
            } else {
                row = row.hover(|s| s.bg(self.theme.menu_hover));
            }
        }

        if let Some(hint) = &item.hint {
            row = row
                .child(item.label.clone())
                .child(div().text_xs().text_color(hint_color).child(hint.clone()));
        } else {
            row = row.child(item.label.clone());
        }

        if !enabled {
            return row;
        }
        row.on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                this.dispatch_context_menu_action(id, window, cx);
                cx.stop_propagation();
            }),
        )
    }
}

/// Clamps a raw anchor into bounds using menu metrics (pure, testable).
pub(crate) fn clamp_menu_anchor(
    anchor: Point<Pixels>,
    bounds: gpui::Bounds<Pixels>,
    menu_width: f32,
    menu_height: f32,
) -> Point<Pixels> {
    gpui::point(
        anchor
            .x
            .min(bounds.right() - px(menu_width + 8.0))
            .max(bounds.left()),
        anchor
            .y
            .min(bounds.bottom() - px(menu_height + 8.0))
            .max(bounds.top()),
    )
}

#[cfg(test)]
mod tests {
    use super::clamp_menu_anchor;
    use gpui::{Bounds, point, px, size};

    #[test]
    fn anchor_clamps_into_bounds() {
        let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(800.0), px(600.0)));
        let clamped = clamp_menu_anchor(point(px(790.0), px(590.0)), bounds, 230.0, 200.0);
        assert_eq!(clamped, point(px(562.0), px(392.0)));
        let inside = clamp_menu_anchor(point(px(100.0), px(100.0)), bounds, 230.0, 200.0);
        assert_eq!(inside, point(px(100.0), px(100.0)));
    }
}
