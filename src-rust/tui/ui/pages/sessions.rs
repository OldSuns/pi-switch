use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};
use unicode_width::UnicodeWidthStr;

use crate::documents::{format_session_time, session_display_title};

use super::super::super::app::{App, Focus, Page};
use super::super::super::input::truncate_width;
use super::super::super::markdown::{MarkdownLine, MarkdownLineKind, MarkdownStyle};
use super::super::Theme;

pub(in crate::tui::ui) fn render_sessions(
    frame: &mut Frame<'_>,
    app: &mut App,
    area: Rect,
    theme: Theme,
) {
    let direction = if area.width >= 76 {
        Direction::Horizontal
    } else {
        Direction::Vertical
    };
    let sections = Layout::default()
        .direction(direction)
        .constraints(if area.width >= 76 {
            [Constraint::Percentage(46), Constraint::Percentage(54)]
        } else {
            [Constraint::Percentage(45), Constraint::Percentage(55)]
        })
        .split(area);
    render_session_list(frame, app, sections[0], theme);
    render_session_preview(frame, app, sections[1], theme);
}

fn render_session_list(frame: &mut Frame<'_>, app: &App, area: Rect, theme: Theme) {
    let groups = app.session_groups();
    let active = app.focus == Focus::Content && app.page == Page::Sessions;
    let filter_hint = if app.session_filtering || !app.session_filter.is_empty() {
        format!(
            "  /{}{}",
            app.session_filter,
            if app.session_filtering { "▌" } else { "" }
        )
    } else {
        String::new()
    };
    let flags = {
        let mut parts = Vec::new();
        if app.named_only {
            parts.push(app.language.pick("named", "仅命名"));
        }
        if app.user_only_preview {
            parts.push(app.language.pick("user-only", "仅用户"));
        }
        if parts.is_empty() {
            String::new()
        } else {
            format!(" [{}]", parts.join(", "))
        }
    };
    let title = format!(
        " {}{}{} ",
        app.language.pick("Sessions", "会话"),
        filter_hint,
        flags
    );
    // Maps each flattened visible-session position to its row index in `items`,
    // skipping group header rows so the cursor always lands on a session.
    let mut session_rows: Vec<usize> = Vec::new();
    let items: Vec<ListItem> = if groups.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            format!(
                "  {}",
                if !app.sessions_loaded {
                    app.language.pick("Loading…", "加载中…")
                } else if app.named_only || !app.session_filter.is_empty() {
                    app.language.pick("No matching sessions", "没有匹配的会话")
                } else {
                    app.language.pick("No sessions found", "未找到会话")
                }
            ),
            theme.label(),
        )))]
    } else {
        // Group header sits flush left; session rows indent 2 spaces under it.
        let header_text_width = area
            .width
            .saturating_sub(2) // border
            .saturating_sub(3) // selected marker
            as usize;
        let session_text_width = header_text_width.saturating_sub(2);
        let mut items: Vec<ListItem> = Vec::new();
        for group in &groups {
            // Group header row — never selectable.
            let header_text =
                group_header_text(&group.cwd, group.sessions.len(), header_text_width, app);
            items.push(ListItem::new(Line::from(Span::styled(
                header_text.to_string(),
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ))));
            for &session_index in &group.sessions {
                let session = &app.sessions[session_index];
                let title = session_display_title(session);
                let time = format_session_time(session.modified);
                let verbose_meta = format!(
                    "{}{}  {time}",
                    session.message_count,
                    app.language.pick("msg", "条"),
                );
                let meta =
                    if UnicodeWidthStr::width(verbose_meta.as_str()) + 2 <= session_text_width {
                        verbose_meta
                    } else {
                        time
                    };
                let meta = truncate_width(&meta, session_text_width);
                let meta_width = UnicodeWidthStr::width(meta.as_str());
                let title_budget = session_text_width.saturating_sub(meta_width + 1);
                let title_text = truncate_width(title, title_budget);
                let title_width = UnicodeWidthStr::width(title_text.as_str());
                let spacing = if meta_width == 0 || title_width == 0 {
                    0
                } else {
                    session_text_width
                        .saturating_sub(title_width + meta_width)
                        .max(1)
                };
                let line = Line::from(vec![
                    Span::styled(
                        format!("  {title_text}{}", " ".repeat(spacing)),
                        if session.name.is_some() {
                            Style::default()
                                .fg(theme.warning)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            theme.value()
                        },
                    ),
                    Span::styled(meta, theme.label()),
                ]);
                items.push(ListItem::new(line));
                session_rows.push(items.len() - 1);
            }
        }
        items
    };

    let block = Block::default()
        .title(Span::styled(
            title,
            if active {
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                theme.label()
            },
        ))
        .borders(Borders::ALL)
        .border_style(theme.panel(active))
        .style(theme.base());
    let list = List::new(items)
        .block(block)
        .highlight_symbol(if active { " > " } else { "   " })
        .highlight_style(if active {
            theme.selected()
        } else {
            Style::default()
                .fg(theme.muted)
                .add_modifier(Modifier::BOLD)
        });
    let mut state = ListState::default();
    if !session_rows.is_empty() {
        let cursor = app.session_cursor.min(session_rows.len() - 1);
        state.select(Some(session_rows[cursor]));
    }
    frame.render_stateful_widget(list, area, &mut state);
}

fn group_header_text(cwd: &str, count: usize, max_width: usize, app: &App) -> String {
    let directory = if cwd.is_empty() {
        app.language.pick("(no directory)", "（无目录）")
    } else {
        cwd
    };
    let count = format!("{} {}", count, app.language.pick("sessions", "个会话"));
    let count_width = UnicodeWidthStr::width(count.as_str());
    let directory_budget = max_width.saturating_sub(count_width + 4);
    let directory = if UnicodeWidthStr::width(directory) <= directory_budget {
        truncate_width(directory, directory_budget)
    } else {
        let last = directory
            .rsplit(['/', '\\'])
            .find(|part| !part.is_empty())
            .unwrap_or(directory);
        truncate_width(last, directory_budget)
    };
    let spacing = max_width
        .saturating_sub(UnicodeWidthStr::width(directory.as_str()) + count_width + 2)
        .max(1);
    truncate_width(
        &format!("▾ {directory}{}{count}", " ".repeat(spacing)),
        max_width,
    )
}

fn render_session_preview(frame: &mut Frame<'_>, app: &mut App, area: Rect, theme: Theme) {
    let preview_focused = app.focus == Focus::SessionPreview;
    let title = if app.user_only_preview {
        app.language.pick("Preview (user)", "预览（仅用户）")
    } else {
        app.language.pick("Preview", "预览")
    };
    let block = Block::default()
        .title(Span::styled(
            format!(" {title} "),
            if preview_focused {
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                theme.label()
            },
        ))
        .borders(Borders::ALL)
        .border_style(theme.panel(preview_focused))
        .style(theme.base());
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let line_width = inner.width as usize;
    let body_width = inner.width.saturating_sub(2).max(1) as usize;
    app.set_preview_geometry(body_width, inner.height);

    let mut lines = Vec::new();
    if let Some(session) = app.selected_session() {
        lines.push(filled_text_line(
            session_display_title(session),
            line_width,
            Style::default()
                .fg(theme.foreground)
                .bg(theme.background)
                .add_modifier(Modifier::BOLD),
        ));
        let metadata = format!(
            "{} {}  ·  {} {}  ·  {}",
            app.language.pick("id", "编号"),
            session.id,
            session.message_count,
            app.language.pick("messages", "条消息"),
            format_session_time(session.modified),
        );
        lines.push(filled_text_line(
            &metadata,
            line_width,
            theme.label().bg(theme.background),
        ));
        let cwd = format!(
            "{} {}",
            app.language.pick("cwd", "目录"),
            if session.cwd.is_empty() {
                "-"
            } else {
                &session.cwd
            }
        );
        lines.push(filled_text_line(
            &cwd,
            line_width,
            theme.label().bg(theme.background),
        ));
        lines.push(filled_text_line(
            &"─".repeat(line_width),
            line_width,
            Style::default().fg(theme.border).bg(theme.background),
        ));
    }

    match (&app.preview, &app.preview_layout) {
        (None, _) if app.selected_session().is_none() => {
            lines.push(filled_text_line(
                app.language.pick("Select a session", "选择一个会话"),
                line_width,
                theme.label().bg(theme.background),
            ));
        }
        (None, _) => {
            lines.push(filled_text_line(
                app.language.pick("No preview", "无预览"),
                line_width,
                theme.label().bg(theme.background),
            ));
        }
        (Some(messages), _) if messages.is_empty() => {
            lines.push(filled_text_line(
                app.language
                    .pick("(no user/assistant text)", "（无用户/助手文本）"),
                line_width,
                theme.label().bg(theme.background),
            ));
        }
        (Some(messages), Some(layout)) => {
            for (index, (message, body)) in messages.iter().zip(&layout.messages).enumerate() {
                let (role, direction, role_color) = match message.role.as_str() {
                    "user" => (app.language.pick("User", "用户"), "▶", theme.accent),
                    "assistant" => (app.language.pick("Assistant", "助手"), "◀", theme.success),
                    other => (other, "•", theme.warning),
                };
                let selected = preview_focused && index == app.preview_message_cursor;
                let background = if selected {
                    theme.accent_dim
                } else {
                    theme.surface
                };
                let marker = if selected { "❯" } else { " " };
                lines.push(filled_text_line(
                    &format!("{marker} {direction} {role}"),
                    line_width,
                    Style::default()
                        .fg(role_color)
                        .bg(background)
                        .add_modifier(Modifier::BOLD),
                ));
                for line in body {
                    lines.push(render_markdown_line(
                        line, line_width, selected, role_color, theme,
                    ));
                }
                lines.push(filled_text_line("", line_width, theme.base()));
            }
        }
        (Some(_), None) => {}
    }

    let visible_height = inner.height.max(1) as usize;
    let max_scroll = lines.len().saturating_sub(visible_height);
    let scroll = (app.preview_scroll as usize).min(max_scroll);
    app.preview_scroll = scroll.min(u16::MAX as usize) as u16;
    let view = lines
        .into_iter()
        .skip(scroll)
        .take(visible_height)
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(view).style(theme.base()), inner);
}

fn render_markdown_line(
    line: &MarkdownLine,
    width: usize,
    selected: bool,
    role_color: Color,
    theme: Theme,
) -> Line<'static> {
    let background = if selected {
        theme.accent_dim
    } else if line.kind == MarkdownLineKind::Code {
        theme.surface_hi
    } else {
        theme.background
    };
    let mut spans = vec![Span::styled(
        "│ ",
        Style::default().fg(role_color).bg(background),
    )];
    spans.extend(line.spans.iter().map(|span| {
        Span::styled(
            span.text.clone(),
            markdown_style(span.style, line.kind, background, selected, theme),
        )
    }));
    filled_line(spans, width, Style::default().bg(background))
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

fn filled_text_line(text: &str, width: usize, style: Style) -> Line<'static> {
    filled_line(
        vec![Span::styled(truncate_width(text, width), style)],
        width,
        style,
    )
}

fn filled_line(mut spans: Vec<Span<'static>>, width: usize, fill: Style) -> Line<'static> {
    let used = spans
        .iter()
        .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
        .sum::<usize>();
    if used < width {
        spans.push(Span::styled(" ".repeat(width - used), fill));
    }
    Line::from(spans)
}
