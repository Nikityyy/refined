//! Converts document structure to Windows HTML clipboard data, and marks
//! the subscripts and superscripts of a copied selection for the model.

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
    let mut script = None;

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
                Event::Text(text) | Event::Code(text) => match script {
                    Some(script) => push_script(&mut plain, script, text),
                    None => plain.push_str(text),
                },
                Event::InlineHtml(text) if script_tag(text).is_some() => {
                    let (kind, opening) = script_tag(text)?;
                    structured = true;
                    script = opening.then_some(kind);
                    let tag = match (kind, opening) {
                        (Script::Sub, true) => "<sub>",
                        (Script::Sub, false) => "</sub>",
                        (Script::Sup, true) => "<sup>",
                        (Script::Sup, false) => "</sup>",
                    };
                    return Some(Event::InlineHtml(tag.into()));
                }
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

#[derive(Clone, Copy, PartialEq)]
enum Script {
    Sub,
    Sup,
}

/// The tag kind and whether it opens, for exactly `<sub>`, `</sub>`,
/// `<sup>`, or `</sup>`. Every other tag stays literal text.
fn script_tag(tag: &str) -> Option<(Script, bool)> {
    let name = tag.strip_prefix('<')?.strip_suffix('>')?;
    let (name, opening) = name.strip_prefix('/').map_or((name, true), |name| (name, false));
    if name.eq_ignore_ascii_case("sub") {
        Some((Script::Sub, opening))
    } else if name.eq_ignore_ascii_case("sup") {
        Some((Script::Sup, opening))
    } else {
        None
    }
}

/// Plain-text editors get Unicode sub- and superscript characters, or
/// `_(…)` and `^(…)` when a character has no such form.
fn push_script(plain: &mut String, script: Script, text: &str) {
    const SUB: (&str, &str) = ("0123456789+-=()aehijklmnoprstuvx", "₀₁₂₃₄₅₆₇₈₉₊₋₌₍₎ₐₑₕᵢⱼₖₗₘₙₒₚᵣₛₜᵤᵥₓ");
    const SUP: (&str, &str) = ("0123456789+-=()in", "⁰¹²³⁴⁵⁶⁷⁸⁹⁺⁻⁼⁽⁾ⁱⁿ");
    let (from, to) = if script == Script::Sub { SUB } else { SUP };
    let mapped: Option<String> = text
        .chars()
        .map(|ch| from.chars().position(|c| c == ch).and_then(|i| to.chars().nth(i)))
        .collect();
    match mapped {
        Some(mapped) => plain.push_str(&mapped),
        None => {
            plain.push_str(if script == Script::Sub { "_(" } else { "^(" });
            plain.push_str(text);
            plain.push(')');
        }
    }
}

/// The text as plain-text editors receive it: `<sub>` and `<sup>` tags
/// become sub- and superscript characters. Used for diffs and word counts.
pub fn readable(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        let tag = rest.find('>').map(|end| &rest[..=end]);
        let opened = tag.and_then(script_tag).filter(|(_, opening)| *opening);
        let closed = tag.zip(opened).and_then(|(tag, (script, _))| {
            let body = &rest[tag.len()..];
            let close = if script == Script::Sub { "</sub>" } else { "</sup>" };
            let end = body.to_ascii_lowercase().find(close)?;
            Some((script, &body[..end], tag.len() + end + close.len()))
        });
        match closed {
            Some((script, inner, length)) if !inner.contains('<') => {
                push_script(&mut out, script, inner);
                rest = &rest[length..];
            }
            _ => {
                out.push('<');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

type Runs = Vec<(Option<Script>, String)>;

fn push_run(runs: &mut Runs, script: Option<Script>, text: &str) {
    match runs.last_mut() {
        Some((last, buffer)) if *last == script => buffer.push_str(text),
        _ => runs.push((script, text.to_owned())),
    }
}

/// Adds `<sub>` and `<sup>` tags to a copied plain-text selection, using the
/// HTML (Word, browsers) or RTF (Word, Acrobat) version of the same
/// selection. Returns None when neither has sub- or superscripts or when
/// they do not match the plain text.
pub fn mark_scripts(plain: &str, html: Option<&str>, rtf: Option<&[u8]>) -> Option<String> {
    html.and_then(html_runs)
        .and_then(|runs| align(plain, runs))
        .or_else(|| rtf.and_then(rtf_runs).and_then(|runs| align(plain, runs)))
}

fn has_script(runs: &Runs) -> bool {
    runs.iter().any(|(script, text)| script.is_some() && !text.trim().is_empty())
}

fn html_runs(html: &str) -> Option<Runs> {
    let html = html
        .split_once("<!--StartFragment-->")
        .map_or(html, |(_, rest)| rest.split("<!--EndFragment-->").next().unwrap_or(rest));
    let mut runs = Runs::new();
    let mut depth = [0usize; 2];
    let mut rest = html;
    while !rest.is_empty() {
        if let Some(comment) = rest.strip_prefix("<!--") {
            rest = &comment[comment.find("-->")? + 3..];
            continue;
        }
        if let Some(tag_body) = rest.strip_prefix('<') {
            let end = tag_body.find('>')?;
            let tag = &tag_body[..end];
            rest = &tag_body[end + 1..];
            let closing = tag.starts_with('/');
            let name = tag
                .trim_start_matches('/')
                .split(|c: char| !c.is_ascii_alphanumeric())
                .next()
                .unwrap_or_default()
                .to_ascii_lowercase();
            match name.as_str() {
                "style" | "script" | "head" | "title" if !closing => {
                    let close = format!("</{name}");
                    let index = rest.to_ascii_lowercase().find(&close)?;
                    rest = &rest[index..];
                }
                "sub" | "sup" => {
                    let slot = &mut depth[usize::from(name == "sup")];
                    *slot = if closing { slot.saturating_sub(1) } else { *slot + 1 };
                }
                _ => {}
            }
            continue;
        }
        let end = rest.find('<').unwrap_or(rest.len());
        let script = if depth[0] > 0 {
            Some(Script::Sub)
        } else if depth[1] > 0 {
            Some(Script::Sup)
        } else {
            None
        };
        push_run(&mut runs, script, &decode_entities(&rest[..end]));
        rest = &rest[end..];
    }
    has_script(&runs).then_some(runs)
}

/// Reads the text of an RTF document with its `\sub` and `\super` runs.
fn rtf_runs(rtf: &[u8]) -> Option<Runs> {
    #[derive(Clone, Copy)]
    struct Group {
        script: Option<Script>,
        skip: bool,
        unicode_skip: usize,
    }
    let mut runs = Runs::new();
    let mut stack = vec![Group { script: None, skip: false, unicode_skip: 1 }];
    let mut pending_skip = 0usize;
    let mut i = 0;
    let push = |runs: &mut Runs, group: &Group, text: &str| {
        if !group.skip {
            push_run(runs, group.script, text);
        }
    };
    while i < rtf.len() {
        let group = *stack.last()?;
        match rtf[i] {
            b'{' => {
                stack.push(group);
                i += 1;
            }
            b'}' => {
                stack.pop();
                if stack.is_empty() {
                    break;
                }
                i += 1;
            }
            b'\\' => {
                i += 1;
                let Some(&next) = rtf.get(i) else { break };
                if next == b'\'' {
                    let hex = std::str::from_utf8(rtf.get(i + 1..i + 3)?).ok()?;
                    let byte = u8::from_str_radix(hex, 16).ok()?;
                    i += 3;
                    if pending_skip > 0 {
                        pending_skip -= 1;
                    } else {
                        push(&mut runs, &group, &windows_1252(byte).to_string());
                    }
                    continue;
                }
                if !next.is_ascii_alphabetic() {
                    i += 1;
                    match next {
                        b'*' => stack.last_mut()?.skip = true,
                        b'~' => push(&mut runs, &group, "\u{a0}"),
                        b'\\' | b'{' | b'}' => push(&mut runs, &group, &(next as char).to_string()),
                        _ => {}
                    }
                    continue;
                }
                let word_start = i;
                while rtf.get(i).is_some_and(u8::is_ascii_alphabetic) {
                    i += 1;
                }
                let word = std::str::from_utf8(&rtf[word_start..i]).ok()?;
                let number_start = i;
                if rtf.get(i) == Some(&b'-') {
                    i += 1;
                }
                while rtf.get(i).is_some_and(u8::is_ascii_digit) {
                    i += 1;
                }
                let number: Option<i32> = std::str::from_utf8(&rtf[number_start..i]).ok()?.parse().ok();
                if rtf.get(i) == Some(&b' ') {
                    i += 1;
                }
                let top = stack.last_mut()?;
                match word {
                    "sub" => top.script = Some(Script::Sub),
                    "super" => top.script = Some(Script::Sup),
                    "nosupersub" | "plain" => top.script = None,
                    "fonttbl" | "colortbl" | "stylesheet" | "info" | "pict" | "listtable"
                    | "listoverridetable" | "themedata" | "datastore" | "latentstyles" => top.skip = true,
                    "uc" => top.unicode_skip = number.unwrap_or(1).max(0) as usize,
                    "u" => {
                        let code = number.unwrap_or(0);
                        let code = if code < 0 { code + 65536 } else { code };
                        if let Some(ch) = char::from_u32(code as u32) {
                            push(&mut runs, &group, &ch.to_string());
                        }
                        pending_skip = group.unicode_skip;
                    }
                    "par" | "line" | "sect" | "page" => push(&mut runs, &group, "\n"),
                    "tab" | "cell" => push(&mut runs, &group, "\t"),
                    _ => {}
                }
            }
            b'\r' | b'\n' => i += 1,
            byte => {
                i += 1;
                if pending_skip > 0 {
                    pending_skip -= 1;
                } else {
                    push(&mut runs, &group, &windows_1252(byte).to_string());
                }
            }
        }
    }
    has_script(&runs).then_some(runs)
}

fn windows_1252(byte: u8) -> char {
    const HIGH: &str = "€\u{81}‚ƒ„…†‡ˆ‰Š‹Œ\u{8d}Ž\u{8f}\u{90}‘’“”•–—˜™š›œ\u{9d}žŸ";
    match byte {
        0x80..=0x9f => HIGH.chars().nth(usize::from(byte - 0x80)).unwrap_or('?'),
        _ => char::from(byte),
    }
}

/// Places the runs' tags in the plain text by matching visible characters,
/// so whitespace and line-break differences between the formats do not matter.
fn align(plain: &str, runs: Runs) -> Option<String> {
    let visible = |c: char| !c.is_whitespace();
    let mut marked = String::with_capacity(plain.len() + 32);
    let mut chars = plain.chars().peekable();
    let copy_space = |marked: &mut String, chars: &mut std::iter::Peekable<std::str::Chars>| {
        while let Some(&c) = chars.peek() {
            if visible(c) {
                break;
            }
            marked.push(c);
            chars.next();
        }
    };
    for (script, text) in runs {
        let mut wanted = text.chars().filter(|c| visible(*c)).peekable();
        if wanted.peek().is_none() {
            continue;
        }
        copy_space(&mut marked, &mut chars);
        let tags = match script {
            Some(Script::Sub) => Some(("<sub>", "</sub>")),
            Some(Script::Sup) => Some(("<sup>", "</sup>")),
            None => None,
        };
        if let Some((open, _)) = tags {
            marked.push_str(open);
        }
        while let Some(want) = wanted.next() {
            if chars.next()? != want {
                return None;
            }
            marked.push(want);
            if wanted.peek().is_some() {
                copy_space(&mut marked, &mut chars);
            }
        }
        if let Some((_, close)) = tags {
            marked.push_str(close);
        }
    }
    marked.extend(chars);
    Some(marked)
}

fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        let decoded = rest.find(';').filter(|end| *end <= 10).and_then(|end| {
            let entity = &rest[1..end];
            let ch = match entity {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some('\u{a0}'),
                _ => entity.strip_prefix('#').and_then(|number| {
                    number
                        .strip_prefix(['x', 'X'])
                        .map_or_else(|| number.parse().ok(), |hex| u32::from_str_radix(hex, 16).ok())
                        .and_then(char::from_u32)
                }),
            }?;
            Some((ch, end + 1))
        });
        match decoded {
            Some((ch, length)) => {
                out.push(ch);
                rest = &rest[length..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
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
    fn pastes_sub_and_superscripts_as_html_and_unicode() {
        let rendered = render("B<sub>n+1</sub> = B<SUB>n</SUB> + x<sup>2</sup> + c<sub>Na</sub>").unwrap();
        assert!(rendered.html.contains("B<sub>n+1</sub> = B<sub>n</sub> + x<sup>2</sup>"));
        assert_eq!(rendered.plain, "Bₙ₊₁ = Bₙ + x² + c_(Na)");
        assert!(render("a <subtle> tag").is_none());
        assert_eq!(readable("B<sub>n+1</sub> < x<SUP>2</SUP>, a<sub>b"), "Bₙ₊₁ < x², a<sub>b");
    }

    #[test]
    fn marks_word_sub_and_superscripts_in_the_selection() {
        let word = "Version:1.0\r\nStartHTML:0\r\n<html><head><style>p{}</style></head><body>\
            <!--StartFragment--><p class=MsoNormal>Die Folge B<sub>n+1</sub> =&nbsp;B<sub><span \
            lang=DE>n</span></sub> + x<sup>2</sup><o:p></o:p></p>\r\n<p>A &amp; B</p><!--EndFragment--></body></html>";
        assert_eq!(
            mark_scripts("Die Folge Bn+1 = Bn + x2\r\nA & B\r\n", Some(word), None).as_deref(),
            Some("Die Folge B<sub>n+1</sub> = B<sub>n</sub> + x<sup>2</sup>\r\nA & B\r\n")
        );
        assert_eq!(mark_scripts("plain text", Some("<p>plain text</p>"), None), None);
        assert_eq!(mark_scripts("other text", Some("<p>B<sub>n</sub></p>"), None), None);
    }

    #[test]
    fn marks_pdf_sub_and_superscripts_from_rtf() {
        let acrobat = br"{\rtf1\ansi\ansicpg1252\uc1{\fonttbl{\f0 Arial;}}{\*\generator Acrobat;}\f0 F\'fcr B{\sub n+1} gilt x{\super 2}\nosupersub  \u8805?\sub 0\nosupersub .\par}";
        assert_eq!(
            mark_scripts("F\u{fc}r Bn+1 gilt x2 \u{2265}0.\r\n", None, Some(acrobat)).as_deref(),
            Some("F\u{fc}r B<sub>n+1</sub> gilt x<sup>2</sup> \u{2265}<sub>0</sub>.\r\n")
        );
        assert_eq!(mark_scripts("x2", None, Some(br"{\rtf1 x2}")), None);
    }

    #[test]
    fn leaves_unformatted_text_to_the_standard_paste_path() {
        assert!(render("Ordinary text").is_none());
    }
}
