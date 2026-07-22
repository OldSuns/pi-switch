use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Command {
    Quit,
    Help,
    Filter,
    New,
    Edit,
    Delete,
    Copy,
    Import,
    SetDefault,
    Backups,
    Doctor,
    Reload,
}

pub(super) struct Shortcut {
    pub(super) command: Command,
    pub(super) code: KeyCode,
    pub(super) key: &'static str,
    pub(super) label: &'static str,
    pub(super) help: &'static str,
}

const SHORTCUTS: &[Shortcut] = &[
    Shortcut {
        command: Command::New,
        code: KeyCode::Char('n'),
        key: "n",
        label: "new",
        help: "new focused item",
    },
    Shortcut {
        command: Command::Edit,
        code: KeyCode::Char('e'),
        key: "e",
        label: "edit",
        help: "edit focused item",
    },
    Shortcut {
        command: Command::Delete,
        code: KeyCode::Char('d'),
        key: "d/Del",
        label: "delete",
        help: "delete focused item",
    },
    Shortcut {
        command: Command::Copy,
        code: KeyCode::Char('c'),
        key: "c",
        label: "copy",
        help: "copy focused item",
    },
    Shortcut {
        command: Command::Import,
        code: KeyCode::Char('i'),
        key: "i",
        label: "import",
        help: "import models from provider",
    },
    Shortcut {
        command: Command::SetDefault,
        code: KeyCode::Char(' '),
        key: "Space",
        label: "default",
        help: "set selected model as default",
    },
    Shortcut {
        command: Command::Filter,
        code: KeyCode::Char('/'),
        key: "/",
        label: "filter",
        help: "filter providers",
    },
    Shortcut {
        command: Command::Reload,
        code: KeyCode::Char('r'),
        key: "r",
        label: "reload",
        help: "reload configuration from disk",
    },
    Shortcut {
        command: Command::Backups,
        code: KeyCode::Char('b'),
        key: "b",
        label: "backups",
        help: "browse backups",
    },
    Shortcut {
        command: Command::Doctor,
        code: KeyCode::Char('v'),
        key: "v",
        label: "check",
        help: "validate configuration",
    },
    Shortcut {
        command: Command::Help,
        code: KeyCode::Char('?'),
        key: "?",
        label: "help",
        help: "show all shortcuts",
    },
    Shortcut {
        command: Command::Quit,
        code: KeyCode::Char('q'),
        key: "q",
        label: "quit",
        help: "quit",
    },
];

pub(super) fn command_for(key: KeyEvent) -> Option<Command> {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return Some(Command::Quit);
    }
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return None;
    }
    if key.code == KeyCode::Delete {
        return Some(Command::Delete);
    }
    SHORTCUTS
        .iter()
        .find(|shortcut| shortcut.code == key.code)
        .map(|shortcut| shortcut.command)
}

pub(super) fn shortcut(command: Command) -> &'static Shortcut {
    SHORTCUTS
        .iter()
        .find(|shortcut| shortcut.command == command)
        .expect("every command shown in the UI must have a shortcut")
}

pub(super) fn all_shortcuts() -> &'static [Shortcut] {
    SHORTCUTS
}
