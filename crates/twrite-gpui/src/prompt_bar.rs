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
    /// Bottom-anchored vim-style line (`:`, `/`, `Ctrl+F`).
    pub fn bottom_bar(editor: &Entity<Editor>, cx: &mut Context<Editor>) -> Div {
        let (prompt, snapshot) = Self::read_state(editor, cx);
        if !prompt.is_open() {
            return div();
        }
        div()
            .w_full()
            .border_t_1()
            .border_color(rgb(0x313244))
            .bg(rgb(0x11111b))
            .px(px(12.0))
            .py(px(6.0))
            .flex()
            .flex_col()
            .gap_1()
            .child(Self::input_row(&prompt))
            .child(Self::controls_row(snapshot.as_ref(), cx))
            .child(Self::item_list(&prompt, 6))
            .child(Self::message_row(&prompt))
    }

    /// Browser-`F1` style floating palette centered near the top.
    pub fn palette(editor: &Entity<Editor>, cx: &mut Context<Editor>) -> Div {
        let (prompt, snapshot) = Self::read_state(editor, cx);
        if !prompt.is_open() {
            return div();
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
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(Self::input_row(&prompt))
                    .child(Self::controls_row(snapshot.as_ref(), cx))
                    .child(Self::item_list(&prompt, 8))
                    .child(Self::message_row(&prompt)),
            )
    }

    fn read_state(
        editor: &Entity<Editor>,
        cx: &mut Context<Editor>,
    ) -> (PromptState, Option<SearchSnapshot>) {
        let editor_ref = editor.read(cx);
        (editor_ref.prompt.clone(), editor_ref.search_snapshot())
    }

    /// Toggle chips (`Aa` / `W` / `All`), match count, and `↑` / `↓` steppers.
    /// Empty unless a hook reports a search snapshot.
    fn controls_row(snapshot: Option<&SearchSnapshot>, cx: &mut Context<Editor>) -> Div {
        let Some(snapshot) = snapshot else {
            return div();
        };
        let count = format!(
            "{}/{}",
            snapshot.current.map(|i| i + 1).unwrap_or(0),
            snapshot.matches.len()
        );
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .child(Self::chip(
                "search-chip-case",
                "Aa",
                snapshot.case_sensitive,
                "c",
                cx,
            ))
            .child(Self::chip(
                "search-chip-word",
                "W",
                snapshot.whole_word,
                "w",
                cx,
            ))
            .child(Self::chip(
                "search-chip-highlight",
                "All",
                snapshot.highlight_all,
                "h",
                cx,
            ))
            .child(div().text_xs().text_color(rgb(0x6c7086)).child(count))
            .child(div().flex_1())
            .child(Self::stepper("search-step-up", "↑", "arrowup", cx))
            .child(Self::stepper("search-step-down", "↓", "arrowdown", cx))
    }

    /// One toggle chip dispatching `Alt+<key>` through the hook chain.
    fn chip(
        id: &'static str,
        label: &str,
        active: bool,
        key: &'static str,
        cx: &mut Context<Editor>,
    ) -> Stateful<Div> {
        let (bg, fg) = if active {
            (rgb(0x89b4fa), rgb(0x11111b))
        } else {
            (rgb(0x313244), rgb(0x6c7086))
        };
        div()
            .id(id)
            .cursor_pointer()
            .rounded_md()
            .px(px(8.0))
            .py(px(2.0))
            .bg(bg)
            .text_xs()
            .font_weight(FontWeight::BOLD)
            .text_color(fg)
            .child(label.to_string())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    this.press_search_key(key, false, true, false, Some(window), cx);
                    cx.stop_propagation();
                }),
            )
    }

    /// One match stepper dispatching a plain arrow key through the hook chain.
    fn stepper(
        id: &'static str,
        label: &str,
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

    /// `prefix + before-cursor + block cursor + after-cursor` flowing as one
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
            row = row.child(
                div()
                    .text_sm()
                    .text_color(rgb(0x6c7086))
                    .child(placeholder.to_string()),
            );
        } else {
            let cursor = prompt.cursor().min(prompt.input().len());
            let before = prompt.input()[..cursor].to_string();
            let rest = &prompt.input()[cursor..];
            let mut chars = rest.chars();
            let (under, after) = match chars.next() {
                Some(ch) => (ch.to_string(), chars.as_str().to_string()),
                None => (" ".to_string(), String::new()),
            };
            row = row.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .child(div().text_sm().text_color(rgb(0xcdd6f4)).child(before))
                    .child(
                        div()
                            .bg(rgb(0x89b4fa))
                            .text_sm()
                            .text_color(rgb(0x11111b))
                            .child(under),
                    )
                    .child(div().text_sm().text_color(rgb(0xcdd6f4)).child(after)),
            );
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
