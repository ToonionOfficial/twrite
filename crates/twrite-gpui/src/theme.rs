use std::collections::HashMap;

use gpui::{Hsla, hsla, rgb};
use twrite_core::{CalloutKind, HighlightTag, Rgba, StyleValue, UnderlineDecoration};

/// Color configuration for syntax elements.
#[derive(Clone, Debug)]
pub struct SyntaxTheme {
    /// Color for language keywords.
    pub keyword: Hsla,
    /// Color for function and method names.
    pub function: Hsla,
    /// Color for type and struct names.
    pub type_name: Hsla,
    /// Color for string literals.
    pub string: Hsla,
    /// Color for numeric literals.
    pub number: Hsla,
    /// Color for comments.
    pub comment: Hsla,
    /// Color for mathematical and logical operators.
    pub operator: Hsla,
    /// Color for punctuation and delimiter characters.
    pub punctuation: Hsla,
    /// Color for headings
    pub heading: Hsla,
    /// Color for bold text.
    pub bold: Hsla,
    /// Color for italic text.
    pub italic: Hsla,
    /// Color for inline code spans.
    pub code: Hsla,
    /// Background pill fill color for inline code spans.
    pub code_bg: Hsla,
    /// Background tint color for `==mark==` highlight spans.
    pub highlight_bg: Hsla,
    /// Accent color for `> [!NOTE]` callout blocks.
    pub callout_note: Hsla,
    /// Accent color for `> [!TIP]` callout blocks.
    pub callout_tip: Hsla,
    /// Accent color for `> [!WARNING]` callout blocks.
    pub callout_warning: Hsla,
    /// Accent color for `> [!CAUTION]` callout blocks.
    pub callout_caution: Hsla,
    /// Accent color for `> [!IMPORTANT]` callout blocks.
    pub callout_important: Hsla,
    /// Color for hyperlink text.
    pub link: Hsla,
    /// Registered colors for `HighlightTag::Custom` names. Unregistered names
    /// fall back to the editor foreground.
    pub custom: HashMap<&'static str, Hsla>,
    /// Color for diagnostic error underlines and squiggles.
    pub error: Hsla,
    /// Color for diagnostic warning underlines and squiggles.
    pub warning: Hsla,
}

impl Default for SyntaxTheme {
    fn default() -> Self {
        Self {
            // Markdown / syntax should mostly use the normal text color.
            keyword: rgb(0xe6e1e9).into(),
            function: rgb(0xe6e1e9).into(),
            type_name: rgb(0xe6e1e9).into(),
            string: rgb(0xe6e1e9).into(),
            number: rgb(0xe6e1e9).into(),
            comment: rgb(0x8f8c96).into(),
            operator: rgb(0xe6e1e9).into(),
            punctuation: rgb(0xb5b1ba).into(),

            heading: rgb(0xe6e1e9).into(),

            bold: rgb(0xe6e1e9).into(),
            italic: rgb(0xe6e1e9).into(),
            code: rgb(0xe6e1e9).into(),
            code_bg: hsla(0.65, 0.4, 0.6, 0.15),
            highlight_bg: hsla(0.11, 0.85, 0.6, 0.35),

            // Semantic UI elements can retain color.
            callout_note: rgb(0x89b4fa).into(),
            callout_tip: rgb(0xa6e3a1).into(),
            callout_warning: rgb(0xf9e2af).into(),
            callout_caution: rgb(0xfab387).into(),
            callout_important: rgb(0xf38ba8).into(),

            link: rgb(0x89b4fa).into(),

            custom: HashMap::new(),

            error: rgb(0xf38ba8).into(),
            warning: rgb(0xf9e2af).into(),
        }
    }
}

impl SyntaxTheme {
    /// Registers a color for a `HighlightTag::Custom` name (e.g. `"speaker"`).
    pub fn set_custom_tag_color(&mut self, name: &'static str, color: Hsla) {
        self.custom.insert(name, color);
    }

    /// Resolves a callout kind to its accent color. Unrecognized kinds render
    /// with the default note accent instead of failing.
    pub fn callout_accent(&self, kind: CalloutKind) -> Hsla {
        match kind {
            CalloutKind::Note => self.callout_note,
            CalloutKind::Tip => self.callout_tip,
            CalloutKind::Warning => self.callout_warning,
            CalloutKind::Caution => self.callout_caution,
            CalloutKind::Important => self.callout_important,
            CalloutKind::Other => self.callout_note,
        }
    }
}

/// Complete theme configuration for the editor.
#[derive(Clone, Debug)]
pub struct EditorTheme {
    /// Background color of the text canvas.
    pub background: Hsla,
    /// Default text color.
    pub foreground: Hsla,
    /// Text insertion cursor color.
    pub cursor: Hsla,
    /// Background highlight color for selected text ranges.
    pub selection: Hsla,
    /// Background wash color for highlight-all search matches.
    pub search_match: Hsla,
    /// Gutter line number color for inactive lines.
    pub line_number: Hsla,
    /// Gutter line number color for the line containing the cursor.
    pub line_number_active: Hsla,
    /// Background fill for the context menu popup.
    pub menu_bg: Hsla,
    /// Border color for the context menu popup.
    pub menu_border: Hsla,
    /// Hover fill for context menu rows.
    pub menu_hover: Hsla,
    /// Primary text color for context menu rows.
    pub menu_fg: Hsla,
    /// Hint (keybinding) text color for context menu rows.
    pub menu_hint: Hsla,
    /// Palette for syntax highlighting tokens.
    pub syntax: SyntaxTheme,
}

impl Default for EditorTheme {
    fn default() -> Self {
        Self {
            background: rgb(0x181825).into(),
            foreground: rgb(0xcdd6f4).into(),
            cursor: rgb(0xf5e0dc).into(),
            selection: hsla(0.65, 0.4, 0.6, 0.25),
            search_match: hsla(0.12, 0.8, 0.65, 0.25),
            line_number: rgb(0x6c7086).into(),
            line_number_active: rgb(0xcdd6f4).into(),
            menu_bg: rgb(0x1e1e2e).into(),
            menu_border: rgb(0x45475a).into(),
            menu_hover: rgb(0x313244).into(),
            menu_fg: rgb(0xcdd6f4).into(),
            menu_hint: rgb(0x6c7086).into(),
            syntax: SyntaxTheme::default(),
        }
    }
}

/// Fully resolved style ready for canvas text run construction.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedTokenStyle {
    /// Foreground text color.
    pub color: Hsla,
    /// Optional background highlight or pill fill color.
    pub background: Option<Hsla>,
    /// Whether the text is rendered with bold font weight.
    pub bold: bool,
    /// Whether the text is rendered with italic font style.
    pub italic: bool,
    /// Optional underline decoration (solid or wavy).
    pub underline: Option<UnderlineDecoration>,
    /// Whether the text is rendered with a strikethrough line.
    pub strikethrough: bool,
}

impl EditorTheme {
    /// Converts a headless Rgba color to GPUI Hsla.
    pub fn rgba_to_hsla(rgba: Rgba) -> Hsla {
        let r = rgba.r as f32 / 255.0;
        let g = rgba.g as f32 / 255.0;
        let b = rgba.b as f32 / 255.0;
        let a = rgba.a as f32 / 255.0;
        gpui::Rgba { r, g, b, a }.into()
    }

    /// Resolves a semantic HighlightTag to its foreground color.
    pub fn tag_color(&self, tag: HighlightTag) -> Hsla {
        match tag {
            HighlightTag::Keyword => self.syntax.keyword,
            HighlightTag::Function => self.syntax.function,
            HighlightTag::Type => self.syntax.type_name,
            HighlightTag::String => self.syntax.string,
            HighlightTag::Number => self.syntax.number,
            HighlightTag::Comment => self.syntax.comment,
            HighlightTag::Operator => self.syntax.operator,
            HighlightTag::Punctuation => self.syntax.punctuation,
            HighlightTag::Heading(_) => self.syntax.heading,
            HighlightTag::Bold => self.syntax.bold,
            HighlightTag::Italic => self.syntax.italic,
            HighlightTag::Code => self.syntax.code,
            HighlightTag::Highlight => self.foreground,
            HighlightTag::Link => self.syntax.link,
            HighlightTag::Custom(name) => self
                .syntax
                .custom
                .get(name)
                .copied()
                .unwrap_or(self.foreground),
            HighlightTag::Dimmed => {
                let mut c = self.syntax.comment;
                c.a = 0.25;
                c
            }
            HighlightTag::Hidden => {
                let mut c = self.syntax.comment;
                c.a = 0.0;
                c
            }
            HighlightTag::Blockquote => self.syntax.comment,
            HighlightTag::Callout(_) => self.foreground,
            HighlightTag::HorizontalRule => self.syntax.punctuation,
            HighlightTag::TaskUnchecked => self.syntax.comment,
            HighlightTag::TaskChecked => self.syntax.string,
        }
    }

    /// Resolves any StyleValue into concrete rendering attributes.
    pub fn resolve_style(&self, style_value: &StyleValue) -> ResolvedTokenStyle {
        match style_value {
            StyleValue::Tag(tag) => {
                let color = self.tag_color(*tag);
                let bold = matches!(tag, HighlightTag::Heading(_) | HighlightTag::Bold);
                let italic = matches!(tag, HighlightTag::Italic | HighlightTag::Comment);
                let background = if matches!(tag, HighlightTag::Code) {
                    Some(self.syntax.code_bg)
                } else if matches!(tag, HighlightTag::Highlight) {
                    Some(self.syntax.highlight_bg)
                } else {
                    None
                };
                let underline = if matches!(tag, HighlightTag::Link) {
                    Some(UnderlineDecoration::Solid)
                } else {
                    None
                };

                ResolvedTokenStyle {
                    color,
                    background,
                    bold,
                    italic,
                    underline,
                    strikethrough: false,
                }
            }
            StyleValue::Direct(direct) => ResolvedTokenStyle {
                color: direct
                    .color
                    .map(Self::rgba_to_hsla)
                    .unwrap_or(self.foreground),
                background: direct.background.map(Self::rgba_to_hsla),
                bold: direct.bold,
                italic: direct.italic,
                underline: direct.underline,
                strikethrough: direct.strikethrough,
            },
        }
    }
}
