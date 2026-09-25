//! Supplemental example: wikilinks with host-owned storage.
//!
//! Not part of the numbered 1-6 sequence; read after `markdown`.
//! Demonstrates the storage-agnostic wikilink protocol: `[[Target]]`,
//! `[[Target|Label]]`, and `[[Target#Fragment]]` render as links, typing
//! `[[` opens an inline completion popup, and activating a link reports
//! `HookEffect::FollowLink` for the host to resolve. Notes live in an
//! in-memory map here to prove the core never touches storage; a file or
//! database store plugs into the same two spots (the provider closure and
//! the effect drain below).
//! Run with: `cargo run --example wikilinks --features markdown`
use std::collections::HashMap;
use std::sync::Arc;

use gpui::*;
use twrite::markdown::{ConcealMode, MarkdownConfig, MarkdownHighlighter, MarkdownHook};
use twrite::{Editor, HookEffect, PromptItem};

/// In-memory note store standing in for any host storage.
fn demo_notes() -> HashMap<String, String> {
    HashMap::from([
        (
            "Overview".to_string(),
            "Welcome to the wikilink demo.\n\nType [[ to complete a link, then click it or press Enter.\n\n- Start with [[Getting Started]]\n- See the [[Roadmap|project roadmap]]\n- Jump straight to [[Roadmap#Milestones]]\n"
                .to_string(),
        ),
        (
            "Getting Started".to_string(),
            "This note lives in a HashMap, not on disk.\n\nBack to [[Overview]], or ahead to the [[Roadmap]].\n"
                .to_string(),
        ),
        (
            "Roadmap".to_string(),
            "Plans live here.\n\n## Milestones\n\n- Inline completion popup\n- Hover previews\n- Transclusions\n\nBack to [[Overview]].\n"
                .to_string(),
        ),
    ])
}

struct WikilinkApp {
    editor: Entity<Editor>,
    notes: Arc<HashMap<String, String>>,
    last_followed: String,
}

impl WikilinkApp {
    /// Drains hook effects and resolves `FollowLink` against the store.
    fn drain_effects(&mut self, cx: &mut Context<Self>) {
        let pending = self.editor.update(cx, |editor, _| editor.take_effects());
        for effect in pending {
            if let HookEffect::FollowLink {
                target,
                fragment,
                label,
            } = effect
            {
                let shown = label.unwrap_or_else(|| target.clone());
                match self.notes.get(&target) {
                    Some(text) => {
                        let text = text.clone();
                        self.editor.update(cx, |editor, _| {
                            let end = editor.buffer.len_bytes();
                            editor.buffer.replace_range(0..end, &text);
                            editor.buffer.set_cursor_offset(0);
                            editor.selection = None;
                        });
                        self.last_followed = match fragment {
                            Some(fragment) => format!("Opened {shown}#{fragment}"),
                            None => format!("Opened {shown}"),
                        };
                    }
                    None => {
                        self.last_followed = format!("Unknown target: {target}");
                    }
                }
            }
        }
    }
}

impl Render for WikilinkApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.drain_effects(cx);
        let editor_read = self.editor.read(cx);
        let cursor_point = editor_read.buffer.cursor_point();
        let status = editor_read.status_text().unwrap_or("READY");

        let status_line = format!(
            "Ln {}, Col {} | {} | {}",
            cursor_point.row + 1,
            cursor_point.column + 1,
            status,
            self.last_followed
        );

        div()
            .size_full()
            .flex()
            .flex_col()
            .child(div().flex_1().child(self.editor.clone()))
            .child(
                div()
                    .h(px(26.0))
                    .bg(rgb(0x11111b))
                    .text_color(rgb(0xa6adc8))
                    .text_size(px(12.0))
                    .px(px(12.0))
                    .flex()
                    .items_center()
                    .child(status_line),
            )
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(800.0), px(600.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("TWrite - Wikilinks Demo".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |window, cx| {
                let notes = Arc::new(demo_notes());
                let editor = cx.new(|cx| {
                    let mut ed = Editor::new(&notes["Overview"], cx);
                    ed.config.line_numbers = true;

                    let markdown_config = MarkdownConfig {
                        conceal_mode: ConcealMode::Hidden,
                        ..Default::default()
                    };
                    // Manual battery wiring (instead of `enable_markdown`)
                    // so the hook instance carries our provider.
                    ed.set_highlighter(MarkdownHighlighter::with_config(markdown_config));
                    let mut hooks = MarkdownHook::new();
                    let candidates = notes.clone();
                    hooks.set_completion_provider(Arc::new(move |query: &str| {
                        let mut rows: Vec<PromptItem> = candidates
                            .keys()
                            .filter(|name| name.contains(query))
                            .map(|name| PromptItem::new(name))
                            .collect();
                        rows.sort_by(|a, b| a.label.cmp(&b.label));
                        rows
                    }));
                    ed.add_hook(hooks);

                    ed
                });

                let focus_handle = editor.read(cx).focus_handle.clone();
                twrite::focus_editor(&focus_handle, window, cx);

                cx.new(|_| WikilinkApp {
                    editor,
                    notes,
                    last_followed: "Click a link or type [[ ".to_string(),
                })
            },
        )
        .unwrap();

        cx.activate(true);
    });
}
