use gpui::prelude::*;
use gpui::{
    Bounds, Context, Div, MouseButton, MouseDownEvent, Pixels, Point, Stateful, Window, div, px,
    size,
};
use twrite_core::{HookContext, HookOutcome};

use super::Editor;
use super::context_menu::{clamp_menu_anchor, menu_overlay_position};

/// Fixed completion popup metrics, mirroring the context menu rows.
pub(crate) const COMPLETION_WIDTH: f32 = 260.0;
pub(crate) const COMPLETION_ROW_HEIGHT: f32 = 28.0;
const COMPLETION_PADDING: f32 = 8.0;

/// Total popup height for `item_count` rows.
pub(crate) fn completion_popup_height(item_count: usize) -> f32 {
    COMPLETION_PADDING + item_count as f32 * COMPLETION_ROW_HEIGHT + COMPLETION_PADDING
}

/// Row index under a window-space click, or `None` outside the rows.
pub(crate) fn completion_row_at_position(
    position: Point<Pixels>,
    top_left: Point<Pixels>,
    item_count: usize,
) -> Option<usize> {
    let relative = f32::from(position.y) - f32::from(top_left.y) - COMPLETION_PADDING;
    if relative < 0.0 {
        return None;
    }
    let index = (relative / COMPLETION_ROW_HEIGHT) as usize;
    (index < item_count).then_some(index)
}

impl Editor {
    /// Window-space popup bounds for the active completion session, if any.
    ///
    /// Polled live from hooks (no cached copy to go stale): the first hook
    /// reporting a non-empty [`twrite_core::CompletionSnapshot`] wins.
    /// Empty snapshots render nothing so a no-match session stays invisible
    /// while keeping its keys.
    pub(crate) fn completion_popup_rect(&self) -> Option<Bounds<Pixels>> {
        let count = self
            .hooks
            .iter()
            .find_map(|hook| hook.completion_snapshot())
            .map(|snapshot| snapshot.items.len())?;
        if count == 0 {
            return None;
        }
        let cursor = self.last_cursor_pixel?;
        let bounds = self.last_bounds?;
        let height = completion_popup_height(count);
        let top_left = clamp_menu_anchor(
            gpui::point(cursor.x, cursor.y),
            bounds,
            COMPLETION_WIDTH,
            height,
        );
        Some(Bounds::new(
            top_left,
            size(px(COMPLETION_WIDTH), px(height)),
        ))
    }

    /// Routes a popup row click to hooks (`on_completion_select`).
    pub(crate) fn dispatch_completion_select(
        &mut self,
        index: usize,
        window: Option<&Window>,
        cx: &mut Context<Self>,
    ) {
        self.reset_blink_cursor(cx);
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
            if self.hooks[hook_idx].on_completion_select(&mut ctx, index) == HookOutcome::Consumed {
                consumed = true;
                break;
            }
            hook_idx += 1;
        }
        if !consumed {
            return;
        }
        if self.buffer.version() != initial_version {
            for hook in &mut self.hooks {
                hook.after_edit(&mut self.buffer);
            }
        }
        for hook in &mut self.hooks {
            hook.on_selection_change(&self.buffer, self.selection.as_ref());
        }
        self.scroll_to_cursor(window);
        self.flush_effects();
        cx.notify();
    }

    /// Renders the completion popup as an absolutely-positioned overlay
    /// below the cursor, clamped into the viewport like the context menu.
    /// Renders nothing when no hook reports rows.
    pub(crate) fn render_completion(&self, cx: &mut Context<Self>) -> Div {
        let snapshot = self
            .hooks
            .iter()
            .find_map(|hook| hook.completion_snapshot());
        let Some(snapshot) = snapshot else {
            return div();
        };
        if snapshot.items.is_empty() {
            return div();
        }
        let Some(cursor) = self.last_cursor_pixel else {
            return div();
        };
        let height = completion_popup_height(snapshot.items.len());
        let pos = menu_overlay_position(
            gpui::point(cursor.x, cursor.y),
            self.last_bounds,
            COMPLETION_WIDTH,
            height,
        );

        let mut list = div().flex().flex_col().py(px(4.0));
        for (index, item) in snapshot.items.iter().enumerate() {
            list = list.child(self.render_completion_row(
                &item.label,
                item.hint.as_deref(),
                index,
                index == snapshot.selected,
                cx,
            ));
        }

        div()
            .absolute()
            .left(pos.x)
            .top(pos.y)
            .w(px(COMPLETION_WIDTH))
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

    fn render_completion_row(
        &self,
        label: &str,
        hint: Option<&str>,
        index: usize,
        highlighted: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let mut row = div()
            .id(("completion-item", index))
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .h(px(COMPLETION_ROW_HEIGHT))
            .px(px(12.0))
            .gap(px(16.0))
            .text_sm()
            .text_color(self.theme.menu_fg)
            .cursor_pointer();
        if highlighted {
            row = row.bg(self.theme.menu_hover);
        } else {
            row = row.hover(|style| style.bg(self.theme.menu_hover));
        }
        row = row.child(div().flex_1().truncate().child(label.to_string()));
        if let Some(hint) = hint {
            row = row.child(
                div()
                    .flex_shrink_0()
                    .text_size(px(11.0))
                    .text_color(self.theme.menu_hint)
                    .child(hint.to_string()),
            );
        }
        row.on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                this.dispatch_completion_select(index, Some(window), cx);
                cx.stop_propagation();
            }),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{completion_popup_height, completion_row_at_position};
    use gpui::{point, px};

    #[test]
    fn popup_height_grows_per_row() {
        assert_eq!(completion_popup_height(1), 44.0);
        assert_eq!(completion_popup_height(3), 100.0);
    }

    #[test]
    fn row_hit_testing_skips_padding_and_overflow() {
        let top_left = point(px(100.0), px(200.0));
        assert_eq!(
            completion_row_at_position(point(px(150.0), px(200.0)), top_left, 3),
            None
        );
        assert_eq!(
            completion_row_at_position(point(px(150.0), px(208.0)), top_left, 3),
            Some(0)
        );
        assert_eq!(
            completion_row_at_position(point(px(150.0), px(236.0)), top_left, 3),
            Some(1)
        );
        assert_eq!(
            completion_row_at_position(point(px(150.0), px(400.0)), top_left, 3),
            None
        );
    }
}
