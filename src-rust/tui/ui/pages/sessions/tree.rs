use super::*;

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

pub(super) fn preview_tree_width(app: &App, width: usize) -> usize {
    let max_prefix = app
        .preview
        .as_ref()
        .map(|preview| {
            app.preview_visible
                .iter()
                .filter_map(|index| preview.messages.get(*index))
                .map(|message| 6 + message.tree.indent * 2)
                .max()
                .unwrap_or(4)
        })
        .unwrap_or(4);
    let cap = width.saturating_sub(10).max(8).min((width / 2).max(8));
    max_prefix.min(cap).max(4).min(width.max(1))
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
        let target = 2 + gutter.position * 2;
        while text.chars().count() < target {
            text.push(' ');
        }
        if text.chars().count() == target {
            text.push(if gutter.show { '│' } else { ' ' });
        }
    }
    if position.show_connector {
        let target = 2 + position.indent.saturating_sub(1) * 2;
        while text.chars().count() < target {
            text.push(' ');
        }
        text.push_str(if position.is_last { "└─" } else { "├─" });
    } else {
        let target = 2 + position.indent * 2;
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
        let target = 2 + gutter.position * 2;
        while text.chars().count() < target {
            text.push(' ');
        }
        if text.chars().count() == target {
            text.push(if gutter.show { '│' } else { ' ' });
        }
    }
    if position.show_connector {
        let parent_lane = 2 + position.indent.saturating_sub(1) * 2;
        while text.chars().count() < parent_lane {
            text.push(' ');
        }
        if text.chars().count() == parent_lane {
            text.push(if position.is_last { ' ' } else { '│' });
        }
    }
    let lane = 2 + position.indent * 2;
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
