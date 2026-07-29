use super::*;

impl App {
    pub(in crate::tui) fn on_overlay_key(&mut self, key: KeyEvent) {
        let Some(mut overlay) = self.overlay.take() else {
            return;
        };
        match &mut overlay {
            Overlay::Help | Overlay::Error(_) | Overlay::Warning(_) | Overlay::Doctor(_) => {
                if matches!(key.code, KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q')) {
                    return;
                }
            }
            Overlay::Loading { .. } => {
                if key.code == KeyCode::Esc {
                    self.notice(
                        NoticeKind::Warning,
                        self.language
                            .pick("This request cannot be cancelled", "当前请求无法取消"),
                    );
                }
            }
            Overlay::Form(form) => {
                if self.on_form_key(form, key) {
                    return;
                }
            }
            Overlay::ModelForm(form) => {
                if self.on_model_form_key(form, key) {
                    return;
                }
            }
            Overlay::ModelDefaultsForm(form) => {
                if self.on_model_defaults_form_key(form, key) {
                    return;
                }
            }
            Overlay::ConfirmDeleteProvider { id, .. } => match key.code {
                KeyCode::Char('y') | KeyCode::Enter => {
                    let id = id.clone();
                    match documents::remove_provider(&self.paths, &id) {
                        Ok(()) => self
                            .reload(Some(self.language.pick("Provider deleted", "提供商已删除"))),
                        Err(error) => self.overlay = Some(Overlay::Error(error.to_string())),
                    }
                    return;
                }
                KeyCode::Esc | KeyCode::Char('n') => return,
                _ => {}
            },
            Overlay::ConfirmRemoveProviderFromPi(id) => match key.code {
                KeyCode::Char('y') | KeyCode::Enter => {
                    let id = id.clone();
                    match documents::set_provider_in_pi(&self.paths, &id, false) {
                        Ok(()) => self.reload(Some(
                            self.language
                                .pick("Provider removed from Pi", "提供商已从 Pi 移除"),
                        )),
                        Err(error) => self.overlay = Some(Overlay::Error(error.to_string())),
                    }
                    return;
                }
                KeyCode::Esc | KeyCode::Char('n') => return,
                _ => {}
            },
            Overlay::ConfirmSaveProviderWithoutPi { form, draft } => match key.code {
                KeyCode::Char('y') | KeyCode::Enter => {
                    match documents::save_provider(&self.paths, form.previous_id.as_deref(), draft)
                    {
                        Ok(()) => {
                            self.reload(Some(self.language.pick("Provider saved", "提供商已保存")))
                        }
                        Err(error) => self.overlay = Some(Overlay::Error(error.to_string())),
                    }
                    return;
                }
                KeyCode::Esc | KeyCode::Char('n') => {
                    self.overlay = Some(Overlay::Form(form.clone()));
                    return;
                }
                _ => {}
            },
            Overlay::ConfirmDeleteModel {
                provider_id,
                model_id,
            } => match key.code {
                KeyCode::Char('y') | KeyCode::Enter => {
                    match documents::remove_model(&self.paths, provider_id, model_id) {
                        Ok(()) => {
                            self.reload(Some(self.language.pick("Model deleted", "模型已删除")))
                        }
                        Err(error) => self.overlay = Some(Overlay::Error(error.to_string())),
                    }
                    return;
                }
                KeyCode::Esc | KeyCode::Char('n') => return,
                _ => {}
            },
            Overlay::ConfirmDeleteSession { path, .. } => match key.code {
                KeyCode::Char('y') | KeyCode::Enter => {
                    let path = std::path::PathBuf::from(path.clone());
                    match documents::delete_session(&path) {
                        Ok(method) => {
                            let message = match method {
                                documents::DeleteMethod::Trash => self
                                    .language
                                    .pick("Session moved to trash", "会话已移到回收站"),
                                documents::DeleteMethod::Unlink => {
                                    self.language.pick("Session deleted", "会话已删除")
                                }
                            };
                            self.sessions.retain(|session| session.path != path);
                            self.clamp_session_selection();
                            self.preview_path = None;
                            self.refresh_preview();
                            self.notice(NoticeKind::Success, message);
                        }
                        Err(error) => self.overlay = Some(Overlay::Error(error.to_string())),
                    }
                    return;
                }
                KeyCode::Esc | KeyCode::Char('n') => return,
                _ => {}
            },
            Overlay::Backups { items, selected } => match key.code {
                KeyCode::Esc | KeyCode::Char('q') => return,
                KeyCode::Up | KeyCode::Char('k') => *selected = selected.saturating_sub(1),

                KeyCode::Down | KeyCode::Char('j') => {
                    *selected = (*selected + 1).min(items.len().saturating_sub(1));
                }
                KeyCode::Enter | KeyCode::Char(' ') if !items.is_empty() => {
                    self.overlay = Some(Overlay::ConfirmRestore(items[*selected].clone()));
                    return;
                }
                _ => {}
            },
            Overlay::ConfirmRestore(backup) => match key.code {
                KeyCode::Char('y') | KeyCode::Enter => {
                    match documents::restore_backup(&self.paths, backup) {
                        Ok(()) => {
                            self.reload(Some(self.language.pick("Backup restored", "备份已恢复")))
                        }
                        Err(error) => self.overlay = Some(Overlay::Error(error.to_string())),
                    }
                    return;
                }
                KeyCode::Esc | KeyCode::Char('n') => return,
                _ => {}
            },
            Overlay::Fetched {
                provider_id,
                models,
                unavailable: _,
                selected,
                cursor,
                ratio_config_used: _,
                overwrite,
                existing: _,
                filter,
                filtering,
            } => {
                if *filtering {
                    match key.code {
                        KeyCode::Esc => {
                            *filtering = false;
                            filter.clear();
                            *cursor = 0;
                        }
                        KeyCode::Enter => {
                            *filtering = false;
                            *cursor = 0;
                        }
                        KeyCode::Backspace => {
                            filter.pop();
                            *cursor = 0;
                        }
                        KeyCode::Char(character)
                            if !key.modifiers.contains(KeyModifiers::CONTROL) =>
                        {
                            filter.push(character);
                            *cursor = 0;
                        }
                        _ => {}
                    }
                } else {
                    let visible = visible_fetched_indices(models, filter);
                    match key.code {
                        KeyCode::Esc => return,
                        KeyCode::Char('/') if !models.is_empty() => {
                            *filtering = true;
                            *cursor = 0;
                        }
                        KeyCode::Up | KeyCode::Char('k') => *cursor = cursor.saturating_sub(1),
                        KeyCode::Down | KeyCode::Char('j') => {
                            if !visible.is_empty() {
                                *cursor = (*cursor + 1).min(visible.len() - 1);
                            }
                        }
                        KeyCode::Char(' ') => {
                            if let Some(&original) = visible.get(*cursor) {
                                if !selected.remove(&original) {
                                    selected.insert(original);
                                }
                            }
                        }
                        KeyCode::Char('a') => {
                            for &original in &visible {
                                selected.insert(original);
                            }
                        }
                        KeyCode::Char('n') => {
                            for &original in &visible {
                                selected.remove(&original);
                            }
                        }
                        KeyCode::Char('i') => {
                            for &original in &visible {
                                if !selected.remove(&original) {
                                    selected.insert(original);
                                }
                            }
                        }
                        KeyCode::Char('o') => {
                            *overwrite = !*overwrite;
                        }
                        KeyCode::Enter | KeyCode::Char('s') => {
                            if selected.is_empty() {
                                self.notice(
                                    NoticeKind::Warning,
                                    self.language
                                        .pick("Select at least one model", "请至少选择一个模型"),
                                );
                            } else {
                                let id = provider_id.clone();
                                let chosen = selected
                                    .iter()
                                    .filter_map(|index| models.get(*index))
                                    .cloned()
                                    .collect::<Vec<_>>();
                                self.import_fetched(&id, chosen, *overwrite);
                                return;
                            }
                        }
                        _ => {}
                    }
                }
            }
            Overlay::CatalogMatches {
                ambiguities,
                index,
                cursor,
                continuation,
            } => match key.code {
                KeyCode::Esc => return,
                KeyCode::Up | KeyCode::Char('k') => *cursor = cursor.saturating_sub(1),
                KeyCode::Down | KeyCode::Char('j') => {
                    let candidate_count = ambiguities
                        .get(*index)
                        .map(|ambiguity| ambiguity.candidates.len())
                        .unwrap_or(0);
                    *cursor = (*cursor + 1).min(candidate_count.saturating_sub(1));
                }
                KeyCode::Enter | KeyCode::Char(' ') => {
                    let Some(ambiguity) = ambiguities.get(*index) else {
                        return;
                    };
                    if ambiguity.candidates.get(*cursor).is_none() {
                        return;
                    }
                    match continuation.as_mut().expect("catalog match continuation") {
                        CatalogContinuation::OpenCode {
                            candidate_indices, ..
                        } => candidate_indices.push(*cursor),
                    }
                    *index += 1;
                    *cursor = 0;
                    if *index == ambiguities.len() {
                        match continuation.take().expect("catalog match continuation") {
                            CatalogContinuation::OpenCode {
                                plan,
                                candidate_indices,
                            } => self.start_opencode_apply(plan, candidate_indices),
                        }
                        return;
                    }
                }
                _ => {}
            },
            Overlay::OpenCodeProviders {
                providers,
                selected,
                cursor,
            } => match key.code {
                KeyCode::Esc => return,
                KeyCode::Up | KeyCode::Char('k') => *cursor = cursor.saturating_sub(1),
                KeyCode::Down | KeyCode::Char('j') => {
                    *cursor = (*cursor + 1).min(providers.len().saturating_sub(1))
                }
                KeyCode::Char(' ') => {
                    if !selected.remove(cursor) {
                        selected.insert(*cursor);
                    }
                }
                KeyCode::Char('a') => {
                    *selected = (0..providers.len()).collect();
                }
                KeyCode::Char('n') => {
                    selected.clear();
                }
                KeyCode::Char('i') => {
                    for index in 0..providers.len() {
                        if !selected.remove(&index) {
                            selected.insert(index);
                        }
                    }
                }
                KeyCode::Enter if !selected.is_empty() => {
                    let chosen = selected
                        .iter()
                        .filter_map(|index| providers.get(*index))
                        .cloned()
                        .collect();
                    self.start_opencode_import(chosen);
                    return;
                }
                KeyCode::Enter => self.notice(
                    NoticeKind::Warning,
                    self.language
                        .pick("Select at least one provider", "请至少选择一个提供商"),
                ),
                _ => {}
            },
            Overlay::ConfirmUpdate { latest } => match key.code {
                KeyCode::Enter | KeyCode::Char('y') => {
                    self.start_install_update(latest.clone());
                    return;
                }
                KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('q') => {
                    self.dismissed_update = Some(latest.clone());
                    documents::dismiss_update(&self.paths.update, latest);
                    return;
                }
                _ => {}
            },
        }
        self.overlay = Some(overlay);
    }
}
