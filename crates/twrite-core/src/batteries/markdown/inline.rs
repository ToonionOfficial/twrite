use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

use crate::syntax::{HighlightTag, StyleSpan, TextStyle};

/// Parses inline CommonMark and GFM elements (bold, italic, strikethrough, code, links)
/// within a single line and appends corresponding style spans.
pub(crate) fn highlight_inline_markdown(
    line_text: &str,
    is_cursor_row: bool,
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
                        if !is_cursor_row
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
                        if !is_cursor_row
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
                        if !is_cursor_row
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
                if !is_cursor_row
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
                        if !is_cursor_row && let Some(delim_tag) = delimiter_tag {
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
}
