use std::collections::{BTreeMap, BTreeSet};
use std::{sync::mpsc, thread};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::documents::{
    self, Backup, CatalogAmbiguity, CatalogFetch, CatalogModel, DoctorCheck, ImportSummary,
    OpenCodeImportPlan, Paths, PreviewMessage, ProviderView, RatioCost, SessionSummary, Snapshot,
};

#[path = "app/actions.rs"]
mod actions;
#[path = "app/forms.rs"]
mod form_keys;
#[path = "app/overlay.rs"]
mod overlay;
#[path = "app/sessions.rs"]
mod sessions;

use super::{
    forms::{FormState, ModelDefaultsFormState, ModelFormState},
    i18n::Language,
    input::{char_len, edit_text_key, insert_char, moved},
    keys::{command_for, Command},
    API_TYPES, COMPACT_WIDTH,
};

/// Copy text to the system clipboard using the platform's native CLI.
/// Falls back gracefully when no clipboard tool is available.
fn copy_text_to_clipboard(text: &str) -> std::result::Result<(), String> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    // On Windows, use `clip`; on macOS, `pbcopy`; on Linux, `xclip` / `xsel` / `wl-copy`.
    #[cfg(target_os = "windows")]
    let (program, args): (&str, Vec<&str>) = ("clip", vec![]);
    #[cfg(target_os = "macos")]
    let (program, args): (&str, Vec<&str>) = ("pbcopy", vec![]);
    #[cfg(all(unix, not(target_os = "macos")))]
    let (program, args): (&str, Vec<&str>) = {
        if std::process::Command::new("which")
            .arg("wl-copy")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
        {
            ("wl-copy", vec!["--"])
        } else if std::process::Command::new("which")
            .arg("xclip")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
        {
            ("xclip", vec!["-selection", "clipboard"])
        } else if std::process::Command::new("which")
            .arg("xsel")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
        {
            ("xsel", vec!["--clipboard", "--input"])
        } else {
            return Err("no clipboard tool found (install xclip, xsel, or wl-copy)".into());
        }
    };

    let mut child = Command::new(program)
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("failed to launch {program}: {e}"))?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(text.as_bytes())
            .map_err(|e| format!("failed to write to {program}: {e}"))?;
    }
    let status = child
        .wait()
        .map_err(|e| format!("failed to wait for {program}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{program} exited with {status}"))
    }
}

pub(super) fn wrap_preview_text(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines = Vec::new();
    for raw in text.split('\n') {
        // Session text may contain invisible trailing whitespace. Rendering it with the selected
        // background creates apparently random highlighted cells, so trim it for display only.
        let raw = raw.trim_end();
        if raw.is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut current = String::new();
        let mut current_width = 0usize;
        let mut wrapped = false;
        for ch in raw.chars() {
            let ch_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(1);
            if current_width + ch_width > width && !current.is_empty() {
                lines.push(std::mem::take(&mut current).trim_end().to_owned());
                current_width = 0;
                wrapped = true;
            }
            if wrapped && current.is_empty() && ch.is_whitespace() {
                continue;
            }
            current.push(ch);
            current_width += ch_width;
            wrapped = false;
        }
        lines.push(current.trim_end().to_owned());
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// Count rendered lines for a preview message (role header + wrapped body + blank separator).
pub(super) fn preview_message_line_count(message: &PreviewMessage, wrap_width: usize) -> usize {
    1 + wrap_preview_text(&message.text, wrap_width).len() + 1
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Focus {
    Menu,
    Content,
    Providers,
    Models,
    SessionPreview,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Page {
    Home,
    Profiles,
    Sessions,
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
    pub(super) const ALL: [Self; 4] = [Self::Home, Self::Profiles, Self::Sessions, Self::Settings];

    pub(super) fn index(self) -> usize {
        match self {
            Self::Home => 0,
            Self::Profiles => 1,
            Self::Sessions => 2,
            Self::Settings => 3,
        }
    }

    pub(super) fn label(self, language: Language) -> &'static str {
        match self {
            Self::Home => language.pick("Home", "主页"),
            Self::Profiles => language.pick("Profiles", "配置"),
            Self::Sessions => language.pick("Sessions", "会话"),
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
    ConfirmDeleteSession {
        path: String,
        label: String,
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
        ratio_config_used: bool,
        overwrite: bool,
        existing: BTreeSet<String>,
        filter: String,
        filtering: bool,
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
    pub(super) session_cursor: usize,
    pub(super) sessions: Vec<SessionSummary>,
    pub(super) sessions_loaded: bool,
    pub(super) session_filter: String,
    pub(super) session_filtering: bool,
    pub(super) named_only: bool,
    pub(super) user_only_preview: bool,
    pub(super) preview: Option<Vec<PreviewMessage>>,
    pub(super) preview_scroll: u16,
    pub(super) preview_path: Option<String>,
    pub(super) preview_message_cursor: usize,
    pub(super) preview_wrap_width: usize,
    pub(super) preview_viewport_height: u16,
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
            session_cursor: 0,
            sessions: Vec::new(),
            sessions_loaded: false,
            session_filter: String::new(),
            session_filtering: false,
            named_only: false,
            user_only_preview: false,
            preview: None,
            preview_scroll: 0,
            preview_path: None,
            preview_message_cursor: 0,
            preview_wrap_width: 40,
            preview_viewport_height: 10,
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
                    mut models,
                    ambiguous,
                    unavailable,
                    ratio_prices,
                    ratio_config_used,
                } = fetched;
                // Auto-resolve ambiguities with the first candidate so the model
                // selection list shows every gateway model up front. Metadata is
                // editable per-model after import, and ratio_config prices overlay
                // on top regardless of which candidate was picked.
                for ambiguity in &ambiguous {
                    if let Some(first) = ambiguity.candidates.first() {
                        models.push(first.model.clone());
                    }
                }
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
                self.show_fetched(
                    provider_id,
                    models,
                    unavailable,
                    &ratio_prices,
                    ratio_config_used,
                );
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
        // Ctrl+C inside session preview copies the selected message instead of quitting.
        if self.in_session_preview()
            && key.modifiers.contains(KeyModifiers::CONTROL)
            && key.code == KeyCode::Char('c')
        {
            self.copy_selected_preview_message();
            return;
        }
        // Session preview has its own key handling, separate from the command dispatch.
        if self.in_session_preview() && self.overlay.is_none() && !self.session_filtering {
            self.on_session_preview_key(key);
            return;
        }
        if self.filtering || self.session_filtering {
            if command_for(key) == Some(Command::Quit) {
                self.quit = true;
                return;
            }
            if self.session_filtering {
                self.on_session_filter_key(key);
            } else {
                self.on_filter_key(key);
            }
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
                Command::Filter if self.in_sessions() => self.session_filtering = true,
                Command::New if self.in_profiles() => self.open_add(),
                Command::New if self.in_sessions() => {
                    self.named_only = !self.named_only;
                    self.session_cursor = 0;
                    self.clamp_session_selection();
                    self.refresh_preview();
                }
                Command::Edit if self.in_profiles() => self.open_edit(),
                Command::Delete if self.in_profiles() => self.open_delete(),
                Command::Delete if self.in_sessions() => self.open_delete_session(),
                Command::Copy if self.in_profiles() => self.duplicate_selected(),
                Command::Import if self.in_profiles() => self.start_fetch(),
                Command::SetDefault if self.in_profiles() && self.in_model_context() => {
                    self.set_selected_default()
                }
                Command::Backups => self.open_backups(),
                Command::Doctor => {
                    self.overlay = Some(Overlay::Doctor(documents::doctor(&self.paths)))
                }
                Command::Reload if self.in_sessions() => self.reload_sessions(Some(
                    self.language.pick("Reloaded sessions", "会话列表已刷新"),
                )),
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
                    if self.page == Page::Sessions {
                        self.ensure_sessions_loaded();
                    }
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

        if self.page == Page::Sessions {
            self.ensure_sessions_loaded();
            if self.focus == Focus::SessionPreview {
                self.on_session_preview_key(key);
                return;
            }
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => self.move_session_selection(-1),
                KeyCode::Down | KeyCode::Char('j') => self.move_session_selection(1),
                KeyCode::Right | KeyCode::Char('l') | KeyCode::Enter => {
                    self.focus_session_preview();
                }
                KeyCode::PageUp => {
                    self.preview_scroll = self.preview_scroll.saturating_sub(5);
                }
                KeyCode::PageDown => {
                    self.preview_scroll = self.preview_scroll.saturating_add(5);
                }
                KeyCode::Char('u') => {
                    self.user_only_preview = !self.user_only_preview;
                    self.refresh_preview();
                }
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

    pub(super) fn on_session_filter_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.session_filter.clear();
                self.session_filtering = false;
                self.session_cursor = 0;
                self.clamp_session_selection();
                self.refresh_preview();
            }
            KeyCode::Enter => {
                self.session_filtering = false;
                self.clamp_session_selection();
                self.refresh_preview();
            }
            KeyCode::Backspace => {
                self.session_filter.pop();
                self.session_cursor = 0;
                self.clamp_session_selection();
                self.refresh_preview();
            }
            KeyCode::Char(character) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.session_filter.push(character);
                self.session_cursor = 0;
                self.clamp_session_selection();
                self.refresh_preview();
            }
            _ => {}
        }
    }

    fn move_session_selection(&mut self, delta: isize) {
        let count = self.visible_sessions().len() as isize;
        if count == 0 {
            self.session_cursor = 0;
            self.preview = None;
            return;
        }
        let next = (self.session_cursor as isize + delta).clamp(0, count - 1);
        self.session_cursor = next as usize;
        self.refresh_preview();
    }

    pub(super) fn open_delete_session(&mut self) {
        let Some(session) = self.selected_session() else {
            self.notice(
                NoticeKind::Warning,
                self.language
                    .pick("Select a session to delete", "请选择要删除的会话"),
            );
            return;
        };
        let label = documents::session_display_title(session).to_owned();
        self.overlay = Some(Overlay::ConfirmDeleteSession {
            path: session.path.display().to_string(),
            label,
        });
    }

    pub(super) fn move_selection(&mut self, delta: isize) {
        if self.focus == Focus::Models || (self.width < COMPACT_WIDTH && self.narrow_detail) {
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
            && (self.focus == Focus::Models || (self.width < COMPACT_WIDTH && self.narrow_detail))
    }

    pub(super) fn in_profiles(&self) -> bool {
        self.page == Page::Profiles && self.focus != Focus::Menu
    }
}

/// Indices into `models` whose id contains the (case-insensitive) filter needle.
/// An empty needle returns every index. `cursor` and `selected` on the overlay
/// are kept in original-index space; only the rendered list is filtered, so
/// toggling the filter never drops a selection.
pub(super) fn visible_fetched_indices(models: &[CatalogModel], filter: &str) -> Vec<usize> {
    let needle = filter.to_lowercase();
    models
        .iter()
        .enumerate()
        .filter(|(_, model)| needle.is_empty() || model.id.to_lowercase().contains(&needle))
        .map(|(index, _)| index)
        .collect()
}
