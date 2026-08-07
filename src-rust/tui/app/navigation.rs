use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::documents;

use super::{App, Focus, Overlay, Page, SettingsAction};
use crate::tui::{
    input::moved,
    keys::{command_for, Command},
    COMPACT_WIDTH,
};

impl App {
    pub(in crate::tui) fn on_key(&mut self, key: KeyEvent) {
        // Ctrl+C inside session preview copies the selected message instead of quitting.
        if self.in_session_preview()
            && key.modifiers.contains(KeyModifiers::CONTROL)
            && key.code == KeyCode::Char('c')
        {
            self.copy_selected_preview_message();
            return;
        }
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
        if self.in_profiles() && self.focus == Focus::Providers && key.code == KeyCode::Char(' ') {
            self.toggle_selected_provider_in_pi();
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
                KeyCode::Right | KeyCode::Char('l') | KeyCode::Enter | KeyCode::Tab => {
                    self.focus_session_preview();
                }
                KeyCode::PageUp => self.move_session_selection(-5),
                KeyCode::PageDown => self.move_session_selection(5),
                KeyCode::Char('u') => {
                    self.user_only_preview = !self.user_only_preview;
                    self.refresh_preview();
                }
                KeyCode::Left | KeyCode::Char('h') | KeyCode::Esc => self.focus = Focus::Menu,
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

    fn on_filter_key(&mut self, key: KeyEvent) {
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

    fn on_session_filter_key(&mut self, key: KeyEvent) {
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

    fn move_selection(&mut self, delta: isize) {
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

    pub(in crate::tui) fn in_model_context(&self) -> bool {
        self.page == Page::Profiles
            && (self.focus == Focus::Models || (self.width < COMPACT_WIDTH && self.narrow_detail))
    }

    pub(in crate::tui) fn in_profiles(&self) -> bool {
        self.page == Page::Profiles && self.focus != Focus::Menu
    }
}
