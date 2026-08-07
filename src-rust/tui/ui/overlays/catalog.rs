use super::*;

pub(super) fn render_catalog_overlay(
    frame: &mut Frame<'_>,
    overlay: &Overlay,
    language: Language,
    area: Rect,
    theme: Theme,
) {
    match overlay {
        Overlay::Fetched {
            models,
            selected,
            cursor,
            unavailable,
            ratio_config_used,
            overwrite,
            existing,
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
            let overwrite_label = if *overwrite {
                language.pick("overwrite: on", "覆盖: 开")
            } else {
                language.pick("overwrite: off", "覆盖: 关")
            };
            let block = Block::default()
                .title(Span::styled(
                    format!(
                        " {}  {}/{} {}  {}  {}{}{} ",
                        language.pick("Model catalog", "模型目录"),
                        selected.len(),
                        models.len(),
                        language.pick("selected", "已选择"),
                        price_source,
                        overwrite_label,
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
                    ),
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD),
                ))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.accent))
                .style(theme.surface_style());
            let inner = block.inner(rect);
            frame.render_widget(block, rect);
            let [list, hint] =
                Layout::vertical([Constraint::Min(1), Constraint::Length(2)]).areas(inner);
            let visible = visible_fetched_indices(models, filter);
            let rows = visible
                .iter()
                .map(|&original| {
                    let model = &models[original];
                    let mut first_line = vec![
                        checkbox_span(selected.contains(&original), theme),
                        Span::raw(model.id.clone()),
                    ];
                    if existing.contains(&model.id) {
                        first_line.push(Span::styled(
                            format!(" {}", language.pick("exists", "已存在")),
                            theme.label(),
                        ));
                    }
                    ListItem::new(Text::from(Line::from(first_line)))
                })
                .collect::<Vec<_>>();
            let mut state =
                ListState::default().with_selected(if *filtering { None } else { Some(*cursor) });
            frame.render_stateful_widget(
                List::new(rows)
                    .highlight_symbol(" > ")
                    .highlight_style(theme.selected()),
                list,
                &mut state,
            );
            if visible.is_empty() {
                frame.render_widget(
                    Paragraph::new(
                        language.pick("No models match this filter.", "没有模型符合当前筛选条件。"),
                    )
                    .style(theme.dim_text())
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
                    ("n", language.pick("none", "全不选")),
                    ("i", language.pick("invert", "反选")),
                    ("o", language.pick("overwrite", "覆盖")),
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
                .title(Span::styled(
                    format!(
                        " {}  {}/{} ",
                        language.pick("Choose metadata source", "选择元数据来源"),
                        index + 1,
                        ambiguities.len()
                    ),
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD),
                ))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.accent))
                .style(theme.surface_style());
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
                        Line::from(vec![
                            Span::styled(
                                format!("{}: ", language.pick("Target provider", "目标提供商")),
                                theme.label(),
                            ),
                            Span::styled(ambiguity.provider_id.clone(), theme.value()),
                        ]),
                        Line::from(vec![
                            Span::styled(
                                format!("{}: ", language.pick("Model", "模型")),
                                theme.label(),
                            ),
                            Span::styled(ambiguity.model_id.clone(), theme.value()),
                        ]),
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
                                theme.label(),
                            )),
                        ]))
                    })
                    .collect::<Vec<_>>();
                let mut state = ListState::default().with_selected(Some(*cursor));
                frame.render_stateful_widget(
                    List::new(rows)
                        .highlight_symbol(" > ")
                        .highlight_style(theme.selected()),
                    list,
                    &mut state,
                );
            }
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(" Enter/Space ", theme.keycap()),
                    Span::styled(language.pick("select  ", "选择  "), theme.label()),
                    Span::styled(" Esc ", theme.keycap()),
                    Span::styled(language.pick("cancel", "取消"), theme.label()),
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
                .title(Span::styled(
                    format!(
                        " {}  {}/{} {} ",
                        language.pick("OpenCode providers", "OpenCode 提供商"),
                        selected.len(),
                        providers.len(),
                        language.pick("selected", "已选择")
                    ),
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD),
                ))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.accent))
                .style(theme.surface_style());
            let inner = block.inner(rect);
            frame.render_widget(block, rect);
            let [list, hint] =
                Layout::vertical([Constraint::Min(1), Constraint::Length(2)]).areas(inner);
            let rows = providers
                .iter()
                .enumerate()
                .map(|(index, provider)| {
                    ListItem::new(Line::from(vec![
                        checkbox_span(selected.contains(&index), theme),
                        Span::raw(provider.as_str()),
                    ]))
                })
                .collect::<Vec<_>>();
            let mut state = ListState::default().with_selected(Some(*cursor));
            frame.render_stateful_widget(
                List::new(rows)
                    .highlight_symbol(" > ")
                    .highlight_style(theme.selected()),
                list,
                &mut state,
            );
            render_key_hints(
                frame,
                hint,
                &[
                    ("Space", language.pick("toggle", "切换")),
                    ("a", language.pick("all", "全选")),
                    ("n", language.pick("none", "全不选")),
                    ("i", language.pick("invert", "反选")),
                    ("Enter", language.pick("import", "导入")),
                    ("Esc", language.pick("cancel", "取消")),
                ],
                theme,
            );
        }
        _ => unreachable!("non-catalog overlay routed to catalog renderer"),
    }
}
