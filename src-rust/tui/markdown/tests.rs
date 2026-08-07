use super::*;

fn text(lines: &[MarkdownLine]) -> String {
    lines
        .iter()
        .map(MarkdownLine::text)
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn renders_common_markdown_with_semantic_styles() {
    let lines = render(
        "# Heading\n\n**bold** *italic* ~~gone~~ `code` [site](https://example.test)\n\n- [x] done\n- next\n\n> quoted\n\n```rs\n  let x = 1;\n```\n\n<div>raw</div>",
        60,
    );
    let visible = text(&lines);
    assert!(visible.contains("Heading"));
    assert!(visible.contains("bold italic gone code site (https://example.test)"));
    assert!(visible.contains("• [x] done"));
    assert!(visible.contains("│ quoted"));
    assert!(visible.contains("  let x = 1;"));
    assert!(visible.contains("<div>raw</div>"));
    assert!(!visible.contains("**bold**"));
    assert!(lines
        .iter()
        .flat_map(|line| &line.spans)
        .any(|span| span.style.bold));
    assert!(lines
        .iter()
        .flat_map(|line| &line.spans)
        .any(|span| span.style.code));
}

#[test]
fn preserves_source_breaks_and_sanitizes_terminal_controls() {
    let lines = render(
        "one\ntwo\tend \u{1b}[31mred\u{1b}[0m \u{1b}]0;title\u{7}\u{0}",
        40,
    );
    let visible = text(&lines);
    assert!(visible.contains("one\ntwo    end red"));
    assert!(!visible.contains('\u{1b}'));
    assert!(!visible.contains("title"));
    assert!(!visible.contains('\u{0}'));
}

#[test]
fn wraps_cjk_combining_text_and_long_tokens_to_cell_width() {
    let lines = render("示例 e\u{301} https://example.test/a/very/long/path", 12);
    assert!(lines.iter().all(|line| line.width() <= 12));
    assert_eq!(
        text(&lines).replace('\n', ""),
        "示例 e\u{301} https://example.test/a/very/long/path"
    );
}

#[test]
fn trims_trailing_space_but_keeps_code_indentation() {
    let lines = render("visible   \n\n```\n    indented   \n```", 20);
    let visible = lines.iter().map(MarkdownLine::text).collect::<Vec<_>>();
    assert!(visible.contains(&"visible".to_owned()));
    assert!(visible.contains(&"    indented".to_owned()));
    assert_eq!(
        lines
            .iter()
            .filter(|line| line.kind == MarkdownLineKind::Code)
            .count(),
        1
    );
    assert!(visible.iter().all(|line| !line.ends_with(' ')));
}
