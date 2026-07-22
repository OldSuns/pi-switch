mod pages;

use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};
use unicode_width::UnicodeWidthStr;

use crate::documents::{PI_DEFAULT_CONTEXT_WINDOW, PI_DEFAULT_MAX_TOKENS};

use super::{
    app::{App, Focus, Notice, NoticeKind, Overlay, Page},
    forms::{FormState, ModelDefaultsFormState, ModelFormState},
    i18n::Language,
    input::{api_label, char_len, mask_secret, pad_width, truncate_width, with_cursor, wrap_width},
    keys::{all_shortcuts, shortcut, Command},
    WIDE_WIDTH,
};
use pages::{render_home, render_menu, render_settings};

const MENU_WIDTH: u16 = 18;
const SHELL_WIDE_WIDTH: u16 = MENU_WIDTH + WIDE_WIDTH;
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
    let active = app.focus == Focus::Providers || (app.width < WIDE_WIDTH && !app.narrow_detail);
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
    let [info, models] = Layout::vertical([Constraint::Length(8), Constraint::Min(5)]).areas(area);
    let width = info.width.saturating_sub(14) as usize;
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
                "{} {}: {}",
                headers.len(),
                app.language.pick("configured", "项"),
                truncate_width(&names, width.saturating_sub(16)),
            )
        })
        .unwrap_or_else(|| app.language.pick("none", "未设置").into());
    let lines = vec![
        Line::from(vec![
            Span::styled(pad_width("API", 10), Style::default().fg(theme.muted)),
            Span::raw(if provider.api.is_empty() {
                app.language.pick("inherited", "继承")
            } else {
                &provider.api
            }),
        ]),
        Line::from(vec![
            Span::styled(
                pad_width(app.language.pick("Base URL", "基础 URL"), 10),
                Style::default().fg(theme.muted),
            ),
            Span::raw(if provider.base_url.is_empty() {
                app.language.pick("built-in default", "内置默认值").into()
            } else {
                truncate_width(&provider.base_url, width)
            }),
        ]),
        Line::from(vec![
            Span::styled(
                pad_width(app.language.pick("API key", "API 密钥"), 10),
                Style::default().fg(theme.muted),
            ),
            Span::raw(if provider.api_key.is_empty() {
                app.language
                    .pick("auth.json / CLI", "auth.json / 命令行")
                    .into()
            } else {
                key
            }),
        ]),
        Line::from(vec![
            Span::styled(
                pad_width(app.language.pick("Auth", "认证"), 10),
                Style::default().fg(theme.muted),
            ),
            Span::raw(if provider.auth_header {
                app.language.pick("enabled", "启用")
            } else {
                app.language.pick("custom headers only", "仅自定义请求头")
            }),
        ]),
        Line::from(vec![
            Span::styled(
                pad_width(app.language.pick("Headers", "请求头"), 10),
                Style::default().fg(theme.muted),
            ),
            Span::raw(header_summary),
        ]),
        Line::from(vec![
            Span::styled(
                pad_width(app.language.pick("Compat", "兼容选项"), 10),
                Style::default().fg(theme.muted),
            ),
            Span::raw(if provider.raw.get("compat").is_some() {
                app.language.pick("custom", "自定义")
            } else {
                app.language.pick("defaults", "默认")
            }),
        ]),
    ];
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

    let items = provider
        .models
        .iter()
        .enumerate()
        .map(|(index, model)| {
            let is_default = app.snapshot.default_provider.as_deref() == Some(&provider.id)
                && app.snapshot.default_model.as_deref() == Some(&model.id);
            let context = model
                .context_window
                .map(|value| value.to_string())
                .unwrap_or_else(|| app.language.pick("unset", "未设置").into());
            let max_tokens = model
                .max_tokens
                .map(|value| value.to_string())
                .unwrap_or_else(|| app.language.pick("unset", "未设置").into());
            let details = format!(
                "    {}  {}{}  ctx {}  max {}",
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
            ListItem::new(Text::from(vec![
                Line::from(vec![
                    Span::styled(
                        format!("{:>2} ", index + 1),
                        Style::default().fg(theme.muted),
                    ),
                    Span::styled(
                        if is_default { "* " } else { "  " },
                        Style::default().fg(theme.success),
                    ),
                    Span::raw(&model.id),
                    if is_default {
                        Span::styled(
                            app.language.pick("  default", "  默认"),
                            Style::default().fg(theme.success),
                        )
                    } else {
                        Span::raw("")
                    },
                ]),
                Line::from(Span::styled(details, Style::default().fg(theme.muted))),
            ]))
        })
        .collect::<Vec<_>>();
    let active = app.focus == Focus::Models || (app.width < WIDE_WIDTH && app.narrow_detail);
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
            keys.push(if app.in_model_context() {
                ("Left", app.language.pick("providers", "提供商"))
            } else {
                ("Enter", app.language.pick("models", "模型"))
            });
        }
        if app.page != Page::Home {
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
            let lines = checks
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
            ..
        } => {
            let rect = modal_rect(area, 76, 22);
            clear_area(frame, rect, theme);
            let block = Block::default()
                .title(format!(
                    " {}  {}/{} {}{} ",
                    language.pick("Model catalog", "模型目录"),
                    selected.len(),
                    models.len(),
                    language.pick("selected", "已选择"),
                    if *unavailable > 0 {
                        format!(
                            "  {} {}",
                            unavailable,
                            language.pick("unavailable", "无元数据")
                        )
                    } else {
                        String::new()
                    }
                ))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.accent));
            let inner = block.inner(rect);
            frame.render_widget(block, rect);
            let rows = models
                .iter()
                .enumerate()
                .map(|(index, model)| {
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
                    ListItem::new(Text::from(vec![
                        Line::from(format!(
                            "{} {}",
                            if selected.contains(&index) {
                                "[x]"
                            } else {
                                "[ ]"
                            },
                            model.id
                        )),
                        Line::from(Span::styled(
                            format!(
                                "    ctx {context}  max {max_tokens}  $/M {} {input} {} {output}",
                                language.pick("in", "输入"),
                                language.pick("out", "输出"),
                            ),
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
                inner,
                &mut state,
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
                inner,
                &mut state,
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
        Constraint::Min(1),
    ])
    .split(inner);
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
        "*".repeat(char_len(&form.api_key)),
        format!(
            "< {} >",
            if form.auth_header {
                language.pick("enabled", "启用")
            } else {
                language.pick("custom only", "仅自定义")
            }
        ),
        if form.headers_json.trim().is_empty() {
            language.pick("< none - Enter to edit >", "< 未设置 - Enter 编辑 >")
        } else {
            language.pick("< configured - Enter to edit >", "< 已配置 - Enter 编辑 >")
        }
        .into(),
        form.compat_json.clone(),
    ];
    let labels = [
        language.pick("Provider ID", "提供商 ID"),
        language.pick("Base URL", "基础 URL"),
        language.pick("API type", "API 类型"),
        language.pick("API key / reference", "API 密钥 / 引用"),
        language.pick("Auth header", "认证请求头"),
        language.pick("Headers (all models)", "请求头（全部模型）"),
        language.pick("Compat JSON", "兼容选项 JSON"),
    ];
    for index in 0..7 {
        let active = form.field == index;
        let label = Line::from(vec![
            Span::styled(
                format!(" {}", pad_width(labels[index], 24)),
                Style::default().fg(if active { theme.accent } else { theme.muted }),
            ),
            Span::styled(
                if active && !matches!(index, 2 | 4) {
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
        let lines = if index == 5 && form.headers_json.is_empty() {
            vec![
                label,
                Line::from(Span::styled(
                    language.pick(
                        r#"  Example: {"User-Agent":"claude-cli/2.1.161"}"#,
                        r#"  示例：{"User-Agent":"claude-cli/2.1.161"}"#,
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
        rows[7],
    );
}

fn render_provider_headers_form(
    frame: &mut Frame<'_>,
    form: &FormState,
    language: Language,
    area: Rect,
    theme: Theme,
) {
    let rect = modal_rect(area, 82, 14);
    clear_area(frame, rect, theme);
    let block = Block::default()
        .title(language.pick(
            " Provider headers - all models ",
            " 提供商请求头 - 全部模型 ",
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    let [intro, editor, hint] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(4),
        Constraint::Length(2),
    ])
    .areas(inner);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(language.pick(
                "JSON object applied to every model in this provider.",
                "JSON 对象会应用到此提供商的全部模型。",
            )),
            Line::from(Span::styled(
                language.pick(
                    r#"Example: {"User-Agent":"claude-cli/2.1.161"}"#,
                    r#"示例：{"User-Agent":"claude-cli/2.1.161"}"#,
                ),
                Style::default().fg(theme.muted),
            )),
            Line::from(Span::styled(
                language.pick(
                    "Values may be literals, $ENV, ${ENV}, or !command.",
                    "值支持字面量、$ENV、${ENV} 或 !command。",
                ),
                Style::default().fg(theme.muted),
            )),
        ]),
        intro,
    );
    frame.render_widget(
        Paragraph::new(with_cursor(&form.headers_json, form.cursor))
            .style(Style::default().bg(theme.surface))
            .wrap(Wrap { trim: false }),
        editor,
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" Enter/Esc/Tab ", Style::default().fg(theme.accent)),
            Span::styled(
                language.pick("done  ", "完成  "),
                Style::default().fg(theme.muted),
            ),
            Span::styled(" Ctrl+S ", Style::default().fg(theme.success)),
            Span::styled(
                language.pick("save provider", "保存提供商"),
                Style::default().fg(theme.muted),
            ),
        ])),
        hint,
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
