use super::*;
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
