use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

use super::super::app::{App, Focus, Page, SettingsAction};
use super::Theme;

pub(super) fn render_menu(frame: &mut Frame<'_>, app: &App, area: Rect, theme: Theme) {
    let items = Page::ALL
        .iter()
        .map(|page| ListItem::new(format!("  {}", page.label())))
        .collect::<Vec<_>>();
    let active = app.focus == Focus::Menu;
    let block = Block::default()
        .title(" Menu ")
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
        .title(" Home ")
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
        .unwrap_or_else(|| "not set".into());
    let overview = vec![
        Line::default(),
        label_line(theme, "Providers", app.snapshot.providers.len().to_string()),
        label_line(theme, "Models", model_count.to_string()),
        label_line(theme, "Default", default),
    ];
    let paths = vec![
        Line::default(),
        label_line(theme, "Models file", app.snapshot.models_path.clone()),
        label_line(theme, "Settings file", app.snapshot.settings_path.clone()),
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
    render_section(frame, sections[0], "Overview", overview, theme);
    render_section(frame, sections[1], "Paths", paths, theme);
}

pub(super) fn render_settings(frame: &mut Frame<'_>, app: &App, area: Rect, theme: Theme) {
    let block = Block::default()
        .title(" Settings ")
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
        label_line(theme, "Models file", app.snapshot.models_path.clone()),
        label_line(theme, "Settings file", app.snapshot.settings_path.clone()),
        label_line(
            theme,
            "OpenCode file",
            app.paths.opencode.display().to_string(),
        ),
    ];
    render_section(frame, sections[0], "Configuration", paths, theme);

    let actions = SettingsAction::ALL
        .into_iter()
        .map(|action| ListItem::new(action.label()))
        .collect::<Vec<_>>();
    let mut state = ListState::default();
    state.select(Some(app.settings_cursor));
    frame.render_stateful_widget(
        List::new(actions)
            .block(
                Block::default()
                    .title("Actions")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.border)),
            )
            .highlight_symbol(" > ")
            .highlight_style(theme.selected()),
        sections[1],
        &mut state,
    );
}

fn label_line(theme: Theme, label: &str, value: String) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  {label:<14}"), Style::default().fg(theme.accent)),
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
