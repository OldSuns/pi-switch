use std::{collections::BTreeSet, sync::mpsc, thread};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::documents::{
    self, Backup, CatalogAmbiguity, CatalogFetch, CatalogModel, DoctorCheck, ImportSummary,
    OpenCodeImportPlan, Paths, ProviderView, Snapshot,
};

use super::{
    forms::{FormState, ModelDefaultsFormState, ModelFormState},
    i18n::Language,
    input::{char_len, insert_char, moved, remove_char},
    keys::{command_for, Command},
    API_TYPES, WIDE_WIDTH,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Focus {
    Menu,
    Content,
    Providers,
    Models,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Page {
    Home,
    Profiles,
    Settings,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum SettingsAction {
    Language,
    FetchMetadata,
    ModelDefaults,
    Reload,
    Doctor,
    Backups,
    ImportOpenCode,
}

impl SettingsAction {
    pub(super) const ALL: [Self; 7] = [
        Self::Language,
        Self::FetchMetadata,
        Self::ModelDefaults,
        Self::Reload,
        Self::Doctor,
        Self::Backups,
        Self::ImportOpenCode,
    ];

    pub(super) fn visible(fetch_metadata: bool) -> impl Iterator<Item = Self> {
        Self::ALL
            .into_iter()
            .filter(move |action| !fetch_metadata || *action != Self::ModelDefaults)
    }

    pub(super) fn label(self, language: Language) -> String {
        match self {
            Self::Language => format!("{}: {}", language.pick("Language", "语言"), language.name()),
            Self::FetchMetadata => language
                .pick(
                    "Fetch model metadata from models.dev",
                    "从 models.dev 获取模型信息",
                )
                .into(),
            Self::ModelDefaults => language
                .pick("Default model parameters", "默认模型参数")
                .into(),
            Self::Reload => language.pick("Reload configuration", "重载配置").into(),
            Self::Doctor => language.pick("Validate configuration", "验证配置").into(),
            Self::Backups => language.pick("Browse backups", "浏览备份").into(),
            Self::ImportOpenCode => language
                .pick("Import from OpenCode", "从 OpenCode 导入")
                .into(),
        }
    }
}

impl Page {
    pub(super) const ALL: [Self; 3] = [Self::Home, Self::Profiles, Self::Settings];

    pub(super) fn index(self) -> usize {
        match self {
            Self::Home => 0,
            Self::Profiles => 1,
            Self::Settings => 2,
        }
    }

    pub(super) fn label(self, language: Language) -> &'static str {
        match self {
            Self::Home => language.pick("Home", "主页"),
            Self::Profiles => language.pick("Profiles", "配置"),
            Self::Settings => language.pick("Settings", "设置"),
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum NoticeKind {
    Success,
    Warning,
}

pub(super) struct Notice {
    pub(super) kind: NoticeKind,
    pub(super) message: String,
    pub(super) ticks: u16,
}

pub(super) enum Overlay {
    Help,
    Error(String),
    Form(FormState),
    ModelForm(ModelFormState),
    ModelDefaultsForm(ModelDefaultsFormState),
    ConfirmDeleteProvider(String),
    ConfirmDeleteModel {
        provider_id: String,
        model_id: String,
    },
    Backups {
        items: Vec<Backup>,
        selected: usize,
    },
    ConfirmRestore(Backup),
    Doctor(Vec<DoctorCheck>),
    Loading {
        message: String,
    },
    Fetched {
        provider_id: String,
        models: Vec<CatalogModel>,
        unavailable: usize,
        selected: BTreeSet<usize>,
        cursor: usize,
    },
    CatalogMatches {
        ambiguities: Vec<CatalogAmbiguity>,
        index: usize,
        cursor: usize,
        continuation: Option<CatalogContinuation>,
    },
    OpenCodeProviders {
        providers: Vec<String>,
        selected: BTreeSet<usize>,
        cursor: usize,
    },
}

pub(super) enum CatalogContinuation {
    Fetched {
        provider_id: String,
        models: Vec<CatalogModel>,
        unavailable: usize,
    },
    OpenCode {
        plan: OpenCodeImportPlan,
        candidate_indices: Vec<usize>,
    },
}

enum BackgroundResult {
    Catalog {
        provider_id: String,
        fetched: CatalogFetch,
    },
    OpenCodePrepared(OpenCodeImportPlan),
    OpenCode(ImportSummary),
}

pub(super) struct App {
    pub(super) paths: Paths,
    pub(super) snapshot: Snapshot,
    pub(super) language: Language,
    pub(super) page: Page,
    pub(super) provider_cursor: usize,
    pub(super) model_cursor: usize,
    pub(super) settings_cursor: usize,
    pub(super) focus: Focus,
    pub(super) filter: String,
    pub(super) filtering: bool,
    pub(super) narrow_detail: bool,
    pub(super) width: u16,
    pub(super) overlay: Option<Overlay>,
    pub(super) notice: Option<Notice>,
    task: Option<mpsc::Receiver<documents::Result<BackgroundResult>>>,
    pub(super) tick_count: usize,
    pub(super) quit: bool,
}

impl App {
    pub(super) fn new(paths: Paths) -> Self {
        match documents::load_snapshot(&paths) {
            Ok(snapshot) => Self::from_snapshot(paths, snapshot),
            Err(error) => {
                let snapshot = Snapshot {
                    models_path: paths.models.display().to_string(),
                    settings_path: paths.settings.display().to_string(),
                    providers: Vec::new(),
                    default_provider: None,
                    default_model: None,
                    language: "en".into(),
                    fetch_model_metadata: true,
                    model_defaults: Default::default(),
                };
                let mut app = Self::from_snapshot(paths, snapshot);
                app.overlay = Some(Overlay::Error(error.to_string()));
                app
            }
        }
    }

    pub(super) fn from_snapshot(paths: Paths, snapshot: Snapshot) -> Self {
        let language = Language::from_code(&snapshot.language);
        Self {
            paths,
            snapshot,
            language,
            page: Page::Home,
            provider_cursor: 0,
            model_cursor: 0,
            settings_cursor: 0,
            focus: Focus::Menu,
            filter: String::new(),
            filtering: false,
            narrow_detail: false,
            width: 120,
            overlay: None,
            notice: None,
            task: None,
            tick_count: 0,
            quit: false,
        }
    }

    pub(super) fn visible_providers(&self) -> Vec<usize> {
        let needle = self.filter.to_lowercase();
        self.snapshot
            .providers
            .iter()
            .enumerate()
            .filter(|(_, provider)| {
                needle.is_empty() || provider.id.to_lowercase().contains(&needle)
            })
            .map(|(index, _)| index)
            .collect()
    }

    pub(super) fn selected_provider(&self) -> Option<&ProviderView> {
        let visible = self.visible_providers();
        visible
            .get(self.provider_cursor)
            .and_then(|index| self.snapshot.providers.get(*index))
    }

    pub(super) fn reload(&mut self, message: Option<&str>) {
        let selected = self.selected_provider().map(|provider| provider.id.clone());
        match documents::load_snapshot(&self.paths) {
            Ok(snapshot) => {
                self.snapshot = snapshot;
                self.provider_cursor = selected
                    .and_then(|id| {
                        self.visible_providers()
                            .iter()
                            .position(|index| self.snapshot.providers[*index].id == id)
                    })
                    .unwrap_or_default();
                self.clamp_selection();
                if let Some(message) = message {
                    self.notice(NoticeKind::Success, message);
                }
            }
            Err(error) => self.overlay = Some(Overlay::Error(error.to_string())),
        }
    }

    pub(super) fn clamp_selection(&mut self) {
        self.provider_cursor = self
            .provider_cursor
            .min(self.visible_providers().len().saturating_sub(1));
        let model_count = self
            .selected_provider()
            .map(|provider| provider.models.len())
            .unwrap_or_default();
        self.model_cursor = self.model_cursor.min(model_count.saturating_sub(1));
    }

    pub(super) fn notice(&mut self, kind: NoticeKind, message: impl Into<String>) {
        self.notice = Some(Notice {
            kind,
            message: message.into(),
            ticks: 30,
        });
    }

    pub(super) fn tick(&mut self) {
        self.tick_count = self.tick_count.wrapping_add(1);
        if let Some(notice) = self.notice.as_mut() {
            notice.ticks = notice.ticks.saturating_sub(1);
            if notice.ticks == 0 {
                self.notice = None;
            }
        }
        let Some(receiver) = self.task.as_ref() else {
            return;
        };
        match receiver.try_recv() {
            Ok(Ok(BackgroundResult::Catalog {
                provider_id,
                fetched,
            })) => {
                let CatalogFetch {
                    models,
                    ambiguous,
                    unavailable,
                } = fetched;
                if unavailable > 0 {
                    self.notice(
                        NoticeKind::Warning,
                        format!(
                            "{} {}",
                            unavailable,
                            self.language.pick(
                                "model(s) skipped without models.dev metadata",
                                "个模型因 models.dev 元数据缺失而跳过"
                            )
                        ),
                    );
                }
                self.task = None;
                if ambiguous.is_empty() {
                    self.show_fetched(provider_id, models, unavailable);
                } else {
                    self.overlay = Some(Overlay::CatalogMatches {
                        ambiguities: ambiguous,
                        index: 0,
                        cursor: 0,
                        continuation: Some(CatalogContinuation::Fetched {
                            provider_id,
                            models,
                            unavailable,
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

    pub(super) fn on_key(&mut self, key: KeyEvent) {
        if self.filtering {
            if command_for(key) == Some(Command::Quit) {
                self.quit = true;
                return;
            }
            self.on_filter_key(key);
            return;
        }
        if self.overlay.is_some() {
            if command_for(key) == Some(Command::Quit)
                && key.modifiers.contains(KeyModifiers::CONTROL)
            {
                self.quit = true;
                return;
            }
            self.on_overlay_key(key);
            return;
        }
        if self.page == Page::Settings
            && self.focus == Focus::Content
            && key.code == KeyCode::Char(' ')
        {
            self.run_settings_action();
            return;
        }
        if let Some(command) = command_for(key) {
            match command {
                Command::Quit => self.quit = true,
                Command::Help => self.overlay = Some(Overlay::Help),
                Command::Filter if self.in_profiles() => self.filtering = true,
                Command::New if self.in_profiles() => self.open_add(),
                Command::Edit if self.in_profiles() => self.open_edit(),
                Command::Delete if self.in_profiles() => self.open_delete(),
                Command::Copy if self.in_profiles() => self.duplicate_selected(),
                Command::Import if self.in_profiles() => self.start_fetch(),
                Command::SetDefault if self.in_profiles() && self.in_model_context() => {
                    self.set_selected_default()
                }
                Command::Backups => self.open_backups(),
                Command::Doctor => {
                    self.overlay = Some(Overlay::Doctor(documents::doctor(&self.paths)))
                }
                Command::Reload => self.reload(Some(
                    self.language
                        .pick("Reloaded Pi configuration", "Pi 配置已重载"),
                )),
                _ => {}
            }
            return;
        }

        if self.focus == Focus::Menu {
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    self.select_page(Page::ALL[self.page.index().saturating_sub(1)])
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.select_page(Page::ALL[(self.page.index() + 1).min(Page::ALL.len() - 1)])
                }
                KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => {
                    self.focus = if self.page == Page::Profiles {
                        Focus::Providers
                    } else {
                        Focus::Content
                    };
                }
                _ => {}
            }
            return;
        }

        if self.page == Page::Settings {
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    self.settings_cursor = self.settings_cursor.saturating_sub(1)
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    let last = SettingsAction::visible(self.snapshot.fetch_model_metadata)
                        .count()
                        .saturating_sub(1);
                    self.settings_cursor = (self.settings_cursor + 1).min(last)
                }
                KeyCode::Enter => self.run_settings_action(),
                KeyCode::Left | KeyCode::Char('h') | KeyCode::Esc | KeyCode::Tab => {
                    self.focus = Focus::Menu
                }
                _ => {}
            }
            return;
        }

        if self.page != Page::Profiles {
            match key.code {
                KeyCode::Left | KeyCode::Char('h') | KeyCode::Esc | KeyCode::Tab => {
                    self.focus = Focus::Menu
                }
                _ => {}
            }
            return;
        }

        match key.code {
            KeyCode::Tab => {
                if self.focus == Focus::Providers {
                    self.focus_models();
                } else {
                    self.focus_providers();
                }
            }
            KeyCode::Enter => self.focus_models(),
            KeyCode::Esc if self.focus == Focus::Models => self.focus_providers(),
            KeyCode::Esc => self.focus = Focus::Menu,
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Left | KeyCode::Char('h') if self.focus == Focus::Models => {
                self.focus_providers()
            }
            KeyCode::Left | KeyCode::Char('h') => self.focus = Focus::Menu,
            KeyCode::Right | KeyCode::Char('l') => self.focus_models(),
            _ => {}
        }
    }

    pub(super) fn on_filter_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.filter.clear();
                self.filtering = false;
            }
            KeyCode::Enter => self.filtering = false,
            KeyCode::Backspace => {
                self.filter.pop();
                self.provider_cursor = 0;
                self.model_cursor = 0;
            }
            KeyCode::Char(character) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.filter.push(character);
                self.provider_cursor = 0;
                self.model_cursor = 0;
            }
            _ => {}
        }
    }

    pub(super) fn on_overlay_key(&mut self, key: KeyEvent) {
        let Some(mut overlay) = self.overlay.take() else {
            return;
        };
        match &mut overlay {
            Overlay::Help | Overlay::Error(_) | Overlay::Doctor(_) => {
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
            Overlay::ConfirmDeleteProvider(id) => match key.code {
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
            } => match key.code {
                KeyCode::Esc => return,
                KeyCode::Up | KeyCode::Char('k') => *cursor = cursor.saturating_sub(1),
                KeyCode::Down | KeyCode::Char('j') => {
                    *cursor = (*cursor + 1).min(models.len().saturating_sub(1))
                }
                KeyCode::Char(' ') => {
                    if !selected.remove(cursor) {
                        selected.insert(*cursor);
                    }
                }
                KeyCode::Enter | KeyCode::Char('s') => {
                    let id = provider_id.clone();
                    let chosen = selected
                        .iter()
                        .filter_map(|index| models.get(*index))
                        .cloned()
                        .collect::<Vec<_>>();
                    self.import_fetched(&id, chosen);
                    return;
                }
                _ => {}
            },
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
                    let Some(candidate) = ambiguity.candidates.get(*cursor) else {
                        return;
                    };
                    match continuation.as_mut().expect("catalog match continuation") {
                        CatalogContinuation::Fetched { models, .. } => {
                            models.push(candidate.model.clone());
                        }
                        CatalogContinuation::OpenCode {
                            candidate_indices, ..
                        } => candidate_indices.push(*cursor),
                    }
                    *index += 1;
                    *cursor = 0;
                    if *index == ambiguities.len() {
                        match continuation.take().expect("catalog match continuation") {
                            CatalogContinuation::Fetched {
                                provider_id,
                                models,
                                unavailable,
                            } => self.show_fetched(provider_id, models, unavailable),
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
        }
        self.overlay = Some(overlay);
    }

    pub(super) fn on_form_key(&mut self, form: &mut FormState, key: KeyEvent) -> bool {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
            let result = form.draft().and_then(|draft| {
                documents::save_provider(&self.paths, form.previous_id.as_deref(), &draft)
            });
            match result {
                Ok(()) => self.reload(Some(self.language.pick("Provider saved", "提供商已保存"))),
                Err(error) => self.overlay = Some(Overlay::Error(error.to_string())),
            }
            return true;
        }
        if form.editing_headers {
            match key.code {
                KeyCode::Esc | KeyCode::Enter | KeyCode::Tab => {
                    form.editing_headers = false;
                    form.cursor = 0;
                }
                KeyCode::Left => form.cursor = form.cursor.saturating_sub(1),
                KeyCode::Right => form.cursor = (form.cursor + 1).min(char_len(&form.headers_json)),
                KeyCode::Home => form.cursor = 0,
                KeyCode::End => form.cursor = char_len(&form.headers_json),
                KeyCode::Backspace if form.cursor > 0 => {
                    let index = form.cursor - 1;
                    remove_char(&mut form.headers_json, index);
                    form.cursor = index;
                }
                KeyCode::Delete => remove_char(&mut form.headers_json, form.cursor),
                KeyCode::Char(character) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    insert_char(&mut form.headers_json, form.cursor, character);
                    form.cursor += 1;
                }
                _ => {}
            }
            return false;
        }
        match key.code {
            KeyCode::Esc => return true,
            KeyCode::Enter if form.field == 5 => {
                form.editing_headers = true;
                form.cursor = char_len(&form.headers_json);
            }
            KeyCode::Tab | KeyCode::Down => form.select_field(form.field + 1),
            KeyCode::BackTab | KeyCode::Up => form.select_field((form.field + 6) % 7),
            KeyCode::Left if form.field == 2 => {
                form.api = (form.api + API_TYPES.len()) % (API_TYPES.len() + 1)
            }
            KeyCode::Right if form.field == 2 => form.api = (form.api + 1) % (API_TYPES.len() + 1),
            KeyCode::Left | KeyCode::Right if form.field == 4 => {
                form.auth_header = !form.auth_header
            }
            KeyCode::Left => form.cursor = form.cursor.saturating_sub(1),
            KeyCode::Right => form.cursor = (form.cursor + 1).min(form.current_len()),
            KeyCode::Home => form.cursor = 0,
            KeyCode::End => form.cursor = form.current_len(),
            KeyCode::Backspace => {
                if form.cursor > 0 {
                    let index = form.cursor - 1;
                    if let Some(text) = form.current_text_mut() {
                        remove_char(text, index);
                    }
                    form.cursor = index;
                }
            }
            KeyCode::Delete => {
                let cursor = form.cursor;
                if let Some(text) = form.current_text_mut() {
                    remove_char(text, cursor);
                }
            }
            KeyCode::Char(character) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                let cursor = form.cursor;
                if let Some(text) = form.current_text_mut() {
                    insert_char(text, cursor, character);
                    form.cursor += 1;
                }
            }
            _ => {}
        }
        false
    }

    pub(super) fn on_model_form_key(&mut self, form: &mut ModelFormState, key: KeyEvent) -> bool {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
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
        match key.code {
            KeyCode::Esc => return true,
            KeyCode::Tab | KeyCode::Down => form.select_field(form.field + 1),
            KeyCode::BackTab | KeyCode::Up => form.select_field((form.field + 6) % 7),
            KeyCode::Left if form.field == 2 => {
                form.api = (form.api + API_TYPES.len()) % (API_TYPES.len() + 1)
            }
            KeyCode::Right if form.field == 2 => form.api = (form.api + 1) % (API_TYPES.len() + 1),
            KeyCode::Left | KeyCode::Right if form.field == 3 => form.reasoning = !form.reasoning,
            KeyCode::Left | KeyCode::Right if form.field == 4 => {
                form.image_input = !form.image_input
            }
            KeyCode::Left => form.cursor = form.cursor.saturating_sub(1),
            KeyCode::Right => form.cursor = (form.cursor + 1).min(form.current_len()),
            KeyCode::Home => form.cursor = 0,
            KeyCode::End => form.cursor = form.current_len(),
            KeyCode::Backspace => {
                if form.cursor > 0 {
                    let index = form.cursor - 1;
                    if let Some(text) = form.current_text_mut() {
                        remove_char(text, index);
                    }
                    form.cursor = index;
                }
            }
            KeyCode::Delete => {
                let cursor = form.cursor;
                if let Some(text) = form.current_text_mut() {
                    remove_char(text, cursor);
                }
            }
            KeyCode::Char(character) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                let cursor = form.cursor;
                if let Some(text) = form.current_text_mut() {
                    insert_char(text, cursor, character);
                    form.cursor += 1;
                }
            }
            _ => {}
        }
        false
    }

    fn on_model_defaults_form_key(
        &mut self,
        form: &mut ModelDefaultsFormState,
        key: KeyEvent,
    ) -> bool {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
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
            KeyCode::Left => form.cursor = form.cursor.saturating_sub(1),
            KeyCode::Right => form.cursor = (form.cursor + 1).min(char_len(form.current_text())),
            KeyCode::Home => form.cursor = 0,
            KeyCode::End => form.cursor = char_len(form.current_text()),
            KeyCode::Backspace if form.cursor > 0 => {
                let index = form.cursor - 1;
                remove_char(form.current_text_mut(), index);
                form.cursor = index;
            }
            KeyCode::Delete => {
                let cursor = form.cursor;
                remove_char(form.current_text_mut(), cursor);
            }
            KeyCode::Char(character) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                let cursor = form.cursor;
                insert_char(form.current_text_mut(), cursor, character);
                form.cursor += 1;
            }
            _ => {}
        }
        false
    }

    pub(super) fn move_selection(&mut self, delta: isize) {
        if self.focus == Focus::Models || (self.width < WIDE_WIDTH && self.narrow_detail) {
            let len = self
                .selected_provider()
                .map(|provider| provider.models.len())
                .unwrap_or_default();
            self.model_cursor = moved(self.model_cursor, delta, len);
        } else {
            self.provider_cursor =
                moved(self.provider_cursor, delta, self.visible_providers().len());
            self.model_cursor = 0;
        }
    }

    pub(super) fn in_model_context(&self) -> bool {
        self.page == Page::Profiles
            && (self.focus == Focus::Models || (self.width < WIDE_WIDTH && self.narrow_detail))
    }

    pub(super) fn in_profiles(&self) -> bool {
        self.page == Page::Profiles && self.focus != Focus::Menu
    }

    fn select_page(&mut self, page: Page) {
        self.page = page;
        self.filtering = false;
        self.narrow_detail = false;
        self.settings_cursor = 0;
    }

    fn focus_models(&mut self) {
        if self.selected_provider().is_some() {
            self.focus = Focus::Models;
            self.narrow_detail = self.width < WIDE_WIDTH;
        }
    }

    fn focus_providers(&mut self) {
        self.focus = Focus::Providers;
        self.narrow_detail = false;
    }

    fn run_settings_action(&mut self) {
        let Some(action) =
            SettingsAction::visible(self.snapshot.fetch_model_metadata).nth(self.settings_cursor)
        else {
            return;
        };
        match action {
            SettingsAction::Language => self.switch_language(),
            SettingsAction::FetchMetadata => self.toggle_fetch_metadata(),
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

    fn switch_language(&mut self) {
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

    fn toggle_fetch_metadata(&mut self) {
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

    fn import_options(&self) -> documents::ImportOptions {
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
            self.overlay = Some(Overlay::ConfirmDeleteProvider(provider.id.clone()));
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
        let options = self.import_options();
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let result = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|error| documents::AppError::Http(error.to_string()))
                .and_then(|runtime| runtime.block_on(documents::fetch_models(provider, options)))
                .map(|fetched| BackgroundResult::Catalog {
                    provider_id: task_provider_id,
                    fetched,
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

    pub(super) fn import_fetched(&mut self, provider_id: &str, models: Vec<CatalogModel>) {
        match documents::import_models(
            &self.paths,
            provider_id,
            &models,
            self.snapshot.fetch_model_metadata,
        ) {
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

    fn show_fetched(&mut self, provider_id: String, models: Vec<CatalogModel>, unavailable: usize) {
        let selected = (0..models.len()).collect();
        self.overlay = Some(Overlay::Fetched {
            provider_id,
            models,
            unavailable,
            selected,
            cursor: 0,
        });
    }

    fn open_opencode_providers(&mut self) {
        match documents::list_opencode_providers(&self.paths) {
            Ok(providers) => {
                let selected = (0..providers.len()).collect();
                self.overlay = Some(Overlay::OpenCodeProviders {
                    providers,
                    selected,
                    cursor: 0,
                });
            }
            Err(error) => self.overlay = Some(Overlay::Error(error.to_string())),
        }
    }

    fn start_opencode_import(&mut self, providers: Vec<String>) {
        let paths = self.paths.clone();
        let options = self.import_options();
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let result = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|error| documents::AppError::Http(error.to_string()))
                .and_then(|runtime| {
                    runtime.block_on(documents::prepare_opencode_import(
                        &paths, &providers, options,
                    ))
                })
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

    fn start_opencode_apply(&mut self, plan: OpenCodeImportPlan, candidate_indices: Vec<usize>) {
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

    fn finish_opencode_import(&mut self, summary: ImportSummary) {
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
