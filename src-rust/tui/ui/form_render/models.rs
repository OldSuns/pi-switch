use super::*;
pub(in crate::tui::ui) fn render_model_form(
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

pub(in crate::tui::ui) fn render_model_defaults_form(
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
