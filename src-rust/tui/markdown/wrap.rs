use unicode_width::UnicodeWidthChar;

use super::{LogicalLine, MarkdownLine, MarkdownLineKind, MarkdownSpan, MarkdownStyle};

pub(super) fn wrap_line(line: LogicalLine, width: usize) -> Vec<MarkdownLine> {
    if line.kind == MarkdownLineKind::Rule {
        return vec![MarkdownLine {
            spans: vec![MarkdownSpan {
                text: "─".repeat(width),
                style: MarkdownStyle {
                    dim: true,
                    ..MarkdownStyle::default()
                },
            }],
            kind: MarkdownLineKind::Rule,
        }];
    }
    if line.spans.is_empty() {
        return vec![MarkdownLine {
            spans: Vec::new(),
            kind: line.kind,
        }];
    }

    let mut output = Vec::new();
    let mut current = MarkdownLine {
        spans: Vec::new(),
        kind: line.kind,
    };
    let first_prefix = fit_prefix(&line.prefix, width);
    let continuation = fit_prefix(&line.continuation, width);
    append_prefix(&mut current, &first_prefix);
    let mut current_width = current.width();
    let mut body_started = false;

    for span in line.spans {
        for ch in span.text.chars() {
            let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
            if ch_width > 0 && current_width + ch_width > width && body_started {
                trim_line_end(&mut current);
                output.push(current);
                current = MarkdownLine {
                    spans: Vec::new(),
                    kind: line.kind,
                };
                append_prefix(&mut current, &continuation);
                current_width = current.width();
                body_started = false;
                if line.kind != MarkdownLineKind::Code && ch.is_whitespace() {
                    continue;
                }
            }
            append_char(&mut current.spans, ch, span.style);
            current_width += ch_width;
            body_started = true;
        }
    }
    trim_line_end(&mut current);
    output.push(current);
    output
}

fn fit_prefix(prefix: &str, width: usize) -> String {
    let budget = width.saturating_sub(1);
    let mut result = String::new();
    let mut used = 0usize;
    for ch in prefix.chars() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + ch_width > budget {
            break;
        }
        result.push(ch);
        used += ch_width;
    }
    result
}

fn append_prefix(line: &mut MarkdownLine, prefix: &str) {
    if prefix.is_empty() {
        return;
    }
    line.spans.push(MarkdownSpan {
        text: prefix.to_owned(),
        style: MarkdownStyle {
            dim: true,
            ..MarkdownStyle::default()
        },
    });
}

fn append_char(spans: &mut Vec<MarkdownSpan>, ch: char, style: MarkdownStyle) {
    if let Some(last) = spans.last_mut().filter(|span| span.style == style) {
        last.text.push(ch);
    } else {
        spans.push(MarkdownSpan {
            text: ch.to_string(),
            style,
        });
    }
}

fn trim_line_end(line: &mut MarkdownLine) {
    while let Some(last) = line.spans.last_mut() {
        let trimmed = last.text.trim_end_matches(char::is_whitespace).len();
        last.text.truncate(trimmed);
        if last.text.is_empty() {
            line.spans.pop();
        } else {
            break;
        }
    }
}
