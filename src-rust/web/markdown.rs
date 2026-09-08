use pulldown_cmark::{html, Event, LinkType, Options, Parser, Tag};

/// Render conversation text without executable HTML or automatic image requests.
pub(super) fn render(text: &str) -> String {
    let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH;
    let mut parser = Parser::new_ext(text, options);
    let events = std::iter::from_fn(move || {
        let event = parser.next()?;
        Some(match event {
            Event::Html(text) | Event::InlineHtml(text) => Event::Text(text),
            Event::Start(Tag::Image { .. }) => Event::Text(subtree_text(&mut parser).into()),
            Event::Start(Tag::Link {
                link_type,
                ref dest_url,
                ..
            }) if !allowed_link(link_type, dest_url) => {
                Event::Text(subtree_text(&mut parser).into())
            }
            event => event,
        })
    });
    let mut output = String::with_capacity(text.len());
    html::push_html(&mut output, events);
    output
}

fn allowed_link(link_type: LinkType, destination: &str) -> bool {
    if destination.chars().any(char::is_control) {
        return false;
    }
    // The HTML writer supplies `mailto:` for email autolinks.
    if link_type == LinkType::Email {
        return true;
    }
    destination.split_once(':').is_some_and(|(scheme, _)| {
        ["http", "https", "mailto"]
            .iter()
            .any(|allowed| scheme.eq_ignore_ascii_case(allowed))
    })
}

// Consume the whole subtree so nested images or links cannot escape through alt text.
fn subtree_text<'a>(events: &mut impl Iterator<Item = Event<'a>>) -> String {
    let mut text = String::new();
    let mut depth = 1;
    for event in events.by_ref() {
        match event {
            Event::Start(_) => depth += 1,
            Event::End(_) => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            Event::Text(value)
            | Event::Code(value)
            | Event::Html(value)
            | Event::InlineHtml(value)
            | Event::InlineMath(value)
            | Event::DisplayMath(value)
            | Event::FootnoteReference(value) => text.push_str(&value),
            Event::SoftBreak | Event::HardBreak | Event::Rule => text.push('\n'),
            Event::TaskListMarker(checked) => {
                text.push_str(if checked { "[x] " } else { "[ ] " });
            }
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::render;

    #[test]
    fn raw_block_and_inline_html_are_visible_text() {
        for markdown in [
            "<script>alert(1)</script>",
            "<iframe src=\"https://tracker.test\"></iframe>",
            "Text <img src=\"https://tracker.test/pixel\" onerror=\"alert(1)\"> end",
            "<svg onload=\"alert(1)\"></svg>",
        ] {
            let output = render(markdown);
            assert!(output.contains("&lt;"), "{output}");
            for tag in ["<script", "<iframe", "<img", "<svg"] {
                assert!(!output.contains(tag), "{output}");
            }
        }
    }

    #[test]
    fn only_explicit_allowed_schemes_produce_links() {
        for destination in [
            "javascript:alert(1)",
            "JaVaScRiPt:alert(1)",
            "javascript&#58;alert(1)",
            "java&#x09;script:alert(1)",
            "data:text/html;base64,PHNjcmlwdD4=",
            "vbscript:msgbox(1)",
            "file:///etc/passwd",
            "//tracker.test/pixel",
            "/api/settings",
            "#local",
        ] {
            assert_eq!(
                render(&format!("[visible]({destination})")),
                "<p>visible</p>\n",
                "destination: {destination}"
            );
        }
    }

    #[test]
    fn safe_links_and_email_autolinks_remain_clickable() {
        for destination in [
            "https://example.test/docs",
            "http://example.test/docs",
            "HTTPS://example.test/docs",
            "mailto:user@example.test",
        ] {
            let output = render(&format!("[docs]({destination})"));
            assert!(
                output.contains(&format!("href=\"{destination}\"")),
                "{output}"
            );
        }
        assert_eq!(
            render("<user@example.test>"),
            "<p><a href=\"mailto:user@example.test\">user@example.test</a></p>\n"
        );
    }

    #[test]
    fn link_attributes_are_escaped() {
        let output =
            render("[docs](https://example.test/?a=1&b=2 \"&quot; onclick=&quot;alert(1)\")");
        assert!(output.contains("href=\"https://example.test/?a=1&amp;b=2\""));
        assert!(output.contains("title=\"&quot; onclick=&quot;alert(1)\""));
        assert!(!output.contains("\" onclick=\""));
    }

    #[test]
    fn image_subtrees_become_plain_alt_text_without_requests() {
        assert_eq!(
            render(
                "![**Alt** `code` [link](https://example.test)](https://tracker.test/pixel.png)"
            ),
            "<p>Alt code link</p>\n"
        );
        assert_eq!(
            render(
                "![outer ![inner](https://tracker.test/inner)](data:image/png;base64,AAAA) after"
            ),
            "<p>outer inner after</p>\n"
        );
        assert_eq!(
            render("[![Alt](https://tracker.test/pixel)](https://example.test)"),
            "<p><a href=\"https://example.test\">Alt</a></p>\n"
        );
    }

    #[test]
    fn unsafe_links_cannot_preserve_nested_images_or_html() {
        assert_eq!(
            render("[**notice** ![image](https://tracker.test/pixel) <b>text</b>](javascript:alert(1))"),
            "<p>notice image &lt;b&gt;text&lt;/b&gt;</p>\n"
        );
    }

    #[test]
    fn preserves_markdown_structure_and_escapes_fenced_code() {
        let output = render(
            "# Heading\n\n> Quote\n\n- first\n- second\n\n1. ordered\n\n~~removed~~\n\n```rust\n<script>alert(1)</script>\n```\n\n| Provider | Model |\n| --- | --- |\n| Pi | Example |\n",
        );
        for fragment in [
            "<h1>Heading</h1>",
            "<blockquote>",
            "<p>Quote</p>",
            "<ul>",
            "<li>first</li>",
            "<li>second</li>",
            "<ol>",
            "<li>ordered</li>",
            "<del>removed</del>",
            "<pre><code class=\"language-rust\">&lt;script&gt;alert(1)&lt;/script&gt;\n</code></pre>",
            "<table>",
            "<th>Provider</th>",
            "<td>Pi</td>",
        ] {
            assert!(output.contains(fragment), "missing {fragment}: {output}");
        }
        assert!(!output.contains("<script>"));
    }
}
