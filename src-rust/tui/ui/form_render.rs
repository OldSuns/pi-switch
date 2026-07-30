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
    if form.editing_compat {
        render_provider_compat_form(frame, form, language, area, theme);
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
        Constraint::Min(1),
    ])
    .split(inner);
    let headers_summary =
        headers_summary(form, language, usize::from(inner.width.saturating_sub(26)));
    let compat_summary =
        compat_summary(form, language, usize::from(inner.width.saturating_sub(26)));
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
        compat_summary,
        format!(
            "< {} >",
            if form.in_pi {
                language.pick("synced to Pi", "同步到 Pi")
            } else {
                language.pick("not synced", "不同步")
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
        language.pick("Compat", "兼容"),
        language.pick("Add to Pi", "加入 Pi"),
    ];
    for index in 0..8 {
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
                if active && !matches!(index, 2 | 4 | 5 | 6 | 7) {
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
                        "  Space to configure User-Agent or other headers",
                        "  空格编辑 User-Agent 或其他请求头",
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
                " Enter ",
                Style::default()
                    .fg(theme.success)
                    .bg(theme.surface)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(language.pick("save  ", "保存  "), theme.label()),
            Span::styled(" Space ", theme.keycap()),
            Span::styled(
                language.pick("open sub-menu  ", "开二级菜单  "),
                theme.label(),
            ),
            Span::styled(" Tab ", theme.keycap()),
            Span::styled(language.pick("next field  ", "下一字段  "), theme.label()),
            Span::styled(
                " Esc ",
                Style::default()
                    .fg(theme.warning)
                    .bg(theme.surface)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(language.pick("cancel  ", "取消  "), theme.label()),
            Span::styled(" Ctrl+U ", theme.keycap()),
            Span::styled(language.pick("clear", "清空"), theme.label()),
        ])),
        rows[8],
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
    let field = if form.editing_headers {
        8
    } else if form.editing_compat {
        100 + form.compat_field
    } else {
        form.field
    };
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
                "Custom HTTP headers for all requests. Space to edit User-Agent and other headers.",
                "所有请求的自定义 HTTP 头。按空格编辑 User-Agent 与其他请求头。",
            )),
        ],
        6 => vec![
            title(language.pick("Compat", "兼容")),
            body(language.pick(
                "Provider compatibility overrides for Pi. Space to open the compat sub-menu: presets, reasoning, thinking format, cache retention, session affinity, and raw JSON.",
                "提供商对 Pi 的兼容性覆写。按空格打开兼容二级菜单:预设、推理、思维格式、缓存保留、会话亲和与原始 JSON。",
            )),
        ],
        7 => vec![
            title(language.pick("Add to Pi", "加入 Pi")),
            body(language.pick(
                "Synced providers are written to Pi models.json; not-synced providers stay in the pi-switch library.",
                "同步到 Pi 的提供商会写入 Pi models.json；不同步的提供商只保存在 pi-switch 库中。",
            )),
        ],
        8 => vec![
            title(language.pick("Headers", "请求头")),
            body(language.pick(
                "User-Agent and other HTTP headers sent on every request.",
                "每次请求发送的 User-Agent 与其他 HTTP 头。",
            )),
        ],
        100 => vec![
            title(language.pick("Preset", "预设")),
            body(language.pick(
                "Apply a bundled set of compat fields matching an official Pi provider preset. Selecting one overwrites the fields it covers; inherit/none applies nothing.",
                "一键应用官方 Pi 提供商预设的一组 compat 字段。选中会覆写其涉及的字段；“默认”表示不选预设、不做任何改动。",
            )),
        ],
        101 => vec![
            title(language.pick("requiresReasoningContentOnAssistantMessages", "requiresReasoningContentOnAssistantMessages")),
            body(language.pick(
                "Replayed assistant turns must include empty reasoning_content when reasoning is on. Needed for DeepSeek-like providers.",
                "开启推理时，回放的 assistant 轮次必须包含空 reasoning_content。DeepSeek 等类似模型需要开启。",
            )),
        ],
        102 => vec![
            title(language.pick("thinkingFormat", "thinkingFormat")),
            body(language.pick(
                "Reasoning/thinking parameter format: openai (reasoning_effort), openrouter, deepseek (thinking:{type}), together, zai, qwen. inherit = do not write.",
                "推理/思维参数的格式:openai(reasoning_effort)、openrouter、deepseek(thinking:{type})、together、zai、qwen。“默认”表示不写入、交由 Pi 自动判断。",
            )),
        ],
        103 => vec![
            title(language.pick("supportsLongCacheRetention", "supportsLongCacheRetention")),
            body(language.pick(
                "Send prompt_cache_retention for long prompt cache retention. inherit = let Pi auto-detect.",
                "发送 prompt_cache_retention 以请求长缓存保留。“默认”表示不写入、交由 Pi 自动检测。",
            )),
        ],
        104 => vec![
            title(language.pick("supportsStore", "supportsStore")),
            body(language.pick(
                "Whether the provider supports the `store` field. inherit = auto-detect.",
                "提供商是否支持 `store` 字段。“默认”表示不写入、交由 Pi 自动检测。",
            )),
        ],
        105 => vec![
            title(language.pick("supportsDeveloperRole", "supportsDeveloperRole")),
            body(language.pick(
                "Use the `developer` role instead of `system`. inherit = auto-detect.",
                "使用 `developer` 角色而非 `system`。“默认”表示不写入、交由 Pi 自动检测。",
            )),
        ],
        106 => vec![
            title(language.pick("supportsReasoningEffort", "supportsReasoningEffort")),
            body(language.pick(
                "Whether the provider supports reasoning_effort. inherit = auto-detect.",
                "提供商是否支持 reasoning_effort。“默认”表示不写入、交由 Pi 自动检测。",
            )),
        ],
        107 => vec![
            title(language.pick("maxTokensField", "maxTokensField")),
            body(language.pick(
                "Which field to use for max tokens: max_completion_tokens or max_tokens. inherit = auto-detect.",
                "用哪个字段传 max tokens:max_completion_tokens 或 max_tokens。“默认”表示不写入、交由 Pi 自动检测。",
            )),
        ],
        108 => vec![
            title(language.pick("supportsStrictMode", "supportsStrictMode")),
            body(language.pick(
                "Whether tool definitions support the `strict` field. inherit = auto-detect.",
                "工具定义是否支持 `strict` 字段。“默认”表示不写入、交由 Pi 自动检测。",
            )),
        ],
        109 => vec![
            title(language.pick("Session affinity", "会话亲和")),
            body(language.pick(
                "Send session affinity headers so the same session routes to the same backend.",
                "发送会话亲和头，使同一会话路由到同一后端。",
            )),
        ],
        110 => vec![
            title(language.pick("Other compat JSON", "其他兼容 JSON")),
            body(language.pick(
                "Raw compat fields preserved as-is, for keys not covered above. Structured keys are rejected here.",
                "原样保留的兼容字段，用于上方未覆盖的键。上方已有的结构化键不可写在此处。",
            )),
        ],
        _ => vec![],
    }
}

pub(super) fn headers_summary(form: &FormState, language: Language, width: usize) -> String {
    let body = match form.header_names() {
        Err(()) => language.pick("invalid JSON", "JSON 无效").to_owned(),
        Ok(names) if names.is_empty() => language
            .pick("none - Space to edit", "未设置 - 空格编辑")
            .to_owned(),
        Ok(names) => names.join(", "),
    };
    format!(
        "< {} >",
        truncate_width(&body, width.saturating_sub(4).max(1))
    )
}

pub(super) fn compat_summary(form: &FormState, language: Language, width: usize) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(preset) = super::PRESETS.get(form.preset).copied() {
        if !preset.is_empty() {
            parts.push(preset.into());
        }
    }
    if let Some(value) = form.requires_reasoning_content {
        parts.push(format!("reasoning:{}", if value { "on" } else { "off" }));
    }
    if let Some(format) = super::THINKING_FORMATS.get(form.thinking_format).copied() {
        if !format.is_empty() {
            parts.push(format.into());
        }
    }
    if let Some(value) = form.supports_long_cache_retention {
        parts.push(format!("longCache:{}", if value { "on" } else { "off" }));
    }
    if let Some(value) = form.supports_store {
        parts.push(format!("store:{}", if value { "on" } else { "off" }));
    }
    if let Some(value) = form.supports_developer_role {
        parts.push(format!("devRole:{}", if value { "on" } else { "off" }));
    }
    if let Some(value) = form.supports_reasoning_effort {
        parts.push(format!("effort:{}", if value { "on" } else { "off" }));
    }
    if let Some(field) = super::MAX_TOKENS_FIELDS.get(form.max_tokens_field).copied() {
        if !field.is_empty() {
            parts.push(field.into());
        }
    }
    if let Some(value) = form.supports_strict_mode {
        parts.push(format!("strict:{}", if value { "on" } else { "off" }));
    }
    if !form.send_session_affinity_headers {
        parts.push(language.pick("affinity:off", "亲和:关").into());
    }
    if !form.other_compat_json.trim().is_empty() {
        parts.push(language.pick("+raw", "+原始").into());
    }
    let body = if parts.is_empty() {
        language.pick("inherit", "默认").to_owned()
    } else {
        parts.join(" · ")
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
            ("Tab", language.pick("next field", "下一字段")),
            ("Esc", language.pick("back", "返回")),
            ("Ctrl+U", language.pick("clear", "清空")),
        ],
        theme,
    );
}

pub(super) fn render_provider_compat_form(
    frame: &mut Frame<'_>,
    form: &FormState,
    language: Language,
    area: Rect,
    theme: Theme,
) {
    use super::{MAX_TOKENS_FIELDS, PRESETS, THINKING_FORMATS};
    let rect = modal_rect(area, 86, 26);
    clear_area(frame, rect, theme);
    let block = Block::default()
        .title(Span::styled(
            language.pick(" Provider compat ", " 提供商兼容 "),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent))
        .style(theme.surface_style());
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    let mut constraints: Vec<Constraint> = (0..11).map(|_| Constraint::Length(2)).collect();
    constraints.push(Constraint::Length(2));
    let rows = Layout::vertical(constraints).split(inner);
    let tristate = |value: Option<bool>| -> &'static str {
        match value {
            None => language.pick("inherit", "默认"),
            Some(true) => language.pick("enabled", "启用"),
            Some(false) => language.pick("disabled", "禁用"),
        }
    };
    let cycle = |index: usize, values: &[&str]| -> String {
        let value = values.get(index).copied().unwrap_or("");
        let label = if value.is_empty() {
            language.pick("inherit", "默认")
        } else {
            value
        };
        format!("< {label} >")
    };
    let values = [
        cycle(form.preset, &PRESETS),
        format!("< {} >", tristate(form.requires_reasoning_content)),
        cycle(form.thinking_format, &THINKING_FORMATS),
        format!("< {} >", tristate(form.supports_long_cache_retention)),
        format!("< {} >", tristate(form.supports_store)),
        format!("< {} >", tristate(form.supports_developer_role)),
        format!("< {} >", tristate(form.supports_reasoning_effort)),
        cycle(form.max_tokens_field, &MAX_TOKENS_FIELDS),
        format!("< {} >", tristate(form.supports_strict_mode)),
        format!(
            "< {} >",
            if form.send_session_affinity_headers {
                language.pick("enabled", "启用")
            } else {
                language.pick("disabled", "禁用")
            }
        ),
        form.other_compat_json.clone(),
    ];
    let labels = [
        language.pick("Preset", "预设"),
        "requiresReasoningContent",
        "thinkingFormat",
        "supportsLongCacheRetention",
        "supportsStore",
        "supportsDeveloperRole",
        "supportsReasoningEffort",
        "maxTokensField",
        "supportsStrictMode",
        language.pick("Session affinity", "会话亲和"),
        language.pick("Other compat JSON", "其他兼容 JSON"),
    ];
    let descriptions = [
        language.pick(
            "Bundle of official provider compat fields",
            "官方提供商兼容字段组合",
        ),
        language.pick(
            "Replayed turns need empty reasoning_content",
            "回放轮次需空 reasoning_content",
        ),
        language.pick("Reasoning parameter format", "推理参数格式"),
        language.pick("Request long prompt cache retention", "请求长缓存保留"),
        language.pick("Supports the store field", "是否支持 store 字段"),
        language.pick(
            "developer role instead of system",
            "developer 角色替代 system",
        ),
        language.pick("Supports reasoning_effort", "是否支持 reasoning_effort"),
        language.pick("Field name for max tokens", "max tokens 字段名"),
        language.pick("Tool definitions support strict", "工具定义支持 strict"),
        language.pick("Route same session to same backend", "同会话路由到同后端"),
        language.pick("Raw fields not covered above", "上方未覆盖的原始字段"),
    ];
    let label_width = 30;
    for index in 0..11 {
        let active = form.compat_field == index;
        let is_text = index == 10;
        let value = if active && is_text {
            with_cursor(&values[index], form.cursor)
        } else {
            values[index].clone()
        };
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(vec![
                    Span::styled(
                        format!(" {}", pad_width(labels[index], label_width)),
                        if active {
                            Style::default()
                                .fg(theme.accent)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            theme.label()
                        },
                    ),
                    Span::styled(
                        value,
                        if active {
                            theme.value().add_modifier(Modifier::BOLD)
                        } else {
                            theme.value()
                        },
                    ),
                ]),
                Line::from(Span::styled(
                    format!("   {}", descriptions[index]),
                    theme.dim_text(),
                )),
            ])
            .wrap(Wrap { trim: false }),
            rows[index],
        );
    }
    render_key_hints(
        frame,
        rows[11],
        &[
            ("Tab", language.pick("next field", "下一字段")),
            ("Esc", language.pick("back", "返回")),
            ("Ctrl+U", language.pick("clear", "清空")),
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
    let visible = form.visible_fields();
    let mut constraints: Vec<Constraint> = visible.iter().map(|_| Constraint::Length(2)).collect();
    constraints.push(Constraint::Min(1));
    let height = (visible.len() as u16).saturating_mul(2).saturating_add(4);
    let rect = modal_rect(area, 84, height);
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
    let rows = Layout::vertical(constraints).split(inner);
    for (index, &field_id) in visible.iter().enumerate() {
        let active = form.field == index;
        frame.render_widget(
            Paragraph::new(model_form_row(form, field_id, active, language, theme)),
            rows[index],
        );
    }
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                " Enter ",
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
            Span::styled(language.pick("cancel  ", "取消  "), theme.label()),
            Span::styled(" Ctrl+U ", theme.keycap()),
            Span::styled(language.pick("clear", "清空"), theme.label()),
        ])),
        rows[visible.len()],
    );
}

#[allow(clippy::too_many_lines)]
fn model_form_row(
    form: &ModelFormState,
    field_id: usize,
    active: bool,
    language: Language,
    theme: Theme,
) -> Line<'static> {
    let label_style = if active {
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD)
    } else {
        theme.label()
    };
    let value_style = if active {
        theme.value().add_modifier(Modifier::BOLD)
    } else {
        theme.value()
    };
    // ponytail: collapse headers pad the label to a fixed column so the summary
    // never runs into the label, and aligns with the field value column.
    match field_id {
        5 => {
            let marker = if form.limits_expanded { "▾" } else { "▸" };
            let summary = if form.context_window.is_empty() && form.max_tokens.is_empty() {
                language.pick("not set", "未设置").to_string()
            } else {
                format!("ctx: {}  max: {}", form.context_window, form.max_tokens)
            };
            Line::from(vec![
                Span::styled(
                    format!(
                        " {marker} {}",
                        pad_width(language.pick("Context & limits", "上下文与限制"), 20)
                    ),
                    label_style,
                ),
                Span::styled(summary, value_style),
            ])
        }
        8 => {
            let marker = if form.pricing_expanded { "▾" } else { "▸" };
            let empty = form.input_cost.is_empty()
                && form.output_cost.is_empty()
                && form.cache_read_cost.is_empty()
                && form.cache_write_cost.is_empty();
            let summary = if empty {
                language.pick("not set", "未设置").to_string()
            } else {
                format!(
                    "in: {}  out: {}  cache read: {}  cache write: {}",
                    form.input_cost, form.output_cost, form.cache_read_cost, form.cache_write_cost
                )
            };
            Line::from(vec![
                Span::styled(
                    format!(
                        " {marker} {}",
                        pad_width(language.pick("Pricing", "定价"), 20)
                    ),
                    label_style,
                ),
                Span::styled(summary, value_style),
            ])
        }
        _ => {
            let (label, value, is_text) = match field_id {
                0 => (
                    language.pick("Model ID", "模型 ID").to_string(),
                    form.id.clone(),
                    true,
                ),
                1 => (
                    language.pick("Display name", "显示名称").to_string(),
                    form.name.clone(),
                    true,
                ),
                2 => (
                    language.pick("API override", "API 覆盖").to_string(),
                    format!(
                        "< {} >",
                        if form.api == 0 {
                            language.pick("inherit", "继承")
                        } else {
                            api_label(form.api)
                        }
                    ),
                    false,
                ),
                3 => (
                    language.pick("Reasoning", "推理").to_string(),
                    format!(
                        "< {} >",
                        if form.reasoning {
                            language.pick("enabled", "启用")
                        } else {
                            language.pick("disabled", "禁用")
                        }
                    ),
                    false,
                ),
                4 => (
                    language.pick("Input", "输入").to_string(),
                    format!(
                        "< {} >",
                        if form.image_input {
                            language.pick("text + image", "文本 + 图像")
                        } else {
                            language.pick("text", "文本")
                        }
                    ),
                    false,
                ),
                6 => (
                    language.pick("Context window", "上下文窗口").to_string(),
                    form.context_window.clone(),
                    true,
                ),
                7 => (
                    language
                        .pick("Max output tokens", "最大输出 Token")
                        .to_string(),
                    form.max_tokens.clone(),
                    true,
                ),
                9 => (
                    language.pick("Input cost / M", "输入成本 / M").to_string(),
                    form.input_cost.clone(),
                    true,
                ),
                10 => (
                    language.pick("Output cost / M", "输出成本 / M").to_string(),
                    form.output_cost.clone(),
                    true,
                ),
                11 => (
                    language
                        .pick("Cache read cost / M", "缓存读取成本 / M")
                        .to_string(),
                    form.cache_read_cost.clone(),
                    true,
                ),
                12 => (
                    language
                        .pick("Cache write cost / M", "缓存写入成本 / M")
                        .to_string(),
                    form.cache_write_cost.clone(),
                    true,
                ),
                _ => (String::new(), String::new(), false),
            };
            let value_rendered = if active && is_text {
                with_cursor(&value, form.cursor)
            } else {
                value
            };
            Line::from(vec![
                Span::styled(format!(" {}", pad_width(&label, 22)), label_style),
                Span::styled(value_rendered, value_style),
            ])
        }
    }
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
            " Enter ",
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
        Span::styled(language.pick("cancel  ", "取消  "), theme.label()),
        Span::styled(" Ctrl+U ", theme.keycap()),
        Span::styled(language.pick("clear", "清空"), theme.label()),
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
