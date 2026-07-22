use std::{
    io::{self, Stdout},
    sync::Arc,
};

use crossterm::{
    cursor, execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Frame, Terminal};

pub(super) fn terminal_error(error: impl std::fmt::Display) -> String {
    format!("terminal error: {error}")
}

type PanicHook = Arc<dyn Fn(&std::panic::PanicHookInfo<'_>) + Send + Sync + 'static>;

pub(super) struct PanicRestoreHookGuard {
    previous: Option<PanicHook>,
}

impl PanicRestoreHookGuard {
    pub(super) fn install() -> Self {
        let previous: PanicHook = std::panic::take_hook().into();
        let previous_for_hook = previous.clone();
        std::panic::set_hook(Box::new(move |info| {
            let mut stdout = io::stdout();
            let _ = restore_stdout(&mut stdout);
            previous_for_hook(info);
        }));
        Self {
            previous: Some(previous),
        }
    }
}

impl Drop for PanicRestoreHookGuard {
    fn drop(&mut self) {
        if let Some(previous) = self.previous.take() {
            std::panic::set_hook(Box::new(move |info| previous(info)));
        }
    }
}

fn restore_stdout(stdout: &mut Stdout) -> Result<(), String> {
    let raw = disable_raw_mode().map_err(terminal_error);
    let screen = execute!(stdout, cursor::Show, LeaveAlternateScreen).map_err(terminal_error);
    raw.and(screen)
}

pub(super) struct TuiTerminal {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    active: bool,
}

impl TuiTerminal {
    pub(super) fn new() -> Result<Self, String> {
        let mut stdout = io::stdout();
        enable_raw_mode().map_err(terminal_error)?;
        if let Err(error) = execute!(stdout, EnterAlternateScreen, cursor::Hide) {
            let _ = restore_stdout(&mut stdout);
            return Err(terminal_error(error));
        }
        match Terminal::new(CrosstermBackend::new(stdout)) {
            Ok(terminal) => Ok(Self {
                terminal,
                active: true,
            }),
            Err(error) => {
                let mut stdout = io::stdout();
                let _ = restore_stdout(&mut stdout);
                Err(terminal_error(error))
            }
        }
    }

    pub(super) fn draw(&mut self, draw: impl FnOnce(&mut Frame<'_>)) -> Result<(), String> {
        self.terminal.draw(draw).map(|_| ()).map_err(terminal_error)
    }

    pub(super) fn restore_best_effort(&mut self) -> Result<(), String> {
        if !self.active {
            return Ok(());
        }
        let raw = disable_raw_mode().map_err(terminal_error);
        let screen = execute!(
            self.terminal.backend_mut(),
            cursor::Show,
            LeaveAlternateScreen
        )
        .map_err(terminal_error);
        let _ = self.terminal.show_cursor();
        self.active = false;
        raw.and(screen)
    }
}

impl Drop for TuiTerminal {
    fn drop(&mut self) {
        let _ = self.restore_best_effort();
    }
}
