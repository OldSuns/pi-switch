use super::*;

pub(super) fn render_form(
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
    let rect = modal_rect(area, 88, 24);
    clear_area(frame, rect, theme);
    let block = Block::default()
        .title(Span::styled(
            if form.previous_id.is_some() {
                language.pick(" Edit provider ", " 编辑提供商 ")
            } else {
                language.pick(" Add provider ", " 新建提供商 ")
            },
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent))
        .style(theme.surface_style());
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
        format!(
            "< {} >",
            if form.in_pi {
                language.pick("added", "已加入")
            } else {
                language.pick("local only", "仅本地")
            }
        ),
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
        language.pick("Add to Pi", "加入 Pi"),
    ];
    for index in 0..9 {
        let active = form.field == index;
        let label = Line::from(vec![
            Span::styled(
                format!(" {}", pad_width(labels[index], 24)),
                if active {
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD)
                } else {
                    theme.label()
                },
            ),
            Span::styled(
                if active && !matches!(index, 2 | 4 | 5 | 6 | 8) {
                    with_cursor(&values[index], form.cursor)
                } else {
                    values[index].clone()
                },
                if active {
                    theme.value().add_modifier(Modifier::BOLD)
                } else {
                    theme.value()
                },
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
                    theme.dim_text(),
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
                    .bg(theme.surface)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(language.pick("save  ", "保存  "), theme.label()),
            Span::styled(" Tab ", theme.keycap()),
            Span::styled(language.pick("next field  ", "下一字段  "), theme.label()),
            Span::styled(
                " Esc ",
                Style::default()
                    .fg(theme.warning)
                    .bg(theme.surface)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(language.pick("cancel", "取消"), theme.label()),
        ])),
        rows[9],
    );
    if form.show_help {
        render_provider_field_help(frame, form, language, area, theme);
    }
}

pub(super) fn render_provider_field_help(
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

pub(super) fn provider_field_help(
    form: &FormState,
    language: Language,
    theme: Theme,
) -> Vec<Line<'static>> {
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
    let field = if form.editing_headers { 9 } else { form.field };
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
            title(language.pick("Add to Pi", "加入 Pi")),
            body(language.pick(
                "Added providers are written to Pi models.json; local-only providers stay in the pi-switch library.",
                "已加入的提供商会写入 Pi models.json；仅本地项只保存在 pi-switch 库中。",
            )),
        ],
        9 => vec![
            title(language.pick("Headers", "请求头")),
            body(language.pick(
                "User-Agent and other HTTP headers sent on every request.",
                "每次请求发送的 User-Agent 与其他 HTTP 头。",
            )),
        ],
        _ => vec![],
    }
}

pub(super) fn headers_summary(form: &FormState, language: Language, width: usize) -> String {
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

pub(super) fn render_provider_headers_form(
    frame: &mut Frame<'_>,
    form: &FormState,
    language: Language,
    area: Rect,
    theme: Theme,
) {
    let rect = modal_rect(area, 82, 12);
    clear_area(frame, rect, theme);
    let block = Block::default()
        .title(Span::styled(
            language.pick(" Provider headers ", " 提供商请求头 "),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent))
        .style(theme.surface_style());
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
        .title(Span::styled(
            format!(" {} ", language.pick("User-Agent", "User-Agent")),
            if user_agent_active {
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                theme.label()
            },
        ))
        .borders(Borders::ALL)
        .border_style(theme.panel(user_agent_active));
    let user_agent_inner = user_agent_block.inner(user_agent);
    frame.render_widget(user_agent_block, user_agent);
    frame.render_widget(
        Paragraph::new(user_agent_value).style(Style::default().bg(theme.surface_hi)),
        user_agent_inner,
    );
    let headers_active = form.headers_field == 1;
    let headers_block = Block::default()
        .title(Span::styled(
            format!(
                " {} ",
                language.pick("Other headers JSON", "其他请求头 JSON")
            ),
            if headers_active {
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                theme.label()
            },
        ))
        .borders(Borders::ALL)
        .border_style(theme.panel(headers_active));
    let headers_inner = headers_block.inner(headers_json);
    frame.render_widget(headers_block, headers_json);
    frame.render_widget(
        Paragraph::new(if headers_active {
            with_cursor(&form.headers_json, form.cursor)
        } else {
            form.headers_json.clone()
        })
        .style(Style::default().bg(theme.surface_hi))
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

pub(super) fn render_model_form(
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
        .title(Span::styled(
            format!("{title} {} ", form.provider_id),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent))
        .style(theme.surface_style());
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
                    if active {
                        Style::default()
                            .fg(theme.accent)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        theme.label()
                    },
                ),
                Span::styled(
                    if active && !matches!(index, 2..=4) {
                        with_cursor(&values[index], form.cursor)
                    } else {
                        values[index].clone()
                    },
                    if active {
                        theme.value().add_modifier(Modifier::BOLD)
                    } else {
                        theme.value()
                    },
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
                    .bg(theme.surface)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(language.pick("save  ", "保存  "), theme.label()),
            Span::styled(
                " Esc ",
                Style::default()
                    .fg(theme.warning)
                    .bg(theme.surface)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(language.pick("cancel", "取消"), theme.label()),
        ])),
        rows[7],
    );
}

pub(super) fn render_model_defaults_form(
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
                .bg(theme.surface)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(language.pick("save  ", "保存  "), theme.label()),
        Span::styled(
            " Esc ",
            Style::default()
                .fg(theme.warning)
                .bg(theme.surface)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(language.pick("cancel", "取消"), theme.label()),
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

pub(super) fn model_default_field(
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
    let input = Style::default().bg(theme.surface_hi);
    let mut spans = vec![
        Span::styled(
            format!(" {label}{}  ", " ".repeat(label_padding)),
            if active {
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                theme.label()
            },
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
