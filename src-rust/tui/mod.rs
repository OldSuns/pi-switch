mod app;
mod forms;
mod i18n;
mod input;
mod keys;
mod terminal;
mod ui;

use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyEventKind};

use crate::documents::{Paths, API_TYPES};

use app::App;
use terminal::{terminal_error, PanicRestoreHookGuard, TuiTerminal};
use ui::draw;

#[cfg(test)]
use crate::documents::{Backup, CatalogModel, ModelView, ProviderView, Snapshot};
#[cfg(test)]
use app::{Focus, Overlay, Page};
#[cfg(test)]
use forms::{FormState, ModelDefaultsFormState, ModelFormState};
#[cfg(test)]
use input::{edit_text_key, mask_secret, truncate_width, wrap_width};
#[cfg(test)]
use keys::{command_for, Command};

const TICK: Duration = Duration::from_millis(120);
const COMPACT_WIDTH: u16 = 48;
const WIDE_WIDTH: u16 = 76;
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
        env, fs, process, thread,
        time::{Duration, SystemTime, UNIX_EPOCH},
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

    fn catalog_model(
        id: &str,
        context_window: u64,
        max_tokens: u64,
        input_cost: f64,
    ) -> CatalogModel {
        CatalogModel {
            id: id.into(),
            config: json!({
                "id": id,
                "name": id,
                "reasoning": false,
                "input": ["text"],
                "cost": {
                    "input": input_cost,
                    "output": input_cost * 2.0,
                    "cacheRead": 0.0,
                    "cacheWrite": 0.0
                },
                "contextWindow": context_window,
                "maxTokens": max_tokens
            }),
        }
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
            assert!(settings.contains("Enter/Space run"));
            if height >= 24 {
                assert!(settings.contains("Fetch model metadata from models.dev"));
                assert!(!settings.contains("Default model parameters"));
                assert!(settings.contains("Import from OpenCode"));
            }
        }
        assert_eq!(truncate_width("示例-provider", 8), "示例-...");
        assert_eq!(mask_secret("sk-1234567890abcdef"), "sk-1...cdef");
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
        assert!(chinese.contains("从models.dev获取模型信息"));
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
        assert!(enabled.contains("●  Fetch model metadata from models.dev"));

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
        assert!(disabled.contains("○  Fetch model metadata from models.dev"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn model_defaults_dialog_keeps_its_left_border() {
        let (root, mut app) = app();
        app.language = super::i18n::Language::Chinese;
        app.page = Page::Settings;
        app.focus = Focus::Content;
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        app.overlay = Some(Overlay::ModelDefaultsForm(ModelDefaultsFormState::new(
            &app.snapshot.model_defaults,
        )));

        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let buffer = terminal.backend().buffer();
        let symbols = buffer.content();
        let symbol_at = |x: u16, y: u16| symbols[(y * 80 + x) as usize].symbol();
        assert_eq!(symbol_at(19, 4), "┌");
        for y in 5..18 {
            assert_eq!(symbol_at(19, y), "│", "missing left border at row {y}");
        }
        assert_eq!(symbol_at(19, 18), "└");
        let content = symbols.iter().map(|cell| cell.symbol()).collect::<String>();
        assert!(content.contains("128000"));
        assert!(content.contains("16384"));
        if let Some(Overlay::ModelDefaultsForm(form)) = app.overlay.as_mut() {
            form.context_window = "256000".into();
            form.field = 1;
        }
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("256000"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn backups_open_restore_confirmation_with_space() {
        let (root, mut app) = app();
        app.overlay = Some(Overlay::Backups {
            items: vec![Backup {
                path: root
                    .join("backup-2026-07-23_14-36-08-527.json")
                    .display()
                    .to_string(),
                name: "backup-2026-07-23_14-36-08-527.json".into(),
            }],
            selected: 0,
        });
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("backup-2026-07-23_14-36-08-527.json"));
        assert!(content.contains("Enter/Space"));
        app.on_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        assert!(matches!(app.overlay, Some(Overlay::ConfirmRestore(_))));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn chinese_provider_form_values_share_one_column() {
        let (root, mut app) = app();
        app.language = super::i18n::Language::Chinese;
        let mut form = FormState::add();
        form.id = "#".into();
        form.base_url = "@".into();
        form.headers_json = "H".into();
        form.compat_json = "%".into();
        form.field = 2;
        app.overlay = Some(Overlay::Form(form));
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let symbols = terminal.backend().buffer().content();
        let value_x = |row: u16, value: &str| {
            (0..80)
                .find(|x| symbols[(row * 80 + x) as usize].symbol() == value)
                .unwrap()
        };
        let columns = [
            value_x(2, "#"),
            value_x(4, "@"),
            value_x(12, "<"),
            value_x(14, "<"),
            value_x(16, "%"),
        ];
        assert!(
            columns.iter().all(|column| *column == columns[0]),
            "{columns:?}"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn profiles_use_the_actual_layout_width_for_responsive_breakpoints() {
        let (root, mut app) = app();
        app.page = Page::Profiles;
        app.focus = Focus::Providers;
        for (width, menu_visible, detail_visible) in [
            (94, true, true),
            (80, false, true),
            (48, false, true),
            (47, false, false),
        ] {
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
    fn compact_profiles_wrap_detail_values_and_model_metadata() {
        let (root, mut app) = app();
        app.page = Page::Profiles;
        app.focus = Focus::Providers;
        let mut terminal = Terminal::new(TestBackend::new(48, 24)).unwrap();
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let rows = terminal
            .backend()
            .buffer()
            .content()
            .chunks(48)
            .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
            .collect::<Vec<_>>();
        let base_url_row = rows
            .iter()
            .position(|row| row.contains("Base URL"))
            .unwrap();

        assert!(rows.iter().any(|row| row.contains("https://example.tes")));
        assert!(rows.iter().any(|row| row.contains("t/v1")));
        assert_eq!(rows[base_url_row + 1].chars().nth(29), Some('t'));
        assert!(rows.iter().any(|row| row.contains("model-a display")));
        assert!(rows.iter().any(|row| row.contains("ctx 128k")));
        assert!(!rows
            .iter()
            .any(|row| row.contains("https://example.test/v1...")));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn provider_session_affinity_compat_is_first_class_and_preserves_other_keys() {
        let mut form = FormState::add();
        form.compat_json = r#"{"supportsDeveloperRole":false}"#.into();

        let compat = form.draft().unwrap().compat.unwrap();
        assert_eq!(compat["sendSessionAffinityHeaders"], true);
        assert_eq!(compat["supportsDeveloperRole"], false);

        form.send_session_affinity_headers = true;
        let compat = form.draft().unwrap().compat.unwrap();
        assert_eq!(compat["sendSessionAffinityHeaders"], true);
        assert_eq!(compat["supportsDeveloperRole"], false);

        form.send_session_affinity_headers = false;
        let compat = form.draft().unwrap().compat.unwrap();
        assert_eq!(compat["sendSessionAffinityHeaders"], false);
        assert_eq!(compat["supportsDeveloperRole"], false);

        form.compat_json =
            r#"{"sendSessionAffinityHeaders":true,"supportsDeveloperRole":false}"#.into();
        assert!(form
            .draft()
            .unwrap_err()
            .to_string()
            .contains("managed by Session affinity"));
        form.compat_json = r#"{"supportsDeveloperRole":false}"#.into();

        let mut provider = ProviderView {
            id: "custom".into(),
            base_url: "https://example.test/v1".into(),
            api: "openai-completions".into(),
            api_key: "$KEY".into(),
            auth_header: true,
            models: Vec::new(),
            raw: json!({
                "compat": {
                    "sendSessionAffinityHeaders": true,
                    "supportsDeveloperRole": false
                }
            }),
        };
        let edited = FormState::edit(&provider);
        assert!(edited.send_session_affinity_headers);
        assert_eq!(edited.compat_json, r#"{"supportsDeveloperRole":false}"#);

        provider.raw = json!({"compat":{"supportsDeveloperRole":false}});
        let edited = FormState::edit(&provider);
        assert!(edited.send_session_affinity_headers);
        assert_eq!(edited.compat_json, r#"{"supportsDeveloperRole":false}"#);

        let (root, mut app) = app();
        let mut form = FormState::add();
        form.field = 6;
        assert!(form.send_session_affinity_headers);
        app.on_form_key(&mut form, KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        assert!(!form.send_session_affinity_headers);
        app.on_form_key(&mut form, KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        assert!(form.send_session_affinity_headers);
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
        provider_form.user_agent = "  claude-cli/2.1.161  ".into();
        provider_form.headers_json = r#"{"user-agent":"old","x-api-key":"$KEY"}"#.into();
        provider_form.compat_json = r#"{"supportsDeveloperRole":false}"#.into();
        let draft = provider_form.draft().unwrap();
        let headers = draft.headers.unwrap();
        assert_eq!(headers["User-Agent"], "claude-cli/2.1.161");
        assert!(headers.get("user-agent").is_none());
        assert_eq!(headers["x-api-key"], "$KEY");
        assert_eq!(draft.compat.unwrap()["supportsDeveloperRole"], false);
        provider_form.user_agent.clear();
        let headers = provider_form.draft().unwrap().headers.unwrap();
        assert!(headers.get("User-Agent").is_none());
        provider_form.headers_json = "[]".into();
        assert!(provider_form.draft().is_err());

        let mut provider = app.snapshot.providers[0].clone();
        provider.raw = json!({
            "headers": {
                "user-agent": "legacy-agent",
                "x-team": "team"
            }
        });
        let edited = FormState::edit(&provider);
        assert_eq!(edited.user_agent, "legacy-agent");
        assert_eq!(edited.headers_json, r#"{"x-team":"team"}"#);

        let mut configured_form = FormState::add();
        configured_form.user_agent = "claude-cli/2.1.161".into();
        configured_form.headers_json = r#"{"x-api-key":"$KEY"}"#.into();
        app.overlay = Some(Overlay::Form(configured_form));
        let mut terminal = Terminal::new(TestBackend::new(64, 20)).unwrap();
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let provider_content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(provider_content.contains("Headers"));
        assert!(!provider_content.contains("(all models)"));
        assert!(provider_content.contains("User-Agent"));
        assert!(provider_content.contains("x-api-key"));

        let mut invalid_form = FormState::add();
        invalid_form.headers_json = "[".into();
        app.overlay = Some(Overlay::Form(invalid_form));
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let invalid_content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(invalid_content.contains("invalid JSON"));

        if let Some(Overlay::Form(form)) = app.overlay.as_mut() {
            *form = FormState::add();
            form.user_agent = "claude-cli/2.1.161".into();
            form.headers_json = r#"{"x-api-key":"$KEY"}"#.into();
            form.field = 5;
        }
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(matches!(
            app.overlay,
            Some(Overlay::Form(ref form)) if form.editing_headers
        ));
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(matches!(
            app.overlay,
            Some(Overlay::Form(ref form)) if form.editing_headers && form.user_agent == "claude-cli/2.1.161"
        ));
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let headers_content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(headers_content.contains("Provider headers"));
        assert!(!headers_content.contains("all models"));
        assert!(headers_content.contains("User-Agent"));
        assert!(headers_content.contains("Other headers JSON"));
        assert!(headers_content.contains("Ctrl+S"));
        assert!(headers_content.contains("Tab"));
        assert!(headers_content.contains("Esc"));
        assert!(!headers_content.contains("User-Agent is separate"));
        app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let json_content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(json_content.contains("Other headers JSON"));
        assert!(json_content.contains("$KEY"));
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        app.on_key(KeyEvent::new(KeyCode::Char('{'), KeyModifiers::NONE));
        app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(matches!(
            app.overlay,
            Some(Overlay::Form(ref form))
                if !form.editing_headers
                    && form.headers_json.contains("x-api-key")
                    && form.headers_json.ends_with("\n{")
                    && form.headers_field == 1
        ));

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
    fn provider_form_question_mark_help_and_api_key_masking() {
        let (root, mut app) = app();
        app.language = super::i18n::Language::Chinese;
        let mut form = FormState::add();
        form.api_key = "sk-1234567890abcdef".into();
        form.field = 4;
        app.overlay = Some(Overlay::Form(form));
        app.on_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
        if let Some(Overlay::Form(ref form)) = app.overlay {
            assert!(form.show_help, "show_help should be true after '?'");
        }
        let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let help_content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        let help_content = help_content.replace(' ', "");
        assert!(help_content.contains("仅自定义"));
        app.on_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
        if let Some(Overlay::Form(ref form)) = app.overlay {
            assert!(!form.show_help);
        }
        if let Some(Overlay::Form(ref mut form)) = app.overlay.as_mut() {
            form.field = 3;
        }
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let masked = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(masked.contains("1234567890abcdef"));
        assert!(!masked.contains("sk-1...cdef"));
        if let Some(Overlay::Form(ref mut form)) = app.overlay.as_mut() {
            form.field = 0;
        }
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let plain = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(plain.contains("sk-1...cdef"));
        assert!(!plain.contains("1234567890abcdef"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn model_limits_are_compact_in_lists_but_raw_in_the_editor() {
        let (root, mut app) = app();
        app.snapshot.providers[0].models[0].context_window = Some(128_000);
        app.snapshot.providers[0].models[0].max_tokens = Some(1_048_576);
        app.page = Page::Profiles;
        app.focus = Focus::Providers;
        let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let profiles = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(profiles.contains("ctx 128k  max 1M"));

        let model = app.snapshot.providers[0].models[0].clone();
        app.overlay = Some(Overlay::ModelForm(ModelFormState::edit(
            "示例-provider",
            &model,
        )));
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let editor = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(editor.contains("128000"));
        assert!(editor.contains("1048576"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ambiguous_models_auto_resolve_into_the_fetched_list() {
        // Ambiguities used to gate the model list behind a metadata-source
        // picker; they now auto-resolve to the first candidate so the user
        // sees every gateway model up front in the selection list.
        let (root, mut app) = app();
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        app.overlay = Some(Overlay::Fetched {
            provider_id: "custom".into(),
            models: vec![
                catalog_model("resolved", 128_000, 16_384, 1.0),
                catalog_model("shared", 1_048_576, 128_000, 2.0),
            ],
            unavailable: 0,
            selected: std::collections::BTreeSet::new(),
            cursor: 0,
            ratio_config_used: true,
            overwrite: false,
            existing: std::collections::BTreeSet::new(),
            filter: String::new(),
            filtering: false,
        });
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("resolved"));
        assert!(content.contains("shared"));
        assert!(content.contains("ratio_config"));
        // The auto-resolved model is selectable like any other.
        app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        app.on_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        if let Some(Overlay::Fetched { ref selected, .. }) = app.overlay {
            assert_eq!(selected.len(), 1);
        }
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
        app.width = 47;
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
        app.on_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        assert_eq!(app.language, super::i18n::Language::Chinese);

        app.settings_cursor = 1;
        app.on_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        assert!(!app.snapshot.fetch_model_metadata);
        app.settings_cursor = 2;
        app.on_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        assert!(matches!(app.overlay, Some(Overlay::ModelDefaultsForm(_))));

        app.overlay = None;
        fs::create_dir_all(app.paths.opencode.parent().unwrap()).unwrap();
        fs::write(
            &app.paths.opencode,
            r#"{"provider":{"first":{"npm":"@ai-sdk/openai-compatible"},"second":{"npm":"@ai-sdk/openai-compatible"}}}"#,
        )
        .unwrap();
        app.settings_cursor = 6;
        app.on_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        assert!(matches!(
            app.overlay,
            Some(Overlay::OpenCodeProviders {
                ref providers,
                ref selected,
                ..
            }) if providers.len() == 2 && selected.is_empty()
        ));
        app.notice = None;
        app.on_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(matches!(app.overlay, Some(Overlay::Loading { .. })));
        let mut imported = false;
        for _ in 0..100 {
            app.tick();
            imported |= app.notice.is_some();
            if imported && app.overlay.is_none() {
                break;
            }
            thread::sleep(Duration::from_millis(1));
        }
        assert!(imported, "OpenCode import did not finish");
        assert!(
            app.overlay.is_none(),
            "loading overlay remained after import"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn text_editing_keeps_unicode_cursor_boundaries() {
        let mut value = "中a".to_owned();
        let mut cursor = 2;

        edit_text_key(
            &mut value,
            &mut cursor,
            KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
        );
        edit_text_key(
            &mut value,
            &mut cursor,
            KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE),
        );
        edit_text_key(
            &mut value,
            &mut cursor,
            KeyEvent::new(KeyCode::Home, KeyModifiers::NONE),
        );
        edit_text_key(
            &mut value,
            &mut cursor,
            KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE),
        );

        assert_eq!((value.as_str(), cursor), ("b", 0));
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

    #[test]
    fn profile_footer_shows_only_the_current_left_action() {
        let (root, mut app) = app();
        app.page = Page::Profiles;
        app.focus = Focus::Models;
        let mut terminal = Terminal::new(TestBackend::new(120, 24)).unwrap();

        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let models_footer = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(models_footer.contains("Left providers"));
        assert!(!models_footer.contains("Left menu"));

        app.focus = Focus::Providers;
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let providers_footer = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(providers_footer.contains("Left menu"));
        assert!(providers_footer.contains("Enter models"));
        assert!(!providers_footer.contains("Left providers"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn interactive_overlays_show_their_key_hints() {
        let (root, mut app) = app();
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();

        app.overlay = Some(Overlay::Fetched {
            provider_id: "custom".into(),
            models: vec![catalog_model("model-a", 128_000, 16_384, 1.0)],
            unavailable: 0,
            selected: [0].into_iter().collect(),
            cursor: 0,
            ratio_config_used: false,
            overwrite: false,
            existing: std::collections::BTreeSet::new(),
            filter: String::new(),
            filtering: false,
        });
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let fetched = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(fetched.contains("Space toggle"));
        assert!(fetched.contains("a all"));
        assert!(fetched.contains("n none"));
        assert!(fetched.contains("i invert"));
        assert!(fetched.contains("o overwrite"));
        assert!(fetched.contains("/ filter"));
        assert!(fetched.contains("Enter/s import"));
        assert!(fetched.contains("Esc cancel"));

        app.overlay = Some(Overlay::OpenCodeProviders {
            providers: vec!["one".into(), "two".into()],
            selected: [0, 1].into_iter().collect(),
            cursor: 0,
        });
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let opencode = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(opencode.contains("Space toggle"));
        assert!(opencode.contains("a all"));
        assert!(opencode.contains("n none"));
        assert!(opencode.contains("i invert"));
        assert!(opencode.contains("Enter import"));
        assert!(opencode.contains("Esc cancel"));

        app.overlay = Some(Overlay::Doctor(Vec::new()));
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let doctor = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(doctor.contains("Esc/Enter close"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn fetched_catalog_filters_selects_all_and_requires_a_choice() {
        let (root, mut app) = app();
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        app.overlay = Some(Overlay::Fetched {
            provider_id: "custom".into(),
            models: vec![
                catalog_model("gpt-4", 128_000, 16_384, 1.0),
                catalog_model("claude-3", 200_000, 8_192, 2.0),
                catalog_model("gemini-1.5", 1_000_000, 32_768, 3.0),
            ],
            unavailable: 0,
            selected: std::collections::BTreeSet::new(),
            cursor: 0,
            ratio_config_used: true,
            overwrite: false,
            existing: std::collections::BTreeSet::new(),
            filter: String::new(),
            filtering: false,
        });

        // Importing with nothing selected warns and keeps the overlay open.
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(matches!(app.overlay, Some(Overlay::Fetched { .. })));
        assert!(app.notice.is_some());
        app.notice = None;

        // 'a' selects every model; 'n' clears them all.
        app.on_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        if let Some(Overlay::Fetched { ref selected, .. }) = app.overlay {
            assert_eq!(selected.len(), 3);
        }
        app.on_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        if let Some(Overlay::Fetched { ref selected, .. }) = app.overlay {
            assert!(selected.is_empty());
        }

        // 'i' inverts the selection: nothing → all, all → nothing.
        app.on_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE));
        if let Some(Overlay::Fetched { ref selected, .. }) = app.overlay {
            assert_eq!(selected.len(), 3);
        }
        app.on_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE));
        if let Some(Overlay::Fetched { ref selected, .. }) = app.overlay {
            assert!(selected.is_empty());
        }

        // '/' enters filtering; typing narrows the visible list.
        app.on_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        app.on_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE));
        app.on_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE));
        app.on_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE));
        if let Some(Overlay::Fetched {
            ref filter,
            ref filtering,
            ..
        }) = app.overlay
        {
            assert_eq!(filter, "gpt");
            assert!(*filtering);
        }

        // Enter exits filter editing but retains the text.
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        if let Some(Overlay::Fetched {
            ref filter,
            ref filtering,
            ..
        }) = app.overlay
        {
            assert_eq!(filter, "gpt");
            assert!(!*filtering);
        }

        // 'a' now selects only the visible (filtered) model.
        app.on_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        if let Some(Overlay::Fetched { ref selected, .. }) = app.overlay {
            assert_eq!(selected.len(), 1);
        }

        // Render: only gpt-4 is visible; the price source label shows ratio_config.
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("gpt-4"));
        assert!(!content.contains("claude-3"));
        assert!(!content.contains("gemini-1.5"));
        assert!(content.contains("ratio_config"));

        // Esc cancels and closes the overlay.
        app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(app.overlay.is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn fetched_existing_models_show_tag_and_overwrite_toggles() {
        let (root, mut app) = app();
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        app.overlay = Some(Overlay::Fetched {
            provider_id: "示例-provider".into(),
            models: vec![
                catalog_model("model-a", 128_000, 16_384, 1.0),
                catalog_model("model-c", 200_000, 8_192, 2.0),
            ],
            unavailable: 0,
            selected: [0].into_iter().collect(),
            cursor: 0,
            ratio_config_used: false,
            overwrite: false,
            existing: ["model-a".into()].into_iter().collect(),
            filter: String::new(),
            filtering: false,
        });
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        // model-a is existing → tagged "exists"; model-c is not.
        assert!(content.contains("exists"));
        // Title shows the overwrite state (off by default).
        assert!(content.contains("overwrite: off"));
        // 'o' toggles overwrite on.
        app.on_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));
        if let Some(Overlay::Fetched { overwrite, .. }) = app.overlay {
            assert!(overwrite);
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn opencode_provider_selector_all_none_and_invert() {
        let (root, mut app) = app();
        app.overlay = Some(Overlay::OpenCodeProviders {
            providers: vec!["one".into(), "two".into(), "three".into()],
            selected: std::collections::BTreeSet::new(),
            cursor: 0,
        });

        // 'a' selects all.
        app.on_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        if let Some(Overlay::OpenCodeProviders { ref selected, .. }) = app.overlay {
            assert_eq!(selected.len(), 3);
        }

        // 'n' clears all.
        app.on_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        if let Some(Overlay::OpenCodeProviders { ref selected, .. }) = app.overlay {
            assert!(selected.is_empty());
        }

        // 'i' inverts empty → all, then all → empty.
        app.on_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE));
        if let Some(Overlay::OpenCodeProviders { ref selected, .. }) = app.overlay {
            assert_eq!(selected.len(), 3);
        }
        app.on_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE));
        if let Some(Overlay::OpenCodeProviders { ref selected, .. }) = app.overlay {
            assert!(selected.is_empty());
        }

        // 'i' with a partial selection toggles only the missing/extra items.
        app.on_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        app.on_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        if let Some(Overlay::OpenCodeProviders { ref selected, .. }) = app.overlay {
            assert_eq!(selected.len(), 2);
        }
        app.on_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE));
        if let Some(Overlay::OpenCodeProviders { ref selected, .. }) = app.overlay {
            assert_eq!(selected.len(), 1);
        }
        let _ = fs::remove_dir_all(&root);
    }
}
