mod parser;
mod sanitize;
mod wrap;

#[cfg(test)]
mod tests;

use unicode_width::UnicodeWidthStr;

use crate::documents::{PreviewMessage, PreviewTreePosition};

use super::app::SessionViewMode;

use parser::render;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct MarkdownStyle {
    pub(super) bold: bool,
    pub(super) italic: bool,
    pub(super) crossed_out: bool,
    pub(super) code: bool,
    pub(super) link: bool,
    pub(super) dim: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum MarkdownLineKind {
    #[default]
    Body,
    Heading,
    Quote,
    Code,
    Rule,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct MarkdownSpan {
    pub(super) text: String,
    pub(super) style: MarkdownStyle,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct MarkdownLine {
    pub(super) spans: Vec<MarkdownSpan>,
    pub(super) kind: MarkdownLineKind,
}

impl MarkdownLine {
    pub(super) fn width(&self) -> usize {
        self.spans
            .iter()
            .map(|span| UnicodeWidthStr::width(span.text.as_str()))
            .sum()
    }

    #[cfg(test)]
    fn text(&self) -> String {
        self.spans.iter().map(|span| span.text.as_str()).collect()
    }
}

#[derive(Clone, Debug)]
pub(super) struct PreviewLayout {
    pub(super) width: usize,
    pub(super) messages: Vec<Vec<MarkdownLine>>,
}

impl PreviewLayout {
    pub(super) fn new<'a>(
        messages: impl IntoIterator<Item = &'a PreviewMessage>,
        width: usize,
        mode: SessionViewMode,
    ) -> Self {
        let width = width.max(1);
        Self {
            width,
            messages: messages
                .into_iter()
                .map(|message| match mode {
                    SessionViewMode::Tree => Vec::new(),
                    SessionViewMode::Full => {
                        render(&message.text, full_message_width(&message.tree, width))
                    }
                })
                .collect(),
        }
    }

    pub(super) fn message_height(&self, index: usize, mode: SessionViewMode) -> usize {
        match mode {
            SessionViewMode::Tree => usize::from(index < self.messages.len()),
            SessionViewMode::Full => self
                .messages
                .get(index)
                .map(|lines| 1 + lines.len().max(1) + 1)
                .unwrap_or(0),
        }
    }
}

pub(super) fn full_tree_width(position: &PreviewTreePosition, width: usize) -> usize {
    let desired = 4 + position.indent * 3;
    let cap = (width / 3).clamp(4, 24).min(width.saturating_sub(8).max(1));
    desired.min(cap).max(1)
}

fn full_message_width(position: &PreviewTreePosition, width: usize) -> usize {
    width
        .saturating_sub(full_tree_width(position, width))
        .max(1)
}

pub(super) fn one_line_summary(text: &str) -> String {
    sanitize::clean(text)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Clone, Debug)]
struct LogicalLine {
    prefix: String,
    continuation: String,
    spans: Vec<MarkdownSpan>,
    kind: MarkdownLineKind,
}

impl LogicalLine {
    fn empty(kind: MarkdownLineKind, prefix: String, continuation: String) -> Self {
        Self {
            prefix,
            continuation,
            spans: Vec::new(),
            kind,
        }
    }
}

fn append_span(spans: &mut Vec<MarkdownSpan>, text: &str, style: MarkdownStyle) {
    if text.is_empty() {
        return;
    }
    if let Some(last) = spans.last_mut().filter(|span| span.style == style) {
        last.text.push_str(text);
    } else {
        spans.push(MarkdownSpan {
            text: text.to_owned(),
            style,
        });
    }
}
