use gpui::*;
use gpui_platform::application;
use twrite::{Editor, EditorBuffer, HighlightTag, StyleSpan, SyntaxHighlighter};

/// A demo syntax highlighter for Markdown headings, inline code, and story script dialogue.
struct StoryAndMarkdownHighlighter;

impl SyntaxHighlighter for StoryAndMarkdownHighlighter {
    fn highlight_line(
        &self,
        _buffer: &EditorBuffer,
        _row: usize,
        line_text: &str,
    ) -> Vec<StyleSpan> {
        let mut spans = Vec::new();

        if line_text.starts_with("# ") {
            spans.push(StyleSpan::tag(0..line_text.len(), HighlightTag::Heading1));
            return spans;
        } else if line_text.starts_with("## ") {
            spans.push(StyleSpan::tag(0..line_text.len(), HighlightTag::Heading2));
            return spans;
        } else if line_text.starts_with("### ") {
            spans.push(StyleSpan::tag(0..line_text.len(), HighlightTag::Heading3));
            return spans;
        }

        if line_text.starts_with('@')
            && let Some(colon_pos) = line_text.find(':')
        {
            spans.push(StyleSpan::tag(0..colon_pos + 1, HighlightTag::Speaker));
        }

        let mut in_quote = false;
        let mut quote_start = 0;
        for (i, c) in line_text.char_indices() {
            if c == '"' {
                if in_quote {
                    spans.push(StyleSpan::tag(quote_start..i + 1, HighlightTag::Dialogue));
                    in_quote = false;
                } else {
                    in_quote = true;
                    quote_start = i;
                }
            }
        }

        let mut in_code = false;
        let mut code_start = 0;
        for (i, c) in line_text.char_indices() {
            if c == '`' {
                if in_code {
                    spans.push(StyleSpan::tag(code_start..i + 1, HighlightTag::Code));
                    in_code = false;
                } else {
                    in_code = true;
                    code_start = i;
                }
            }
        }

        spans
    }
}

struct AppView {
    editor: Entity<Editor>,
}

impl Render for AppView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.editor.clone())
    }
}

fn main() {
    application().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(800.0), px(600.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("TWrite - Syntax Highlighting Demo".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |window, cx| {
                let editor = cx.new(|cx| {
                    let mut ed = Editor::new(
                        "# TWrite Syntax Engine\n\n## Story Scripting & Markdown\n\n@Narrator: \"The adventure begins at dusk.\"\n@Alice: \"Look at that mystical ancient ruins!\"\n@Bob: \"Let us inspect the inscriptions.\"\n\nInline `code spans` render with custom background color.\nHeadings render bold with distinct theme palette colors.\nSpeakers and dialogue are highlighted cleanly.\n",
                        cx,
                    );
                    ed.config.line_numbers = true;
                    ed.set_highlighter(StoryAndMarkdownHighlighter);
                    ed
                });

                let focus_handle = editor.read(cx).focus_handle.clone();
                focus_handle.focus(window, cx);

                cx.new(|_| AppView { editor })
            },
        )
        .unwrap();

        cx.activate(true);
    });
}
