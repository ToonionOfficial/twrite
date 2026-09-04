use std::ops::Range;

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

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
    links
}

#[cfg(test)]
mod tests {
    use super::super::highlight::MarkdownHighlighter;
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
}
