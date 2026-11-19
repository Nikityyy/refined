//! Converts document structure to Windows HTML clipboard data.

use pulldown_cmark::{html, Event, Parser, Tag, TagEnd};

pub struct RichPaste {
    pub html: String,
    pub plain: String,
}

pub fn render(markdown: &str) -> Option<RichPaste> {
    let markdown = normalize_ordered_bullets(markdown);
    let mut fragment = String::new();
    let mut plain = String::new();
    let mut lists: Vec<Option<u64>> = Vec::new();
    let mut structured = false;

    html::push_html(
        &mut fragment,
        Parser::new(&markdown).filter_map(|event| {
            match &event {
                Event::Start(Tag::Heading { .. } | Tag::Emphasis | Tag::Strong) => {
                    structured = true
                }
                Event::Start(Tag::List(start)) => {
                    structured = true;
                    lists.push(*start);
                }
                Event::Start(Tag::Item) => {
                    end_line(&mut plain);
                    for _ in 1..lists.len() {
                        plain.push_str("  ");
                    }
                    if let Some(Some(number)) = lists.last_mut() {
                        plain.push_str(&format!("{number}. "));
                        *number = number.saturating_add(1);
                    } else {
                        plain.push_str("\u{2022} ");
                    }
                }
                Event::End(TagEnd::List(_)) => {
                    lists.pop();
                }
                Event::End(TagEnd::Item | TagEnd::Heading(_) | TagEnd::Paragraph) => {
                    end_line(&mut plain);
                }
                Event::HardBreak => plain.push('\n'),
                Event::SoftBreak => plain.push(' '),
                Event::Text(text) | Event::Code(text) => plain.push_str(text),
                Event::Html(text) | Event::InlineHtml(text) => {
                    plain.push_str(text);
                    return Some(Event::Text(text.clone()));
                }
                // Preserve labels while preventing the paste from loading external content.
                Event::Start(Tag::Link { .. } | Tag::Image { .. })
                | Event::End(TagEnd::Link | TagEnd::Image) => return None,
                _ => {}
            }
            Some(event)
        }),
    );

    structured.then(|| RichPaste {
        html: clipboard_html(&fragment),
        plain: plain.trim_end_matches('\n').to_owned(),
    })
}

fn end_line(text: &mut String) {
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
}

fn normalize_ordered_bullets(markdown: &str) -> String {
    let mut normalized = String::with_capacity(markdown.len());
    for line in markdown.split_inclusive('\n') {
        let body = line.trim_start_matches([' ', '\t']);
        if let Some(ordered) = body.strip_prefix("- ") {
            let digits = ordered.bytes().take_while(u8::is_ascii_digit).count();
            if digits > 0 && ordered[digits..].starts_with(". ") {
                normalized.push_str(&line[..line.len() - body.len()]);
                normalized.push_str(ordered);
                continue;
            }
        }
        normalized.push_str(line);
    }
    normalized
}

fn clipboard_html(fragment: &str) -> String {
    const BEFORE: &str = "<html><head><meta charset=\"utf-8\"></head><body><!--StartFragment-->";
    const AFTER: &str = "<!--EndFragment--></body></html>";
    const HEADER: &str = "Version:1.0\r\nStartHTML:0000000000\r\nEndHTML:0000000000\r\nStartFragment:0000000000\r\nEndFragment:0000000000\r\n";
    // Clipboard format offsets count UTF-8 bytes from the start of the header.
    let start_html = HEADER.len();
    let start_fragment = start_html + BEFORE.len();
    let end_fragment = start_fragment + fragment.len();
    let end_html = end_fragment + AFTER.len();
    format!(
        "Version:1.0\r\nStartHTML:{start_html:010}\r\nEndHTML:{end_html:010}\r\nStartFragment:{start_fragment:010}\r\nEndFragment:{end_fragment:010}\r\n{BEFORE}{fragment}{AFTER}\0"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_a_heading_and_single_numbered_list_with_utf8_offsets() {
        let rendered = render("# Betriebswirtschaft\n\n- 1. \u{00d6}konomie\n- 2. Sachen").unwrap();
        let offset = |name| {
            rendered
                .html
                .lines()
                .find_map(|line| line.strip_prefix(name))
                .unwrap()
                .parse::<usize>()
                .unwrap()
        };
        let fragment = &rendered.html[offset("StartFragment:")..offset("EndFragment:")];
        assert!(fragment.contains("<h1>Betriebswirtschaft</h1>"));
        assert!(fragment.contains("<ol>\n<li>\u{00d6}konomie</li>\n<li>Sachen</li>\n</ol>"));
        assert_eq!(
            rendered.plain,
            "Betriebswirtschaft\n1. \u{00d6}konomie\n2. Sachen"
        );
        assert_eq!(offset("EndHTML:"), rendered.html.len() - 1);
    }

    #[test]
    fn keeps_inline_formatting_and_neutralizes_markup_and_external_resources() {
        let rendered =
            render("**Text** ![Caption](https://example.com/image.png) <img src=x>").unwrap();
        assert!(rendered.html.contains("<strong>Text</strong>"));
        assert!(rendered.html.contains("Caption"));
        assert!(rendered.html.contains("&lt;img"));
        assert!(!rendered.html.contains("https://example.com"));
    }

    #[test]
    fn renders_bold_terms_in_a_numbered_list_without_a_heading() {
        let rendered = render(
            "Betriebswirtschaft ist wichtig.\n\n1. **\u{00d6}konomie** - Produktion und Konsum.\n2. **Wirtschaft** - G\u{00fc}ter und Dienstleistungen.",
        ).unwrap();
        assert!(rendered.html.contains("<ol>"));
        assert!(rendered.html.contains("<strong>\u{00d6}konomie</strong>"));
        assert!(rendered.html.contains("<strong>Wirtschaft</strong>"));
        assert!(rendered
            .plain
            .contains("1. \u{00d6}konomie - Produktion und Konsum."));
        assert!(!rendered.plain.contains("**"));
    }

    #[test]
    fn leaves_unformatted_text_to_the_standard_paste_path() {
        assert!(render("Ordinary text").is_none());
    }
}
