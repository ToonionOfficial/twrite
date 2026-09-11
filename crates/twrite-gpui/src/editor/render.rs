use gpui::prelude::*;
use gpui::{Context, MouseButton, Render, div};

use crate::canvas::EditorCanvas;
use crate::prompt_bar::PromptBar;

use super::{Editor, SelectionGranularity};

impl Render for Editor {
    fn render(
        &mut self,
        _window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) -> impl gpui::prelude::IntoElement {
        let cursor_style = if self.is_hovering_task || self.hovered_link.is_some() {
            gpui::CursorStyle::PointingHand
        } else {
            gpui::CursorStyle::IBeam
        };

        let root = div()
            .track_focus(&self.focus_handle)
            .key_context("Editor")
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .cursor(cursor_style)
            .bg(self.theme.background);
        let root = if let Some(family) = self
            .config
            .font_family
            .clone()
            .or_else(|| self.selected_font_family.clone())
        {
            root.font_family(family)
        } else {
            root
        };

        let root = root
            .on_key_down(cx.listener(Self::handle_key_down))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::handle_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::handle_mouse_up))
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.is_selecting = false;
                    this.drag_initial_range = None;
                    this.selection_granularity = SelectionGranularity::Character;
                    if let Some(sel) = this.selection
                        && sel.is_empty()
                    {
                        this.selection = None;
                    }
                    for hook in &mut this.hooks {
                        hook.on_selection_change(&this.buffer, this.selection.as_ref());
                    }
                    let changed = this.is_hovering_task || this.hovered_link.is_some();
                    this.is_hovering_task = false;
                    this.hovered_link = None;
                    if changed {
                        cx.notify();
                    }
                }),
            )
            .on_mouse_move(cx.listener(Self::handle_mouse_move))
            .on_scroll_wheel(cx.listener(Self::handle_scroll_wheel))
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .child(EditorCanvas::new(cx.entity().clone())),
            );

        if !self.prompt.is_open() {
            return root;
        }
        let snapshot = self.search_snapshot();
        let placement = self
            .prompt
            .spec()
            .map(|s| s.placement)
            .unwrap_or(twrite_core::PromptPlacement::BottomBar);
        match placement {
            twrite_core::PromptPlacement::TopPalette => {
                root.child(PromptBar::palette(&self.prompt, snapshot, cx))
            }
            twrite_core::PromptPlacement::BottomBar => {
                root.child(PromptBar::bottom_bar(&self.prompt, snapshot, cx))
            }
        }
    }
}
