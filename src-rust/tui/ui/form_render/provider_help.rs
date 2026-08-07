use super::*;
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
