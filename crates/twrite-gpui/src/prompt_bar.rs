use gpui::*;
use twrite_core::{PromptState, SearchSnapshot};

use crate::editor::Editor;

/// Interactive renderer for the shared headless [`PromptState`].
///
/// Hooks own the meaning (search, ex-commands, custom palettes) via
/// `HookContext::prompt`; this only draws the box and routes clicks back
/// through [`Editor::press_search_key`], so chips and buttons share the exact
/// keyboard path (`Alt+C` toggles Match Case, plain `arrowdown` walks to the
/// next match). Renders nothing when the prompt is closed, and no toggle
/// chrome when no hook reports a search snapshot (foreign prompts).
pub struct PromptBar;

impl PromptBar {
    /// Bottom-anchored vim-style line or dedicated search/replace toolbar.
    ///
    /// Takes plain data on purpose: reading the entity here would
    /// re-enterantly borrow it mid-render and panic.
    pub fn bottom_bar(
        prompt: &PromptState,
        snapshot: Option<SearchSnapshot>,
        cx: &mut Context<Editor>,
    ) -> Div {
        if !prompt.is_open() {
            return div();
        }
        if let Some(snapshot) = snapshot {
            return Self::search_toolbar(prompt, &snapshot, cx);
        }
        div()
            .w_full()
            .border_t_1()
            .border_color(rgb(0x313244))
            .bg(rgb(0x11111b))
            .px(px(12.0))
            .py(px(6.0))
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            })
            .flex()
            .flex_col()
            .gap_1()
            .child(Self::input_row(prompt))
            .child(Self::item_list(prompt, 6))
            .child(Self::message_row(prompt))
    }

    /// Browser-`F1` style floating palette centered near the top.
    pub fn palette(
        prompt: &PromptState,
        snapshot: Option<SearchSnapshot>,
        cx: &mut Context<Editor>,
    ) -> Div {
        if !prompt.is_open() {
            return div();
        }
        if let Some(snapshot) = snapshot {
            return div()
                .absolute()
                .top(px(48.0))
                .w_full()
                .flex()
                .justify_center()
                .child(
                    div()
                        .w(px(720.0))
                        .rounded_md()
                        .border_1()
                        .border_color(rgb(0x45475a))
                        .bg(rgb(0x181825))
                        .p(px(8.0))
                        .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                            cx.stop_propagation();
                        })
                        .child(Self::search_toolbar_inner(prompt, &snapshot, cx)),
                );
        }
        div()
            .absolute()
            .top(px(48.0))
            .w_full()
            .flex()
            .justify_center()
            .child(
                div()
                    .w(px(560.0))
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(0x45475a))
                    .bg(rgb(0x1e1e2e))
                    .px(px(12.0))
                    .py(px(8.0))
                    .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                        cx.stop_propagation();
                    })
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(Self::input_row(prompt))
                    .child(Self::item_list(prompt, 8))
                    .child(Self::message_row(prompt)),
            )
    }

    /// Outer container for bottom-bar search and replace toolbar.
    fn search_toolbar(
        prompt: &PromptState,
        snapshot: &SearchSnapshot,
        cx: &mut Context<Editor>,
    ) -> Div {
        div()
            .w_full()
            .border_t_1()
            .border_color(rgb(0x313244))
            .bg(rgb(0x181825))
            .px(px(12.0))
            .py(px(6.0))
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            })
            .child(Self::search_toolbar_inner(prompt, snapshot, cx))
    }

    /// Inner content of search toolbar (Find row + optional Replace row).
    fn search_toolbar_inner(
        prompt: &PromptState,
        snapshot: &SearchSnapshot,
        cx: &mut Context<Editor>,
    ) -> Div {
        let mut col = div().flex().flex_col().gap_2();
        col = col.child(Self::find_row(prompt, snapshot, cx));
        if snapshot.replace_mode {
            col = col.child(Self::replace_row(prompt, snapshot, cx));
        }
        col
    }

    /// First row: find input, steppers, match count, checkboxes, close button.
    fn find_row(prompt: &PromptState, snapshot: &SearchSnapshot, cx: &mut Context<Editor>) -> Div {
        let is_find_active = !snapshot.is_replace_prompt;
        let find_text = if is_find_active {
            prompt.input().to_string()
        } else {
            snapshot.query.clone()
        };
        let cursor_idx = if is_find_active {
            Some(prompt.cursor())
        } else {
            None
        };

        let count_text = if snapshot.matches.is_empty() {
            if snapshot.query.is_empty() && prompt.input().is_empty() {
                String::new()
            } else {
                "No matches".to_string()
            }
        } else {
            format!(
                "{}/{}",
                snapshot.current.map(|i| i + 1).unwrap_or(0),
                snapshot.matches.len()
            )
        };

        let mut row = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .child(Self::replace_toggle_button(snapshot.replace_mode, cx))
            .child(Self::text_box(
                "search-find-box",
                &find_text,
                cursor_idx,
                "Find in page",
                is_find_active,
                "focus_search",
                cx,
            ))
            .child(Self::stepper_button("search-step-prev", "^", "arrowup", cx))
            .child(Self::stepper_button(
                "search-step-next",
                "v",
                "arrowdown",
                cx,
            ));

        if !count_text.is_empty() {
            let color = if snapshot.matches.is_empty() {
                rgb(0xf38ba8)
            } else {
                rgb(0xa6adc8)
            };
            row = row.child(div().text_xs().text_color(color).child(count_text));
        }

        row = row
            .child(Self::checkbox(
                "search-check-highlight",
                "Highlight All",
                Some(0),
                snapshot.highlight_all,
                "h",
                cx,
            ))
            .child(Self::checkbox(
                "search-check-case",
                "Match Case",
                Some(6),
                snapshot.case_sensitive,
                "c",
                cx,
            ))
            .child(Self::checkbox(
                "search-check-word",
                "Whole Words",
                Some(0),
                snapshot.whole_word,
                "w",
                cx,
            ))
            .child(div().flex_1())
            .child(Self::close_button(cx));

        row
    }

    /// Second row: replace input, replace button, replace all button, status message.
    fn replace_row(
        prompt: &PromptState,
        snapshot: &SearchSnapshot,
        cx: &mut Context<Editor>,
    ) -> Div {
        let is_replace_active = snapshot.is_replace_prompt;
        let replace_text = if is_replace_active {
            prompt.input().to_string()
        } else {
            snapshot.replacement.clone()
        };
        let cursor_idx = if is_replace_active {
            Some(prompt.cursor())
        } else {
            None
        };

        let mut row = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .child(div().w(px(24.0)))
            .child(Self::text_box(
                "search-replace-box",
                &replace_text,
                cursor_idx,
                "Replace with",
                is_replace_active,
                "focus_replace",
                cx,
            ))
            .child(Self::action_button(
                "search-btn-replace",
                "Replace",
                "enter",
                true,
                false,
                cx,
            ))
            .child(Self::action_button(
                "search-btn-replace-all",
                "Replace All",
                "a",
                false,
                true,
                cx,
            ));

        if let Some(msg) = prompt.message() {
            row = row.child(
                div()
                    .text_xs()
                    .text_color(rgb(0xa6e3a1))
                    .child(msg.to_string()),
            );
        }

        row
    }

    /// Styled text input box with focus border, text cursor, and click-to-focus.
    fn text_box(
        id: &'static str,
        text: &str,
        cursor: Option<usize>,
        placeholder: &'static str,
        focused: bool,
        focus_key: &'static str,
        cx: &mut Context<Editor>,
    ) -> Stateful<Div> {
        let border_color = if focused {
            rgb(0x89b4fa)
        } else {
            rgb(0x45475a)
        };

        let mut box_div = div()
            .id(id)
            .cursor_text()
            .w(px(240.0))
            .h(px(28.0))
            .rounded_md()
            .border_1()
            .border_color(border_color)
            .bg(rgb(0x11111b))
            .px(px(8.0))
            .flex()
            .items_center()
            .overflow_hidden();

        if let Some(cursor_offset) = cursor {
            if text.is_empty() {
                box_div = box_div
                    .child(div().w(px(1.5)).h(px(15.0)).bg(rgb(0x89b4fa)).mr(px(2.0)))
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0x6c7086))
                            .child(placeholder.to_string()),
                    );
            } else {
                let cursor = cursor_offset.min(text.len());
                let before = text[..cursor].to_string();
                let after = text[cursor..].to_string();
                if !before.is_empty() {
                    box_div =
                        box_div.child(div().text_sm().text_color(rgb(0xcdd6f4)).child(before));
                }
                box_div = box_div.child(div().w(px(1.5)).h(px(15.0)).bg(rgb(0x89b4fa)));
                if !after.is_empty() {
                    box_div = box_div.child(div().text_sm().text_color(rgb(0xcdd6f4)).child(after));
                }
            }
        } else if text.is_empty() {
            box_div = box_div.child(
                div()
                    .text_sm()
                    .text_color(rgb(0x6c7086))
                    .child(placeholder.to_string()),
            );
        } else {
            box_div = box_div.child(
                div()
                    .text_sm()
                    .text_color(rgb(0xcdd6f4))
                    .child(text.to_string()),
            );
        }

        box_div = box_div.on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                if !focused {
                    this.press_search_key(focus_key, false, false, false, Some(window), cx);
                }
                cx.stop_propagation();
            }),
        );

        box_div
    }

    /// Checkbox with full words and keyboard mnemonic underline.
    fn checkbox(
        id: &'static str,
        label: &'static str,
        underline_index: Option<usize>,
        checked: bool,
        alt_key: &'static str,
        cx: &mut Context<Editor>,
    ) -> Stateful<Div> {
        let box_bg = if checked {
            rgb(0x89b4fa)
        } else {
            rgb(0x181825)
        };
        let box_border = if checked {
            rgb(0x89b4fa)
        } else {
            rgb(0x585b70)
        };
        let text_color = if checked {
            rgb(0xcdd6f4)
        } else {
            rgb(0xa6adc8)
        };

        let mut check_icon = div()
            .w(px(14.0))
            .h(px(14.0))
            .rounded(px(3.0))
            .border_1()
            .border_color(box_border)
            .bg(box_bg)
            .flex()
            .items_center()
            .justify_center();

        if checked {
            check_icon = check_icon.child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(0x11111b))
                    .child("✓"),
            );
        }

        let mut label_div = div().text_xs().text_color(text_color).flex().flex_row();
        if let Some(idx) = underline_index {
            let before = &label[..idx];
            let under = &label[idx..idx + 1];
            let after = &label[idx + 1..];
            if !before.is_empty() {
                label_div = label_div.child(before.to_string());
            }
            label_div = label_div.child(div().underline().child(under.to_string()));
            if !after.is_empty() {
                label_div = label_div.child(after.to_string());
            }
        } else {
            label_div = label_div.child(label.to_string());
        }

        div()
            .id(id)
            .cursor_pointer()
            .rounded_md()
            .px(px(6.0))
            .py(px(2.0))
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .hover(|s| s.bg(rgb(0x313244)))
            .child(check_icon)
            .child(label_div)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    this.press_search_key(alt_key, false, true, false, Some(window), cx);
                    cx.stop_propagation();
                }),
            )
    }

    /// Match stepper button (`^` / `v`).
    fn stepper_button(
        id: &'static str,
        label: &'static str,
        key: &'static str,
        cx: &mut Context<Editor>,
    ) -> Stateful<Div> {
        div()
            .id(id)
            .cursor_pointer()
            .rounded_md()
            .px(px(8.0))
            .py(px(2.0))
            .bg(rgb(0x313244))
            .hover(|s| s.bg(rgb(0x45475a)))
            .text_xs()
            .font_weight(FontWeight::BOLD)
            .text_color(rgb(0xcdd6f4))
            .child(label.to_string())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    this.press_search_key(key, false, false, false, Some(window), cx);
                    cx.stop_propagation();
                }),
            )
    }

    /// Toggle replace section button (`▶` / `▼`).
    fn replace_toggle_button(expanded: bool, cx: &mut Context<Editor>) -> Stateful<Div> {
        let (bg, fg, label) = if expanded {
            (rgb(0x45475a), rgb(0x89b4fa), "▼")
        } else {
            (rgb(0x313244), rgb(0x6c7086), "▶")
        };
        div()
            .id("search-toggle-replace")
            .cursor_pointer()
            .rounded_md()
            .w(px(24.0))
            .h(px(24.0))
            .flex()
            .items_center()
            .justify_center()
            .bg(bg)
            .hover(|s| s.bg(rgb(0x45475a)))
            .text_xs()
            .font_weight(FontWeight::BOLD)
            .text_color(fg)
            .child(label.to_string())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    this.press_search_key("toggle_replace", false, false, false, Some(window), cx);
                    cx.stop_propagation();
                }),
            )
    }

    /// Action button (Replace / Replace All).
    fn action_button(
        id: &'static str,
        label: &'static str,
        key: &'static str,
        ctrl: bool,
        alt: bool,
        cx: &mut Context<Editor>,
    ) -> Stateful<Div> {
        div()
            .id(id)
            .cursor_pointer()
            .rounded_md()
            .border_1()
            .border_color(rgb(0x45475a))
            .bg(rgb(0x313244))
            .hover(|s| s.bg(rgb(0x45475a)).border_color(rgb(0x89b4fa)))
            .px(px(10.0))
            .py(px(2.0))
            .text_xs()
            .font_weight(FontWeight::BOLD)
            .text_color(rgb(0xcdd6f4))
            .child(label.to_string())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    this.press_search_key(key, ctrl, alt, false, Some(window), cx);
                    cx.stop_propagation();
                }),
            )
    }

    /// Close button (`✕`) at far right of find bar.
    fn close_button(cx: &mut Context<Editor>) -> Stateful<Div> {
        div()
            .id("search-close")
            .cursor_pointer()
            .rounded_md()
            .px(px(6.0))
            .py(px(2.0))
            .text_sm()
            .text_color(rgb(0x6c7086))
            .hover(|s| s.text_color(rgb(0xcdd6f4)).bg(rgb(0x313244)))
            .child("✕")
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    this.press_search_key("escape", false, false, false, Some(window), cx);
                    cx.stop_propagation();
                }),
            )
    }

    /// `prefix + before-cursor + bar cursor + after-cursor` flowing as one
    /// line (no growing spacers: the cursor stays glued to the text), or the
    /// placeholder when the input is empty.
    fn input_row(prompt: &PromptState) -> Div {
        let mut row = div().w_full().flex().flex_row().items_center().gap_2();

        let spec = prompt.spec();
        let prefix = spec.map(|s| s.prefix.as_str()).unwrap_or("");
        if !prefix.is_empty() {
            row = row.child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(0xf9e2af))
                    .child(prefix.to_string()),
            );
        }

        if prompt.input().is_empty() {
            let placeholder = spec.map(|s| s.placeholder.as_str()).unwrap_or("");
            row = row
                .child(div().w(px(1.5)).h(px(15.0)).bg(rgb(0x89b4fa)).mr(px(2.0)))
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(0x6c7086))
                        .child(placeholder.to_string()),
                );
        } else {
            let cursor = prompt.cursor().min(prompt.input().len());
            let before = prompt.input()[..cursor].to_string();
            let after = prompt.input()[cursor..].to_string();
            let mut input_content = div().flex().flex_row().items_center();
            if !before.is_empty() {
                input_content =
                    input_content.child(div().text_sm().text_color(rgb(0xcdd6f4)).child(before));
            }
            input_content = input_content.child(div().w(px(1.5)).h(px(15.0)).bg(rgb(0x89b4fa)));
            if !after.is_empty() {
                input_content =
                    input_content.child(div().text_sm().text_color(rgb(0xcdd6f4)).child(after));
            }
            row = row.child(input_content);
        }
        row
    }

    /// Up to `limit` item rows, selected row highlighted.
    fn item_list(prompt: &PromptState, limit: usize) -> Div {
        if prompt.items().is_empty() {
            return div();
        }
        let mut list = div().flex().flex_col();
        for (idx, item) in prompt.items().iter().enumerate().take(limit) {
            let selected = idx == prompt.selected_index();
            let mut row = div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .rounded_md()
                .px(px(8.0))
                .py(px(2.0));
            row = if selected { row.bg(rgb(0x45475a)) } else { row };
            row = row.child(
                div()
                    .text_sm()
                    .text_color(rgb(0xcdd6f4))
                    .child(item.label.clone()),
            );
            if let Some(hint) = &item.hint {
                row = row.child(
                    div()
                        .text_xs()
                        .text_color(rgb(0x6c7086))
                        .child(hint.clone()),
                );
            }
            list = list.child(row);
        }
        list
    }

    /// Validation / error message line (empty when none).
    fn message_row(prompt: &PromptState) -> Div {
        match prompt.message() {
            Some(message) => div()
                .text_xs()
                .text_color(rgb(0xf38ba8))
                .child(message.to_string()),
            None => div(),
        }
    }
}
