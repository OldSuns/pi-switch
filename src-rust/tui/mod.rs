mod app;
mod forms;
mod i18n;
mod input;
mod keys;
mod terminal;
mod ui;

use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyEventKind};

use crate::documents::Paths;

use app::App;
use terminal::{terminal_error, PanicRestoreHookGuard, TuiTerminal};
use ui::draw;

#[cfg(test)]
use crate::documents::{ModelView, ProviderView, Snapshot};
#[cfg(test)]
use app::{Focus, Overlay, Page};
#[cfg(test)]
use forms::{FormState, ModelDefaultsFormState, ModelFormState};
#[cfg(test)]
use input::{truncate_width, wrap_width};
#[cfg(test)]
use keys::{command_for, Command};

const TICK: Duration = Duration::from_millis(120);
const WIDE_WIDTH: u16 = 76;
const API_TYPES: [&str; 4] = [
    "openai-completions",
    "openai-responses",
    "anthropic-messages",
    "google-generative-ai",
];

pub fn run() -> Result<(), String> {
    let _panic_guard = PanicRestoreHookGuard::install();
    let paths = Paths::discover().map_err(|error| error.to_string())?;
    let mut terminal = TuiTerminal::new()?;
    let mut app = App::new(paths);
    let mut last_tick = Instant::now();

    while !app.quit {
        terminal.draw(|frame| draw(frame, &mut app))?;
        let timeout = TICK.saturating_sub(last_tick.elapsed());
        if event::poll(timeout).map_err(terminal_error)? {
            if let Event::Key(key) = event::read().map_err(terminal_error)? {
                if key.kind != KeyEventKind::Release {
                    app.on_key(key);
                }
            }
        }
        if last_tick.elapsed() >= TICK {
            app.tick();
            last_tick = Instant::now();
        }
    }
    terminal.restore_best_effort()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::{backend::TestBackend, Terminal};
    use serde_json::json;
    use std::{
        env, fs, process,
        time::{SystemTime, UNIX_EPOCH},
    };
    use unicode_width::UnicodeWidthStr;

    fn app() -> (std::path::PathBuf, App) {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = env::temp_dir().join(format!("pi-switch-ui-{}-{stamp}", process::id()));
        let paths = Paths::from_home(&root);
        let model = |id: &str| ModelView {
            id: id.into(),
            name: Some(format!("{id} display")),
            api: None,
            reasoning: false,
            input: vec!["text".into()],
            context_window: Some(128_000),
            max_tokens: Some(16_384),
        };
        let snapshot = Snapshot {
            models_path: paths.models.display().to_string(),
            settings_path: paths.settings.display().to_string(),
            providers: vec![ProviderView {
                id: "示例-provider".into(),
                base_url: "https://example.test/v1".into(),
                api: "openai-completions".into(),
                api_key: "$EXAMPLE_KEY".into(),
                auth_header: true,
                models: vec![model("model-a"), model("模型-b")],
                raw: json!({}),
            }],
            default_provider: Some("示例-provider".into()),
            default_model: Some("model-a".into()),
            language: "en".into(),
            fetch_model_metadata: true,
            model_defaults: Default::default(),
        };
        (root, App::from_snapshot(paths, snapshot))
    }

    #[test]
    fn responsive_layout_renders_wide_and_narrow_with_cjk() {
        let (root, mut app) = app();
        for (width, height) in [(120, 36), (80, 24), (64, 20)] {
            let backend = TestBackend::new(width, height);
            let mut terminal = Terminal::new(backend).unwrap();

            app.page = Page::Home;
            app.focus = Focus::Menu;
            terminal.draw(|frame| draw(frame, &mut app)).unwrap();
            let menu = terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>();
            assert!(menu.contains("pi-switch"));
            assert!(menu.contains("Home"));
            assert!(menu.contains("Profiles"));
            assert!(menu.contains("Settings"));
            if width == 120 {
                assert!(menu.contains("models.json"));
                assert!(menu.contains("settings.json"));
            }

            app.page = Page::Profiles;
            app.focus = Focus::Providers;
            terminal.draw(|frame| draw(frame, &mut app)).unwrap();
            let profiles = terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>();
            assert!(profiles.contains("Providers"));
            assert!(profiles.contains("openai-completions"));
            assert!(profiles.contains("new"));

            app.page = Page::Settings;
            app.focus = Focus::Content;
            terminal.draw(|frame| draw(frame, &mut app)).unwrap();
            let settings = terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>();
            assert!(settings.contains("Configuration"));
            assert!(settings.contains("Actions"));
            if height >= 24 {
                assert!(settings.contains("Fetch model metadata from pi.dev"));
                assert!(!settings.contains("Default model parameters"));
                assert!(settings.contains("Import from OpenCode"));
            }
        }
        assert_eq!(truncate_width("示例-provider", 8), "示例-...");
        assert_eq!(UnicodeWidthStr::width("示例-provider"), 13);
        let wrapped = wrap_width("示例-provider", 6);
        assert_eq!(wrapped.concat(), "示例-provider");
        assert!(wrapped
            .iter()
            .all(|line| UnicodeWidthStr::width(line.as_str()) <= 6));

        app.overlay = Some(Overlay::Loading {
            message: "loading".into(),
        });
        app.on_overlay_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(matches!(app.overlay, Some(Overlay::Loading { .. })));

        app.overlay = None;
        app.language = super::i18n::Language::Chinese;
        app.page = Page::Settings;
        app.focus = Focus::Content;
        let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let chinese = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        let chinese = chinese.replace(' ', "");
        assert!(chinese.contains("设置"));
        assert!(chinese.contains("语言:中文"));
        assert!(chinese.contains("从pi.dev获取模型信息"));
        assert!(!chinese.contains("默认模型参数"));
        assert!(chinese.contains("从OpenCode导入"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn settings_show_model_defaults_only_when_metadata_fetch_is_off() {
        let (root, mut app) = app();
        app.page = Page::Settings;
        app.focus = Focus::Content;
        let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();

        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let enabled = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(!enabled.contains("Default model parameters"));
        assert!(enabled.contains("●  Fetch model metadata from pi.dev"));

        app.snapshot.fetch_model_metadata = false;
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let disabled = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(disabled.contains("Default model parameters"));
        assert!(disabled.contains("○  Fetch model metadata from pi.dev"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn model_defaults_dialog_keeps_its_left_border() {
        let (root, mut app) = app();
        app.overlay = Some(Overlay::ModelDefaultsForm(ModelDefaultsFormState::new(
            &app.snapshot.model_defaults,
        )));
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();

        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let buffer = terminal.backend().buffer();
        let symbols = buffer.content();
        let symbol_at = |x: u16, y: u16| symbols[(y * 80 + x) as usize].symbol();
        assert_eq!(symbol_at(1, 2), "┌");
        for y in 3..21 {
            assert_eq!(symbol_at(1, y), "│", "missing left border at row {y}");
        }
        assert_eq!(symbol_at(1, 21), "└");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn profiles_use_the_actual_layout_width_for_responsive_breakpoints() {
        let (root, mut app) = app();
        app.page = Page::Profiles;
        app.focus = Focus::Providers;
        for (width, menu_visible, detail_visible) in
            [(94, true, true), (80, false, true), (75, false, false)]
        {
            let mut terminal = Terminal::new(TestBackend::new(width, 24)).unwrap();
            terminal.draw(|frame| draw(frame, &mut app)).unwrap();
            let content = terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>();

            assert_eq!(content.contains("Home"), menu_visible);
            assert_eq!(content.contains("Base URL"), detail_visible);
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn contextual_actions_open_provider_and_model_crud() {
        let (root, mut app) = app();
        app.page = Page::Profiles;
        app.focus = Focus::Providers;
        app.on_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        assert!(matches!(app.overlay, Some(Overlay::Form(_))));

        app.overlay = None;
        app.focus = Focus::Models;
        app.on_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        assert!(matches!(
            app.overlay,
            Some(Overlay::ModelForm(ref form))
                if form.context_window.is_empty() && form.max_tokens.is_empty()
        ));

        app.overlay = None;
        app.on_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
        match app.overlay.as_ref() {
            Some(Overlay::ModelForm(form)) => {
                assert_eq!(form.id, "model-a");
                assert_eq!(form.name, "model-a display");
                assert_eq!(form.context_window, "128000");
            }
            _ => panic!("model edit did not open"),
        }

        app.overlay = None;
        app.on_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));
        assert!(matches!(
            app.overlay,
            Some(Overlay::ConfirmDeleteModel { ref model_id, .. }) if model_id == "model-a"
        ));

        app.overlay = None;
        app.on_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE));
        assert!(matches!(
            app.overlay,
            Some(Overlay::ModelForm(ref form)) if form.previous_id.is_none() && form.id == "model-a-copy"
        ));

        let mut provider_form = FormState::add();
        provider_form.headers_json = r#"{"x-api-key":"$KEY"}"#.into();
        provider_form.compat_json = r#"{"supportsDeveloperRole":false}"#.into();
        let draft = provider_form.draft().unwrap();
        assert_eq!(draft.headers.unwrap()["x-api-key"], "$KEY");
        assert_eq!(draft.compat.unwrap()["supportsDeveloperRole"], false);
        provider_form.headers_json = "[]".into();
        assert!(provider_form.draft().is_err());

        app.overlay = Some(Overlay::Form(FormState::add()));
        let mut terminal = Terminal::new(TestBackend::new(64, 20)).unwrap();
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let provider_content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(provider_content.contains("Headers JSON"));

        let model = app.snapshot.providers[0].models[0].clone();
        app.overlay = Some(Overlay::ModelForm(ModelFormState::edit(
            "示例-provider",
            &model,
        )));
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let model_content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(model_content.contains("Context window"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn menu_routes_pages_and_scopes_profile_commands() {
        let (root, mut app) = app();
        app.on_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        assert!(app.overlay.is_none());

        app.narrow_detail = true;
        app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert!(app.page == Page::Profiles);
        assert!(!app.narrow_detail);
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(app.focus == Focus::Providers);
        app.width = 75;
        app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        assert!(app.focus == Focus::Models);
        assert!(app.narrow_detail);
        app.on_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        assert!(app.focus == Focus::Providers);
        assert!(!app.narrow_detail);
        app.on_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        assert!(matches!(app.overlay, Some(Overlay::Form(_))));

        app.overlay = None;
        app.on_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        assert!(app.focus == Focus::Menu);
        app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert!(app.page == Page::Settings);
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(app.focus == Focus::Content);
        app.on_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
        assert!(matches!(app.overlay, Some(Overlay::Doctor(_))));

        app.overlay = None;
        app.settings_cursor = 0;
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.language, super::i18n::Language::Chinese);

        app.settings_cursor = 1;
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(!app.snapshot.fetch_model_metadata);
        app.settings_cursor = 2;
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(matches!(app.overlay, Some(Overlay::ModelDefaultsForm(_))));

        app.overlay = None;
        fs::create_dir_all(app.paths.opencode.parent().unwrap()).unwrap();
        fs::write(
            &app.paths.opencode,
            r#"{"provider":{"first":{"npm":"@ai-sdk/openai-compatible"},"second":{"npm":"@ai-sdk/openai-compatible"}}}"#,
        )
        .unwrap();
        app.settings_cursor = 6;
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(matches!(
            app.overlay,
            Some(Overlay::OpenCodeProviders {
                ref providers,
                ref selected,
                ..
            }) if providers.len() == 2 && selected.len() == 2
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn shortcuts_use_the_same_intuitive_commands_shown_in_the_ui() {
        assert!(matches!(
            command_for(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE)),
            Some(Command::New)
        ));
        assert!(matches!(
            command_for(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE)),
            Some(Command::SetDefault)
        ));
        assert!(matches!(
            command_for(KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE)),
            Some(Command::Delete)
        ));
        assert!(command_for(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)).is_none());
        assert!(command_for(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE)).is_none());
    }
}
