use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};
use unicode_width::UnicodeWidthStr;

use crate::documents::{format_session_time, session_display_title};

use super::super::app::{wrap_preview_text, App, Focus, Page, SettingsAction};
use super::super::input::{pad_width, truncate_width};
use super::Theme;

pub(super) fn render_menu(frame: &mut Frame<'_>, app: &App, area: Rect, theme: Theme) {
    let items = Page::ALL
        .iter()
        .map(|page| {
            let label = page.label(app.language);
            ListItem::new(Line::from(Span::styled(
                format!("  {label}"),
                theme.value(),
            )))
        })
        .collect::<Vec<_>>();
    let active = app.focus == Focus::Menu;
    let block = Block::default()
        .title(Span::styled(
            format!(" {} ", app.language.pick("Menu", "菜单")),
            if active {
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                theme.label()
            },
        ))
        .borders(Borders::RIGHT)
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
    state.select(Some(app.page.index()));
    frame.render_stateful_widget(list, area, &mut state);
}

pub(super) fn render_home(frame: &mut Frame<'_>, app: &App, area: Rect, theme: Theme) {
    let block = Block::default()
        .title(Span::styled(
            format!(" {} ", app.language.pick("Home", "主页")),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(theme.panel(false))
        .style(theme.base());
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let model_count = app
        .snapshot
        .providers
        .iter()
        .map(|provider| provider.models.len())
        .sum::<usize>();
    let default = app
        .snapshot
        .default_provider
        .as_deref()
        .zip(app.snapshot.default_model.as_deref())
        .map(|(provider, model)| format!("{provider}/{model}"))
        .unwrap_or_else(|| app.language.pick("not set", "未设置").into());
    let overview = {
        let mut lines = Vec::new();
        if let Some(latest) = &app.update_available {
            lines.push(Line::from(Span::styled(
                format!(
                    "  {}{}",
                    app.language.pick("Update available: ", "有新版本："),
                    latest
                ),
                Style::default()
                    .fg(theme.warning)
                    .add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(Span::styled(
                format!(
                    "  {}",
                    app.language
                        .pick("npm i -g @oldsuns/pi-switch", "npm i -g @oldsuns/pi-switch")
                ),
                theme.label(),
            )));
            lines.push(Line::default());
        }
        lines.push(metric_line(
            theme,
            app.language.pick("Providers", "提供商"),
            app.snapshot.providers.len().to_string(),
        ));
        lines.push(metric_line(
            theme,
            app.language.pick("Models", "模型"),
            model_count.to_string(),
        ));
        lines.push(Line::default());
        lines.push(label_line(
            theme,
            app.language.pick("Default", "默认模型"),
            default,
        ));
        lines
    };
    let paths = vec![
        Line::default(),
        label_line(
            theme,
            app.language.pick("Models file", "模型文件"),
            app.snapshot.models_path.clone(),
        ),
        label_line(
            theme,
            app.language.pick("Settings file", "设置文件"),
            app.snapshot.settings_path.clone(),
        ),
    ];

    let direction = if inner.width >= 76 {
        Direction::Horizontal
    } else {
        Direction::Vertical
    };
    let sections = Layout::default()
        .direction(direction)
        .constraints(if inner.width >= 76 {
            [Constraint::Percentage(38), Constraint::Percentage(62)]
        } else {
            [Constraint::Percentage(50), Constraint::Percentage(50)]
        })
        .split(inner);
    render_section(
        frame,
        sections[0],
        app.language.pick("Overview", "概览"),
        overview,
        theme,
    );
    render_section(
        frame,
        sections[1],
        app.language.pick("Paths", "路径"),
        paths,
        theme,
    );
}

pub(super) fn render_settings(frame: &mut Frame<'_>, app: &App, area: Rect, theme: Theme) {
    let block = Block::default()
        .title(Span::styled(
            format!(" {} ", app.language.pick("Settings", "设置")),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(theme.panel(app.focus == Focus::Content))
        .style(theme.base());
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(7), Constraint::Min(0)])
        .split(inner);
    let paths = vec![
        Line::default(),
        label_line(
            theme,
            app.language.pick("Models file", "模型文件"),
            app.snapshot.models_path.clone(),
        ),
        label_line(
            theme,
            app.language.pick("Settings file", "设置文件"),
            app.snapshot.settings_path.clone(),
        ),
        label_line(
            theme,
            app.language.pick("OpenCode file", "OpenCode 文件"),
            app.paths.opencode.display().to_string(),
        ),
    ];
    render_section(
        frame,
        sections[0],
        app.language.pick("Configuration", "配置文件"),
        paths,
        theme,
    );

    let actions = SettingsAction::visible(app.snapshot.fetch_model_metadata)
        .map(|action| {
            let label = action.label(app.language);
            if !action.is_toggle() {
                return ListItem::new(Line::from(Span::styled(label, theme.value())));
            }
            let enabled = match action {
                SettingsAction::FetchMetadata => app.snapshot.fetch_model_metadata,
                SettingsAction::AutoCheckUpdates => app.snapshot.check_updates,
                _ => false,
            };
            ListItem::new(Line::from(vec![
                Span::styled(
                    if enabled { "●" } else { "○" },
                    Style::default().fg(if enabled { theme.accent } else { theme.muted }),
                ),
                Span::raw("  "),
                Span::styled(label, theme.value()),
            ]))
        })
        .collect::<Vec<_>>();
    let mut state = ListState::default();
    state.select(Some(
        app.settings_cursor.min(actions.len().saturating_sub(1)),
    ));
    frame.render_stateful_widget(
        List::new(actions)
            .block(
                Block::default()
                    .title(Span::styled(
                        app.language.pick("Actions", "操作"),
                        Style::default()
                            .fg(theme.accent)
                            .add_modifier(Modifier::BOLD),
                    ))
                    .borders(Borders::ALL)
                    .border_style(theme.panel(app.focus == Focus::Content)),
            )
            .highlight_symbol(" > ")
            .highlight_style(theme.selected()),
        sections[1],
        &mut state,
    );
}

pub(super) fn render_sessions(frame: &mut Frame<'_>, app: &mut App, area: Rect, theme: Theme) {
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
    let visible = app.visible_sessions();
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
    let items = if visible.is_empty() {
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
        let item_text_width = area
            .width
            .saturating_sub(2) // border
            .saturating_sub(3) // selected marker
            .saturating_sub(2) as usize; // leading padding
        visible
            .iter()
            .map(|index| {
                let session = &app.sessions[*index];
                let title = session_display_title(session);
                let time = format_session_time(session.modified);
                let verbose_meta = format!(
                    "{}{}  {time}",
                    session.message_count,
                    app.language.pick("msg", "条"),
                );
                let meta = if UnicodeWidthStr::width(verbose_meta.as_str()) + 2 <= item_text_width {
                    verbose_meta
                } else {
                    time
                };
                let meta = truncate_width(&meta, item_text_width);
                let meta_width = UnicodeWidthStr::width(meta.as_str());
                let title_budget = item_text_width.saturating_sub(meta_width + 1);
                let title_text = truncate_width(title, title_budget);
                let title_width = UnicodeWidthStr::width(title_text.as_str());
                let spacing = if meta_width == 0 || title_width == 0 {
                    0
                } else {
                    item_text_width
                        .saturating_sub(title_width + meta_width)
                        .max(1)
                };
                let mut lines = vec![Line::from(vec![
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
                ])];
                if !session.cwd.is_empty() && area.width >= 40 {
                    lines.push(Line::from(Span::styled(
                        format!(
                            "    {}",
                            truncate_width(&session.cwd, item_text_width.saturating_sub(2))
                        ),
                        theme.dim_text(),
                    )));
                }
                ListItem::new(lines)
            })
            .collect()
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
    if !visible.is_empty() {
        state.select(Some(app.session_cursor.min(visible.len() - 1)));
    }
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_session_preview(frame: &mut Frame<'_>, app: &mut App, area: Rect, theme: Theme) {
    let active = (app.focus == Focus::Content || app.focus == Focus::SessionPreview)
        && app.page == Page::Sessions;
    let preview_focused = app.focus == Focus::SessionPreview;
    let title = if app.user_only_preview {
        app.language.pick("Preview (user)", "预览（仅用户）")
    } else {
        app.language.pick("Preview", "预览")
    };
    let block = Block::default()
        .title(Span::styled(
            format!(" {title} "),
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
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let wrap_width = inner.width.saturating_sub(2).max(8) as usize;
    app.preview_wrap_width = wrap_width;
    app.preview_viewport_height = inner.height;

    let mut lines = Vec::new();
    if let Some(session) = app.selected_session() {
        lines.push(Line::from(vec![
            Span::styled(app.language.pick("id ", "编号 "), theme.label()),
            Span::styled(session.id.clone(), theme.value()),
        ]));
        if !session.cwd.is_empty() {
            lines.push(Line::from(vec![
                Span::styled(app.language.pick("cwd ", "目录 "), theme.label()),
                Span::styled(
                    truncate_width(&session.cwd, inner.width.saturating_sub(6) as usize),
                    theme.value(),
                ),
            ]));
        }
        lines.push(Line::default());
    }

    match app.preview.as_ref() {
        None if app.selected_session().is_none() => {
            lines.push(Line::from(Span::styled(
                app.language.pick("Select a session", "选择一个会话"),
                theme.label(),
            )));
        }
        None => {
            lines.push(Line::from(Span::styled(
                app.language.pick("No preview", "无预览"),
                theme.label(),
            )));
        }
        Some(messages) if messages.is_empty() => {
            lines.push(Line::from(Span::styled(
                app.language
                    .pick("(no user/assistant text)", "（无用户/助手文本）"),
                theme.label(),
            )));
        }
        Some(messages) => {
            let wrap_width = inner.width.saturating_sub(2).max(8) as usize;
            for (index, message) in messages.iter().enumerate() {
                let role = match message.role.as_str() {
                    "user" => app.language.pick("user", "用户"),
                    "assistant" => app.language.pick("assistant", "助手"),
                    other => other,
                };
                let role_style = if message.role == "user" {
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                        .fg(theme.success)
                        .add_modifier(Modifier::BOLD)
                };
                let is_selected = preview_focused && index == app.preview_message_cursor;
                let header_marker = if is_selected { "❯" } else { " " };
                let header_style = if is_selected {
                    role_style.bg(theme.accent_dim)
                } else {
                    role_style
                };
                lines.push(Line::from(vec![Span::styled(
                    format!("{header_marker} [{role}]"),
                    header_style,
                )]));
                for wrapped in wrap_preview_text(&message.text, wrap_width) {
                    let body_style = if is_selected {
                        Style::default()
                            .fg(theme.foreground)
                            .bg(theme.accent_dim)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        theme.value()
                    };
                    lines.push(Line::from(Span::styled(wrapped, body_style)));
                }
                lines.push(Line::default());
            }
        }
    }

    let scroll = app.preview_scroll as usize;
    let visible_height = inner.height as usize;
    let max_scroll = lines.len().saturating_sub(visible_height.max(1));
    let scroll = scroll.min(max_scroll);
    let view: Vec<Line<'static>> = lines
        .into_iter()
        .skip(scroll)
        .take(visible_height.max(1))
        .collect();
    frame.render_widget(
        Paragraph::new(view)
            .style(theme.base())
            .wrap(Wrap { trim: false }),
        inner,
    );
}

fn metric_line(theme: Theme, label: &str, value: String) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  {}", pad_width(label, 14)), theme.label()),
        Span::styled(
            value,
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
    ])
}

fn label_line(theme: Theme, label: &str, value: String) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  {}", pad_width(label, 14)), theme.label()),
        Span::styled(value, theme.value()),
    ])
}

fn render_section(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    lines: Vec<Line<'static>>,
    theme: Theme,
) {
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .title(Span::styled(
                        title.to_owned(),
                        Style::default()
                            .fg(theme.accent)
                            .add_modifier(Modifier::BOLD),
                    ))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.border))
                    .style(theme.base()),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
}
