use gpui::{Hsla, hsla, rgb};
use twrite_core::{HighlightTag, Rgba, StyleValue, UnderlineDecoration};

/// Color configuration for syntax elements.
#[derive(Clone, Debug)]
pub struct SyntaxTheme {
    pub keyword: Hsla,
    pub function: Hsla,
    pub type_name: Hsla,
    pub string: Hsla,
    pub number: Hsla,
    pub comment: Hsla,
    pub operator: Hsla,
    pub punctuation: Hsla,
    pub heading1: Hsla,
    pub heading2: Hsla,
    pub heading3: Hsla,
    pub bold: Hsla,
    pub italic: Hsla,
    pub code: Hsla,
    pub code_bg: Hsla,
    pub link: Hsla,
    pub speaker: Hsla,
    pub dialogue: Hsla,
    pub choice: Hsla,
    pub error: Hsla,
    pub warning: Hsla,
}

impl Default for SyntaxTheme {
    fn default() -> Self {
        Self {
            keyword: rgb(0xcba6f7).into(),
            function: rgb(0x89b4fa).into(),
            type_name: rgb(0xf9e2af).into(),
            string: rgb(0xa6e3a1).into(),
            number: rgb(0xfab387).into(),
            comment: rgb(0x6c7086).into(),
            operator: rgb(0x89dceb).into(),
            punctuation: rgb(0x9399b2).into(),
            heading1: rgb(0xf38ba8).into(),
            heading2: rgb(0xfab387).into(),
            heading3: rgb(0xf9e2af).into(),
            bold: rgb(0xcdd6f4).into(),
            italic: rgb(0xb4befe).into(),
            code: rgb(0xf5c2e7).into(),
            code_bg: hsla(0.65, 0.4, 0.6, 0.15),
            link: rgb(0x89b4fa).into(),
            speaker: rgb(0xf9e2af).into(),
            dialogue: rgb(0xa6e3a1).into(),
            choice: rgb(0xcba6f7).into(),
            error: rgb(0xf38ba8).into(),
            warning: rgb(0xf9e2af).into(),
        }
    }
}

/// Complete theme configuration for the editor.
#[derive(Clone, Debug)]
pub struct EditorTheme {
    pub background: Hsla,
    pub foreground: Hsla,
    pub cursor: Hsla,
    pub selection: Hsla,
    pub line_number: Hsla,
    pub line_number_active: Hsla,
    pub syntax: SyntaxTheme,
}

impl Default for EditorTheme {
    fn default() -> Self {
        Self {
            background: rgb(0x181825).into(),
            foreground: rgb(0xcdd6f4).into(),
            cursor: rgb(0xf5e0dc).into(),
            selection: hsla(0.65, 0.4, 0.6, 0.25),
            line_number: rgb(0x6c7086).into(),
            line_number_active: rgb(0xcdd6f4).into(),
            syntax: SyntaxTheme::default(),
        }
    }
}

/// Fully resolved style ready for canvas text run construction.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedTokenStyle {
    pub color: Hsla,
    pub background: Option<Hsla>,
    pub bold: bool,
    pub italic: bool,
    pub underline: Option<UnderlineDecoration>,
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
            HighlightTag::Heading1 => self.syntax.heading1,
            HighlightTag::Heading2 => self.syntax.heading2,
            HighlightTag::Heading3 => self.syntax.heading3,
            HighlightTag::Bold => self.syntax.bold,
            HighlightTag::Italic => self.syntax.italic,
            HighlightTag::Code => self.syntax.code,
            HighlightTag::Link => self.syntax.link,
            HighlightTag::Speaker => self.syntax.speaker,
            HighlightTag::Dialogue => self.syntax.dialogue,
            HighlightTag::Choice => self.syntax.choice,
        }
    }

    /// Resolves any StyleValue into concrete rendering attributes.
    pub fn resolve_style(&self, style_value: &StyleValue) -> ResolvedTokenStyle {
        match style_value {
            StyleValue::Tag(tag) => {
                let color = self.tag_color(*tag);
                let bold = matches!(
                    tag,
                    HighlightTag::Heading1
                        | HighlightTag::Heading2
                        | HighlightTag::Heading3
                        | HighlightTag::Bold
                );
                let italic = matches!(tag, HighlightTag::Italic | HighlightTag::Comment);
                let background = if matches!(tag, HighlightTag::Code) {
                    Some(self.syntax.code_bg)
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
