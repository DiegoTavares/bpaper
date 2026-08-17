//! Inline Markdown for panel rows. A vault's task lines are Markdown, so
//! `[name](url)` should read as a link in a pane instead of as syntax — but the
//! file keeps the raw text, and so does the inline editor. Pure functions over
//! strings — no GPUI.

/// A run of a line: literal text, or an inline link.
#[derive(Debug, Clone, PartialEq)]
pub enum InlineSpan {
    Text(String),
    Link { text: String, url: String },
}

/// Splits `text` into literal runs and `[name](url)` links. Anything that isn't
/// a well-formed link to an absolute URL stays literal, so a half-typed bracket
/// renders as exactly what the user typed and a relative path never becomes a
/// link that opens nothing.
pub fn parse_inline_links(text: &str) -> Vec<InlineSpan> {
    let mut spans = Vec::new();
    let mut literal_start = 0;
    let mut cursor = 0;
    while let Some(offset) = text[cursor..].find('[') {
        let open = cursor + offset;
        match parse_link_at(text, open) {
            Some((link, end)) => {
                if literal_start < open {
                    spans.push(InlineSpan::Text(text[literal_start..open].to_string()));
                }
                spans.push(link);
                cursor = end;
                literal_start = end;
            }
            None => cursor = open + 1,
        }
    }
    if literal_start < text.len() {
        spans.push(InlineSpan::Text(text[literal_start..].to_string()));
    }
    spans
}

/// Parses `[name](url)` at `open`, returning the link and the byte offset just
/// past its closing paren.
fn parse_link_at(text: &str, open: usize) -> Option<(InlineSpan, usize)> {
    let label_start = open + 1;
    let label_end = text[label_start..].find(']')? + label_start;
    let url_start = label_end + 1;
    if !text[url_start..].starts_with('(') {
        return None;
    }
    let url_start = url_start + 1;
    let url_end = closing_paren(text, url_start)?;
    let label = &text[label_start..label_end];
    // A title (`[name](url "title")`) has nowhere to go in a one-line row, so
    // the URL is the first word and the title is dropped.
    let url = text[url_start..url_end].split_whitespace().next()?;
    if label.is_empty() || !is_absolute_url(url) {
        return None;
    }
    Some((
        InlineSpan::Link {
            text: label.to_string(),
            url: url.to_string(),
        },
        url_end + 1,
    ))
}

/// The `)` closing the paren opened just before `start`, allowing for the
/// balanced parens that show up in real URLs (`…/Foo_(disambiguation)`).
fn closing_paren(text: &str, start: usize) -> Option<usize> {
    let mut depth = 1usize;
    for (offset, character) in text[start..].char_indices() {
        match character {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(start + offset);
                }
            }
            _ => {}
        }
    }
    None
}

fn is_absolute_url(url: &str) -> bool {
    let Some(separator) = url.find(':') else {
        return false;
    };
    let scheme = &url[..separator];
    scheme.starts_with(|character: char| character.is_ascii_alphabetic())
        && scheme.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '.')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(value: &str) -> InlineSpan {
        InlineSpan::Text(value.to_string())
    }

    fn link(value: &str, url: &str) -> InlineSpan {
        InlineSpan::Link {
            text: value.to_string(),
            url: url.to_string(),
        }
    }

    #[test]
    fn plain_text_is_one_span() {
        assert_eq!(
            parse_inline_links("Review the planner"),
            vec![text("Review the planner")]
        );
    }

    #[test]
    fn splits_text_around_links() {
        assert_eq!(
            parse_inline_links("See [chat](https://chat.example.com/room) before Friday"),
            vec![
                text("See "),
                link("chat", "https://chat.example.com/room"),
                text(" before Friday"),
            ]
        );
    }

    #[test]
    fn parses_adjacent_links() {
        assert_eq!(
            parse_inline_links("[a](https://a.example)[b](mailto:b@example.com)"),
            vec![
                link("a", "https://a.example"),
                link("b", "mailto:b@example.com"),
            ]
        );
    }

    #[test]
    fn allows_balanced_parens_in_the_url() {
        assert_eq!(
            parse_inline_links("[wiki](https://example.com/Foo_(bar))!"),
            vec![link("wiki", "https://example.com/Foo_(bar)"), text("!")]
        );
    }

    #[test]
    fn drops_a_link_title() {
        assert_eq!(
            parse_inline_links("[a](https://a.example \"Title\")"),
            vec![link("a", "https://a.example")]
        );
    }

    #[test]
    fn malformed_or_relative_links_stay_literal() {
        for line in [
            "an [unclosed link",
            "[no parens] here",
            "[empty]()",
            "[](https://a.example)",
            "[note](./daily/2026-08-17.md)",
        ] {
            assert_eq!(parse_inline_links(line), vec![text(line)], "{line}");
        }
    }

    #[test]
    fn a_bare_url_is_not_a_link() {
        assert_eq!(
            parse_inline_links("Fill https://example.com/sheet"),
            vec![text("Fill https://example.com/sheet")]
        );
    }
}
