use std::thread;

use super::*;

impl App {
    /// Launch the background npm update check on a worker thread. The result
    /// is delivered through a dedicated channel polled in `tick`, independent
    /// of the catalog-import `task` channel. No-op when `enabled` is false.
    pub(in crate::tui) fn spawn_update_check(&mut self, paths: &Paths, enabled: bool) {
        if !enabled {
            return;
        }
        let cache_path = paths.update.clone();
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let _ = sender.send(documents::check_npm_update(&cache_path));
        });
        self.update_check = Some(receiver);
    }

    pub(in crate::tui) fn tick(&mut self) {
        self.tick_count = self.tick_count.wrapping_add(1);
        if let Some(notice) = self.notice.as_mut() {
            notice.ticks = notice.ticks.saturating_sub(1);
            if notice.ticks == 0 {
                self.notice = None;
            }
        }
        if self.task.is_some() {
            self.poll_task();
        }

        // The npm update check runs on its own channel so it never competes
        // with the catalog-import `task`. A single result is expected, after
        // which the receiver is dropped; failures are silent.
        if let Some(receiver) = self.update_check.as_ref() {
            match receiver.try_recv() {
                Ok(Ok(Some(latest))) => {
                    self.update_available = Some(latest.clone());
                    // Manual checks always pop up the install dialog. Auto-checks
                    // skip the dialog if the user previously dismissed this exact
                    // version, leaving only the home-page banner.
                    let show_dialog = self.update_check_manual
                        || self.dismissed_update.as_deref() != Some(latest.as_str());
                    if show_dialog && self.overlay.is_none() {
                        self.overlay = Some(Overlay::ConfirmUpdate { latest });
                    }
                }
                Ok(Ok(None)) | Ok(Err(_)) | Err(mpsc::TryRecvError::Disconnected) => {
                    if self.update_check_manual {
                        self.notice(
                            NoticeKind::Success,
                            self.language
                                .pick("You're on the latest version", "已是最新版本")
                                .to_owned(),
                        );
                    }
                }
                Err(mpsc::TryRecvError::Empty) => return,
            }
            self.update_check = None;
            self.update_check_manual = false;
        }

        // Poll the background install task. On success the user is told to
        // restart; on failure the error is surfaced as a notice.
        if let Some(receiver) = self.install_task.as_ref() {
            match receiver.try_recv() {
                Ok(Ok(())) => {
                    self.overlay = None;
                    self.notice(
                        NoticeKind::Success,
                        self.language
                            .pick(
                                "Update installed. Restart pi-switch to apply.",
                                "更新已安装，请重启 pi-switch 生效。",
                            )
                            .to_owned(),
                    );
                }
                Ok(Err(error)) => {
                    self.overlay = None;
                    self.notice(NoticeKind::Warning, error.to_string());
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.overlay = None;
                    self.notice(
                        NoticeKind::Warning,
                        self.language
                            .pick("Install task ended unexpectedly", "安装任务意外结束")
                            .to_owned(),
                    );
                }
                Err(mpsc::TryRecvError::Empty) => return,
            }
            self.install_task = None;
        }
    }

    fn poll_task(&mut self) {
        let Some(receiver) = self.task.as_ref() else {
            return;
        };
        match receiver.try_recv() {
            Ok(Ok(BackgroundResult::ModelIds { provider_id, ids })) => {
                self.task = None;
                let defaults = self.import_options().defaults;
                let models = ids.iter().map(|id| defaults.model(id)).collect();
                let ratio_prices = std::collections::BTreeMap::new();
                self.show_fetched(provider_id, models, 0, &ratio_prices, false);
            }
            Ok(Ok(BackgroundResult::Catalog {
                provider_id,
                fetched,
                overwrite,
            })) => {
                let CatalogFetch {
                    mut models,
                    mut ambiguous,
                    unavailable,
                    ratio_prices,
                    ratio_config_used: _,
                    catalog_unreachable,
                } = fetched;
                // Apply ratio_config prices on top of catalog metadata for both
                // resolved models and ambiguous candidates.
                for model in &mut models {
                    if let Some(cost) = ratio_prices.get(&model.id) {
                        if let Some(object) = model.config.as_object_mut() {
                            object.insert("cost".into(), cost.to_cost_json());
                        }
                    }
                }
                for ambiguity in &mut ambiguous {
                    for candidate in &mut ambiguity.candidates {
                        if let Some(cost) = ratio_prices.get(&candidate.model.id) {
                            if let Some(object) = candidate.model.config.as_object_mut() {
                                object.insert("cost".into(), cost.to_cost_json());
                            }
                        }
                    }
                }
                if catalog_unreachable {
                    self.notice(
                        NoticeKind::Warning,
                        self.language
                            .pick(
                                "models.dev unreachable — imported all models with default metadata",
                                "models.dev 不可达 — 已用默认元数据导入全部模型",
                            )
                            .to_string(),
                    );
                }
                if unavailable > 0 {
                    self.notice(
                        NoticeKind::Warning,
                        format!(
                            "{} {}",
                            unavailable,
                            self.language.pick(
                                "model(s) imported with default metadata (no models.dev match)",
                                "个模型无 models.dev 匹配，已用默认元数据导入"
                            )
                        ),
                    );
                }
                self.task = None;
                if ambiguous.is_empty() {
                    self.import_fetched(&provider_id, models, overwrite);
                } else {
                    self.overlay = Some(Overlay::CatalogMatches {
                        ambiguities: ambiguous.clone(),
                        index: 0,
                        cursor: 0,
                        continuation: Some(CatalogContinuation::ProviderImport {
                            provider_id,
                            resolved_models: models,
                            candidate_indices: Vec::new(),
                            overwrite,
                        }),
                    });
                }
            }
            Ok(Ok(BackgroundResult::OpenCodePrepared(plan))) => {
                self.task = None;
                if plan.ambiguous.is_empty() {
                    self.start_opencode_apply(plan, Vec::new());
                } else {
                    self.overlay = Some(Overlay::CatalogMatches {
                        ambiguities: plan.ambiguous.clone(),
                        index: 0,
                        cursor: 0,
                        continuation: Some(CatalogContinuation::OpenCode {
                            plan,
                            candidate_indices: Vec::new(),
                        }),
                    });
                }
            }
            Ok(Ok(BackgroundResult::OpenCode(summary))) => {
                self.overlay = None;
                self.finish_opencode_import(summary);
                self.task = None;
            }
            Ok(Err(error)) => {
                self.overlay = Some(Overlay::Error(error.to_string()));
                self.task = None;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.overlay = Some(Overlay::Error(
                    self.language
                        .pick("background task ended unexpectedly", "后台任务意外结束")
                        .into(),
                ));
                self.task = None;
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
    }
}
