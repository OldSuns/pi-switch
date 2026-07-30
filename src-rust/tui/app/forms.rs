use super::*;

impl App {
    pub(in crate::tui) fn on_form_key(&mut self, form: &mut FormState, key: KeyEvent) -> bool {
        let save_provider = |app: &mut Self, form: &FormState| {
            let draft = match form.draft() {
                Ok(draft) => draft,
                Err(error) => {
                    app.overlay = Some(Overlay::Error(error.to_string()));
                    return;
                }
            };
            let removes_default = !draft.in_pi
                && form.previous_id.as_deref() == app.snapshot.default_provider.as_deref();
            if removes_default {
                app.overlay = Some(Overlay::ConfirmSaveProviderWithoutPi {
                    form: form.clone(),
                    draft,
                });
            } else {
                match documents::save_provider(&app.paths, form.previous_id.as_deref(), &draft) {
                    Ok(()) => app.reload(Some(app.language.pick("Provider saved", "提供商已保存"))),
                    Err(error) => app.overlay = Some(Overlay::Error(error.to_string())),
                }
            }
        };
        if form.show_help {
            if matches!(key.code, KeyCode::Esc | KeyCode::Enter | KeyCode::Char('?')) {
                form.show_help = false;
            }
            return false;
        }
        if form.editing_headers {
            match key.code {
                KeyCode::Esc => {
                    form.editing_headers = false;
                    form.cursor = 0;
                }
                KeyCode::Tab => form.select_headers_field(form.headers_field + 1),
                KeyCode::BackTab => form.select_headers_field(form.headers_field + 1),
                KeyCode::Enter if form.headers_field == 1 => {
                    insert_char(&mut form.headers_json, form.cursor, '\n');
                    form.cursor += 1;
                }
                _ => {
                    let mut cursor = form.cursor;
                    if form.headers_field == 0 {
                        edit_text_key(&mut form.user_agent, &mut cursor, key);
                    } else {
                        edit_text_key(&mut form.headers_json, &mut cursor, key);
                    }
                    form.cursor = cursor;
                }
            }
            return false;
        }
        if form.editing_compat {
            match key.code {
                KeyCode::Esc => {
                    form.editing_compat = false;
                    form.cursor = 0;
                }
                KeyCode::Tab | KeyCode::Down => form.select_compat_field(form.compat_field + 1),
                KeyCode::BackTab | KeyCode::Up => {
                    form.select_compat_field(form.compat_field + 10);
                }
                KeyCode::Enter if form.compat_field == 10 => {
                    insert_char(&mut form.other_compat_json, form.cursor, '\n');
                    form.cursor += 1;
                }
                KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') if form.compat_field == 0 => {
                    form.preset = if key.code == KeyCode::Left {
                        (form.preset + PRESETS.len() - 1) % PRESETS.len()
                    } else {
                        (form.preset + 1) % PRESETS.len()
                    };
                    form.apply_preset();
                }
                KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') if form.compat_field == 1 => {
                    form.requires_reasoning_content =
                        cycle_tristate(form.requires_reasoning_content, key.code);
                }
                KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') if form.compat_field == 2 => {
                    form.thinking_format = if key.code == KeyCode::Left {
                        (form.thinking_format + THINKING_FORMATS.len() - 1) % THINKING_FORMATS.len()
                    } else {
                        (form.thinking_format + 1) % THINKING_FORMATS.len()
                    };
                }
                KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') if form.compat_field == 3 => {
                    form.supports_long_cache_retention =
                        cycle_tristate(form.supports_long_cache_retention, key.code);
                }
                KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') if form.compat_field == 4 => {
                    form.supports_store = cycle_tristate(form.supports_store, key.code);
                }
                KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') if form.compat_field == 5 => {
                    form.supports_developer_role =
                        cycle_tristate(form.supports_developer_role, key.code);
                }
                KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') if form.compat_field == 6 => {
                    form.supports_reasoning_effort =
                        cycle_tristate(form.supports_reasoning_effort, key.code);
                }
                KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') if form.compat_field == 7 => {
                    form.max_tokens_field = if key.code == KeyCode::Left {
                        (form.max_tokens_field + MAX_TOKENS_FIELDS.len() - 1)
                            % MAX_TOKENS_FIELDS.len()
                    } else {
                        (form.max_tokens_field + 1) % MAX_TOKENS_FIELDS.len()
                    };
                }
                KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') if form.compat_field == 8 => {
                    form.supports_strict_mode = cycle_tristate(form.supports_strict_mode, key.code);
                }
                KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') if form.compat_field == 9 => {
                    form.send_session_affinity_headers = !form.send_session_affinity_headers;
                }
                _ => {
                    let mut cursor = form.cursor;
                    if let Some(text) = form.current_text_mut() {
                        edit_text_key(text, &mut cursor, key);
                        form.cursor = cursor;
                    }
                }
            }
            return false;
        }
        if key.code == KeyCode::Char('?') && form.current_text().is_none() {
            form.show_help = true;
            return false;
        }
        match key.code {
            KeyCode::Esc => return true,
            KeyCode::Enter if form.field == 5 => {
                form.editing_headers = true;
                form.headers_field = 0;
                form.cursor = char_len(&form.user_agent);
            }
            KeyCode::Enter if form.field == 6 => {
                form.editing_compat = true;
                form.compat_field = 0;
                form.cursor = 0;
            }
            KeyCode::Char(' ') if form.field == 5 => {
                form.editing_headers = true;
                form.headers_field = 0;
                form.cursor = char_len(&form.user_agent);
            }
            KeyCode::Char(' ') if form.field == 6 => {
                form.editing_compat = true;
                form.compat_field = 0;
                form.cursor = 0;
            }
            KeyCode::Enter => {
                save_provider(self, form);
                return true;
            }
            KeyCode::Tab | KeyCode::Down => form.select_field(form.field + 1),
            KeyCode::BackTab | KeyCode::Up => form.select_field((form.field + 7) % 8),
            KeyCode::Left if form.field == 2 => {
                form.api = (form.api + API_TYPES.len()) % (API_TYPES.len() + 1)
            }
            KeyCode::Right if form.field == 2 => form.api = (form.api + 1) % (API_TYPES.len() + 1),
            KeyCode::Left | KeyCode::Right if form.field == 4 => {
                form.auth_header = !form.auth_header
            }
            KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') if form.field == 7 => {
                form.in_pi = !form.in_pi
            }
            _ => {
                let mut cursor = form.cursor;
                if let Some(text) = form.current_text_mut() {
                    edit_text_key(text, &mut cursor, key);
                    form.cursor = cursor;
                }
            }
        }
        false
    }

    pub(in crate::tui) fn on_model_form_key(
        &mut self,
        form: &mut ModelFormState,
        key: KeyEvent,
    ) -> bool {
        if key.code == KeyCode::Enter {
            let result = form.draft().and_then(|draft| {
                documents::save_model(
                    &self.paths,
                    &form.provider_id,
                    form.previous_id.as_deref(),
                    &draft,
                )
            });
            match result {
                Ok(()) => self.reload(Some(self.language.pick("Model saved", "模型已保存"))),
                Err(error) => self.overlay = Some(Overlay::Error(error.to_string())),
            }
            return true;
        }
        let field_id = form.current_field_id();
        match key.code {
            KeyCode::Esc => return true,
            KeyCode::Tab | KeyCode::Down => form.select_field(form.field + 1),
            KeyCode::BackTab | KeyCode::Up => {
                let count = form.visible_fields().len();
                form.select_field((form.field + count.saturating_sub(1)) % count.max(1));
            }
            KeyCode::Left if field_id == 2 => {
                form.api = (form.api + API_TYPES.len()) % (API_TYPES.len() + 1)
            }
            KeyCode::Right if field_id == 2 => form.api = (form.api + 1) % (API_TYPES.len() + 1),
            KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') if field_id == 3 => {
                form.reasoning = !form.reasoning
            }
            KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') if field_id == 4 => {
                form.image_input = !form.image_input
            }
            KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') if field_id == 5 => {
                form.limits_expanded = !form.limits_expanded;
                form.cursor = 0;
            }
            KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') if field_id == 8 => {
                form.pricing_expanded = !form.pricing_expanded;
                form.cursor = 0;
            }
            _ => {
                let mut cursor = form.cursor;
                if let Some(text) = form.current_text_mut() {
                    edit_text_key(text, &mut cursor, key);
                    form.cursor = cursor;
                }
            }
        }
        false
    }

    pub(in crate::tui) fn on_model_defaults_form_key(
        &mut self,
        form: &mut ModelDefaultsFormState,
        key: KeyEvent,
    ) -> bool {
        if key.code == KeyCode::Enter {
            let result = form
                .draft()
                .and_then(|defaults| documents::set_model_defaults(&self.paths, &defaults));
            match result {
                Ok(()) => self.reload(Some(
                    self.language
                        .pick("Default model parameters saved", "默认模型参数已保存"),
                )),
                Err(error) => self.overlay = Some(Overlay::Error(error.to_string())),
            }
            return true;
        }
        match key.code {
            KeyCode::Esc => return true,
            KeyCode::Tab | KeyCode::Down => form.select_field(form.field + 1),
            KeyCode::BackTab | KeyCode::Up => form.select_field((form.field + 5) % 6),
            _ => {
                let mut cursor = form.cursor;
                edit_text_key(form.current_text_mut(), &mut cursor, key);
                form.cursor = cursor;
            }
        }
        false
    }
}
