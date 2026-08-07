use super::form_render::*;
use super::*;

mod catalog;

pub(super) fn render_overlay(
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
                Line::from(language.pick(
                    "Space           providers: add/remove Pi; models: set default",
                    "Space           提供商：添加/移除 Pi；模型：设为默认",
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
        Overlay::Warning(message) => {
            let rect = modal_rect(area, 72, 10);
            let body = Paragraph::new(vec![
                Line::from(Span::styled(
                    language.pick("Provider library rebuilt", "提供商库已重建"),
                    Style::default()
                        .fg(theme.warning)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(message.as_str()),
                Line::from(""),
                Line::from(Span::styled(
                    language.pick("Esc or Enter to continue", "按 Esc 或 Enter 继续"),
                    Style::default().fg(theme.muted),
                )),
            ])
            .wrap(Wrap { trim: false });
            render_modal(
                frame,
                rect,
                language.pick(" Warning ", " 警告 "),
                body,
                theme.warning,
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
        Overlay::ConfirmDeleteProvider { id, in_pi } => {
            let rect = modal_rect(area, 64, 9);
            let body = Paragraph::new(vec![
                Line::from(format!(
                    "{} '{id}'?",
                    language.pick("Permanently delete provider", "永久删除提供商")
                )),
                Line::from(if *in_pi {
                    language.pick(
                        "It will be deleted locally and unsynced from Pi. Any default selection is cleared.",
                        "它将从本地库删除并取消同步到 Pi，关联的默认选择也会被清除。",
                    )
                } else {
                    language.pick(
                        "It will be deleted from the local provider library.",
                        "它将从本地提供商库中删除。",
                    )
                }),
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
        Overlay::ConfirmRemoveProviderFromPi(id) => {
            let rect = modal_rect(area, 64, 9);
            let body = Paragraph::new(vec![
                Line::from(format!(
                    "{} '{id}'?",
                    language.pick("Stop syncing provider to Pi", "取消同步提供商到 Pi")
                )),
                Line::from(language.pick(
                    "The local provider is kept; it will no longer be synced to Pi. Its default model selection will be cleared.",
                    "本地配置会保留，但不再同步到 Pi，关联的默认模型选择也将被清除。",
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
                language.pick(" Confirm removal ", " 确认移除 "),
                body,
                theme.warning,
                theme,
            );
        }
        Overlay::ConfirmSaveProviderWithoutPi { draft, .. } => {
            let rect = modal_rect(area, 64, 9);
            let body = Paragraph::new(vec![
                Line::from(format!(
                    "{} '{}' {}",
                    language.pick("Save provider", "保存提供商"),
                    draft.id,
                    language.pick("as not synced?", "为不同步？")
                )),
                Line::from(language.pick(
                    "It will no longer be synced to Pi and its default model selection will be cleared.",
                    "它将不再同步到 Pi，关联的默认模型选择也会被清除。",
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
                language.pick(" Confirm save ", " 确认保存 "),
                body,
                theme.warning,
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
        Overlay::ConfirmDeleteSession { path, label } => {
            let rect = modal_rect(area, 68, 9);
            let body = Paragraph::new(vec![
                Line::from(format!(
                    "{} '{label}'?",
                    language.pick("Delete session", "删除会话")
                )),
                Line::from(Span::styled(
                    truncate_width(path, 60),
                    Style::default().fg(theme.muted),
                )),
                Line::from(language.pick(
                    "Prefers trash, then permanent delete.",
                    "优先移到回收站，失败则永久删除。",
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
                language.pick(" Confirm session delete ", " 确认删除会话 "),
                body,
                theme.error,
                theme,
            );
        }
        Overlay::Backups { items, selected } => {
            let rect = modal_rect(area, 76, 20);
            clear_area(frame, rect, theme);
            let block = Block::default()
                .title(Span::styled(
                    format!(" {}  {} ", language.pick("Backups", "备份"), items.len()),
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
                Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);
            if items.is_empty() {
                frame.render_widget(
                    Paragraph::new(language.pick("No backups yet.", "暂无备份。"))
                        .style(theme.dim_text())
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
                    Span::styled(language.pick("restore  ", "恢复  "), theme.label()),
                    Span::styled(" Esc ", theme.keycap()),
                    Span::styled(language.pick("close", "关闭"), theme.label()),
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
        Overlay::ConfirmUpdate { latest } => {
            let rect = modal_rect(area, 60, 10);
            let body = Paragraph::new(vec![
                Line::from(format!(
                    "{} {} \u{2192} {}",
                    language.pick("New version available:", "发现新版本："),
                    env!("CARGO_PKG_VERSION"),
                    latest
                )),
                Line::from(""),
                Line::from(language.pick(
                    "Install @oldsuns/pi-switch globally via npm?",
                    "是否通过 npm 全局安装 @oldsuns/pi-switch？",
                )),
                Line::from(""),
                Line::from(Span::styled(
                    language.pick("Enter/y install   Esc/n skip", "Enter/y 安装   Esc/n 跳过"),
                    Style::default().fg(theme.muted),
                )),
            ])
            .wrap(Wrap { trim: true });
            render_modal(
                frame,
                rect,
                language.pick(" Update available ", " 有新版本 "),
                body,
                theme.warning,
                theme,
            );
        }
        overlay @ (Overlay::Fetched { .. }
        | Overlay::CatalogMatches { .. }
        | Overlay::OpenCodeProviders { .. }) => {
            catalog::render_catalog_overlay(frame, overlay, language, area, theme);
        }
    }
}
