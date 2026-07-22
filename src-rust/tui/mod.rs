mod app;
mod forms;
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
use app::{Focus, Overlay};
#[cfg(test)]
use forms::{FormState, ModelFormState};
#[cfg(test)]
use input::{truncate_width, wrap_width};
#[cfg(test)]
use keys::{command_for, Command};

const TICK: Duration = Duration::from_millis(120);
const WIDE_WIDTH: u16 = 96;
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
            context_window: 128_000,
            max_tokens: 16_384,
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
        };
        (root, App::from_snapshot(paths, snapshot))
    }

    #[test]
    fn responsive_layout_renders_wide_and_narrow_with_cjk() {
        let (root, mut app) = app();
        for (width, height) in [(120, 36), (80, 24), (64, 20)] {
            let backend = TestBackend::new(width, height);
            let mut terminal = Terminal::new(backend).unwrap();
            terminal.draw(|frame| draw(frame, &mut app)).unwrap();
            let content = terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>();
            assert!(content.contains("pi-switch"));
            assert!(content.contains("provider"));
            assert!(content.contains("new"));
        }
        assert_eq!(truncate_width("示例-provider", 8), "示例-...");
        assert_eq!(UnicodeWidthStr::width("示例-provider"), 13);
        let wrapped = wrap_width("示例-provider", 6);
        assert_eq!(wrapped.concat(), "示例-provider");
        assert!(wrapped
            .iter()
            .all(|line| UnicodeWidthStr::width(line.as_str()) <= 6));

        app.overlay = Some(Overlay::Loading {
            provider_id: "示例-provider".into(),
        });
        app.on_overlay_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(matches!(app.overlay, Some(Overlay::Loading { .. })));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn contextual_actions_open_provider_and_model_crud() {
        let (root, mut app) = app();
        app.on_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        assert!(matches!(app.overlay, Some(Overlay::Form(_))));

        app.overlay = None;
        app.focus = Focus::Models;
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
    }
}
