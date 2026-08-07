use super::*;
pub(in crate::tui::ui) fn render_form(
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
