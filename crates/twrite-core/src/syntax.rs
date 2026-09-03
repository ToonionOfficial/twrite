use std::ops::Range;

use crate::EditorBuffer;

/// An 8-bit per channel RGBA color representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rgba {
    /// Red color component (0-255).
    pub r: u8,
    /// Green color component (0-255).
    pub g: u8,
    /// Blue color component (0-255).
    pub b: u8,
    /// Alpha opacity component (0-255).
    pub a: u8,
}

impl Rgba {
    /// Creates a new color with red, green, blue, and alpha values.
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Creates an opaque color with red, green, and blue values.
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// Converts hexadecimal color code (e.g. `0xFF5500`) to opaque RGBA.
    pub const fn hex(hex: u32) -> Self {
        let r = ((hex >> 16) & 0xFF) as u8;
        let g = ((hex >> 8) & 0xFF) as u8;
        let b = (hex & 0xFF) as u8;
        Self { r, g, b, a: 255 }
    }
}

/// Visual style for underlined text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnderlineDecoration {
    /// Solid straight line.
    Solid,
    /// Wavy squiggly line.
    Wavy,
}

/// Semantic categorization for syntax tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HighlightTag {
    /// Programming language keywords.
    Keyword,
    /// Function names and declarations.
    Function,
    /// Type and struct names.
    Type,
    /// String literals.
    String,
    /// Numeric literals.
    Number,
    /// Comments.
    Comment,
    /// Operators.
    Operator,
    /// Punctuation symbols.
    Punctuation,
    /// Markdown heading level 1.
    Heading1,
    /// Markdown heading level 2.
    Heading2,
    /// Markdown heading level 3.
    Heading3,
    /// Markdown heading level 4.
    Heading4,
    /// Markdown heading level 5.
    Heading5,
    /// Markdown heading level 6.
    Heading6,
    /// Bold text.
    Bold,
    /// Italic text.
    Italic,
    /// Monospace code spans or blocks.
    Code,
    /// Hyperlinks.
    Link,
    /// Story character or speaker identifier.
    Speaker,
    /// Story dialogue text.
    Dialogue,
    /// Story scene or branch navigation choice.
    Choice,
    /// Dimmed or concealed syntax delimiters (e.g. inactive markdown markers).
    Dimmed,
    /// Fully concealed / hidden syntax delimiters (transparent on inactive lines).
    Hidden,
}

/// Direct styling attributes for a span of text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TextStyle {
    /// Text foreground color.
    pub color: Option<Rgba>,
    /// Background highlight color.
    pub background: Option<Rgba>,
    /// Whether the text is rendered in bold font weight.
    pub bold: bool,
    /// Whether the text is rendered in italic font style.
    pub italic: bool,
    /// Underline decoration style if any.
    pub underline: Option<UnderlineDecoration>,
    /// Strikethrough line through the text.
    pub strikethrough: bool,
}

impl TextStyle {
    /// Creates an empty text style with all attributes set to default.
    pub const fn new() -> Self {
        Self {
            color: None,
            background: None,
            bold: false,
            italic: false,
            underline: None,
            strikethrough: false,
        }
    }

    /// Sets foreground color.
    pub const fn color(mut self, color: Rgba) -> Self {
        self.color = Some(color);
        self
    }

    /// Sets background color.
    pub const fn background(mut self, background: Rgba) -> Self {
        self.background = Some(background);
        self
    }

    /// Sets bold attribute.
    pub const fn bold(mut self) -> Self {
        self.bold = true;
        self
    }

    /// Sets italic attribute.
    pub const fn italic(mut self) -> Self {
        self.italic = true;
        self
    }

    /// Sets underline attribute.
    pub const fn underline(mut self, underline: UnderlineDecoration) -> Self {
        self.underline = Some(underline);
        self
    }

    /// Sets strikethrough attribute.
    pub const fn strikethrough(mut self) -> Self {
        self.strikethrough = true;
        self
    }
}

/// The style applied to a token, either via a semantic tag or direct visual attributes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StyleValue {
    /// Styled according to theme's mapping for this semantic tag.
    Tag(HighlightTag),
    /// Explicit styling attributes.
    Direct(TextStyle),
}

impl From<HighlightTag> for StyleValue {
    fn from(tag: HighlightTag) -> Self {
        Self::Tag(tag)
    }
}

impl From<TextStyle> for StyleValue {
    fn from(style: TextStyle) -> Self {
        Self::Direct(style)
    }
}

/// A styled region of text on a single line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyleSpan {
    /// Byte range within the line string (0-indexed).
    pub range: Range<usize>,
    /// The style applied to this range.
    pub style: StyleValue,
}

impl StyleSpan {
    /// Creates a new style span for a given range and style.
    pub fn new(range: Range<usize>, style: impl Into<StyleValue>) -> Self {
        Self {
            range,
            style: style.into(),
        }
    }

    /// Convenience constructor for semantic tag styling.
    pub fn tag(range: Range<usize>, tag: HighlightTag) -> Self {
        Self {
            range,
            style: StyleValue::Tag(tag),
        }
    }

    /// Convenience constructor for direct text styling.
    pub fn direct(range: Range<usize>, style: TextStyle) -> Self {
        Self {
            range,
            style: StyleValue::Direct(style),
        }
    }
}

/// Trait implemented by language tokenizers and syntax highlighters.
pub trait SyntaxHighlighter: Send + Sync + 'static {
    /// Analyzes a single line of text and returns all highlight spans for that line.
    ///
    /// Ranges in the returned `StyleSpan`s are byte offsets relative to `line_text`.
    fn highlight_line(&self, buffer: &EditorBuffer, row: usize, line_text: &str) -> Vec<StyleSpan>;
}

/// A contiguous segment of text on a line with its resolved style and selection status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyledSegment<'a> {
    /// Byte range within the line.
    pub range: Range<usize>,
    /// The style value covering this segment, if any.
    pub style: Option<&'a StyleValue>,
    /// Whether this segment is within the active selection.
    pub is_selected: bool,
}

/// Computes disjoint, sorted styled segments for a line by splitting across syntax span and selection boundaries.
pub fn split_line_intervals<'a>(
    line_len: usize,
    spans: &'a [StyleSpan],
    selection_range: Option<(usize, usize)>,
) -> Vec<StyledSegment<'a>> {
    if line_len == 0 {
        return Vec::new();
    }

    let mut boundaries = Vec::with_capacity(spans.len() * 2 + 4);
    boundaries.push(0);
    boundaries.push(line_len);

    if let Some((s_start, s_end)) = selection_range {
        boundaries.push(s_start.min(line_len));
        boundaries.push(s_end.min(line_len));
    }

    for span in spans {
        boundaries.push(span.range.start.min(line_len));
        boundaries.push(span.range.end.min(line_len));
    }

    boundaries.sort_unstable();
    boundaries.dedup();

    let mut segments = Vec::with_capacity(boundaries.len());

    for window in boundaries.windows(2) {
        let start = window[0];
        let end = window[1];
        if start >= end {
            continue;
        }

        let is_selected = if let Some((s_start, s_end)) = selection_range {
            start >= s_start && end <= s_end
        } else {
            false
        };

        let style = spans
            .iter()
            .rev()
            .find(|s| s.range.start <= start && end <= s.range.end)
            .map(|s| &s.style);

        segments.push(StyledSegment {
            range: start..end,
            style,
            is_selected,
        });
    }

    segments
}

/// A rendered visual line where concealed syntax tokens have been collapsed,
/// maintaining exact bidirectional mapping to source buffer byte offsets.
#[derive(Debug, Clone)]
pub struct ConcealedLine {
    /// The transformed text to be shaped and rendered on screen.
    pub display_text: String,
    /// Syntax highlight spans adjusted to display text coordinates.
    pub spans: Vec<StyleSpan>,
    /// Map from display text byte offset to source buffer line byte offset.
    byte_map: Vec<usize>,
}

impl ConcealedLine {
    /// Constructs a concealed line from raw line text and syntax spans.
    pub fn build(line_text: &str, spans: &[StyleSpan]) -> Self {
        let has_hidden = spans
            .iter()
            .any(|s| matches!(s.style, StyleValue::Tag(HighlightTag::Hidden)));

        if !has_hidden {
            let byte_map = (0..=line_text.len()).collect();
            return Self {
                display_text: line_text.to_string(),
                spans: spans.to_vec(),
                byte_map,
            };
        }

        let mut display_text = String::with_capacity(line_text.len());
        let mut byte_map = Vec::with_capacity(line_text.len() + 1);

        for (byte_idx, ch) in line_text.char_indices() {
            let is_hidden = spans.iter().any(|s| {
                matches!(s.style, StyleValue::Tag(HighlightTag::Hidden))
                    && s.range.contains(&byte_idx)
            });

            if !is_hidden {
                let ch_len = ch.len_utf8();
                for b in 0..ch_len {
                    byte_map.push(byte_idx + b);
                }
                display_text.push(ch);
            }
        }
        byte_map.push(line_text.len());

        let mut new_spans = Vec::new();
        for span in spans {
            if matches!(span.style, StyleValue::Tag(HighlightTag::Hidden)) {
                continue;
            }

            let new_start = byte_map
                .iter()
                .position(|&src_idx| src_idx >= span.range.start)
                .unwrap_or(display_text.len());
            let new_end = byte_map
                .iter()
                .position(|&src_idx| src_idx >= span.range.end)
                .unwrap_or(display_text.len());

            if new_start < new_end {
                new_spans.push(StyleSpan {
                    range: new_start..new_end,
                    style: span.style.clone(),
                });
            }
        }

        Self {
            display_text,
            spans: new_spans,
            byte_map,
        }
    }

    /// Converts a display byte offset to a source buffer line byte offset.
    pub fn display_to_source(&self, display_col: usize) -> usize {
        if display_col >= self.byte_map.len() {
            *self.byte_map.last().unwrap_or(&0)
        } else {
            self.byte_map[display_col]
        }
    }

    /// Converts a source buffer line byte offset to the nearest display byte offset.
    pub fn source_to_display(&self, source_col: usize) -> usize {
        self.byte_map
            .partition_point(|&src_idx| src_idx < source_col)
            .min(self.display_text.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockHighlighter;

    impl SyntaxHighlighter for MockHighlighter {
        fn highlight_line(
            &self,
            _buffer: &EditorBuffer,
            _row: usize,
            line_text: &str,
        ) -> Vec<StyleSpan> {
            if line_text.starts_with("# ") {
                vec![StyleSpan::tag(0..line_text.len(), HighlightTag::Heading1)]
            } else {
                vec![]
            }
        }
    }

    #[test]
    fn test_syntax_highlighter_trait() {
        let buffer = EditorBuffer::new("# Title\nBody");
        let highlighter = MockHighlighter;

        let spans_0 = highlighter.highlight_line(&buffer, 0, "# Title");
        assert_eq!(spans_0.len(), 1);
        assert_eq!(spans_0[0].range, 0..7);
        assert_eq!(spans_0[0].style, StyleValue::Tag(HighlightTag::Heading1));

        let spans_1 = highlighter.highlight_line(&buffer, 1, "Body");
        assert!(spans_1.is_empty());
    }

    #[test]
    fn test_rgba_hex_conversion() {
        let red = Rgba::hex(0xFF0000);
        assert_eq!(red, Rgba::new(255, 0, 0, 255));

        let custom = Rgba::hex(0x123456);
        assert_eq!(custom, Rgba::new(0x12, 0x34, 0x56, 255));
    }

    #[test]
    fn test_split_line_empty() {
        let segments = split_line_intervals(0, &[], None);
        assert!(segments.is_empty());
    }

    #[test]
    fn test_split_line_plain_text() {
        let segments = split_line_intervals(11, &[], None);
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].range, 0..11);
        assert_eq!(segments[0].style, None);
        assert!(!segments[0].is_selected);
    }

    #[test]
    fn test_split_line_with_single_span() {
        let spans = vec![StyleSpan::tag(0..5, HighlightTag::Keyword)];
        let segments = split_line_intervals(11, &spans, None);

        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].range, 0..5);
        assert_eq!(
            segments[0].style,
            Some(&StyleValue::Tag(HighlightTag::Keyword))
        );
        assert!(!segments[0].is_selected);

        assert_eq!(segments[1].range, 5..11);
        assert_eq!(segments[1].style, None);
        assert!(!segments[1].is_selected);
    }

    #[test]
    fn test_split_line_with_overlapping_selection() {
        let spans = vec![StyleSpan::tag(0..5, HighlightTag::Keyword)];
        let segments = split_line_intervals(11, &spans, Some((3, 8)));

        assert_eq!(segments.len(), 4);

        assert_eq!(segments[0].range, 0..3);
        assert_eq!(
            segments[0].style,
            Some(&StyleValue::Tag(HighlightTag::Keyword))
        );
        assert!(!segments[0].is_selected);

        assert_eq!(segments[1].range, 3..5);
        assert_eq!(
            segments[1].style,
            Some(&StyleValue::Tag(HighlightTag::Keyword))
        );
        assert!(segments[1].is_selected);

        assert_eq!(segments[2].range, 5..8);
        assert_eq!(segments[2].style, None);
        assert!(segments[2].is_selected);

        assert_eq!(segments[3].range, 8..11);
        assert_eq!(segments[3].style, None);
        assert!(!segments[3].is_selected);
    }

    #[test]
    fn test_concealed_line_headings_align_and_collapse() {
        let line1 = "# hello";
        let spans1 = vec![
            StyleSpan::tag(0..2, HighlightTag::Hidden),
            StyleSpan::tag(2..7, HighlightTag::Heading1),
        ];
        let concealed1 = ConcealedLine::build(line1, &spans1);
        assert_eq!(concealed1.display_text, "hello");
        assert_eq!(concealed1.spans.len(), 1);
        assert_eq!(concealed1.spans[0].range, 0..5);
        assert_eq!(
            concealed1.spans[0].style,
            StyleValue::Tag(HighlightTag::Heading1)
        );
        assert_eq!(concealed1.display_to_source(0), 2);
        assert_eq!(concealed1.source_to_display(2), 0);

        let line2 = "## hello";
        let spans2 = vec![
            StyleSpan::tag(0..3, HighlightTag::Hidden),
            StyleSpan::tag(3..8, HighlightTag::Heading2),
        ];
        let concealed2 = ConcealedLine::build(line2, &spans2);
        assert_eq!(concealed2.display_text, "hello");
        assert_eq!(concealed2.spans.len(), 1);
        assert_eq!(concealed2.spans[0].range, 0..5);
        assert_eq!(
            concealed2.spans[0].style,
            StyleValue::Tag(HighlightTag::Heading2)
        );
        assert_eq!(concealed2.display_to_source(0), 3);
        assert_eq!(concealed2.source_to_display(3), 0);

        assert_eq!(concealed1.display_text, concealed2.display_text);

        let line_inline = "Hi **bold**!";
        let spans_inline = vec![
            StyleSpan::tag(3..5, HighlightTag::Hidden),
            StyleSpan::tag(5..9, HighlightTag::Bold),
            StyleSpan::tag(9..11, HighlightTag::Hidden),
        ];
        let concealed_inline = ConcealedLine::build(line_inline, &spans_inline);
        assert_eq!(concealed_inline.display_text, "Hi bold!");
        assert_eq!(concealed_inline.spans.len(), 1);
        assert_eq!(concealed_inline.spans[0].range, 3..7);
        assert_eq!(
            concealed_inline.spans[0].style,
            StyleValue::Tag(HighlightTag::Bold)
        );
        assert_eq!(concealed_inline.display_to_source(3), 5);
        assert_eq!(concealed_inline.source_to_display(5), 3);
    }
}
