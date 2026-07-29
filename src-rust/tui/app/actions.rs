use super::*;

impl App {
    pub(super) fn select_page(&mut self, page: Page) {
        self.page = page;
        self.filtering = false;
        self.session_filtering = false;
        self.narrow_detail = false;
        self.settings_cursor = 0;
        if page == Page::Sessions {
            self.ensure_sessions_loaded();
        }
    }

    pub(super) fn focus_models(&mut self) {
        if self.selected_provider().is_some() {
            self.focus = Focus::Models;
            self.narrow_detail = self.width < COMPACT_WIDTH;
        }
    }

    pub(super) fn focus_providers(&mut self) {
        self.focus = Focus::Providers;
        self.narrow_detail = false;
    }

    pub(super) fn run_settings_action(&mut self) {
        let Some(action) =
            SettingsAction::visible(self.snapshot.fetch_model_metadata).nth(self.settings_cursor)
        else {
            return;
        };
        match action {
            SettingsAction::Language => self.switch_language(),
            SettingsAction::FetchMetadata => self.toggle_fetch_metadata(),
            SettingsAction::AutoCheckUpdates => self.toggle_check_updates(),
            SettingsAction::CheckUpdateNow => self.check_update_now(),
            SettingsAction::ModelDefaults => {
                self.overlay = Some(Overlay::ModelDefaultsForm(ModelDefaultsFormState::new(
                    &self.snapshot.model_defaults,
                )))
            }
            SettingsAction::Reload => self.reload(Some(
                self.language
                    .pick("Reloaded Pi configuration", "Pi 配置已重载"),
            )),
            SettingsAction::Doctor => {
                self.overlay = Some(Overlay::Doctor(documents::doctor(&self.paths)))
            }
            SettingsAction::Backups => self.open_backups(),
            SettingsAction::ImportOpenCode => self.open_opencode_providers(),
        }
    }

    pub(super) fn switch_language(&mut self) {
        let language = self.language.next();
        match documents::set_language(&self.paths, language.code()) {
            Ok(()) => {
                self.language = language;
                self.snapshot.language = language.code().into();
                self.notice(
                    NoticeKind::Success,
                    format!("{}: {}", language.pick("Language", "语言"), language.name()),
                );
            }
            Err(error) => self.overlay = Some(Overlay::Error(error.to_string())),
        }
    }

    pub(super) fn toggle_selected_provider_in_pi(&mut self) {
        let Some(provider) = self.selected_provider() else {
            return;
        };
        let id = provider.id.clone();
        if provider.in_pi && self.snapshot.default_provider.as_deref() == Some(&id) {
            self.overlay = Some(Overlay::ConfirmRemoveProviderFromPi(id));
            return;
        }
        let in_pi = !provider.in_pi;
        match documents::set_provider_in_pi(&self.paths, &id, in_pi) {
            Ok(()) => self.reload(Some(self.language.pick(
                if in_pi {
                    "Provider synced to Pi"
                } else {
                    "Provider not synced"
                },
                if in_pi {
                    "提供商已同步到 Pi"
                } else {
                    "提供商已设为不同步"
                },
            ))),
            Err(error) => self.overlay = Some(Overlay::Error(error.to_string())),
        }
    }

    pub(super) fn toggle_fetch_metadata(&mut self) {
        let enabled = !self.snapshot.fetch_model_metadata;
        match documents::set_fetch_model_metadata(&self.paths, enabled) {
            Ok(()) => {
                self.snapshot.fetch_model_metadata = enabled;
                self.notice(
                    NoticeKind::Success,
                    self.language.pick(
                        if enabled {
                            "models.dev model metadata enabled"
                        } else {
                            "models.dev model metadata disabled"
                        },
                        if enabled {
                            "已启用 models.dev 模型信息"
                        } else {
                            "已关闭 models.dev 模型信息"
                        },
                    ),
                );
            }
            Err(error) => self.overlay = Some(Overlay::Error(error.to_string())),
        }
    }

    pub(super) fn toggle_check_updates(&mut self) {
        let enabled = !self.snapshot.check_updates;
        match documents::set_check_updates(&self.paths, enabled) {
            Ok(()) => {
                self.snapshot.check_updates = enabled;
                self.notice(
                    NoticeKind::Success,
                    self.language.pick(
                        if enabled {
                            "Automatic update check enabled"
                        } else {
                            "Automatic update check disabled"
                        },
                        if enabled {
                            "已启用自动检查更新"
                        } else {
                            "已关闭自动检查更新"
                        },
                    ),
                );
            }
            Err(error) => self.overlay = Some(Overlay::Error(error.to_string())),
        }
    }

    /// Trigger an immediate update check, bypassing the 24h cache so a fresh
    /// network request is made. Runs regardless of the auto-check toggle; the
    /// result is delivered through the same `update_check` channel and surfaced
    /// with a notice on completion.
    pub(super) fn check_update_now(&mut self) {
        if self.update_check.is_some() {
            self.notice(
                NoticeKind::Warning,
                self.language.pick("Already checking…", "正在检查…"),
            );
            return;
        }
        // Clear any prior result so the banner doesn't show a stale version
        // while the fresh request is in flight.
        self.update_available = None;
        // Discard the cache so check_npm_update must hit the registry instead
        // of returning a cached `latest`, and clear any prior dismissal so the
        // confirmation dialog is shown again for the freshly checked version.
        let _ = std::fs::remove_file(&self.paths.update);
        self.dismissed_update = None;
        let cache_path = self.paths.update.clone();
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let _ = sender.send(documents::check_npm_update(&cache_path));
        });
        self.update_check = Some(receiver);
        self.update_check_manual = true;
        self.notice(
            NoticeKind::Success,
            self.language.pick("Checking for updates…", "正在检查更新…"),
        );
    }

    /// Spawn a background `npm i -g @oldsuns/pi-switch` to install the latest
    /// version. Shows a loading overlay while the install runs; the result is
    /// polled in `tick()` and surfaced as a notice.
    pub(super) fn start_install_update(&mut self, _latest: String) {
        if self.install_task.is_some() {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let _ = sender.send(documents::install_update());
        });
        self.install_task = Some(receiver);
        self.overlay = Some(Overlay::Loading {
            message: self
                .language
                .pick(
                    "Installing @oldsuns/pi-switch…",
                    "正在安装 @oldsuns/pi-switch…",
                )
                .to_string(),
        });
    }

    pub(super) fn import_options(&self) -> documents::ImportOptions {
        documents::ImportOptions {
            fetch_metadata: self.snapshot.fetch_model_metadata,
            defaults: self.snapshot.model_defaults.clone(),
        }
    }

    pub(super) fn open_add(&mut self) {
        if self.in_model_context() {
            if let Some(provider_id) = self.selected_provider().map(|provider| provider.id.clone())
            {
                self.overlay = Some(Overlay::ModelForm(ModelFormState::add(&provider_id)));
            }
        } else {
            self.overlay = Some(Overlay::Form(FormState::add()));
        }
    }

    pub(super) fn open_edit(&mut self) {
        let Some(provider) = self.selected_provider().cloned() else {
            return;
        };
        if self.in_model_context() {
            let Some(model_id) = provider.models.get(self.model_cursor) else {
                self.notice(
                    NoticeKind::Warning,
                    self.language
                        .pick("Select a model to edit", "请选择要编辑的模型"),
                );
                return;
            };
            self.overlay = Some(Overlay::ModelForm(ModelFormState::edit(
                &provider.id,
                model_id,
            )));
        } else {
            self.overlay = Some(Overlay::Form(FormState::edit(&provider)));
        }
    }

    pub(super) fn open_delete(&mut self) {
        let Some(provider) = self.selected_provider() else {
            return;
        };
        if self.in_model_context() {
            let Some(model_id) = provider.models.get(self.model_cursor) else {
                self.notice(
                    NoticeKind::Warning,
                    self.language
                        .pick("Select a model to delete", "请选择要删除的模型"),
                );
                return;
            };
            self.overlay = Some(Overlay::ConfirmDeleteModel {
                provider_id: provider.id.clone(),
                model_id: model_id.id.clone(),
            });
        } else {
            self.overlay = Some(Overlay::ConfirmDeleteProvider {
                id: provider.id.clone(),
                in_pi: provider.in_pi,
            });
        }
    }

    pub(super) fn duplicate_selected(&mut self) {
        let Some(provider) = self.selected_provider().cloned() else {
            return;
        };
        if self.in_model_context() {
            let Some(model_id) = provider.models.get(self.model_cursor) else {
                self.notice(
                    NoticeKind::Warning,
                    self.language
                        .pick("Select a model to copy", "请选择要复制的模型"),
                );
                return;
            };
            self.overlay = Some(Overlay::ModelForm(ModelFormState::copy(
                &provider.id,
                model_id,
            )));
            return;
        }
        match documents::duplicate_provider(&self.paths, &provider.id) {
            Ok(copy_id) => {
                self.reload(None);
                self.provider_cursor = self
                    .visible_providers()
                    .iter()
                    .position(|index| self.snapshot.providers[*index].id == copy_id)
                    .unwrap_or(self.provider_cursor);
                self.notice(
                    NoticeKind::Success,
                    format!(
                        "{} {copy_id}",
                        self.language.pick("Created provider", "已创建提供商")
                    ),
                );
            }
            Err(error) => self.overlay = Some(Overlay::Error(error.to_string())),
        }
    }

    pub(super) fn set_selected_default(&mut self) {
        let Some(provider) = self.selected_provider() else {
            return;
        };
        if !provider.in_pi {
            self.notice(
                NoticeKind::Warning,
                self.language.pick(
                    "Add this provider to Pi before setting a default model",
                    "请先将此提供商加入 Pi，再设置默认模型",
                ),
            );
            return;
        }
        let Some(model) = provider.models.get(self.model_cursor) else {
            self.notice(
                NoticeKind::Warning,
                self.language
                    .pick("Select a provider model first", "请先选择提供商中的模型"),
            );
            return;
        };
        let provider_id = provider.id.clone();
        let model_id = model.id.clone();
        match documents::set_default(&self.paths, &provider_id, &model_id) {
            Ok(()) => self.reload(Some(
                self.language
                    .pick("Default model updated", "默认模型已更新"),
            )),
            Err(error) => self.overlay = Some(Overlay::Error(error.to_string())),
        }
    }

    pub(super) fn start_fetch(&mut self) {
        let Some(provider) = self.selected_provider().cloned() else {
            return;
        };
        let provider_id = provider.id.clone();
        let task_provider_id = provider_id.clone();
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let result =
                documents::fetch_model_ids(&provider).map(|ids| BackgroundResult::ModelIds {
                    provider_id: task_provider_id,
                    ids,
                });
            let _ = sender.send(result);
        });
        self.task = Some(receiver);
        self.overlay = Some(Overlay::Loading {
            message: format!(
                "{} {provider_id}",
                self.language.pick("Fetching models for", "正在获取模型：")
            ),
        });
    }

    /// Phase 2 of the import flow: after the user has selected model IDs from
    /// the fetched list, resolve models.dev metadata (and ratio_config pricing)
    /// for just those IDs. Ambiguous matches are presented interactively;
    /// models without any models.dev match fall back to defaults.
    pub(super) fn start_resolve_metadata(
        &mut self,
        provider_id: String,
        ids: Vec<String>,
        overwrite: bool,
    ) {
        let Some(provider) = self
            .snapshot
            .providers
            .iter()
            .find(|provider| provider.id == provider_id)
            .cloned()
        else {
            self.overlay = Some(Overlay::Error(
                self.language
                    .pick("Provider no longer exists", "提供商已不存在")
                    .to_string(),
            ));
            return;
        };
        let options = self.import_options();
        let task_provider_id = provider_id.clone();
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let result = documents::resolve_metadata(provider, ids, options).map(
                |fetched| BackgroundResult::Catalog {
                    provider_id: task_provider_id,
                    fetched,
                    overwrite,
                },
            );
            let _ = sender.send(result);
        });
        self.task = Some(receiver);
        self.overlay = Some(Overlay::Loading {
            message: self
                .language
                .pick(
                    "Fetching models.dev metadata",
                    "正在获取 models.dev 模型信息",
                )
                .to_string(),
        });
    }

    pub(super) fn import_fetched(
        &mut self,
        provider_id: &str,
        models: Vec<CatalogModel>,
        overwrite: bool,
    ) {
        match documents::import_models(&self.paths, provider_id, &models, overwrite) {
            Ok(summary) => self.reload(Some(&format!(
                "{} {}, {} {}",
                self.language.pick("Added", "新增"),
                summary.added,
                self.language.pick("updated", "更新"),
                summary.updated
            ))),
            Err(error) => self.overlay = Some(Overlay::Error(error.to_string())),
        }
    }

    pub(super) fn show_fetched(
        &mut self,
        provider_id: String,
        mut models: Vec<CatalogModel>,
        unavailable: usize,
        ratio_prices: &BTreeMap<String, RatioCost>,
        ratio_config_used: bool,
    ) {
        // Apply ratio_config prices on top of catalog (models.dev or default) metadata.
        for model in &mut models {
            if let Some(cost) = ratio_prices.get(&model.id) {
                if let Some(object) = model.config.as_object_mut() {
                    object.insert("cost".into(), cost.to_cost_json());
                }
            }
        }
        // Models that already exist in the provider are pre-checked (so the
        // user can see them at a glance) and tracked for an "exists" tag in
        // the list. Default is to skip them on import unless `o` is toggled.
        let existing: BTreeSet<String> = self
            .snapshot
            .providers
            .iter()
            .find(|provider| provider.id == provider_id)
            .map(|provider| {
                provider
                    .models
                    .iter()
                    .map(|model| model.id.clone())
                    .collect()
            })
            .unwrap_or_default();
        let selected = (0..models.len())
            .filter(|&index| existing.contains(&models[index].id))
            .collect();
        self.overlay = Some(Overlay::Fetched {
            provider_id,
            models,
            unavailable,
            selected,
            cursor: 0,
            ratio_config_used,
            overwrite: false,
            existing,
            filter: String::new(),
            filtering: false,
        });
    }

    pub(super) fn open_opencode_providers(&mut self) {
        match documents::list_opencode_providers(&self.paths) {
            Ok(providers) => {
                // Default to nothing selected — the user chooses what to import.
                let selected = BTreeSet::new();
                self.overlay = Some(Overlay::OpenCodeProviders {
                    providers,
                    selected,
                    cursor: 0,
                });
            }
            Err(error) => self.overlay = Some(Overlay::Error(error.to_string())),
        }
    }

    pub(super) fn start_opencode_import(&mut self, providers: Vec<String>) {
        let paths = self.paths.clone();
        let options = self.import_options();
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let result = documents::prepare_opencode_import(&paths, &providers, options)
                .map(BackgroundResult::OpenCodePrepared);
            let _ = sender.send(result);
        });
        self.task = Some(receiver);
        self.overlay = Some(Overlay::Loading {
            message: self
                .language
                .pick("Importing OpenCode configuration", "正在导入 OpenCode 配置")
                .into(),
        });
    }

    pub(super) fn start_opencode_apply(
        &mut self,
        plan: OpenCodeImportPlan,
        candidate_indices: Vec<usize>,
    ) {
        let paths = self.paths.clone();
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let result = documents::apply_opencode_import(&paths, plan, &candidate_indices)
                .map(BackgroundResult::OpenCode);
            let _ = sender.send(result);
        });
        self.task = Some(receiver);
        self.overlay = Some(Overlay::Loading {
            message: self
                .language
                .pick("Importing OpenCode configuration", "正在导入 OpenCode 配置")
                .into(),
        });
    }

    pub(super) fn finish_opencode_import(&mut self, summary: ImportSummary) {
        if !summary.changed {
            self.notice(
                NoticeKind::Success,
                self.language.pick(
                    "Pi configuration already matches OpenCode",
                    "Pi 配置已与 OpenCode 一致",
                ),
            );
            return;
        }
        self.reload(None);
        let mut message = format!(
            "{} {} {}, {} {}",
            self.language.pick("Imported", "已导入"),
            summary.providers,
            self.language.pick("provider(s)", "个提供商"),
            summary.models,
            self.language.pick("model(s)", "个模型"),
        );
        if self.snapshot.fetch_model_metadata {
            message.push_str(&format!(
                "; models.dev {} {}, {} {}",
                self.language.pick("matched", "匹配"),
                summary.metadata,
                self.language.pick("unresolved", "未解析"),
                summary.unresolved,
            ));
        } else {
            message.push_str(&format!(
                "; {} {}",
                self.language.pick("defaults applied", "已应用默认参数"),
                summary.defaults,
            ));
        }
        self.notice(
            if summary.unresolved == 0 {
                NoticeKind::Success
            } else {
                NoticeKind::Warning
            },
            message,
        );
    }

    pub(super) fn open_backups(&mut self) {
        match documents::list_backups(&self.paths) {
            Ok(items) => self.overlay = Some(Overlay::Backups { items, selected: 0 }),
            Err(error) => self.overlay = Some(Overlay::Error(error.to_string())),
        }
    }
}
