use super::*;
use crate::documents::{PreviewMessage, PreviewTreePosition, SessionPreview};
use crate::tui::markdown::one_line_summary;

const TREE_CURSOR_WIDTH: usize = 2;
const MIN_VISIBLE_CONTENT_WIDTH: usize = 4;
const MAX_VISIBLE_CONTENT_WIDTH: usize = 20;
const MIN_TREE_CONTEXT_WIDTH: usize = 2;
const MAX_TREE_CONTEXT_WIDTH: usize = 12;

pub(super) fn compact_horizontal_scroll(app: &App, width: usize) -> usize {
    let viewport = width.saturating_sub(TREE_CURSOR_WIDTH);
    let Some(preview) = app.preview.as_ref() else {
        return 0;
    };
    let Some(full_index) = app.selected_preview_tree_index() else {
        return 0;
    };
    let Some(message) = preview.messages.get(full_index) else {
        return 0;
    };
    let anchor = UnicodeWidthStr::width(compact_tree_prefix(message, app).as_str());
    let visible_content =
        MAX_VISIBLE_CONTENT_WIDTH.min(MIN_VISIBLE_CONTENT_WIDTH.max(viewport / 3));
    if anchor <= viewport.saturating_sub(visible_content) {
        return 0;
    }
    let context = MAX_TREE_CONTEXT_WIDTH.min(MIN_TREE_CONTEXT_WIDTH.max(viewport / 4));
    anchor.saturating_sub(context)
}

pub(super) fn render_compact_tree_line(
    app: &App,
    preview: &SessionPreview,
    full_index: usize,
    width: usize,
    horizontal_scroll: usize,
    selected: bool,
    theme: Theme,
) -> Line<'static> {
    let message = &preview.messages[full_index];
    let background = if selected {
        theme.accent_dim
    } else {
        theme.background
    };
    let cursor = Span::styled(
        if selected { "› " } else { "  " },
        Style::default()
            .fg(theme.accent)
            .bg(background)
            .add_modifier(Modifier::BOLD),
    );
    let active_leaf = preview.active_message_id.as_deref() == Some(message.id.as_str());
    let prefix_color = if active_leaf || message.tree.active_path {
        theme.accent
    } else {
        theme.dim
    };
    let mut body = vec![Span::styled(
        compact_tree_prefix(message, app),
        Style::default().fg(prefix_color).bg(background),
    )];
    if let Some(label) = message.label.as_deref() {
        body.push(Span::styled(
            format!("[{label}] "),
            Style::default().fg(theme.warning).bg(background),
        ));
    }
    let (role, role_color) = role_label(message, app, theme);
    body.push(Span::styled(
        format!("{role}: "),
        Style::default()
            .fg(role_color)
            .bg(background)
            .add_modifier(Modifier::BOLD),
    ));
    body.push(Span::styled(
        one_line_summary(&message.text),
        Style::default().fg(theme.foreground).bg(background),
    ));
    let available = width.saturating_sub(TREE_CURSOR_WIDTH);
    let mut spans = vec![cursor];
    spans.extend(clip_spans(body, horizontal_scroll, available));
    filled_line(spans, width, Style::default().bg(background))
}

fn compact_tree_prefix(message: &PreviewMessage, app: &App) -> String {
    let position = &message.tree;
    let mut text = String::new();
    let connector_position = position.indent.saturating_sub(1);
    for level in 0..position.indent {
        if position.show_connector && level == connector_position {
            text.push(if position.is_last { '└' } else { '├' });
            text.push(if message.tree.has_children {
                if app.preview_collapsed.contains(&message.id) {
                    '⊞'
                } else {
                    '⊟'
                }
            } else {
                '─'
            });
            text.push(' ');
            continue;
        }
        let gutter = position
            .gutters
            .iter()
            .find(|gutter| gutter.position == level);
        text.push(if gutter.is_some_and(|gutter| gutter.show) {
            '│'
        } else {
            ' '
        });
        text.push_str("  ");
    }
    if message.tree.has_children && !position.show_connector {
        text.push(if app.preview_collapsed.contains(&message.id) {
            '⊞'
        } else {
            '⊟'
        });
        text.push(' ');
    }
    if position.active_path {
        text.push_str("• ");
    }
    text
}

fn clip_spans(spans: Vec<Span<'static>>, start: usize, width: usize) -> Vec<Span<'static>> {
    if width == 0 {
        return Vec::new();
    }
    let mut skipped = 0usize;
    let mut used = 0usize;
    let mut clipped = Vec::new();
    for span in spans {
        let mut text = String::new();
        for character in span.content.chars() {
            let character_width = unicode_width::UnicodeWidthChar::width(character).unwrap_or(0);
            if skipped < start {
                if skipped + character_width <= start {
                    skipped += character_width;
                    continue;
                }
                skipped = start;
            }
            if used + character_width > width {
                break;
            }
            text.push(character);
            used += character_width;
        }
        if !text.is_empty() {
            clipped.push(Span::styled(text, span.style));
        }
        if used >= width {
            break;
        }
    }
    clipped
}

pub(super) fn role_label(message: &PreviewMessage, app: &App, theme: Theme) -> (String, Color) {
    let label = match message.role.as_str() {
        "user" => app.language.pick("User", "用户"),
        "assistant" => "Pi",
        "branchSummary" => app.language.pick("Branch summary", "分支摘要"),
        "compaction" => app.language.pick("Compaction", "压缩摘要"),
        other => other,
    };
    let color = match message.role.as_str() {
        "user" => theme.accent,
        "assistant" => theme.success,
        _ => theme.warning,
    };
    (label.to_owned(), color)
}

pub(super) fn render_markdown_line(
    line: &MarkdownLine,
    position: &PreviewTreePosition,
    tree_width: usize,
    width: usize,
    selected: bool,
    theme: Theme,
) -> Line<'static> {
    let background = if selected {
        theme.accent_dim
    } else if line.kind == MarkdownLineKind::Code {
        theme.surface_hi
    } else {
        theme.background
    };
    let mut spans = vec![tree_body_prefix(position, tree_width, background, theme)];
    spans.extend(line.spans.iter().map(|span| {
        Span::styled(
            span.text.clone(),
            markdown_style(span.style, line.kind, background, selected, theme),
        )
    }));
    filled_line(spans, width, Style::default().bg(background))
}

pub(super) fn preview_node_status(app: &App) -> String {
    let Some(preview) = app.preview.as_ref() else {
        return app.language.pick("No tree node", "无树节点").into();
    };
    let Some(index) = app.selected_preview_tree_index() else {
        return app.language.pick("No tree node", "无树节点").into();
    };
    let Some(message) = preview.messages.get(index) else {
        return app.language.pick("No tree node", "无树节点").into();
    };
    let state = if preview.active_message_id.as_deref() == Some(message.id.as_str()) {
        app.language.pick("◆ Current", "◆ 当前")
    } else if message.tree.active_path {
        app.language.pick("● Active", "● 活动路径")
    } else {
        app.language.pick("○ Alternate", "○ 备选分支")
    };
    let mut parts = vec![
        state.into(),
        format!(
            "{} {}/{}",
            app.language.pick("Node", "节点"),
            index + 1,
            preview.messages.len()
        ),
    ];
    if let Some((position, count)) = preview.branch_position(index) {
        parts.push(format!(
            "{} {position}/{count}",
            app.language.pick("Branch", "分支")
        ));
    }
    parts.push(format!(
        "{}{}",
        app.language.pick("L", "层"),
        message.tree.level + 1
    ));
    if message.tree.has_children {
        let descendants = preview.descendant_count(index);
        if app.preview_collapsed.contains(&message.id) {
            parts.push(format!(
                "▸ {} {descendants}",
                app.language.pick("hidden", "已隐藏")
            ));
        } else {
            parts.push(format!(
                "▾ {} {}",
                preview.direct_child_count(index),
                app.language.pick("children", "个子节点")
            ));
        }
    } else if preview.direct_child_count(index) == 1 {
        parts.push(app.language.pick("Linear continuation", "串行延续").into());
    } else if preview.active_message_id.as_deref() != Some(message.id.as_str()) {
        parts.push(app.language.pick("Leaf", "叶节点").into());
    }
    parts.join("  ·  ")
}

pub(super) fn tree_prefix(
    position: &PreviewTreePosition,
    width: usize,
    selected: bool,
    active_leaf: bool,
    background: Color,
    theme: Theme,
) -> Span<'static> {
    let mut text = String::with_capacity(width);
    text.push_str(if selected { "▶ " } else { "  " });
    for gutter in &position.gutters {
        let target = 2 + gutter.position * 3;
        while text.chars().count() < target {
            text.push(' ');
        }
        if text.chars().count() == target {
            text.push(if gutter.show { '│' } else { ' ' });
        }
    }
    if position.show_connector {
        let target = 2 + position.indent.saturating_sub(1) * 3;
        while text.chars().count() < target {
            text.push(' ');
        }
        text.push_str(if position.is_last { "└─" } else { "├─" });
    } else {
        let target = 2 + position.indent * 3;
        while text.chars().count() < target {
            text.push(' ');
        }
    }
    text.push(if active_leaf {
        '◆'
    } else if position.active_path {
        '●'
    } else {
        '○'
    });
    text.push(' ');
    let text = fit_tree_prefix(&text, width);
    let style = Style::default()
        .fg(if active_leaf {
            theme.success
        } else if position.active_path {
            theme.accent
        } else {
            theme.dim
        })
        .bg(background)
        .add_modifier(if selected {
            Modifier::BOLD
        } else {
            Modifier::empty()
        });
    Span::styled(text, style)
}

pub(super) fn tree_body_prefix(
    position: &PreviewTreePosition,
    width: usize,
    background: Color,
    theme: Theme,
) -> Span<'static> {
    let mut text = String::from("  ");
    for gutter in &position.gutters {
        let target = 2 + gutter.position * 3;
        while text.chars().count() < target {
            text.push(' ');
        }
        if text.chars().count() == target {
            text.push(if gutter.show { '│' } else { ' ' });
        }
    }
    if position.show_connector {
        let parent_lane = 2 + position.indent.saturating_sub(1) * 3;
        while text.chars().count() < parent_lane {
            text.push(' ');
        }
        if text.chars().count() == parent_lane {
            text.push(if position.is_last { ' ' } else { '│' });
        }
    }
    let lane = 2 + position.indent * 3;
    while text.chars().count() < lane {
        text.push(' ');
    }
    text.push('│');
    text.push(' ');
    Span::styled(
        fit_tree_prefix(&text, width),
        Style::default()
            .fg(if position.active_path {
                theme.accent
            } else {
                theme.dim
            })
            .bg(background),
    )
}

fn fit_tree_prefix(text: &str, width: usize) -> String {
    let width = width.max(1);
    if UnicodeWidthStr::width(text) <= width {
        return format!("{text}{}", " ".repeat(width - UnicodeWidthStr::width(text)));
    }
    let suffix = text
        .chars()
        .rev()
        .scan(0usize, |used, character| {
            let character_width = unicode_width::UnicodeWidthChar::width(character).unwrap_or(0);
            if *used + character_width + 1 > width {
                None
            } else {
                *used += character_width;
                Some(character)
            }
        })
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    format!("…{}", truncate_width(&suffix, width.saturating_sub(1)))
}

fn markdown_style(
    markdown: MarkdownStyle,
    kind: MarkdownLineKind,
    background: Color,
    selected: bool,
    theme: Theme,
) -> Style {
    let foreground = if markdown.link {
        theme.accent
    } else if markdown.dim {
        theme.dim
    } else {
        match kind {
            MarkdownLineKind::Heading => theme.accent,
            MarkdownLineKind::Quote => theme.warning,
            MarkdownLineKind::Rule => theme.border,
            MarkdownLineKind::Body | MarkdownLineKind::Code => theme.foreground,
        }
    };
    let mut style = Style::default().fg(foreground).bg(background);
    if markdown.bold || kind == MarkdownLineKind::Heading {
        style = style.add_modifier(Modifier::BOLD);
    }
    if markdown.italic {
        style = style.add_modifier(Modifier::ITALIC);
    }
    if markdown.crossed_out {
        style = style.add_modifier(Modifier::CROSSED_OUT);
    }
    if markdown.link {
        style = style.add_modifier(Modifier::UNDERLINED);
    }
    if markdown.code {
        style = style.fg(theme.warning);
        if !selected {
            style = style.bg(theme.surface_hi);
        }
    }
    style
}
