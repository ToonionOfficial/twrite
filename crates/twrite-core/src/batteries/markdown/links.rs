use std::ops::Range;

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

/// URL scheme prefix marking wikilink destinations from [`extract_markdown_links`].
/// Hosts branch on this prefix to route note navigation instead of opening a browser.
pub const WIKILINK_SCHEME: &str = "wikilink:";

/// A parsed `[[Note]]`, `[[Note|Alias]]`, or `[[Note#Heading]]` reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WikilinkTarget {
    /// Byte range of the whole `[[...]]` construct within the line.
    pub full_range: Range<usize>,
    /// Byte range of the visible label (alias when present, else the target text).
    pub display_range: Range<usize>,
    /// Target note name, trimmed.
    pub note: String,
    /// Target heading within the note, if any.
    pub heading: Option<String>,
    /// Display alias, if any.
    pub alias: Option<String>,
}

/// Builds the link URL for a wikilink target (`wikilink:Note#Heading`).
pub fn wikilink_url(note: &str, heading: Option<&str>) -> String {
    match heading {
        Some(heading) => format!("{WIKILINK_SCHEME}{note}#{heading}"),
        None => format!("{WIKILINK_SCHEME}{note}"),
    }
}

/// Reports whether an extracted link URL points at a wikilink target.
pub fn is_wikilink_url(url: &str) -> bool {
    url.starts_with(WIKILINK_SCHEME)
}

/// Parses wikilink references on a single line.
///
/// `![[...]]` embeds stay literal for the transclusions issue to claim.
/// Empty content, unclosed openers, and nested openers never parse.
pub fn parse_wikilinks(line_text: &str) -> Vec<WikilinkTarget> {
    let bytes = line_text.as_bytes();
    let mut targets = Vec::new();
    let mut search_from = 0;
    while search_from + 1 < bytes.len() {
        let open = find_wikilink_open(bytes, search_from);
        let Some(open) = open else {
            break;
        };
        let content_start = open + 2;
        let Some(close) = find_wikilink_close(bytes, content_start) else {
            break;
        };
        let content = &line_text[content_start..close];
        if let Some(target) = parse_wikilink_content(line_text, content, open, close + 2) {
            targets.push(target);
            search_from = close + 2;
        } else {
            search_from = open + 2;
        }
    }
    targets
}

fn find_wikilink_open(bytes: &[u8], from: usize) -> Option<usize> {
    let mut index = from;
    while index + 1 < bytes.len() {
        if bytes[index] == b'[' && bytes[index + 1] == b'[' {
            let embed = index > 0 && bytes[index - 1] == b'!';
            let escaped = index > 0 && bytes[index - 1] == b'\\';
            if !embed && !escaped {
                return Some(index);
            }
            index += 2;
        } else {
            index += 1;
        }
    }
    None
}

fn find_wikilink_close(bytes: &[u8], from: usize) -> Option<usize> {
    let mut index = from;
    while index + 1 < bytes.len() {
        if bytes[index] == b'[' && bytes[index + 1] == b'[' {
            return None;
        }
        if bytes[index] == b']' && bytes[index + 1] == b']' {
            return Some(index);
        }
        index += 1;
    }
    None
}

fn parse_wikilink_content(
    line_text: &str,
    content: &str,
    open: usize,
    full_end: usize,
) -> Option<WikilinkTarget> {
    if content.is_empty() || content.trim().is_empty() {
        return None;
    }
    let (target_text, alias) = match content.find('|') {
        Some(pipe) => {
            let alias = content[pipe + 1..].trim();
            if alias.is_empty() {
                return None;
            }
            (&content[..pipe], Some(alias.to_string()))
        }
        None => (content, None),
    };
    let (note_text, heading) = match target_text.find('#') {
        Some(hash) => {
            let heading = target_text[hash + 1..].trim();
            if heading.is_empty() {
                return None;
            }
            (&target_text[..hash], Some(heading.to_string()))
        }
        None => (target_text, None),
    };
    let note = note_text.trim();
    if note.is_empty() || note.contains('[') || note.contains(']') {
        return None;
    }
    let content_start = open + 2;
    let display_range = match content.find('|') {
        Some(pipe) => {
            let alias_start = content_start + pipe + 1;
            alias_start..content_start + content.len()
        }
        None => content_start..content_start + content.len(),
    };
    let _ = line_text;
    Some(WikilinkTarget {
        full_range: open..full_end,
        display_range,
        note: note.to_string(),
        heading,
        alias,
    })
}

/// Extracts all markdown hyperlink destinations and their label character ranges from a single line.
pub fn extract_markdown_links(line_text: &str) -> Vec<(Range<usize>, String)> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    let parser = Parser::new_ext(line_text, options).into_offset_iter();
    let mut links = Vec::new();
    let mut current_link: Option<(usize, String)> = None;

    for (event, range) in parser {
        match event {
            Event::Start(Tag::Link { dest_url, .. }) => {
                current_link = Some((range.start, dest_url.to_string()));
            }
            Event::End(TagEnd::Link) => {
                if let Some((start, dest_url)) = current_link.take() {
                    let end = range.end.min(line_text.len());
                    if let Some(bracket_idx) = line_text[start..end].find("](") {
                        let label_start = start + 1;
                        let label_end = start + bracket_idx;
                        links.push((label_start..label_end, dest_url));
                    } else if let Some(bracket_idx) = line_text[start..end].find("][") {
                        let label_start = start + 1;
                        let label_end = start + bracket_idx;
                        links.push((label_start..label_end, dest_url));
                    } else if line_text[start..end].starts_with('<')
                        && line_text[start..end].ends_with('>')
                    {
                        links.push((start + 1..end - 1, dest_url));
                    } else {
                        links.push((start..end, dest_url));
                    }
                }
            }
            _ => {}
        }
    }
    for target in parse_wikilinks(line_text) {
        let overlaps = links.iter().any(|(range, _)| {
            range.start < target.full_range.end && target.full_range.start < range.end
        });
        if overlaps {
            continue;
        }
        let url = wikilink_url(&target.note, target.heading.as_deref());
        links.push((target.display_range.clone(), url));
    }
    links
}

#[cfg(test)]
mod tests {
    use super::super::highlight::MarkdownHighlighter;
    use super::*;
    use crate::{EditorBuffer, SyntaxHighlighter};

    #[test]
    fn test_markdown_highlighter_extract_links_trait() {
        let buffer = EditorBuffer::new("[Google](https://google.com)");
        let highlighter = MarkdownHighlighter::new();
        let links = highlighter.extract_links(&buffer, 0, "[Google](https://google.com)");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].0, 1..7);
        assert_eq!(links[0].1, "https://google.com");

        let none = highlighter.extract_links(&buffer, 0, "plain text");
        assert!(none.is_empty());
    }

    #[test]
    fn test_parse_wikilink_forms() {
        let plain = parse_wikilinks("See [[Note]] here");
        assert_eq!(plain.len(), 1);
        assert_eq!(plain[0].note, "Note");
        assert_eq!(plain[0].heading, None);
        assert_eq!(plain[0].alias, None);
        assert_eq!(plain[0].full_range, 4..12);
        assert_eq!(plain[0].display_range, 6..10);

        let aliased = parse_wikilinks("See [[Note|Alias]] here");
        assert_eq!(aliased.len(), 1);
        assert_eq!(aliased[0].note, "Note");
        assert_eq!(aliased[0].alias.as_deref(), Some("Alias"));
        assert_eq!(aliased[0].display_range, 11..16);

        let headed = parse_wikilinks("See [[Note#Heading]] here");
        assert_eq!(headed.len(), 1);
        assert_eq!(headed[0].note, "Note");
        assert_eq!(headed[0].heading.as_deref(), Some("Heading"));

        let both = parse_wikilinks("[[Note#Heading|Alias]]");
        assert_eq!(both.len(), 1);
        assert_eq!(both[0].note, "Note");
        assert_eq!(both[0].heading.as_deref(), Some("Heading"));
        assert_eq!(both[0].alias.as_deref(), Some("Alias"));
    }

    #[test]
    fn test_parse_wikilink_literal_cases() {
        for line in [
            "unclosed [[Note",
            "empty [[]]",
            "blank [[  ]]",
            "no links here",
        ] {
            assert!(
                parse_wikilinks(line).is_empty(),
                "{line:?} must not parse as a wikilink"
            );
        }
        assert!(
            parse_wikilinks("![[Embed]]").is_empty(),
            "embeds stay literal until transclusions land"
        );
        let multi = parse_wikilinks("[[One]] and [[Two|2]]");
        assert_eq!(multi.len(), 2);
        assert_eq!(multi[0].note, "One");
        assert_eq!(multi[1].note, "Two");
    }

    #[test]
    fn test_extract_links_includes_wikilinks() {
        let buffer = EditorBuffer::new("See [[Note|Alias]] here");
        let highlighter = MarkdownHighlighter::new();
        let links = highlighter.extract_links(&buffer, 0, "See [[Note|Alias]] here");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].0, 11..16);
        assert_eq!(links[0].1, "wikilink:Note");
    }
}
