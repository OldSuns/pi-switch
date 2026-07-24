use std::collections::{BTreeMap, BTreeSet};
use std::{sync::mpsc, thread};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::documents::{
    self, Backup, CatalogAmbiguity, CatalogFetch, CatalogModel, DoctorCheck, ImportSummary,
    OpenCodeImportPlan, Paths, PreviewMessage, ProviderView, RatioCost, SessionSummary, Snapshot,
};

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

    pub(super) fn visible_sessions(&self) -> Vec<usize> {
        self.sessions
            .iter()
            .enumerate()
            .filter(|(_, session)| {
                documents::session_matches(session, &self.session_filter, self.named_only)
            })
            .map(|(index, _)| index)
            .collect()
    }

    pub(super) fn selected_session(&self) -> Option<&SessionSummary> {
        let visible = self.visible_sessions();
        visible
            .get(self.session_cursor)
            .and_then(|index| self.sessions.get(*index))
    }

    pub(super) fn ensure_sessions_loaded(&mut self) {
        if self.sessions_loaded {
            return;
        }
        self.reload_sessions(None);
    }

    pub(super) fn reload_sessions(&mut self, message: Option<&str>) {
        let selected_path = self
            .selected_session()
            .map(|session| session.path.display().to_string());
        match documents::list_sessions() {
            Ok(sessions) => {
                self.sessions = sessions;
                self.sessions_loaded = true;
                self.session_cursor = selected_path
                    .and_then(|path| {
                        self.visible_sessions().into_iter().position(|index| {
                            self.sessions[index].path.display().to_string() == path
                        })
                    })
                    .unwrap_or(0);
                self.clamp_session_selection();
                self.refresh_preview();
                if let Some(message) = message {
                    self.notice(NoticeKind::Success, message);
                }
            }
            Err(error) => {
                self.sessions_loaded = true;
                self.overlay = Some(Overlay::Error(error.to_string()));
            }
        }
    }

    pub(super) fn clamp_session_selection(&mut self) {
        let count = self.visible_sessions().len();
        self.session_cursor = if count == 0 {
            0
        } else {
            self.session_cursor.min(count - 1)
        };
    }

    pub(super) fn refresh_preview(&mut self) {
        let Some(session) = self.selected_session() else {
            self.preview = None;
            self.preview_path = None;
            self.preview_scroll = 0;
            self.preview_message_cursor = 0;
            if self.focus == Focus::SessionPreview {
                self.focus = Focus::Content;
            }
            return;
        };
        let path = session.path.display().to_string();
        match documents::load_preview(&session.path, self.user_only_preview) {
            Ok(messages) => {
                self.preview = Some(messages);
                self.preview_path = Some(path);
                self.preview_scroll = 0;
                self.preview_message_cursor = 0;
            }
            Err(error) => {
                self.preview = None;
                self.preview_path = Some(path);
                self.preview_message_cursor = 0;
                if self.focus == Focus::SessionPreview {
                    self.focus = Focus::Content;
                }
                self.notice(NoticeKind::Warning, error.to_string());
            }
        }
    }

    pub(super) fn in_sessions(&self) -> bool {
        self.page == Page::Sessions && self.focus == Focus::Content
    }

    pub(super) fn in_session_list(&self) -> bool {
        self.page == Page::Sessions && self.focus == Focus::Content
    }

    pub(super) fn in_session_preview(&self) -> bool {
        self.page == Page::Sessions && self.focus == Focus::SessionPreview
    }

    pub(super) fn preview_message_count(&self) -> usize {
        self.preview
            .as_ref()
            .map(|messages| messages.len())
            .unwrap_or(0)
    }

    pub(super) fn selected_preview_message(&self) -> Option<&PreviewMessage> {
        let messages = self.preview.as_ref()?;
        messages.get(self.preview_message_cursor)
    }

    pub(super) fn clamp_preview_message_cursor(&mut self) {
        let count = self.preview_message_count();
        self.preview_message_cursor = if count == 0 {
            0
        } else {
            self.preview_message_cursor.min(count - 1)
        };
    }

    pub(super) fn ensure_preview_message_visible(&mut self) {
        let Some(messages) = self.preview.as_ref() else {
            self.preview_scroll = 0;
            return;
        };
        if messages.is_empty() {
            self.preview_scroll = 0;
            return;
        }
        let header_lines = self.preview_header_line_count();
        let wrap_width = self.preview_wrap_width.max(8);
        let mut offset = header_lines;
        for (index, message) in messages.iter().enumerate() {
            let block_lines = preview_message_line_count(message, wrap_width);
            if index == self.preview_message_cursor {
                let viewport = self.preview_viewport_height.max(1) as usize;
                let start = self.preview_scroll as usize;
                let end = start + viewport;
                let message_end = offset + block_lines;
                if offset < start {
                    self.preview_scroll = offset.min(u16::MAX as usize) as u16;
                } else if block_lines >= viewport {
                    if offset >= end {
                        self.preview_scroll = offset.min(u16::MAX as usize) as u16;
                    }
                } else if message_end > end {
                    self.preview_scroll =
                        message_end.saturating_sub(viewport).min(u16::MAX as usize) as u16;
                }
                return;
            }
            offset += block_lines;
        }
    }

    fn preview_header_line_count(&self) -> usize {
        let mut lines = 0;
        if self.selected_session().is_some() {
            lines += 1; // id
            if self
                .selected_session()
                .is_some_and(|session| !session.cwd.is_empty())
            {
                lines += 1;
            }
            lines += 1; // blank
        }
        lines
    }

    pub(super) fn focus_session_preview(&mut self) {
        if self.preview_message_count() == 0 {
            self.notice(
                NoticeKind::Warning,
                self.language
                    .pick("No messages to browse", "没有可浏览的消息"),
            );
            return;
        }
        self.focus = Focus::SessionPreview;
        self.clamp_preview_message_cursor();
        self.ensure_preview_message_visible();
    }

    pub(super) fn move_preview_message(&mut self, delta: isize) {
        let count = self.preview_message_count() as isize;
        if count == 0 {
            self.preview_message_cursor = 0;
            return;
        }
        let next = (self.preview_message_cursor as isize + delta).clamp(0, count - 1);
        self.preview_message_cursor = next as usize;
        self.ensure_preview_message_visible();
    }

    pub(super) fn scroll_preview_lines(&mut self, delta: isize) {
        let max_scroll = self
            .preview_total_line_count()
            .saturating_sub(self.preview_viewport_height.max(1) as usize);
        let current = self.preview_scroll as usize;
        let next = if delta < 0 {
            current.saturating_sub(delta.unsigned_abs())
        } else {
            current.saturating_add(delta as usize).min(max_scroll)
        };
        self.preview_scroll = next.min(u16::MAX as usize) as u16;
        if let Some(cursor) = self.preview_message_cursor_at_line(next) {
            self.preview_message_cursor = cursor;
        }
    }

    fn preview_total_line_count(&self) -> usize {
        let mut lines = self.preview_header_line_count();
        match self.preview.as_ref() {
            Some(messages) if !messages.is_empty() => {
                let wrap_width = self.preview_wrap_width.max(8);
                lines += messages
                    .iter()
                    .map(|message| preview_message_line_count(message, wrap_width))
                    .sum::<usize>();
            }
            _ => lines += 1,
        }
        lines
    }

    fn preview_message_cursor_at_line(&self, line: usize) -> Option<usize> {
        let messages = self.preview.as_ref()?;
        if messages.is_empty() {
            return None;
        }
        let wrap_width = self.preview_wrap_width.max(8);
        let mut offset = self.preview_header_line_count();
        for (index, message) in messages.iter().enumerate() {
            let block_lines = preview_message_line_count(message, wrap_width);
            if line < offset + block_lines {
                return Some(index);
            }
            offset += block_lines;
        }
        Some(messages.len() - 1)
    }

    pub(super) fn copy_selected_preview_message(&mut self) {
        let Some(message) = self.selected_preview_message().cloned() else {
            self.notice(
                NoticeKind::Warning,
                self.language
                    .pick("Select a message to copy", "请选择要复制的消息"),
            );
            return;
        };
        match copy_text_to_clipboard(&message.text) {
            Ok(()) => self.notice(
                NoticeKind::Success,
                self.language.pick("Copied message", "已复制消息"),
            ),
            Err(error) => self.overlay = Some(Overlay::Error(error)),
        }
    }

    pub(super) fn on_mouse(&mut self, mouse: crossterm::event::MouseEvent) {
        use crossterm::event::{MouseButton, MouseEventKind};
        if self.overlay.is_some() || self.filtering || self.session_filtering {
            return;
        }
        if self.page != Page::Sessions || self.focus == Focus::Menu {
            return;
        }
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                if self.in_session_preview() {
                    self.scroll_preview_lines(-3);
                } else if self.in_session_list() {
                    self.move_session_selection(-1);
                }
            }
            MouseEventKind::ScrollDown => {
                if self.in_session_preview() {
                    self.scroll_preview_lines(3);
                } else if self.in_session_list() {
                    self.move_session_selection(1);
                }
            }
            MouseEventKind::Down(MouseButton::Left) if self.in_session_list() => {
                // click stays on list; right key enters preview
            }
            _ => {}
        }
    }

    fn on_session_preview_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Up | KeyCode::Char('k') => self.move_preview_message(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_preview_message(1),
            KeyCode::PageUp => {
                self.move_preview_message(-5);
            }
            KeyCode::PageDown => {
                self.move_preview_message(5);
            }
            KeyCode::Left | KeyCode::Char('h') | KeyCode::Esc | KeyCode::Tab => {
                self.focus = Focus::Content;
            }
            KeyCode::Char('u') => {
                self.user_only_preview = !self.user_only_preview;
                self.refresh_preview();
            }
            KeyCode::Char('r') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.reload_sessions(Some(
                    self.language.pick("Reloaded sessions", "会话列表已刷新"),
                ));
                self.focus_session_preview();
            }
            _ => {}
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
            KeyCode::Tab | KeyCode::Down => form.select_field(form.field + 1),
            KeyCode::BackTab | KeyCode::Up => form.select_field((form.field + 7) % 8),
            KeyCode::Left if form.field == 2 => {
                form.api = (form.api + API_TYPES.len()) % (API_TYPES.len() + 1)
            }
            KeyCode::Right if form.field == 2 => form.api = (form.api + 1) % (API_TYPES.len() + 1),
            KeyCode::Left | KeyCode::Right if form.field == 4 => {
                form.auth_header = !form.auth_header
            }
            KeyCode::Left | KeyCode::Right if form.field == 6 => {
                form.send_session_affinity_headers = !form.send_session_affinity_headers
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
            _ => {
                let mut cursor = form.cursor;
                edit_text_key(form.current_text_mut(), &mut cursor, key);
                form.cursor = cursor;
            }
        }
        false
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

    fn select_page(&mut self, page: Page) {
        self.page = page;
        self.filtering = false;
        self.session_filtering = false;
        self.narrow_detail = false;
        self.settings_cursor = 0;
        if page == Page::Sessions {
            self.ensure_sessions_loaded();
        }
    }

    fn focus_models(&mut self) {
        if self.selected_provider().is_some() {
            self.focus = Focus::Models;
            self.narrow_detail = self.width < COMPACT_WIDTH;
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
            let result = documents::fetch_models(provider, options).map(|fetched| {
                BackgroundResult::Catalog {
                    provider_id: task_provider_id,
                    fetched,
                }
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

    fn show_fetched(
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

    fn open_opencode_providers(&mut self) {
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

    fn start_opencode_import(&mut self, providers: Vec<String>) {
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
