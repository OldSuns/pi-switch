mod pages;

use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};
use unicode_width::UnicodeWidthStr;

use crate::documents::{CatalogModel, PI_DEFAULT_CONTEXT_WINDOW, PI_DEFAULT_MAX_TOKENS};

use super::{
    app::{visible_fetched_indices, App, Focus, Notice, NoticeKind, Overlay, Page},
    forms::{FormState, ModelDefaultsFormState, ModelFormState},
    i18n::Language,
    input::{api_label, mask_secret, pad_width, truncate_width, with_cursor, wrap_width},
    keys::{all_shortcuts, shortcut, Command},
    COMPACT_WIDTH, WIDE_WIDTH,
};
use pages::{render_home, render_menu, render_settings};

const MENU_WIDTH: u16 = 18;
const SHELL_WIDE_WIDTH: u16 = MENU_WIDTH + WIDE_WIDTH;
const DETAIL_LABEL_WIDTH: usize = 10;
const MIN_MODELS_HEIGHT: u16 = 5;
const MODEL_DEFAULT_VALUE_WIDTH: usize = 16;

#[derive(Clone, Copy)]
struct Theme {
    foreground: Color,
    background: Color,
    surface: Color,
    accent: Color,
    success: Color,
    warning: Color,
    error: Color,
    muted: Color,
    border: Color,
}

impl Theme {
    fn detect() -> Self {
        if std::env::var_os("NO_COLOR").is_some() {
            return Self {
                foreground: Color::Reset,
                background: Color::Reset,
                surface: Color::Reset,
                accent: Color::Reset,
                success: Color::Reset,
                warning: Color::Reset,
                error: Color::Reset,
                muted: Color::Reset,
                border: Color::Reset,
            };
        }
        Self {
            foreground: Color::Rgb(230, 237, 243),
            background: Color::Rgb(13, 17, 23),
            surface: Color::Rgb(22, 27, 34),
            accent: Color::Rgb(88, 166, 255),
            success: Color::Rgb(63, 185, 80),
            warning: Color::Rgb(210, 153, 34),
            error: Color::Rgb(248, 81, 73),
            muted: Color::Rgb(139, 148, 158),
            border: Color::Rgb(48, 54, 61),
        }
    }

    fn base(self) -> Style {
        Style::default().fg(self.foreground).bg(self.background)
    }

    fn selected(self) -> Style {
        Style::default()
            .fg(self.foreground)
            .bg(self.surface)
            .add_modifier(Modifier::BOLD)
    }
}

pub(super) fn draw(frame: &mut Frame<'_>, app: &mut App) {
    let theme = Theme::detect();
    frame.render_widget(Block::default().style(theme.base()), frame.area());
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(8),
        Constraint::Length(1),
    ])
    .areas(frame.area());
    render_header(frame, app, header, theme);
    if frame.area().width >= SHELL_WIDE_WIDTH {
        let [menu, content] =
            Layout::horizontal([Constraint::Length(MENU_WIDTH), Constraint::Min(1)]).areas(body);
        render_menu(frame, app, menu, theme);
        render_page(frame, app, content, theme);
    } else if app.focus == Focus::Menu {
        app.width = body.width;
        render_menu(frame, app, body, theme);
    } else {
        render_page(frame, app, body, theme);
    }
    render_footer(frame, app, footer, theme);
    if let Some(notice) = app.notice.as_ref() {
        render_notice(frame, notice, frame.area(), theme);
    }
    if let Some(overlay) = app.overlay.as_ref() {
        render_overlay(
            frame,
            overlay,
            app.language,
            app.tick_count,
            frame.area(),
            theme,
        );
    }
}

fn render_page(frame: &mut Frame<'_>, app: &mut App, area: Rect, theme: Theme) {
    app.width = area.width;
    match app.page {
        Page::Home => render_home(frame, app, area, theme),
        Page::Profiles => render_profiles_page(frame, app, area, theme),
        Page::Settings => render_settings(frame, app, area, theme),
    }
}

fn render_profiles_page(frame: &mut Frame<'_>, app: &App, area: Rect, theme: Theme) {
    if app.width >= WIDE_WIDTH {
        let [providers, detail] =
            Layout::horizontal([Constraint::Length(34), Constraint::Min(42)]).areas(area);
        render_providers(frame, app, providers, theme);
        render_detail(frame, app, detail, theme);
    } else if app.width >= COMPACT_WIDTH {
        let [providers, detail] =
            Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)])
                .areas(area);
        render_providers(frame, app, providers, theme);
        render_detail(frame, app, detail, theme);
    } else if app.narrow_detail {
        render_detail(frame, app, area, theme);
    } else {
        render_providers(frame, app, area, theme);
    }
}

fn render_header(frame: &mut Frame<'_>, app: &App, area: Rect, theme: Theme) {
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
    let title = Line::from(vec![
        Span::styled(
            " pi-switch ",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(env!("CARGO_PKG_VERSION"), Style::default().fg(theme.muted)),
        Span::raw("  "),
        Span::styled(
            app.language.pick("default ", "默认 "),
            Style::default().fg(theme.muted),
        ),
        Span::raw(default),
        Span::styled(
            format!(
                "  {} {}  {} {}",
                app.snapshot.providers.len(),
                app.language.pick("provider(s)", "个提供商"),
                model_count,
                app.language.pick("model(s)", "个模型"),
            ),
            Style::default().fg(theme.muted),
        ),
    ]);
    let path_width = area.width.saturating_sub(4) as usize;
    let path = Line::from(vec![
        Span::styled(
            app.language.pick(" models ", " 模型文件 "),
            Style::default().fg(theme.muted),
        ),
        Span::raw(truncate_width(
            &app.snapshot.models_path,
            path_width.saturating_sub(8),
        )),
    ]);
    frame.render_widget(
        Paragraph::new(vec![title, path]).block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(theme.border)),
        ),
        area,
    );
}

fn render_providers(frame: &mut Frame<'_>, app: &App, area: Rect, theme: Theme) {
    let visible = app.visible_providers();
    let content_width = area.width.saturating_sub(6) as usize;
    let title = if app.filtering || !app.filter.is_empty() {
        format!(
            " {}  /{}{} ",
            app.language.pick("Providers", "提供商"),
            app.filter,
            if app.filtering { "_" } else { "" }
        )
    } else {
        format!(
            " {}  {} ",
            app.language.pick("Providers", "提供商"),
            visible.len()
        )
    };
    let items = visible
        .iter()
        .map(|index| {
            let provider = &app.snapshot.providers[*index];
            let default = app.snapshot.default_provider.as_deref() == Some(&provider.id);
            let marker = if default { "*" } else { " " };
            let mut lines = wrap_width(&provider.id, content_width)
                .into_iter()
                .enumerate()
                .map(|(line, id)| {
                    Line::from(vec![
                        Span::styled(
                            if line == 0 {
                                format!("{marker} ")
                            } else {
                                "  ".into()
                            },
                            Style::default().fg(theme.success),
                        ),
                        Span::styled(id, Style::default().add_modifier(Modifier::BOLD)),
                    ])
                })
                .collect::<Vec<_>>();
            let api = if provider.api.is_empty() {
                app.language.pick("inherited", "继承")
            } else {
                &provider.api
            };
            lines.extend(wrap_width(api, content_width).into_iter().map(|api| {
                Line::from(vec![
                    Span::raw("  "),
                    Span::styled(api, Style::default().fg(theme.muted)),
                ])
            }));
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!(
                        "{} {}",
                        provider.models.len(),
                        app.language.pick(
                            if provider.models.len() == 1 {
                                "model"
                            } else {
                                "models"
                            },
                            "个模型"
                        )
                    ),
                    Style::default().fg(theme.muted),
                ),
            ]));
            ListItem::new(Text::from(lines))
        })
        .collect::<Vec<_>>();
    let active = app.focus == Focus::Providers || (app.width < COMPACT_WIDTH && !app.narrow_detail);
    let border = if active { theme.accent } else { theme.border };
    let block = Block::default()
        .title(title)
        .borders(Borders::RIGHT)
        .border_style(Style::default().fg(border));
    if items.is_empty() {
        frame.render_widget(
            Paragraph::new(if app.filter.is_empty() {
                app.language.pick(
                    "No providers yet. Press n to add one.",
                    "暂无提供商，按 n 新建。",
                )
            } else {
                app.language.pick(
                    "No provider matches this filter.",
                    "没有提供商符合当前筛选条件。",
                )
            })
            .style(Style::default().fg(theme.muted))
            .block(block)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true }),
            area,
        );
        return;
    }
    let mut state = ListState::default().with_selected(Some(app.provider_cursor));
    let highlight = if active {
        theme.selected()
    } else {
        Style::default()
            .fg(theme.muted)
            .add_modifier(Modifier::BOLD)
    };
    let list = List::new(items)
        .block(block)
        .highlight_symbol(if active { " > " } else { "   " })
        .highlight_style(highlight);
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_detail(frame: &mut Frame<'_>, app: &App, area: Rect, theme: Theme) {
    let Some(provider) = app.selected_provider() else {
        frame.render_widget(
            Paragraph::new(
                app.language
                    .pick("Select or add a provider", "请选择或新建提供商"),
            )
            .style(Style::default().fg(theme.muted))
            .alignment(Alignment::Center),
            area,
        );
        return;
    };
    let key = mask_secret(&provider.api_key);
    let headers = provider
        .raw
        .get("headers")
        .and_then(serde_json::Value::as_object);
    let header_summary = headers
        .filter(|headers| !headers.is_empty())
        .map(|headers| {
            let names = headers.keys().cloned().collect::<Vec<_>>().join(", ");
            format!(
                "{} {}: {names}",
                headers.len(),
                app.language.pick("configured", "项")
            )
        })
        .unwrap_or_else(|| app.language.pick("none", "未设置").into());
    let value_width = area.width.saturating_sub(DETAIL_LABEL_WIDTH as u16).max(1) as usize;
    let lines = [
        detail_field_lines(
            "API",
            if provider.api.is_empty() {
                app.language.pick("inherited", "继承")
            } else {
                &provider.api
            },
            value_width,
            theme,
        ),
        detail_field_lines(
            app.language.pick("Base URL", "基础 URL"),
            if provider.base_url.is_empty() {
                app.language.pick("built-in default", "内置默认值")
            } else {
                &provider.base_url
            },
            value_width,
            theme,
        ),
        detail_field_lines(
            app.language.pick("API key", "API 密钥"),
            if provider.api_key.is_empty() {
                app.language.pick("auth.json / CLI", "auth.json / 命令行")
            } else {
                &key
            },
            value_width,
            theme,
        ),
        detail_field_lines(
            app.language.pick("Auth", "认证"),
            if provider.auth_header {
                app.language.pick("enabled", "启用")
            } else {
                app.language.pick("custom headers only", "仅自定义请求头")
            },
            value_width,
            theme,
        ),
        detail_field_lines(
            app.language.pick("Headers", "请求头"),
            &header_summary,
            value_width,
            theme,
        ),
        detail_field_lines(
            app.language.pick("Compat", "兼容选项"),
            if provider.raw.get("compat").is_some() {
                app.language.pick("custom", "自定义")
            } else {
                app.language.pick("defaults", "默认")
            },
            value_width,
            theme,
        ),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    let desired_info_height = lines.len() as u16 + 1;
    let max_info_height = area.height.saturating_sub(MIN_MODELS_HEIGHT).max(1);
    let info_height = desired_info_height.max(8).min(max_info_height);
    let [info, models] = Layout::vertical([
        Constraint::Length(info_height),
        Constraint::Min(MIN_MODELS_HEIGHT),
    ])
    .areas(area);
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .title(format!(" {} ", provider.id))
                    .borders(Borders::BOTTOM)
                    .border_style(Style::default().fg(theme.border)),
            )
            .wrap(Wrap { trim: false }),
        info,
    );

    let content_width = models.width.saturating_sub(6) as usize;
    let items = provider
        .models
        .iter()
        .enumerate()
        .map(|(index, model)| {
            let is_default = app.snapshot.default_provider.as_deref() == Some(&provider.id)
                && app.snapshot.default_model.as_deref() == Some(&model.id);
            let context = model
                .context_window
                .map(format_token_count)
                .unwrap_or_else(|| app.language.pick("unset", "未设置").into());
            let max_tokens = model
                .max_tokens
                .map(format_token_count)
                .unwrap_or_else(|| app.language.pick("unset", "未设置").into());
            let details = format!(
                "{}  {}{}  ctx {}  max {}",
                model
                    .name
                    .as_deref()
                    .filter(|name| *name != model.id)
                    .unwrap_or(""),
                model
                    .api
                    .as_deref()
                    .unwrap_or_else(|| app.language.pick("inherit", "继承")),
                if model.reasoning {
                    app.language.pick("  reasoning", "  推理")
                } else {
                    ""
                },
                context,
                max_tokens
            );
            ListItem::new(Text::from(model_item_lines(
                index,
                &model.id,
                if is_default {
                    app.language.pick("  default", "  默认")
                } else {
                    ""
                },
                &details,
                content_width,
                theme,
            )))
        })
        .collect::<Vec<_>>();
    let active = app.focus == Focus::Models || (app.width < COMPACT_WIDTH && app.narrow_detail);
    let border = if active { theme.accent } else { theme.border };
    let position = if provider.models.is_empty() {
        String::new()
    } else {
        format!("  {}/{}", app.model_cursor + 1, provider.models.len())
    };
    let block = Block::default()
        .title(format!(
            " {}  {}{} ",
            app.language.pick("Models", "模型"),
            provider.models.len(),
            position
        ))
        .borders(if active { Borders::ALL } else { Borders::TOP })
        .border_style(Style::default().fg(border));
    if items.is_empty() {
        frame.render_widget(
            Paragraph::new(app.language.pick(
                "No models yet. Press n to add one or i to import the catalog.",
                "暂无模型，按 n 新建或按 i 从实时目录导入。",
            ))
            .style(Style::default().fg(theme.muted))
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true })
            .block(block),
            models,
        );
    } else {
        let mut state = ListState::default().with_selected(Some(app.model_cursor));
        let highlight = if active {
            theme.selected()
        } else {
            Style::default()
                .fg(theme.muted)
                .add_modifier(Modifier::BOLD)
        };
        frame.render_stateful_widget(
            List::new(items)
                .block(block)
                .highlight_symbol(if active { " > " } else { "   " })
                .highlight_style(highlight),
            models,
            &mut state,
        );
    }
}

fn detail_field_lines(
    label: &str,
    value: &str,
    value_width: usize,
    theme: Theme,
) -> Vec<Line<'static>> {
    let label = pad_width(label, DETAIL_LABEL_WIDTH);
    let indent = " ".repeat(DETAIL_LABEL_WIDTH);
    wrap_width(value, value_width)
        .into_iter()
        .enumerate()
        .map(|(index, value)| {
            Line::from(vec![
                Span::styled(
                    if index == 0 {
                        label.clone()
                    } else {
                        indent.clone()
                    },
                    Style::default().fg(theme.muted),
                ),
                Span::raw(value),
            ])
        })
        .collect()
}

fn model_item_lines(
    index: usize,
    id: &str,
    default_label: &str,
    details: &str,
    width: usize,
    theme: Theme,
) -> Vec<Line<'static>> {
    let number = format!("{:>2} ", index + 1);
    let marker = if default_label.is_empty() { "  " } else { "* " };
    let prefix_width = UnicodeWidthStr::width(number.as_str()) + UnicodeWidthStr::width(marker);
    let id_width = width
        .saturating_sub(prefix_width + UnicodeWidthStr::width(default_label))
        .max(1);
    let id_lines = wrap_width(id, id_width);
    let last_id_line = id_lines.len().saturating_sub(1);
    let mut lines = id_lines
        .into_iter()
        .enumerate()
        .map(|(line_index, id)| {
            let mut spans = if line_index == 0 {
                vec![
                    Span::styled(number.clone(), Style::default().fg(theme.muted)),
                    Span::styled(marker, Style::default().fg(theme.success)),
                    Span::raw(id),
                ]
            } else {
                vec![Span::raw(" ".repeat(prefix_width)), Span::raw(id)]
            };
            if line_index == last_id_line && !default_label.is_empty() {
                spans.push(Span::styled(
                    default_label.to_owned(),
                    Style::default().fg(theme.success),
                ));
            }
            Line::from(spans)
        })
        .collect::<Vec<_>>();
    lines.extend(
        wrap_width(details, width.saturating_sub(4).max(1))
            .into_iter()
            .map(|details| {
                Line::from(Span::styled(
                    format!("    {details}"),
                    Style::default().fg(theme.muted),
                ))
            }),
    );
    lines
}

fn render_footer(frame: &mut Frame<'_>, app: &App, area: Rect, theme: Theme) {
    let compact = app.width < 76;
    let binding = |command| {
        let shortcut = shortcut(command);
        (
            shortcut.key,
            if app.language == Language::English {
                shortcut.label
            } else {
                app.language.command_label(command)
            },
        )
    };
    let mut keys = if app.filtering {
        vec![
            ("Enter", app.language.pick("apply", "应用")),
            ("Esc", app.language.pick("clear", "清除")),
        ]
    } else if app.focus == Focus::Menu {
        vec![
            ("Up/Down", app.language.pick("navigate", "导航")),
            ("Enter", app.language.pick("open", "打开")),
            ("q", app.language.pick("quit", "退出")),
        ]
    } else if app.in_model_context() {
        let commands = if compact {
            &[
                Command::New,
                Command::Edit,
                Command::Delete,
                Command::SetDefault,
            ][..]
        } else {
            &[
                Command::New,
                Command::Edit,
                Command::Delete,
                Command::Copy,
                Command::SetDefault,
                Command::Import,
                Command::Help,
            ][..]
        };
        commands.iter().map(|command| binding(*command)).collect()
    } else if app.page == Page::Profiles {
        let commands = if compact {
            &[
                Command::New,
                Command::Edit,
                Command::Delete,
                Command::Import,
                Command::Help,
            ][..]
        } else {
            &[
                Command::New,
                Command::Edit,
                Command::Delete,
                Command::Copy,
                Command::Import,
                Command::Filter,
                Command::Help,
            ][..]
        };
        commands.iter().map(|command| binding(*command)).collect()
    } else if app.page == Page::Settings {
        vec![
            ("Up/Down", app.language.pick("select", "选择")),
            ("Enter/Space", app.language.pick("run", "执行")),
        ]
    } else {
        vec![
            binding(Command::Reload),
            ("Left", app.language.pick("menu", "菜单")),
        ]
    };
    if !compact && !app.filtering && app.focus != Focus::Menu {
        if app.page == Page::Profiles {
            if app.in_model_context() {
                keys.push(("Left", app.language.pick("providers", "提供商")));
            } else {
                keys.push(("Enter", app.language.pick("models", "模型")));
                keys.push(("Left", app.language.pick("menu", "菜单")));
            }
        } else if app.page != Page::Home {
            keys.push(("Left", app.language.pick("menu", "菜单")));
        }
    }
    let mut spans = Vec::new();
    for (key, label) in keys {
        spans.push(Span::styled(
            format!(" {key} "),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!("{label}  "),
            Style::default().fg(theme.muted),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_key_hints(frame: &mut Frame<'_>, area: Rect, hints: &[(&str, &str)], theme: Theme) {
    let mut lines = Vec::new();
    let mut spans = Vec::new();
    let mut line_width = 0;
    for &(key, label) in hints {
        let key_width = UnicodeWidthStr::width(key) + 2;
        let label_width = UnicodeWidthStr::width(label) + 2;
        if !spans.is_empty() && line_width + key_width + label_width > area.width as usize {
            lines.push(Line::from(std::mem::take(&mut spans)));
            line_width = 0;
        }
        spans.push(Span::styled(
            format!(" {key} "),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!("{label}  "),
            Style::default().fg(theme.muted),
        ));
        line_width += key_width + label_width;
    }
    if !spans.is_empty() {
        lines.push(Line::from(spans));
    }
    frame.render_widget(Paragraph::new(Text::from(lines)), area);
}

fn render_notice(frame: &mut Frame<'_>, notice: &Notice, area: Rect, theme: Theme) {
    let color = match notice.kind {
        NoticeKind::Success => theme.success,
        NoticeKind::Warning => theme.warning,
    };
    let width = (UnicodeWidthStr::width(notice.message.as_str()) as u16 + 4)
        .min(area.width.saturating_sub(2))
        .max(12);
    let rect = Rect::new(area.right().saturating_sub(width + 1), area.y + 1, width, 3);
    clear_area(frame, rect, theme);
    frame.render_widget(
        Paragraph::new(truncate_width(
            &notice.message,
            width.saturating_sub(4) as usize,
        ))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(color)),
        )
        .alignment(Alignment::Center),
        rect,
    );
}

fn render_overlay(
    frame: &mut Frame<'_>,
    overlay: &Overlay,
    language: Language,
    tick: usize,
    area: Rect,
    theme: Theme,
) {
    match overlay {
        Overlay::Help => {
            let rect = modal_rect(area, 76, 22);
            let mut lines = vec![
                Line::from(language.pick(
                    "Up/Down or j/k  move selection",
                    "上/下 或 j/k      移动选择",
                )),
                Line::from(language.pick(
                    "Left/Right      move between menu and content",
                    "左/右            在菜单与内容间移动",
                )),
                Line::from(language.pick(
                    "Enter/Esc       open / go back",
                    "Enter/Esc       打开 / 返回",
                )),
            ];
            lines.extend(
                all_shortcuts()
                    .iter()
                    .filter(|binding| binding.command != Command::Help)
                    .map(|binding| {
                        let help = if language == Language::English {
                            binding.help
                        } else {
                            language.command_help(binding.command)
                        };
                        Line::from(format!("{:<16} {help}", binding.key))
                    }),
            );
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                language.pick(
                    "Esc or Enter closes this window",
                    "按 Esc 或 Enter 关闭窗口",
                ),
                Style::default().fg(theme.muted),
            )));
            render_modal(
                frame,
                rect,
                language.pick(" Help ", " 帮助 "),
                Paragraph::new(lines),
                theme.accent,
                theme,
            );
        }
        Overlay::Error(message) => {
            let rect = modal_rect(area, 72, 10);
            let body = Paragraph::new(vec![
                Line::from(Span::styled(
                    language.pick("Operation failed", "操作失败"),
                    Style::default()
                        .fg(theme.error)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(message.as_str()),
                Line::from(""),
                Line::from(Span::styled(
                    language.pick("Esc or Enter to close", "按 Esc 或 Enter 关闭"),
                    Style::default().fg(theme.muted),
                )),
            ])
            .wrap(Wrap { trim: false });
            render_modal(
                frame,
                rect,
                language.pick(" Error ", " 错误 "),
                body,
                theme.error,
                theme,
            );
        }
        Overlay::Form(form) => render_form(frame, form, language, area, theme),
        Overlay::ModelForm(form) => render_model_form(frame, form, language, area, theme),
        Overlay::ModelDefaultsForm(form) => {
            render_model_defaults_form(frame, form, language, area, theme)
        }
        Overlay::ConfirmDeleteProvider(id) => {
            let rect = modal_rect(area, 56, 8);
            let body = Paragraph::new(vec![
                Line::from(format!(
                    "{} '{id}'?",
                    language.pick("Delete provider", "删除提供商")
                )),
                Line::from(language.pick(
                    "Its default selection will also be cleared.",
                    "关联的默认选择也会被清除。",
                )),
                Line::from(""),
                Line::from(Span::styled(
                    language.pick(
                        "Enter/y confirm   Esc/n cancel",
                        "Enter/y 确认   Esc/n 取消",
                    ),
                    Style::default().fg(theme.muted),
                )),
            ])
            .wrap(Wrap { trim: true });
            render_modal(
                frame,
                rect,
                language.pick(" Confirm delete ", " 确认删除 "),
                body,
                theme.error,
                theme,
            );
        }
        Overlay::ConfirmDeleteModel {
            provider_id,
            model_id,
        } => {
            let rect = modal_rect(area, 62, 8);
            let body = Paragraph::new(vec![
                Line::from(format!(
                    "{} '{model_id}' {} '{provider_id}'?",
                    language.pick("Delete model", "删除模型"),
                    language.pick("from", "来自提供商")
                )),
                Line::from(language.pick(
                    "Its default selection will be cleared if necessary.",
                    "如有需要，关联的默认选择也会被清除。",
                )),
                Line::from(""),
                Line::from(Span::styled(
                    language.pick(
                        "Enter/y confirm   Esc/n cancel",
                        "Enter/y 确认   Esc/n 取消",
                    ),
                    Style::default().fg(theme.muted),
                )),
            ])
            .wrap(Wrap { trim: true });
            render_modal(
                frame,
                rect,
                language.pick(" Confirm model delete ", " 确认删除模型 "),
                body,
                theme.error,
                theme,
            );
        }
        Overlay::Backups { items, selected } => {
            let rect = modal_rect(area, 76, 20);
            clear_area(frame, rect, theme);
            let block = Block::default()
                .title(format!(
                    " {}  {} ",
                    language.pick("Backups", "备份"),
                    items.len()
                ))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.accent));
            let inner = block.inner(rect);
            frame.render_widget(block, rect);
            let [list, hint] =
                Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);
            if items.is_empty() {
                frame.render_widget(
                    Paragraph::new(language.pick("No backups yet.", "暂无备份。"))
                        .alignment(Alignment::Center),
                    list,
                );
            } else {
                let rows = items
                    .iter()
                    .map(|item| ListItem::new(item.name.clone()))
                    .collect::<Vec<_>>();
                let mut state = ListState::default().with_selected(Some(*selected));
                frame.render_stateful_widget(
                    List::new(rows).highlight_symbol(" > ").highlight_style(
                        Style::default()
                            .fg(theme.accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                    list,
                    &mut state,
                );
            }
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(" Enter/Space ", Style::default().fg(theme.accent)),
                    Span::styled(
                        language.pick("restore  ", "恢复  "),
                        Style::default().fg(theme.muted),
                    ),
                    Span::styled(" Esc ", Style::default().fg(theme.accent)),
                    Span::styled(
                        language.pick("close", "关闭"),
                        Style::default().fg(theme.muted),
                    ),
                ])),
                hint,
            );
        }
        Overlay::ConfirmRestore(backup) => {
            let rect = modal_rect(area, 62, 8);
            let body = Paragraph::new(vec![
                Line::from(format!(
                    "{} {}?",
                    language.pick("Restore", "恢复"),
                    backup.name
                )),
                Line::from(language.pick(
                    "The current document is backed up first.",
                    "恢复前会先备份当前文件。",
                )),
                Line::from(""),
                Line::from(Span::styled(
                    language.pick(
                        "Enter/y confirm   Esc/n cancel",
                        "Enter/y 确认   Esc/n 取消",
                    ),
                    Style::default().fg(theme.muted),
                )),
            ])
            .wrap(Wrap { trim: true });
            render_modal(
                frame,
                rect,
                language.pick(" Confirm restore ", " 确认恢复 "),
                body,
                theme.warning,
                theme,
            );
        }
        Overlay::Doctor(checks) => {
            let rect = modal_rect(area, 82, 22);
            let mut lines = checks
                .iter()
                .flat_map(|check| {
                    let color = if check.ok { theme.success } else { theme.error };
                    [
                        Line::from(vec![
                            Span::styled(
                                if check.ok { "OK " } else { "!! " },
                                Style::default().fg(color).add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(
                                check.label.clone(),
                                Style::default().add_modifier(Modifier::BOLD),
                            ),
                        ]),
                        Line::from(Span::styled(
                            format!("   {}", check.detail),
                            Style::default().fg(theme.muted),
                        )),
                    ]
                })
                .collect::<Vec<_>>();
            lines.push(Line::default());
            lines.push(Line::from(Span::styled(
                language.pick("Esc/Enter close", "Esc/Enter 关闭"),
                Style::default().fg(theme.muted),
            )));
            render_modal(
                frame,
                rect,
                language.pick(" Doctor ", " 配置检查 "),
                Paragraph::new(lines).wrap(Wrap { trim: false }),
                theme.accent,
                theme,
            );
        }
        Overlay::Loading { message } => {
            let rect = modal_rect(area, 52, 7);
            let spinner = ["|", "/", "-", "\\"][tick % 4];
            let body = Paragraph::new(vec![
                Line::from(""),
                Line::from(vec![
                    Span::styled(
                        format!("{spinner} "),
                        Style::default()
                            .fg(theme.accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(message),
                ]),
                Line::from(Span::styled(
                    language.pick(
                        "Please wait; this request cannot be cancelled",
                        "请稍候，当前请求无法取消",
                    ),
                    Style::default().fg(theme.muted),
                )),
            ])
            .alignment(Alignment::Center);
            render_modal(
                frame,
                rect,
                language.pick(" Model catalog ", " 模型目录 "),
                body,
                theme.accent,
                theme,
            );
        }
        Overlay::Fetched {
            models,
            selected,
            cursor,
            unavailable,
            ratio_config_used,
            filter,
            filtering,
            ..
        } => {
            let rect = near_full_rect(area);
            clear_area(frame, rect, theme);
            let price_source = if *ratio_config_used {
                language.pick("prices: ratio_config", "价格: ratio_config")
            } else {
                language.pick("prices: models.dev", "价格: models.dev")
            };
            let filter_label = if *filtering || !filter.is_empty() {
                format!("  /{}{}", filter, if *filtering { "_" } else { "" })
            } else {
                String::new()
            };
            let block = Block::default()
                .title(format!(
                    " {}  {}/{} {}  {}{}{} ",
                    language.pick("Model catalog", "模型目录"),
                    selected.len(),
                    models.len(),
                    language.pick("selected", "已选择"),
                    price_source,
                    if *unavailable > 0 {
                        format!(
                            "  {} {}",
                            unavailable,
                            language.pick("unavailable", "无元数据")
                        )
                    } else {
                        String::new()
                    },
                    filter_label,
                ))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.accent));
            let inner = block.inner(rect);
            frame.render_widget(block, rect);
            let [list, hint] =
                Layout::vertical([Constraint::Min(1), Constraint::Length(2)]).areas(inner);
            let visible = visible_fetched_indices(models, filter);
            let rows = visible
                .iter()
                .map(|&original| {
                    let model = &models[original];
                    ListItem::new(Text::from(vec![
                        Line::from(format!(
                            "{} {}",
                            if selected.contains(&original) {
                                "[x]"
                            } else {
                                "[ ]"
                            },
                            model.id
                        )),
                        Line::from(Span::styled(
                            format!("    {}", catalog_summary(model, language)),
                            Style::default().fg(theme.muted),
                        )),
                    ]))
                })
                .collect::<Vec<_>>();
            let mut state =
                ListState::default().with_selected(if *filtering { None } else { Some(*cursor) });
            frame.render_stateful_widget(
                List::new(rows).highlight_symbol(" > ").highlight_style(
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                list,
                &mut state,
            );
            if visible.is_empty() {
                frame.render_widget(
                    Paragraph::new(
                        language.pick("No models match this filter.", "没有模型符合当前筛选条件。"),
                    )
                    .style(Style::default().fg(theme.muted))
                    .alignment(Alignment::Center),
                    list,
                );
            }
            render_key_hints(
                frame,
                hint,
                &[
                    ("Space", language.pick("toggle", "切换")),
                    ("a", language.pick("all", "全选")),
                    ("/", language.pick("filter", "筛选")),
                    ("Enter/s", language.pick("import", "导入")),
                    ("Esc", language.pick("cancel", "取消")),
                ],
                theme,
            );
        }
        Overlay::CatalogMatches {
            ambiguities,
            index,
            cursor,
            ..
        } => {
            let rect = near_full_rect(area);
            clear_area(frame, rect, theme);
            let block = Block::default()
                .title(format!(
                    " {}  {}/{} ",
                    language.pick("Choose metadata source", "选择元数据来源"),
                    index + 1,
                    ambiguities.len()
                ))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.accent));
            let inner = block.inner(rect);
            frame.render_widget(block, rect);
            let [heading, list, hint] = Layout::vertical([
                Constraint::Length(3),
                Constraint::Min(1),
                Constraint::Length(1),
            ])
            .areas(inner);
            if let Some(ambiguity) = ambiguities.get(*index) {
                frame.render_widget(
                    Paragraph::new(vec![
                        Line::from(format!(
                            "{}: {}",
                            language.pick("Target provider", "目标提供商"),
                            ambiguity.provider_id
                        )),
                        Line::from(format!(
                            "{}: {}",
                            language.pick("Model", "模型"),
                            ambiguity.model_id
                        )),
                    ])
                    .wrap(Wrap { trim: false }),
                    heading,
                );
                let rows = ambiguity
                    .candidates
                    .iter()
                    .map(|candidate| {
                        ListItem::new(Text::from(vec![
                            Line::from(Span::styled(
                                candidate.provider_id.clone(),
                                Style::default().add_modifier(Modifier::BOLD),
                            )),
                            Line::from(Span::styled(
                                format!("    {}", catalog_summary(&candidate.model, language)),
                                Style::default().fg(theme.muted),
                            )),
                        ]))
                    })
                    .collect::<Vec<_>>();
                let mut state = ListState::default().with_selected(Some(*cursor));
                frame.render_stateful_widget(
                    List::new(rows).highlight_symbol(" > ").highlight_style(
                        Style::default()
                            .fg(theme.accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                    list,
                    &mut state,
                );
            }
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(" Enter/Space ", Style::default().fg(theme.accent)),
                    Span::styled(
                        language.pick("select  ", "选择  "),
                        Style::default().fg(theme.muted),
                    ),
                    Span::styled(" Esc ", Style::default().fg(theme.accent)),
                    Span::styled(
                        language.pick("cancel", "取消"),
                        Style::default().fg(theme.muted),
                    ),
                ])),
                hint,
            );
        }
        Overlay::OpenCodeProviders {
            providers,
            selected,
            cursor,
        } => {
            let rect = modal_rect(area, 70, 22);
            clear_area(frame, rect, theme);
            let block = Block::default()
                .title(format!(
                    " {}  {}/{} {} ",
                    language.pick("OpenCode providers", "OpenCode 提供商"),
                    selected.len(),
                    providers.len(),
                    language.pick("selected", "已选择")
                ))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.accent));
            let inner = block.inner(rect);
            frame.render_widget(block, rect);
            let [list, hint] =
                Layout::vertical([Constraint::Min(1), Constraint::Length(2)]).areas(inner);
            let rows = providers
                .iter()
                .enumerate()
                .map(|(index, provider)| {
                    ListItem::new(format!(
                        "{} {provider}",
                        if selected.contains(&index) {
                            "[x]"
                        } else {
                            "[ ]"
                        }
                    ))
                })
                .collect::<Vec<_>>();
            let mut state = ListState::default().with_selected(Some(*cursor));
            frame.render_stateful_widget(
                List::new(rows).highlight_symbol(" > ").highlight_style(
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                list,
                &mut state,
            );
            render_key_hints(
                frame,
                hint,
                &[
                    ("Space", language.pick("toggle", "切换")),
                    ("a", language.pick("all", "全选")),
                    ("Enter", language.pick("import", "导入")),
                    ("Esc", language.pick("cancel", "取消")),
                ],
                theme,
            );
        }
    }
}

fn render_form(
    frame: &mut Frame<'_>,
    form: &FormState,
    language: Language,
    area: Rect,
    theme: Theme,
) {
    if form.editing_headers {
        render_provider_headers_form(frame, form, language, area, theme);
        return;
    }
    let rect = modal_rect(area, 88, 22);
    clear_area(frame, rect, theme);
    let block = Block::default()
        .title(if form.previous_id.is_some() {
            language.pick(" Edit provider ", " 编辑提供商 ")
        } else {
            language.pick(" Add provider ", " 新建提供商 ")
        })
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    let rows = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(2),
        Constraint::Length(2),
        Constraint::Length(2),
        Constraint::Length(2),
        Constraint::Length(2),
        Constraint::Length(2),
        Constraint::Length(2),
        Constraint::Min(1),
    ])
    .split(inner);
    let headers_summary =
        headers_summary(form, language, usize::from(inner.width.saturating_sub(26)));
    let values = [
        form.id.clone(),
        form.base_url.clone(),
        format!(
            "< {} >",
            if form.api == 0 {
                language.pick("inherit", "继承")
            } else {
                api_label(form.api)
            }
        ),
        if form.field == 3 {
            form.api_key.clone()
        } else {
            mask_secret(&form.api_key)
        },
        format!(
            "< {} >",
            if form.auth_header {
                language.pick("enabled", "启用")
            } else {
                language.pick("custom only", "仅自定义")
            }
        ),
        headers_summary,
        format!(
            "< {} >",
            if form.send_session_affinity_headers {
                language.pick("enabled", "启用")
            } else {
                language.pick("disabled", "禁用")
            }
        ),
        form.compat_json.clone(),
    ];
    let labels = [
        language.pick("Provider ID", "提供商 ID"),
        language.pick("Base URL", "基础 URL"),
        language.pick("API type", "API 类型"),
        language.pick("API key", "API 密钥"),
        language.pick("Auth header", "认证请求头"),
        language.pick("Headers", "请求头"),
        language.pick("Session affinity", "会话亲和"),
        language.pick("Other compat JSON", "其他兼容 JSON"),
    ];
    for index in 0..8 {
        let active = form.field == index;
        let label = Line::from(vec![
            Span::styled(
                format!(" {}", pad_width(labels[index], 24)),
                Style::default().fg(if active { theme.accent } else { theme.muted }),
            ),
            Span::styled(
                if active && !matches!(index, 2 | 4 | 5 | 6) {
                    with_cursor(&values[index], form.cursor)
                } else {
                    values[index].clone()
                },
                Style::default().add_modifier(if active {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
            ),
        ]);
        let lines = if index == 5
            && form.user_agent.trim().is_empty()
            && form.headers_json.trim().is_empty()
        {
            vec![
                label,
                Line::from(Span::styled(
                    language.pick(
                        "  Enter to configure User-Agent or other headers",
                        "  Enter 编辑 User-Agent 或其他请求头",
                    ),
                    Style::default().fg(theme.muted),
                )),
            ]
        } else {
            vec![label]
        };
        frame.render_widget(
            Paragraph::new(lines).wrap(Wrap { trim: false }),
            rows[index],
        );
    }
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                " Ctrl+S ",
                Style::default()
                    .fg(theme.success)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                language.pick("save  ", "保存  "),
                Style::default().fg(theme.muted),
            ),
            Span::styled(
                " Tab ",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                language.pick("next field  ", "下一字段  "),
                Style::default().fg(theme.muted),
            ),
            Span::styled(
                " Esc ",
                Style::default()
                    .fg(theme.warning)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                language.pick("cancel", "取消"),
                Style::default().fg(theme.muted),
            ),
        ])),
        rows[8],
    );
    if form.show_help {
        render_provider_field_help(frame, form, language, area, theme);
    }
}

fn render_provider_field_help(
    frame: &mut Frame<'_>,
    form: &FormState,
    language: Language,
    area: Rect,
    theme: Theme,
) {
    let rect = modal_rect(area, 76, 18);
    let mut lines = provider_field_help(form, language, theme);
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        language.pick("Esc/Enter/? close", "按 Esc/Enter/? 关闭"),
        Style::default().fg(theme.muted),
    )));
    render_modal(
        frame,
        rect,
        language.pick(" Field help ", " 字段帮助 "),
        Paragraph::new(lines).wrap(Wrap { trim: false }),
        theme.accent,
        theme,
    );
}

fn provider_field_help(form: &FormState, language: Language, theme: Theme) -> Vec<Line<'static>> {
    let title = |name: &'static str| {
        Line::from(Span::styled(
            name,
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ))
    };
    let body = |text: &'static str| Line::from(Span::styled(text, Style::default()));
    let blank = || Line::from("");
    let field = if form.editing_headers { 8 } else { form.field };
    match field {
        0 => vec![
            title(language.pick("Provider ID", "提供商 ID")),
            body(language.pick(
                "Identifier for this provider in models.json. Must be unique.",
                "在 models.json 中此提供商的标识符，必须唯一。",
            )),
        ],
        1 => vec![
            title(language.pick("Base URL", "基础 URL")),
            body(language.pick(
                "Endpoint base, e.g. https://api.openai.com/v1",
                "接入点基础地址,例如 https://api.openai.com/v1",
            )),
        ],
        2 => vec![
            title(language.pick("API type", "API 类型")),
            body(language.pick("Protocol family of the provider.", "提供商的协议族。")),
        ],
        3 => vec![
            title(language.pick("API key", "API 密钥")),
            body(language.pick(
                "Secret sent in the auth header. Supports $ENV / ${ENV} interpolation.",
                "认证头中发送的密钥。支持 $ENV / ${ENV} 插值。",
            )),
        ],
        4 => vec![
            title(language.pick("Auth header", "认证请求头")),
            body(language.pick(
                "enabled  - Pi auto-sends the key as auth header.",
                "启用  - Pi 自动把密钥作为认证头发送。",
            )),
            body(language.pick(
                "custom only - Pi does NOT auto-send; use only your own headers.",
                "仅自定义 - Pi 不自动发送;仅用你手填的 headers。",
            )),
            blank(),
            body(language.pick("By API type:", "按 API 类型:")),
            body(language.pick(
                "  anthropic-messages -> x-api-key",
                "  anthropic-messages -> x-api-key",
            )),
            body(language.pick(
                "  google-generative-ai -> ?key=<key> query param",
                "  google-generative-ai -> ?key=<key> 查询参数",
            )),
            body(language.pick(
                "  others -> Authorization: Bearer <key>",
                "  其他 -> Authorization: Bearer <key>",
            )),
        ],
        5 => vec![
            title(language.pick("Headers", "请求头")),
            body(language.pick(
                "Custom HTTP headers for all requests. Enter to edit User-Agent and other headers.",
                "所有请求的自定义 HTTP 头。按 Enter 编辑 User-Agent 与其他请求头。",
            )),
        ],
        6 => vec![
            title(language.pick("Session affinity", "会话亲和")),
            body(language.pick(
                "Send session affinity headers so the same session routes to the same backend.",
                "发送会话亲和头,使同一会话路由到同一后端。",
            )),
        ],
        7 => vec![
            title(language.pick("Other compat JSON", "其他兼容 JSON")),
            body(language.pick(
                "Raw compat fields preserved as-is. sendSessionAffinityHeaders is managed above.",
                "原样保留的兼容字段。sendSessionAffinityHeaders 由上方字段管理。",
            )),
        ],
        8 => vec![
            title(language.pick("Headers", "请求头")),
            body(language.pick(
                "User-Agent and other HTTP headers sent on every request.",
                "每次请求发送的 User-Agent 与其他 HTTP 头。",
            )),
        ],
        _ => vec![],
    }
}

fn headers_summary(form: &FormState, language: Language, width: usize) -> String {
    let body = match form.header_names() {
        Err(()) => language.pick("invalid JSON", "JSON 无效").to_owned(),
        Ok(names) if names.is_empty() => language
            .pick("none - Enter to edit", "未设置 - Enter 编辑")
            .to_owned(),
        Ok(names) => names.join(", "),
    };
    format!(
        "< {} >",
        truncate_width(&body, width.saturating_sub(4).max(1))
    )
}

fn render_provider_headers_form(
    frame: &mut Frame<'_>,
    form: &FormState,
    language: Language,
    area: Rect,
    theme: Theme,
) {
    let rect = modal_rect(area, 82, 12);
    clear_area(frame, rect, theme);
    let block = Block::default()
        .title(language.pick(" Provider headers ", " 提供商请求头 "))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    let [user_agent, headers_json, hint] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(3),
        Constraint::Length(2),
    ])
    .areas(inner);
    let user_agent_active = form.headers_field == 0;
    let user_agent_value = if user_agent_active {
        with_cursor(&form.user_agent, form.cursor)
    } else {
        form.user_agent.clone()
    };
    let user_agent_block = Block::default()
        .title(format!(" {} ", language.pick("User-Agent", "User-Agent")))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if user_agent_active {
            theme.accent
        } else {
            theme.border
        }));
    let user_agent_inner = user_agent_block.inner(user_agent);
    frame.render_widget(user_agent_block, user_agent);
    frame.render_widget(
        Paragraph::new(user_agent_value).style(Style::default().bg(theme.surface)),
        user_agent_inner,
    );
    let headers_active = form.headers_field == 1;
    let headers_block = Block::default()
        .title(format!(
            " {} ",
            language.pick("Other headers JSON", "其他请求头 JSON")
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if headers_active {
            theme.accent
        } else {
            theme.border
        }));
    let headers_inner = headers_block.inner(headers_json);
    frame.render_widget(headers_block, headers_json);
    frame.render_widget(
        Paragraph::new(if headers_active {
            with_cursor(&form.headers_json, form.cursor)
        } else {
            form.headers_json.clone()
        })
        .style(Style::default().bg(theme.surface))
        .wrap(Wrap { trim: false }),
        headers_inner,
    );
    render_key_hints(
        frame,
        hint,
        &[
            ("Ctrl+S", language.pick("save", "保存")),
            ("Tab", language.pick("next field", "下一字段")),
            ("Esc", language.pick("back", "返回")),
        ],
        theme,
    );
}

fn render_model_form(
    frame: &mut Frame<'_>,
    form: &ModelFormState,
    language: Language,
    area: Rect,
    theme: Theme,
) {
    let rect = modal_rect(area, 84, 22);
    let title = if form.previous_id.is_some() {
        language.pick(" Edit model ", " 编辑模型 ")
    } else {
        language.pick(" Add model ", " 新建模型 ")
    };
    clear_area(frame, rect, theme);
    let block = Block::default()
        .title(format!("{title} {} ", form.provider_id))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    let rows = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(2),
        Constraint::Length(2),
        Constraint::Length(2),
        Constraint::Length(2),
        Constraint::Length(2),
        Constraint::Length(2),
        Constraint::Min(1),
    ])
    .split(inner);
    let values = [
        form.id.clone(),
        form.name.clone(),
        format!(
            "< {} >",
            if form.api == 0 {
                language.pick("inherit", "继承")
            } else {
                api_label(form.api)
            }
        ),
        format!(
            "< {} >",
            if form.reasoning {
                language.pick("enabled", "启用")
            } else {
                language.pick("disabled", "禁用")
            }
        ),
        format!(
            "< {} >",
            if form.image_input {
                language.pick("text + image", "文本 + 图像")
            } else {
                language.pick("text", "文本")
            }
        ),
        form.context_window.clone(),
        form.max_tokens.clone(),
    ];
    let labels = [
        language.pick("Model ID", "模型 ID"),
        language.pick("Display name", "显示名称"),
        language.pick("API override", "API 覆盖"),
        language.pick("Reasoning", "推理"),
        language.pick("Input", "输入"),
        language.pick("Context window", "上下文窗口"),
        language.pick("Max output tokens", "最大输出 Token"),
    ];
    for index in 0..7 {
        let active = form.field == index;
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    format!(" {}", pad_width(labels[index], 22)),
                    Style::default().fg(if active { theme.accent } else { theme.muted }),
                ),
                Span::styled(
                    if active && !matches!(index, 2..=4) {
                        with_cursor(&values[index], form.cursor)
                    } else {
                        values[index].clone()
                    },
                    Style::default().add_modifier(if active {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
                ),
            ])),
            rows[index],
        );
    }
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                " Ctrl+S ",
                Style::default()
                    .fg(theme.success)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                language.pick("save  ", "保存  "),
                Style::default().fg(theme.muted),
            ),
            Span::styled(
                " Esc ",
                Style::default()
                    .fg(theme.warning)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                language.pick("cancel", "取消"),
                Style::default().fg(theme.muted),
            ),
        ])),
        rows[7],
    );
}

fn render_model_defaults_form(
    frame: &mut Frame<'_>,
    form: &ModelDefaultsFormState,
    language: Language,
    area: Rect,
    theme: Theme,
) {
    let values = [
        &form.context_window,
        &form.max_tokens,
        &form.input_cost,
        &form.output_cost,
        &form.cache_read_cost,
        &form.cache_write_cost,
    ];
    let labels = [
        language.pick("Context window", "上下文窗口"),
        language.pick("Max output tokens", "最大输出 Token"),
        language.pick("Input cost / M", "输入成本 / M"),
        language.pick("Output cost / M", "输出成本 / M"),
        language.pick("Cache read cost / M", "缓存读取成本 / M"),
        language.pick("Cache write cost / M", "缓存写入成本 / M"),
    ];
    let defaults = [
        PI_DEFAULT_CONTEXT_WINDOW.to_string(),
        PI_DEFAULT_MAX_TOKENS.to_string(),
        "0".into(),
        "0".into(),
        "0".into(),
        "0".into(),
    ];
    let label_width = labels
        .iter()
        .map(|label| UnicodeWidthStr::width(*label))
        .max()
        .unwrap_or_default();
    let rect = modal_rect(
        area,
        (label_width + MODEL_DEFAULT_VALUE_WIDTH + 9) as u16,
        15,
    );
    let mut lines = Vec::with_capacity(13);
    for index in 0..6 {
        let active = form.field == index;
        lines.push(model_default_field(
            labels[index],
            label_width,
            values[index],
            &defaults[index],
            form.cursor,
            active,
            theme,
        ));
        lines.push(Line::default());
    }
    lines.push(Line::from(vec![
        Span::styled(
            " Ctrl+S ",
            Style::default()
                .fg(theme.success)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            language.pick("save  ", "保存  "),
            Style::default().fg(theme.muted),
        ),
        Span::styled(
            " Esc ",
            Style::default()
                .fg(theme.warning)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            language.pick("cancel", "取消"),
            Style::default().fg(theme.muted),
        ),
    ]));
    render_modal(
        frame,
        rect,
        language.pick(" Default model parameters ", " 默认模型参数 "),
        Paragraph::new(lines),
        theme.accent,
        theme,
    );
}

fn model_default_field(
    label: &str,
    label_width: usize,
    value: &str,
    default: &str,
    cursor: usize,
    active: bool,
    theme: Theme,
) -> Line<'static> {
    let label_padding = label_width.saturating_sub(UnicodeWidthStr::width(label));
    let border = Style::default().fg(if active { theme.accent } else { theme.border });
    let input = Style::default().bg(theme.surface);
    let mut spans = vec![
        Span::styled(
            format!(" {label}{}  ", " ".repeat(label_padding)),
            Style::default().fg(if active { theme.accent } else { theme.muted }),
        ),
        Span::styled("[", border),
        Span::styled(" ", input),
    ];
    if value.is_empty() {
        let cursor = if active { "|" } else { "" };
        let default = truncate_width(
            default,
            MODEL_DEFAULT_VALUE_WIDTH.saturating_sub(cursor.len()),
        );
        let padding = MODEL_DEFAULT_VALUE_WIDTH
            .saturating_sub(cursor.len() + UnicodeWidthStr::width(default.as_str()));
        spans.push(Span::styled(cursor, input.fg(theme.foreground)));
        spans.push(Span::styled(default, input.fg(theme.muted)));
        spans.push(Span::styled(" ".repeat(padding), input));
    } else {
        let value = truncate_width(
            &if active {
                with_cursor(value, cursor)
            } else {
                value.into()
            },
            MODEL_DEFAULT_VALUE_WIDTH,
        );
        let padding =
            MODEL_DEFAULT_VALUE_WIDTH.saturating_sub(UnicodeWidthStr::width(value.as_str()));
        spans.push(Span::styled(
            format!("{value}{}", " ".repeat(padding)),
            input.fg(theme.foreground).add_modifier(if active {
                Modifier::BOLD
            } else {
                Modifier::empty()
            }),
        ));
    }
    spans.push(Span::styled(" ", input));
    spans.push(Span::styled("]", border));
    Line::from(spans)
}

fn render_modal<'a>(
    frame: &mut Frame<'_>,
    rect: Rect,
    title: &'a str,
    body: Paragraph<'a>,
    border: Color,
    theme: Theme,
) {
    clear_area(frame, rect, theme);
    frame.render_widget(
        body.block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border)),
        ),
        rect,
    );
}

fn clear_area(frame: &mut Frame<'_>, area: Rect, theme: Theme) {
    // Clear adjacent cells too: a wide glyph can start outside the overlay and occupy its border.
    let bounds = frame.area();
    let x = area.x.saturating_sub(1).max(bounds.x);
    let right = area.right().saturating_add(1).min(bounds.right());
    let clear = Rect::new(x, area.y, right.saturating_sub(x), area.height);
    frame.render_widget(Clear, clear);
    frame.render_widget(Block::default().style(theme.base()), clear);
}

fn catalog_summary(model: &CatalogModel, language: Language) -> String {
    let context = model.config["contextWindow"]
        .as_u64()
        .expect("validated catalog context window");
    let max_tokens = model.config["maxTokens"]
        .as_u64()
        .expect("validated catalog max tokens");
    let input = model.config["cost"]["input"]
        .as_f64()
        .expect("validated catalog input cost");
    let output = model.config["cost"]["output"]
        .as_f64()
        .expect("validated catalog output cost");
    format!(
        "ctx {}  max {}  $/M {} {input} {} {output}",
        format_token_count(context),
        format_token_count(max_tokens),
        language.pick("in", "输入"),
        language.pick("out", "输出"),
    )
}

fn format_token_count(value: u64) -> String {
    if value >= 1_000_000 {
        "1M".into()
    } else if value >= 100_000 {
        format!("{}k", value / 1_000)
    } else {
        value.to_string()
    }
}

fn modal_rect(area: Rect, desired_width: u16, desired_height: u16) -> Rect {
    let width = desired_width.min(area.width.saturating_sub(2).max(1));
    let height = desired_height.min(area.height.saturating_sub(2).max(1));
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

/// Near-full-screen rect leaving a 1-cell margin on every side. Used by the
/// model-import overlay so the catalog list has room to breathe instead of
/// sitting in a small centered modal.
fn near_full_rect(area: Rect) -> Rect {
    let width = area.width.saturating_sub(2).max(1);
    let height = area.height.saturating_sub(2).max(1);
    Rect::new(area.x + 1, area.y + 1, width, height)
}
