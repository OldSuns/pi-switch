use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use unicode_width::UnicodeWidthStr;

use super::{
    append_span, sanitize::clean, wrap::wrap_line, LogicalLine, MarkdownLine, MarkdownLineKind,
    MarkdownStyle,
};

#[derive(Clone, Copy, Debug, Default)]
struct InlineState {
    emphasis: usize,
    strong: usize,
    crossed_out: usize,
    link: usize,
}

impl InlineState {
    fn style(self) -> MarkdownStyle {
        MarkdownStyle {
            bold: self.strong > 0,
            italic: self.emphasis > 0,
            crossed_out: self.crossed_out > 0,
            link: self.link > 0,
            ..MarkdownStyle::default()
        }
    }
}

#[derive(Clone, Debug)]
struct ListState {
    ordered: bool,
    next: u64,
}

#[derive(Clone, Debug)]
struct ItemState {
    marker: String,
    first_line: bool,
    depth: usize,
}

#[derive(Clone, Debug)]
struct LinkState {
    destination: String,
    label: String,
}

#[derive(Default)]
struct MarkdownRenderer {
    lines: Vec<LogicalLine>,
    current: Option<LogicalLine>,
    inline: InlineState,
    quote_depth: usize,
    lists: Vec<ListState>,
    items: Vec<ItemState>,
    links: Vec<LinkState>,
    images: Vec<LinkState>,
    code_block: bool,
}

impl MarkdownRenderer {
    fn render(mut self, text: &str, width: usize) -> Vec<MarkdownLine> {
        let clean = clean(text);
        let mut options = Options::empty();
        options.insert(Options::ENABLE_STRIKETHROUGH);
        options.insert(Options::ENABLE_TASKLISTS);

        for event in Parser::new_ext(&clean, options) {
            self.event(event);
        }
        self.finish_line(false);
        while self
            .lines
            .last()
            .is_some_and(|line| line.spans.is_empty() && line.kind == MarkdownLineKind::Body)
        {
            self.lines.pop();
        }

        let mut rendered = self
            .lines
            .into_iter()
            .flat_map(|line| wrap_line(line, width.max(1)))
            .collect::<Vec<_>>();
        if rendered.is_empty() {
            rendered.push(MarkdownLine::default());
        }
        rendered
    }

    fn event(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(text) => self.push_text(&text, None, true),
            Event::Code(text) => {
                let mut style = self.inline.style();
                style.code = true;
                self.push_text(&text, Some(style), true);
            }
            Event::Html(text) | Event::InlineHtml(text) => {
                let mut style = self.inline.style();
                style.code = true;
                self.push_text(&text, Some(style), true);
            }
            Event::SoftBreak | Event::HardBreak => self.finish_line(false),
            Event::Rule => {
                self.finish_line(false);
                self.lines.push(LogicalLine::empty(
                    MarkdownLineKind::Rule,
                    String::new(),
                    String::new(),
                ));
                self.blank_line();
            }
            Event::TaskListMarker(checked) => {
                let mut style = self.inline.style();
                style.dim = true;
                self.push_text(if checked { "[x] " } else { "[ ] " }, Some(style), false);
            }
            Event::FootnoteReference(label) => {
                let mut style = self.inline.style();
                style.dim = true;
                self.push_text(&format!("[{label}]"), Some(style), false);
            }
            Event::InlineMath(text) => self.push_text(&format!("${text}$"), None, true),
            Event::DisplayMath(text) => {
                self.finish_line(false);
                self.push_text(&format!("$${text}$$"), None, true);
                self.finish_line(false);
            }
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => self.ensure_line(if self.quote_depth > 0 {
                MarkdownLineKind::Quote
            } else {
                MarkdownLineKind::Body
            }),
            Tag::Heading { .. } => {
                self.finish_line(false);
                self.ensure_line(MarkdownLineKind::Heading);
                self.inline.strong += 1;
            }
            Tag::BlockQuote(_) => {
                self.finish_line(false);
                self.quote_depth += 1;
            }
            Tag::CodeBlock(kind) => {
                self.finish_line(false);
                self.code_block = true;
                if let CodeBlockKind::Fenced(language) = kind {
                    let language = language.trim();
                    if !language.is_empty() {
                        let style = MarkdownStyle {
                            dim: true,
                            ..MarkdownStyle::default()
                        };
                        self.push_text(language, Some(style), false);
                        self.finish_line(true);
                    }
                }
            }
            Tag::List(start) => {
                self.finish_line(false);
                self.lists.push(ListState {
                    ordered: start.is_some(),
                    next: start.unwrap_or(1),
                });
            }
            Tag::Item => {
                self.finish_line(false);
                let depth = self.lists.len().max(1);
                let marker = self
                    .lists
                    .last_mut()
                    .map(|list| {
                        if list.ordered {
                            let marker = format!("{}. ", list.next);
                            list.next += 1;
                            marker
                        } else {
                            "• ".to_owned()
                        }
                    })
                    .unwrap_or_else(|| "• ".to_owned());
                self.items.push(ItemState {
                    marker,
                    first_line: true,
                    depth,
                });
            }
            Tag::Emphasis => self.inline.emphasis += 1,
            Tag::Strong => self.inline.strong += 1,
            Tag::Strikethrough => self.inline.crossed_out += 1,
            Tag::Link { dest_url, .. } => {
                self.inline.link += 1;
                self.links.push(LinkState {
                    destination: dest_url.into_string(),
                    label: String::new(),
                });
            }
            Tag::Image { dest_url, .. } => {
                self.inline.link += 1;
                self.images.push(LinkState {
                    destination: dest_url.into_string(),
                    label: String::new(),
                });
                let mut style = self.inline.style();
                style.dim = true;
                self.push_text("image: ", Some(style), false);
            }
            Tag::HtmlBlock => self.ensure_line(MarkdownLineKind::Body),
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => {
                self.finish_line(false);
                if self.items.is_empty() && self.quote_depth == 0 {
                    self.blank_line();
                }
            }
            TagEnd::Heading(_) => {
                self.inline.strong = self.inline.strong.saturating_sub(1);
                self.finish_line(false);
                self.blank_line();
            }
            TagEnd::BlockQuote(_) => {
                self.finish_line(false);
                self.quote_depth = self.quote_depth.saturating_sub(1);
                if self.quote_depth == 0 {
                    self.blank_line();
                }
            }
            TagEnd::CodeBlock => {
                self.finish_line(true);
                self.code_block = false;
                self.blank_line();
            }
            TagEnd::List(_) => {
                self.finish_line(false);
                self.lists.pop();
                if self.lists.is_empty() {
                    self.blank_line();
                }
            }
            TagEnd::Item => {
                self.finish_line(false);
                self.items.pop();
            }
            TagEnd::Emphasis => self.inline.emphasis = self.inline.emphasis.saturating_sub(1),
            TagEnd::Strong => self.inline.strong = self.inline.strong.saturating_sub(1),
            TagEnd::Strikethrough => {
                self.inline.crossed_out = self.inline.crossed_out.saturating_sub(1)
            }
            TagEnd::Link => {
                if let Some(link) = self.links.pop() {
                    self.append_destination(link);
                }
                self.inline.link = self.inline.link.saturating_sub(1);
            }
            TagEnd::Image => {
                if let Some(image) = self.images.pop() {
                    self.append_destination(image);
                }
                self.inline.link = self.inline.link.saturating_sub(1);
            }
            TagEnd::HtmlBlock => self.finish_line(false),
            _ => {}
        }
    }

    fn append_destination(&mut self, link: LinkState) {
        let destination = link.destination.trim();
        if destination.is_empty() || link.label.trim() == destination {
            return;
        }
        let mut style = self.inline.style();
        style.link = true;
        self.push_text(&format!(" ({destination})"), Some(style), false);
    }

    fn push_text(&mut self, text: &str, style: Option<MarkdownStyle>, capture: bool) {
        if capture {
            if let Some(link) = self.links.last_mut() {
                link.label.push_str(text);
            }
            if let Some(image) = self.images.last_mut() {
                image.label.push_str(text);
            }
        }
        let style = style.unwrap_or_else(|| self.inline.style());
        let kind = if self.code_block {
            MarkdownLineKind::Code
        } else if self.quote_depth > 0 {
            MarkdownLineKind::Quote
        } else {
            MarkdownLineKind::Body
        };
        let parts = text.split('\n').collect::<Vec<_>>();
        for (index, part) in parts.iter().enumerate() {
            if index > 0 {
                self.finish_line(self.code_block);
            }
            if part.is_empty() && index + 1 == parts.len() && text.ends_with('\n') {
                continue;
            }
            self.ensure_line(kind);
            if !part.is_empty() {
                let line = self.current.as_mut().expect("line was created");
                append_span(&mut line.spans, part, style);
            }
        }
    }

    fn ensure_line(&mut self, kind: MarkdownLineKind) {
        if self.current.is_some() {
            return;
        }
        let quote = "│ ".repeat(self.quote_depth);
        let mut prefix = quote.clone();
        let mut continuation = quote;
        if let Some(item) = self.items.last_mut() {
            let indent = "  ".repeat(item.depth.saturating_sub(1));
            prefix.push_str(&indent);
            continuation.push_str(&indent);
            if item.first_line {
                prefix.push_str(&item.marker);
                item.first_line = false;
            } else {
                prefix.push_str(&" ".repeat(UnicodeWidthStr::width(item.marker.as_str())));
            }
            continuation.push_str(&" ".repeat(UnicodeWidthStr::width(item.marker.as_str())));
        }
        self.current = Some(LogicalLine::empty(kind, prefix, continuation));
    }

    fn finish_line(&mut self, force_empty: bool) {
        let Some(line) = self.current.take() else {
            return;
        };
        if force_empty || !line.spans.is_empty() {
            self.lines.push(line);
        }
    }

    fn blank_line(&mut self) {
        if self
            .lines
            .last()
            .is_some_and(|line| line.spans.is_empty() && line.kind == MarkdownLineKind::Body)
        {
            return;
        }
        self.lines.push(LogicalLine::empty(
            MarkdownLineKind::Body,
            String::new(),
            String::new(),
        ));
    }
}

pub(super) fn render(text: &str, width: usize) -> Vec<MarkdownLine> {
    MarkdownRenderer::default().render(text, width)
}
