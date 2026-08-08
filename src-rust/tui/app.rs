use std::collections::{BTreeMap, BTreeSet};
use std::{sync::mpsc, thread};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::documents::{
    self, Backup, CatalogAmbiguity, CatalogFetch, CatalogModel, DoctorCheck, ImportSummary,
    OpenCodeImportPlan, Paths, PreviewMessage, ProviderView, RatioCost, SessionPreview,
    SessionSummary, Snapshot,
};

#[path = "app/actions.rs"]
mod actions;
#[path = "app/clipboard.rs"]
mod clipboard;
#[path = "app/forms.rs"]
mod form_keys;
#[path = "app/navigation.rs"]
mod navigation;
#[path = "app/overlay.rs"]
mod overlay;
#[path = "app/sessions.rs"]
mod sessions;
#[path = "app/tasks.rs"]
mod tasks;

use clipboard::copy_text_to_clipboard;

use super::{
    forms::{
        FormState, ModelDefaultsFormState, ModelFormState, MAX_TOKENS_FIELDS, PRESETS,
        THINKING_FORMATS,
    },
    i18n::Language,
    input::{char_len, cycle_tristate, edit_text_key, insert_char},
    markdown::PreviewLayout,
    API_TYPES, COMPACT_WIDTH,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Focus {
    Menu,
    Content,
    Providers,
    Models,
    SessionPreview,
}

#[derive(Clone, Debug)]
pub(in crate::tui) struct SessionGroup {
    pub(in crate::tui) cwd: String,
    /// Indices into `App::sessions` belonging to this group.
    pub(in crate::tui) sessions: Vec<usize>,
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
    AutoCheckUpdates,
    CheckUpdateNow,
}

impl SettingsAction {
    pub(super) const ALL: [Self; 9] = [
        Self::Language,
        Self::FetchMetadata,
        Self::ModelDefaults,
        Self::Reload,
        Self::Doctor,
        Self::Backups,
        Self::ImportOpenCode,
        Self::AutoCheckUpdates,
        Self::CheckUpdateNow,
    ];

    pub(super) fn visible(fetch_metadata: bool) -> impl Iterator<Item = Self> {
        Self::ALL
            .into_iter()
            .filter(move |action| !fetch_metadata || *action != Self::ModelDefaults)
    }

    /// Whether this action renders as a toggle with an on/off indicator dot.
    pub(super) fn is_toggle(self) -> bool {
        matches!(self, Self::FetchMetadata | Self::AutoCheckUpdates)
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
            Self::CheckUpdateNow => language.pick("Check for updates now", "检查更新").into(),
            Self::AutoCheckUpdates => language
                .pick("Automatically check for updates", "自动检查更新")
                .into(),
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
    Warning(String),
    Form(FormState),
    ModelForm(ModelFormState),
    ModelDefaultsForm(ModelDefaultsFormState),
    ConfirmDeleteProvider {
        id: String,
        in_pi: bool,
    },
    ConfirmRemoveProviderFromPi(String),
    ConfirmSaveProviderWithoutPi {
        form: FormState,
        draft: documents::ProviderDraft,
    },
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
    ConfirmUpdate {
        latest: String,
    },
}

pub(super) enum CatalogContinuation {
    OpenCode {
        plan: OpenCodeImportPlan,
        candidate_indices: Vec<usize>,
    },
    ProviderImport {
        provider_id: String,
        resolved_models: Vec<CatalogModel>,
        candidate_indices: Vec<usize>,
        overwrite: bool,
    },
}

enum BackgroundResult {
    ModelIds {
        provider_id: String,
        ids: Vec<String>,
    },
    Catalog {
        provider_id: String,
        fetched: CatalogFetch,
        overwrite: bool,
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
    pub(super) preview: Option<SessionPreview>,
    pub(super) preview_visible: Vec<usize>,
    pub(super) preview_collapsed: BTreeSet<String>,
    pub(super) preview_child_history: BTreeMap<String, String>,
    pub(super) preview_layout: Option<PreviewLayout>,
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
    pub(super) update_available: Option<String>,
    update_check: Option<mpsc::Receiver<documents::Result<Option<String>>>>,
    update_check_manual: bool,
    install_task: Option<mpsc::Receiver<documents::Result<()>>>,
    dismissed_update: Option<String>,
    pub(super) tick_count: usize,
    pub(super) quit: bool,
}

impl App {
    pub(super) fn new(paths: Paths) -> Self {
        match documents::load_snapshot(&paths) {
            Ok(snapshot) => {
                let warning = snapshot.warning.clone();
                let check_updates = snapshot.check_updates;
                let mut app = Self::from_snapshot(paths.clone(), snapshot);
                if let Some(warning) = warning {
                    app.overlay = Some(Overlay::Warning(warning));
                }
                app.dismissed_update = documents::read_dismissed_update(&paths.update);
                app.spawn_update_check(&paths, check_updates);
                app
            }
            Err(error) => {
                let snapshot = Snapshot {
                    providers_path: paths.providers.display().to_string(),
                    models_path: paths.models.display().to_string(),
                    settings_path: paths.settings.display().to_string(),
                    providers: Vec::new(),
                    default_provider: None,
                    default_model: None,
                    language: "en".into(),
                    fetch_model_metadata: true,
                    check_updates: true,
                    model_defaults: Default::default(),
                    warning: None,
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
            preview_visible: Vec::new(),
            preview_collapsed: BTreeSet::new(),
            preview_child_history: BTreeMap::new(),
            preview_layout: None,
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
            update_available: None,
            update_check: None,
            update_check_manual: false,
            install_task: None,
            dismissed_update: None,
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
