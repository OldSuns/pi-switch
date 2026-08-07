use super::*;

pub(super) fn render_profiles_page(frame: &mut Frame<'_>, app: &App, area: Rect, theme: Theme) {
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

fn render_providers(frame: &mut Frame<'_>, app: &App, area: Rect, theme: Theme) {
    let visible = app.visible_providers();
    let content_width = area.width.saturating_sub(6) as usize;
    let enabled = visible
        .iter()
        .filter(|index| app.snapshot.providers[**index].in_pi)
        .count();
    let title = if app.filtering || !app.filter.is_empty() {
        format!(
            " {}  /{}{} ",
            app.language.pick("Providers", "提供商"),
            app.filter,
            if app.filtering { "_" } else { "" }
        )
    } else {
        format!(
            " {}  {}/{} ",
            app.language.pick("Providers", "提供商"),
            enabled,
            visible.len()
        )
    };
    let items = visible
        .iter()
        .map(|index| {
            let provider = &app.snapshot.providers[*index];
            let default = app.snapshot.default_provider.as_deref() == Some(&provider.id);
            let marker = if default { "*" } else { " " };
            let prefix = if provider.in_pi { "[x]" } else { "[ ]" };
            let row_style = if provider.in_pi {
                Style::default().bg(theme.surface)
            } else {
                Style::default()
            };
            let mut lines = wrap_width(&provider.id, content_width.saturating_sub(5))
                .into_iter()
                .enumerate()
                .map(|(line, id)| {
                    Line::from(vec![
                        Span::styled(
                            if line == 0 {
                                format!("{prefix}{marker} ")
                            } else {
                                "     ".into()
                            },
                            Style::default().fg(theme.success),
                        ),
                        Span::styled(id, Style::default().add_modifier(Modifier::BOLD)),
                    ])
                    .style(row_style)
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
            ListItem::new(Text::from(lines)).style(row_style)
        })
        .collect::<Vec<_>>();
    let active = app.focus == Focus::Providers || (app.width < COMPACT_WIDTH && !app.narrow_detail);
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
        .borders(Borders::RIGHT)
        .border_style(theme.panel(active))
        .style(theme.base());
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
            .style(theme.dim_text())
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
                    .title(Span::styled(
                        format!(" {} ", provider.id),
                        Style::default()
                            .fg(theme.accent)
                            .add_modifier(Modifier::BOLD),
                    ))
                    .borders(Borders::BOTTOM)
                    .border_style(Style::default().fg(theme.border))
                    .style(theme.base()),
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
    let position = if provider.models.is_empty() {
        String::new()
    } else {
        format!("  {}/{}", app.model_cursor + 1, provider.models.len())
    };
    let block = Block::default()
        .title(Span::styled(
            format!(
                " {}  {}{} ",
                app.language.pick("Models", "模型"),
                provider.models.len(),
                position
            ),
            if active {
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                theme.label()
            },
        ))
        .borders(if active { Borders::ALL } else { Borders::TOP })
        .border_style(theme.panel(active))
        .style(theme.base());
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
                    theme.dim_text(),
                ),
                Span::styled(value, theme.value()),
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
