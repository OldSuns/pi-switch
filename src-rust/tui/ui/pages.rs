use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

use super::super::app::{App, Focus, Page, SettingsAction};
use super::super::input::pad_width;
use super::Theme;

pub(super) fn render_menu(frame: &mut Frame<'_>, app: &App, area: Rect, theme: Theme) {
    let items = Page::ALL
        .iter()
        .map(|page| ListItem::new(format!("  {}", page.label(app.language))))
        .collect::<Vec<_>>();
    let active = app.focus == Focus::Menu;
    let block = Block::default()
        .title(format!(" {} ", app.language.pick("Menu", "菜单")))
        .borders(Borders::RIGHT)
        .border_style(Style::default().fg(if active { theme.accent } else { theme.border }));
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
        .title(format!(" {} ", app.language.pick("Home", "主页")))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border));
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
    let overview = vec![
        Line::default(),
        label_line(
            theme,
            app.language.pick("Providers", "提供商"),
            app.snapshot.providers.len().to_string(),
        ),
        label_line(
            theme,
            app.language.pick("Models", "模型"),
            model_count.to_string(),
        ),
        label_line(theme, app.language.pick("Default", "默认模型"), default),
    ];
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
        .title(format!(" {} ", app.language.pick("Settings", "设置")))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border));
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
            if action != SettingsAction::FetchMetadata {
                return ListItem::new(label);
            }
            let enabled = app.snapshot.fetch_model_metadata;
            ListItem::new(Line::from(vec![
                Span::styled(
                    if enabled { "●" } else { "○" },
                    Style::default().fg(if enabled {
                        theme.foreground
                    } else {
                        theme.muted
                    }),
                ),
                Span::raw("  "),
                Span::raw(label),
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
                    .title(app.language.pick("Actions", "操作"))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.border)),
            )
            .highlight_symbol(" > ")
            .highlight_style(Style::default().add_modifier(Modifier::BOLD)),
        sections[1],
        &mut state,
    );
}

fn label_line(theme: Theme, label: &str, value: String) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("  {}", pad_width(label, 14)),
            Style::default().fg(theme.accent),
        ),
        Span::styled(value, Style::default().fg(theme.foreground)),
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
                    .title(title)
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.border)),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
}
