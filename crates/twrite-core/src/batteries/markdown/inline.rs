use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

use super::links::parse_wikilinks;
use crate::syntax::{HighlightTag, StyleSpan, StyleValue, TextStyle};

/// Parses inline CommonMark and GFM elements (bold, italic, highlight,
/// strikethrough, code, links) within a single line and appends corresponding
/// style spans.
///
/// `cursor_offset` is the cursor byte offset within the line when the cursor
/// sits on this row (`None` otherwise). A construct whose source range
/// contains the cursor keeps its delimiters visible so it can be edited;
/// every other construct conceals, Obsidian style.
pub(crate) fn highlight_inline_markdown(
    line_text: &str,
    cursor_offset: Option<usize>,
    delimiter_tag: Option<HighlightTag>,
    spans: &mut Vec<StyleSpan>,
) {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_TABLES);

    let parser = Parser::new_ext(line_text, options).into_offset_iter();

    let mut active_strong = None;
    let mut active_emphasis = None;
    let mut active_strike = None;
    let mut active_link = None;

    for (event, range) in parser {
        match event {
            Event::Start(Tag::Strong) => {
                active_strong = Some(range.start);
            }
            Event::End(TagEnd::Strong) => {
                if let Some(start) = active_strong.take() {
                    let end = range.end.min(line_text.len());
                    if start < end {
                        if !cursor_inside_construct(cursor_offset, start, end)
                            && end >= start + 4
                            && let Some(delim_tag) = delimiter_tag
                        {
                            spans.push(StyleSpan::tag(start..start + 2, delim_tag));
                            spans.push(StyleSpan::tag(start + 2..end - 2, HighlightTag::Bold));
                            spans.push(StyleSpan::tag(end - 2..end, delim_tag));
                        } else {
                            spans.push(StyleSpan::tag(start..end, HighlightTag::Bold));
                        }
                    }
                }
            }
            Event::Start(Tag::Emphasis) => {
                active_emphasis = Some(range.start);
            }
            Event::End(TagEnd::Emphasis) => {
                if let Some(start) = active_emphasis.take() {
                    let end = range.end.min(line_text.len());
                    if start < end {
                        if !cursor_inside_construct(cursor_offset, start, end)
                            && end >= start + 2
                            && let Some(delim_tag) = delimiter_tag
                        {
                            spans.push(StyleSpan::tag(start..start + 1, delim_tag));
                            spans.push(StyleSpan::tag(start + 1..end - 1, HighlightTag::Italic));
                            spans.push(StyleSpan::tag(end - 1..end, delim_tag));
                        } else {
                            spans.push(StyleSpan::tag(start..end, HighlightTag::Italic));
                        }
                    }
                }
            }
            Event::Start(Tag::Strikethrough) => {
                active_strike = Some(range.start);
            }
            Event::End(TagEnd::Strikethrough) => {
                if let Some(start) = active_strike.take() {
                    let end = range.end.min(line_text.len());
                    if start < end {
                        if !cursor_inside_construct(cursor_offset, start, end)
                            && end >= start + 4
                            && let Some(delim_tag) = delimiter_tag
                        {
                            spans.push(StyleSpan::tag(start..start + 2, delim_tag));
                            spans.push(StyleSpan::direct(
                                start + 2..end - 2,
                                TextStyle {
                                    strikethrough: true,
                                    ..Default::default()
                                },
                            ));
                            spans.push(StyleSpan::tag(end - 2..end, delim_tag));
                        } else {
                            spans.push(StyleSpan::direct(
                                start..end,
                                TextStyle {
                                    strikethrough: true,
                                    ..Default::default()
                                },
                            ));
                        }
                    }
                }
            }
            Event::Code(cow) => {
                let end = (range.start + cow.len() + 2).min(line_text.len());
                if !cursor_inside_construct(cursor_offset, range.start, end)
                    && end >= range.start + 2
                    && let Some(delim_tag) = delimiter_tag
                {
                    spans.push(StyleSpan::tag(range.start..range.start + 1, delim_tag));
                    spans.push(StyleSpan::tag(range.start + 1..end - 1, HighlightTag::Code));
                    spans.push(StyleSpan::tag(end - 1..end, delim_tag));
                } else {
                    spans.push(StyleSpan::tag(range.start..end, HighlightTag::Code));
                }
            }
            Event::Start(Tag::Link { .. }) => {
                active_link = Some(range.start);
            }
            Event::End(TagEnd::Link) => {
                if let Some(start) = active_link.take() {
                    let end = range.end.min(line_text.len());
                    if start < end {
                        if !cursor_inside_construct(cursor_offset, start, end)
                            && let Some(delim_tag) = delimiter_tag
                        {
                            if let Some(bracket_idx) = line_text[start..end].find("](") {
                                let label_start = start + 1;
                                let label_end = start + bracket_idx;
                                spans.push(StyleSpan::tag(start..label_start, delim_tag));
                                spans.push(StyleSpan::tag(
                                    label_start..label_end,
                                    HighlightTag::Link,
                                ));
                                spans.push(StyleSpan::tag(label_end..end, delim_tag));
                            } else if let Some(bracket_idx) = line_text[start..end].find("][") {
                                let label_start = start + 1;
                                let label_end = start + bracket_idx;
                                spans.push(StyleSpan::tag(start..label_start, delim_tag));
                                spans.push(StyleSpan::tag(
                                    label_start..label_end,
                                    HighlightTag::Link,
                                ));
                                spans.push(StyleSpan::tag(label_end..end, delim_tag));
                            } else if line_text[start..end].starts_with('<')
                                && line_text[start..end].ends_with('>')
                            {
                                spans.push(StyleSpan::tag(start..start + 1, delim_tag));
                                spans.push(StyleSpan::tag(start + 1..end - 1, HighlightTag::Link));
                                spans.push(StyleSpan::tag(end - 1..end, delim_tag));
                            } else {
                                spans.push(StyleSpan::tag(start..end, HighlightTag::Link));
                            }
                        } else {
                            spans.push(StyleSpan::tag(start..end, HighlightTag::Link));
                        }
                    }
                }
            }
            Event::TaskListMarker(checked) => {
                let tag = if checked {
                    HighlightTag::String
                } else {
                    HighlightTag::Comment
                };
                spans.push(StyleSpan::tag(range, tag));
            }
            _ => {}
        }
    }

    // pulldown-cmark has no mark support, so `==` pairs are scanned
    // separately against spans the pulldown pass already emitted.
    highlight_mark_spans(line_text, cursor_offset, delimiter_tag, spans);
    highlight_wikilink_spans(line_text, cursor_offset, delimiter_tag, spans);
}

/// Reports whether the cursor sits inside a concealable construct so its
/// delimiters stay visible for editing. The end edge is exclusive: a cursor
/// resting just past the closing delimiter has left the construct.
fn cursor_inside_construct(cursor_offset: Option<usize>, start: usize, end: usize) -> bool {
    cursor_offset.is_some_and(|cursor| start <= cursor && cursor < end)
}

/// Scans `==mark==` pairs pulldown-cmark does not parse and appends
/// highlight spans with the same conceal/reveal behavior as emphasis.
///
/// A pair whose range intersects an emitted code, link-delimiter, or
/// concealed span stays literal, so `==` inside code spans and URLs never
/// corrupts them. Overlap with emphasis spans is allowed for nesting.
fn highlight_mark_spans(
    line_text: &str,
    cursor_offset: Option<usize>,
    delimiter_tag: Option<HighlightTag>,
    spans: &mut Vec<StyleSpan>,
) {
    let bytes = line_text.as_bytes();
    let mut search_from = 0;
    while search_from + 1 < bytes.len() {
        let Some(open) = find_mark_delimiter(bytes, search_from, true) else {
            break;
        };
        let content_start = open + 2;
        let Some(close) = find_mark_delimiter(bytes, content_start, false) else {
            break;
        };
        // The opener search resumes after a failed pair so one broken run
        // cannot swallow a later valid one (`==a ==b==` highlights `b`).
        if close == content_start || overlaps_reserved(spans, open, close + 2) {
            search_from = open + 1;
            continue;
        }
        let end = close + 2;
        if !cursor_inside_construct(cursor_offset, open, end)
            && let Some(delim_tag) = delimiter_tag
        {
            spans.push(StyleSpan::tag(open..open + 2, delim_tag));
            spans.push(StyleSpan::tag(
                content_start..close,
                HighlightTag::Highlight,
            ));
            spans.push(StyleSpan::tag(close..end, delim_tag));
        } else {
            spans.push(StyleSpan::tag(open..end, HighlightTag::Highlight));
        }
        search_from = end;
    }
}

/// Locates the next `==` delimiter at or after `from`. Openers must not run
/// into a third `=` and must lead with content (`== x` stays literal);
/// closers must follow content (`x ==` stays literal). Closer search always
/// starts past an opener, so the byte before a candidate always exists.
fn find_mark_delimiter(bytes: &[u8], from: usize, opening: bool) -> Option<usize> {
    let mut index = from;
    while index + 1 < bytes.len() {
        if bytes[index] == b'=' && bytes[index + 1] == b'=' {
            let adjacent = if opening {
                index + 2
            } else {
                index.saturating_sub(1)
            };
            let adjacent_ok = bytes
                .get(adjacent)
                .is_none_or(|byte| !byte.is_ascii_whitespace() && *byte != b'=');
            if adjacent_ok {
                return Some(index);
            }
            index += 1;
        } else {
            index += 1;
        }
    }
    None
}

/// Reports whether `start..end` intersects an emitted span that mark pairs
/// must not cross: code spans, link brackets/URLs, and concealed ranges.
fn overlaps_reserved(spans: &[StyleSpan], start: usize, end: usize) -> bool {
    spans.iter().any(|span| {
        matches!(
            span.style,
            StyleValue::Tag(HighlightTag::Code | HighlightTag::Hidden | HighlightTag::Dimmed)
        ) && span.range.start < end
            && start < span.range.end
    })
}

/// Emits `Link` spans for `[[Note]]`, `[[Note|Alias]]`, and
/// `[[Note#Heading]]` with the same conceal/reveal behavior as standard
/// links: inactive rows show the label only, the cursor row reveals the raw
/// construct for editing. Aliased links conceal the `[[Note|` prefix so only
/// the alias reads.
fn highlight_wikilink_spans(
    line_text: &str,
    cursor_offset: Option<usize>,
    delimiter_tag: Option<HighlightTag>,
    spans: &mut Vec<StyleSpan>,
) {
    for target in parse_wikilinks(line_text) {
        let start = target.full_range.start;
        let end = target.full_range.end;
        let overlaps_link = spans.iter().any(|span| {
            matches!(span.style, StyleValue::Tag(HighlightTag::Link))
                && span.range.start < end
                && start < span.range.end
        });
        if overlaps_link || overlaps_reserved(spans, start, end) {
            continue;
        }
        if cursor_inside_construct(cursor_offset, start, end) || delimiter_tag.is_none() {
            spans.push(StyleSpan::tag(start..end, HighlightTag::Link));
            continue;
        }
        let delimiter_tag = delimiter_tag.unwrap_or(HighlightTag::Dimmed);
        let display = target.display_range.clone();
        if display.start > start {
            spans.push(StyleSpan::tag(start..display.start, delimiter_tag));
        }
        spans.push(StyleSpan::tag(display.clone(), HighlightTag::Link));
        if display.end < end {
            spans.push(StyleSpan::tag(display.end..end, delimiter_tag));
        }
    }
}
