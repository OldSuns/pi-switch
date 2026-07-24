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
use app::{wrap_preview_text, Focus, Overlay, Page};
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
            match event::read().map_err(terminal_error)? {
                Event::Key(key) if key.kind != KeyEventKind::Release => {
                    app.on_key(key);
                }
                Event::Mouse(mouse) => {
                    app.on_mouse(mouse);
                }
                _ => {}
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
            providers_path: paths.providers.display().to_string(),
            models_path: paths.models.display().to_string(),
            settings_path: paths.settings.display().to_string(),
            providers: vec![ProviderView {
                id: "示例-provider".into(),
                in_pi: true,
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
            warning: None,
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

    include!("tests/layout.rs");
    include!("tests/profiles.rs");
    include!("tests/sessions.rs");
    include!("tests/interaction.rs");
}
