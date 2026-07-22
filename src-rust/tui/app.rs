use std::{collections::BTreeSet, sync::mpsc, thread};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::documents::{self, Backup, DoctorCheck, Paths, ProviderView, Snapshot};

use super::{
    forms::{FormState, ModelFormState},
    input::{insert_char, moved, remove_char},
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
    Reload,
    Doctor,
    Backups,
    ImportOpenCode,
}

impl SettingsAction {
    pub(super) const ALL: [Self; 4] = [
        Self::Reload,
        Self::Doctor,
        Self::Backups,
        Self::ImportOpenCode,
    ];

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Reload => "Reload configuration",
            Self::Doctor => "Validate configuration",
            Self::Backups => "Browse backups",
            Self::ImportOpenCode => "Import from OpenCode",
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

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Home => "Home",
            Self::Profiles => "Profiles",
            Self::Settings => "Settings",
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
        provider_id: String,
    },
    Fetched {
        provider_id: String,
        models: Vec<String>,
        selected: BTreeSet<usize>,
        cursor: usize,
    },
}

pub(super) struct App {
    pub(super) paths: Paths,
    pub(super) snapshot: Snapshot,
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
    pub(super) task: Option<mpsc::Receiver<documents::Result<Vec<String>>>>,
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
                };
                let mut app = Self::from_snapshot(paths, snapshot);
                app.overlay = Some(Overlay::Error(error.to_string()));
                app
            }
        }
    }

    pub(super) fn from_snapshot(paths: Paths, snapshot: Snapshot) -> Self {
        Self {
            paths,
            snapshot,
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
            Ok(Ok(models)) => {
                let provider_id = match self.overlay.take() {
                    Some(Overlay::Loading { provider_id }) => provider_id,
                    _ => String::new(),
                };
                let selected = (0..models.len()).collect();
                self.overlay = Some(Overlay::Fetched {
                    provider_id,
                    models,
                    selected,
                    cursor: 0,
                });
                self.task = None;
            }
            Ok(Err(error)) => {
                self.overlay = Some(Overlay::Error(error.to_string()));
                self.task = None;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.overlay = Some(Overlay::Error(
                    "model catalog task ended unexpectedly".into(),
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
                Command::Reload => self.reload(Some("Reloaded Pi configuration")),
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
                    self.settings_cursor =
                        (self.settings_cursor + 1).min(SettingsAction::ALL.len() - 1)
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
                    self.notice(NoticeKind::Warning, "Model request cannot be cancelled");
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
            Overlay::ConfirmDeleteProvider(id) => match key.code {
                KeyCode::Char('y') | KeyCode::Enter => {
                    let id = id.clone();
                    match documents::remove_provider(&self.paths, &id) {
                        Ok(()) => self.reload(Some("Provider deleted")),
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
                        Ok(()) => self.reload(Some("Model deleted")),
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
                KeyCode::Enter if !items.is_empty() => {
                    self.overlay = Some(Overlay::ConfirmRestore(items[*selected].clone()));
                    return;
                }
                _ => {}
            },
            Overlay::ConfirmRestore(backup) => match key.code {
                KeyCode::Char('y') | KeyCode::Enter => {
                    match documents::restore_backup(&self.paths, backup) {
                        Ok(()) => self.reload(Some("Backup restored")),
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
        }
        self.overlay = Some(overlay);
    }

    pub(super) fn on_form_key(&mut self, form: &mut FormState, key: KeyEvent) -> bool {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
            let result = form.draft().and_then(|draft| {
                documents::save_provider(&self.paths, form.previous_id.as_deref(), &draft)
            });
            match result {
                Ok(()) => self.reload(Some("Provider saved")),
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
                Ok(()) => self.reload(Some("Model saved")),
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
        match SettingsAction::ALL[self.settings_cursor] {
            SettingsAction::Reload => self.reload(Some("Reloaded Pi configuration")),
            SettingsAction::Doctor => {
                self.overlay = Some(Overlay::Doctor(documents::doctor(&self.paths)))
            }
            SettingsAction::Backups => self.open_backups(),
            SettingsAction::ImportOpenCode => self.import_opencode(),
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
                self.notice(NoticeKind::Warning, "Select a model to edit");
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
                self.notice(NoticeKind::Warning, "Select a model to delete");
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
                self.notice(NoticeKind::Warning, "Select a model to copy");
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
                self.notice(NoticeKind::Success, format!("Created provider {copy_id}"));
            }
            Err(error) => self.overlay = Some(Overlay::Error(error.to_string())),
        }
    }

    pub(super) fn set_selected_default(&mut self) {
        let Some(provider) = self.selected_provider() else {
            return;
        };
        let Some(model) = provider.models.get(self.model_cursor) else {
            self.notice(NoticeKind::Warning, "Select a provider model first");
            return;
        };
        let provider_id = provider.id.clone();
        let model_id = model.id.clone();
        match documents::set_default(&self.paths, &provider_id, &model_id) {
            Ok(()) => self.reload(Some("Default model updated")),
            Err(error) => self.overlay = Some(Overlay::Error(error.to_string())),
        }
    }

    pub(super) fn start_fetch(&mut self) {
        let Some(provider) = self.selected_provider().cloned() else {
            return;
        };
        let provider_id = provider.id.clone();
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let result = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|error| documents::AppError::Http(error.to_string()))
                .and_then(|runtime| runtime.block_on(documents::fetch_models(provider)));
            let _ = sender.send(result);
        });
        self.task = Some(receiver);
        self.overlay = Some(Overlay::Loading { provider_id });
    }

    pub(super) fn import_fetched(&mut self, provider_id: &str, models: Vec<String>) {
        match documents::import_models(&self.paths, provider_id, &models) {
            Ok(added) => self.reload(Some(&format!("Imported {added} new model(s)"))),
            Err(error) => self.overlay = Some(Overlay::Error(error.to_string())),
        }
    }

    fn import_opencode(&mut self) {
        match documents::import_opencode(&self.paths) {
            Ok(summary) if summary.changed => self.reload(Some(&format!(
                "Imported {} provider(s) and {} model(s) from OpenCode",
                summary.providers, summary.models
            ))),
            Ok(_) => self.notice(
                NoticeKind::Success,
                "Pi configuration already matches OpenCode",
            ),
            Err(error) => self.overlay = Some(Overlay::Error(error.to_string())),
        }
    }

    pub(super) fn open_backups(&mut self) {
        match documents::list_backups(&self.paths) {
            Ok(items) => self.overlay = Some(Overlay::Backups { items, selected: 0 }),
            Err(error) => self.overlay = Some(Overlay::Error(error.to_string())),
        }
    }
}
