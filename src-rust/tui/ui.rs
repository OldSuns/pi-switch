mod form_render;
mod overlays;
mod pages;
mod profiles;

use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};
use unicode_width::UnicodeWidthStr;

use crate::documents::{CatalogModel, PI_DEFAULT_CONTEXT_WINDOW, PI_DEFAULT_MAX_TOKENS};

use super::{
    app::{visible_fetched_indices, App, Focus, Notice, NoticeKind, Overlay, Page},
    forms::{
        FormState, ModelDefaultsFormState, ModelFormState, MAX_TOKENS_FIELDS, PRESETS,
        THINKING_FORMATS,
    },
    i18n::Language,
    input::{api_label, mask_secret, pad_width, truncate_width, with_cursor, wrap_width},
    keys::{all_shortcuts, shortcut, Command},
    COMPACT_WIDTH, WIDE_WIDTH,
};
use pages::{render_home, render_menu, render_sessions, render_settings};
use profiles::render_profiles_page;

const MENU_WIDTH: u16 = 18;
const SHELL_WIDE_WIDTH: u16 = MENU_WIDTH + WIDE_WIDTH;
const DETAIL_LABEL_WIDTH: usize = 10;
const MIN_MODELS_HEIGHT: u16 = 5;
const MODEL_DEFAULT_VALUE_WIDTH: usize = 16;

#[derive(Clone, Copy)]
pub(super) struct Theme {
    pub(super) foreground: Color,
    pub(super) background: Color,
    pub(super) surface: Color,
    pub(super) surface_hi: Color,
    pub(super) accent: Color,
    pub(super) accent_dim: Color,
    pub(super) success: Color,
    pub(super) warning: Color,
    pub(super) error: Color,
    pub(super) muted: Color,
    pub(super) dim: Color,
    pub(super) border: Color,
}

impl Theme {
    fn detect() -> Self {
        if std::env::var_os("NO_COLOR").is_some() {
            return Self {
                foreground: Color::Reset,
                background: Color::Reset,
                surface: Color::Reset,
                surface_hi: Color::Reset,
                accent: Color::Reset,
                accent_dim: Color::Reset,
                success: Color::Reset,
                warning: Color::Reset,
                error: Color::Reset,
                muted: Color::Reset,
                dim: Color::Reset,
                border: Color::Reset,
            };
        }
        // ponytail: single precision-instrument palette; multi-theme later if needed
        Self {
            foreground: Color::Rgb(220, 230, 242),
            background: Color::Rgb(8, 12, 18),
            surface: Color::Rgb(14, 20, 28),
            surface_hi: Color::Rgb(20, 30, 42),
            accent: Color::Rgb(64, 160, 255),
            accent_dim: Color::Rgb(32, 78, 128),
            success: Color::Rgb(56, 168, 92),
            warning: Color::Rgb(196, 148, 48),
            error: Color::Rgb(220, 84, 76),
            muted: Color::Rgb(110, 124, 140),
            dim: Color::Rgb(70, 82, 96),
            border: Color::Rgb(36, 48, 62),
        }
    }

    pub(super) fn base(self) -> Style {
        Style::default().fg(self.foreground).bg(self.background)
    }

    pub(super) fn selected(self) -> Style {
        Style::default()
            .fg(self.foreground)
            .bg(self.accent_dim)
            .add_modifier(Modifier::BOLD)
    }

    pub(super) fn panel(self, active: bool) -> Style {
        if active {
            Style::default()
                .fg(self.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(self.border)
        }
    }

    pub(super) fn keycap(self) -> Style {
        Style::default()
            .fg(self.accent)
            .bg(self.surface)
            .add_modifier(Modifier::BOLD)
    }

    pub(super) fn label(self) -> Style {
        Style::default().fg(self.muted)
    }

    pub(super) fn value(self) -> Style {
        Style::default().fg(self.foreground)
    }

    pub(super) fn dim_text(self) -> Style {
        Style::default().fg(self.dim)
    }

    fn surface_style(self) -> Style {
        Style::default().fg(self.foreground).bg(self.surface)
    }
}

pub(super) fn draw(frame: &mut Frame<'_>, app: &mut App) {
    let theme = Theme::detect();
    frame.render_widget(Block::default().style(theme.base()), frame.area());
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(8),
        Constraint::Length(1),
    ])
    .areas(frame.area());
    render_header(frame, app, header, theme);
    if frame.area().width >= SHELL_WIDE_WIDTH {
        let [menu, content] =
            Layout::horizontal([Constraint::Length(MENU_WIDTH), Constraint::Min(1)]).areas(body);
        render_menu(frame, app, menu, theme);
        render_page(frame, app, content, theme);
    } else if app.focus == Focus::Menu {
        app.width = body.width;
        render_menu(frame, app, body, theme);
    } else {
        render_page(frame, app, body, theme);
    }
    render_footer(frame, app, footer, theme);
    if let Some(overlay) = app.overlay.as_ref() {
        overlays::render_overlay(
            frame,
            overlay,
            app.language,
            app.tick_count,
            frame.area(),
            theme,
        );
    }
    if let Some(notice) = app.notice.as_ref() {
        render_notice(frame, notice, frame.area(), theme);
    }
}

fn render_page(frame: &mut Frame<'_>, app: &mut App, area: Rect, theme: Theme) {
    app.width = area.width;
    match app.page {
        Page::Home => render_home(frame, app, area, theme),
        Page::Profiles => render_profiles_page(frame, app, area, theme),
        Page::Sessions => {
            app.ensure_sessions_loaded();
            render_sessions(frame, app, area, theme);
        }
        Page::Settings => render_settings(frame, app, area, theme),
    }
}

fn render_header(frame: &mut Frame<'_>, app: &App, area: Rect, theme: Theme) {
    let model_count = app
        .snapshot
        .providers
        .iter()
        .map(|provider| provider.models.len())
        .sum::<usize>();
    let default = app
        .snapshot
        .default_provider
        .as_deref()
        .zip(app.snapshot.default_model.as_deref())
        .map(|(provider, model)| format!("{provider}/{model}"))
        .unwrap_or_else(|| app.language.pick("not set", "未设置").into());
    let title = Line::from(vec![
        Span::styled(
            " pi-switch ",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(version_label(app), version_style(app, theme)),
        Span::styled("  ·  ", theme.dim_text()),
        Span::styled(app.language.pick("default ", "默认 "), theme.label()),
        Span::styled(default, theme.value().add_modifier(Modifier::BOLD)),
        Span::styled("  ·  ", theme.dim_text()),
        Span::styled(
            format!(
                "{} {}  ·  {} {}",
                app.snapshot.providers.len(),
                app.language.pick("provider(s)", "个提供商"),
                model_count,
                app.language.pick("model(s)", "个模型"),
            ),
            theme.label(),
        ),
    ]);
    let path_width = area.width.saturating_sub(4) as usize;
    let path = Line::from(vec![
        Span::styled(
            app.language.pick(" provider library ", " 提供商库 "),
            theme.dim_text(),
        ),
        Span::styled(
            truncate_width(&app.snapshot.providers_path, path_width.saturating_sub(8)),
            theme.label(),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(vec![title, path]).block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(theme.border))
                .style(theme.base()),
        ),
        area,
    );
}

/// Version label for the header: the current version, or `current ↑ latest`
/// when a newer npm version was detected by the background check.
fn version_label(app: &App) -> String {
    match &app.update_available {
        Some(latest) => format!("{} \u{2191} {}", env!("CARGO_PKG_VERSION"), latest),
        None => env!("CARGO_PKG_VERSION").to_owned(),
    }
}

/// Style for the header version: highlighted in the warning accent when an
/// update is available, otherwise the usual dimmed secondary text.
fn version_style(app: &App, theme: Theme) -> Style {
    if app.update_available.is_some() {
        Style::default()
            .fg(theme.warning)
            .add_modifier(Modifier::BOLD)
    } else {
        theme.dim_text()
    }
}

fn render_footer(frame: &mut Frame<'_>, app: &App, area: Rect, theme: Theme) {
    let compact = app.width < 76;
    let binding = |command| {
        let shortcut = shortcut(command);
        (
            shortcut.key,
            if app.language == Language::English {
                shortcut.label
            } else {
                app.language.command_label(command)
            },
        )
    };
    let mut keys = if app.filtering {
        vec![
            ("Enter", app.language.pick("apply", "应用")),
            ("Esc", app.language.pick("clear", "清除")),
        ]
    } else if app.focus == Focus::Menu {
        vec![
            ("Up/Down", app.language.pick("navigate", "导航")),
            ("Enter", app.language.pick("open", "打开")),
            ("q", app.language.pick("quit", "退出")),
        ]
    } else if app.in_model_context() {
        let commands = if compact {
            &[
                Command::New,
                Command::Edit,
                Command::Delete,
                Command::SetDefault,
            ][..]
        } else {
            &[
                Command::New,
                Command::Edit,
                Command::Delete,
                Command::Copy,
                Command::SetDefault,
                Command::Import,
                Command::Help,
            ][..]
        };
        commands.iter().map(|command| binding(*command)).collect()
    } else if app.page == Page::Profiles {
        let commands = if compact {
            &[Command::New, Command::Edit, Command::Delete, Command::Help][..]
        } else {
            &[
                Command::New,
                Command::Edit,
                Command::Delete,
                Command::Copy,
                Command::Import,
                Command::Filter,
                Command::Help,
            ][..]
        };
        let mut keys = vec![("Space", app.language.pick("add/remove", "添加/移除"))];
        keys.extend(commands.iter().map(|command| binding(*command)));
        keys
    } else if app.page == Page::Sessions {
        if app.session_filtering {
            vec![
                ("Enter", app.language.pick("apply", "应用")),
                ("Esc", app.language.pick("clear", "清除")),
            ]
        } else if app.focus == Focus::SessionPreview {
            vec![
                ("Up/Down", app.language.pick("tree node", "树节点")),
                (
                    "Ctrl+←/→",
                    app.language.pick("parent / child", "父节点/子节点"),
                ),
                ("Alt+←/→", app.language.pick("sibling branch", "相邻分支")),
                ("Tab", app.language.pick("collapse / expand", "折叠/展开")),
                ("PgUp/PgDn", app.language.pick("scroll", "滚动")),
                ("Ctrl+C", app.language.pick("copy", "复制")),
                ("Left", app.language.pick("list", "列表")),
            ]
        } else if compact {
            vec![
                ("/", app.language.pick("filter", "筛选")),
                ("Right", app.language.pick("open", "进入")),
                ("d", app.language.pick("delete", "删除")),
                ("r", app.language.pick("reload", "刷新")),
                ("n", app.language.pick("named", "命名")),
                ("u", app.language.pick("user", "用户")),
                ("q", app.language.pick("quit", "退出")),
            ]
        } else {
            vec![
                ("Up/Down", app.language.pick("select", "选择")),
                ("Right", app.language.pick("browse", "浏览消息")),
                ("/", app.language.pick("filter", "筛选")),
                ("n", app.language.pick("named only", "仅命名")),
                ("u", app.language.pick("user-only", "仅用户")),
                ("d", app.language.pick("delete", "删除")),
                ("r", app.language.pick("reload", "刷新")),
                ("q", app.language.pick("quit", "退出")),
            ]
        }
    } else if app.page == Page::Settings {
        vec![
            ("Up/Down", app.language.pick("select", "选择")),
            ("Enter/Space", app.language.pick("run", "执行")),
        ]
    } else {
        vec![
            binding(Command::Reload),
            ("Left", app.language.pick("menu", "菜单")),
        ]
    };
    if !compact && !app.filtering && !app.session_filtering && app.focus != Focus::Menu {
        if app.page == Page::Profiles {
            if app.in_model_context() {
                keys.push(("Left", app.language.pick("providers", "提供商")));
            } else {
                keys.push(("Enter", app.language.pick("models", "模型")));
                keys.push(("Left", app.language.pick("menu", "菜单")));
            }
        } else if app.page == Page::Sessions {
            if app.focus != Focus::SessionPreview {
                keys.push(("Left", app.language.pick("menu", "菜单")));
            }
        } else if app.page != Page::Home {
            keys.push(("Left", app.language.pick("menu", "菜单")));
        }
    }
    let mut spans = Vec::new();
    for (key, label) in keys {
        spans.push(Span::styled(format!(" {key} "), theme.keycap()));
        spans.push(Span::styled(format!("{label}  "), theme.label()));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)).style(theme.base()), area);
}

fn checkbox_span(checked: bool, theme: Theme) -> Span<'static> {
    let (glyph, style) = if checked {
        (
            "[✓] ",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        ("[ ] ", Style::default().fg(theme.muted))
    };
    Span::styled(glyph, style)
}

fn render_key_hints(frame: &mut Frame<'_>, area: Rect, hints: &[(&str, &str)], theme: Theme) {
    let mut lines = Vec::new();
    let mut spans = Vec::new();
    let mut line_width = 0;
    for &(key, label) in hints {
        let key_width = UnicodeWidthStr::width(key) + 2;
        let label_width = UnicodeWidthStr::width(label) + 2;
        if !spans.is_empty() && line_width + key_width + label_width > area.width as usize {
            lines.push(Line::from(std::mem::take(&mut spans)));
            line_width = 0;
        }
        spans.push(Span::styled(format!(" {key} "), theme.keycap()));
        spans.push(Span::styled(format!("{label}  "), theme.label()));
        line_width += key_width + label_width;
    }
    if !spans.is_empty() {
        lines.push(Line::from(spans));
    }
    frame.render_widget(Paragraph::new(Text::from(lines)).style(theme.base()), area);
}

fn render_notice(frame: &mut Frame<'_>, notice: &Notice, area: Rect, theme: Theme) {
    let color = match notice.kind {
        NoticeKind::Success => theme.success,
        NoticeKind::Warning => theme.warning,
    };
    let width = (UnicodeWidthStr::width(notice.message.as_str()) as u16 + 4)
        .min(area.width.saturating_sub(2))
        .max(12);
    let rect = Rect::new(area.right().saturating_sub(width + 1), area.y + 1, width, 3);
    clear_area(frame, rect, theme);
    frame.render_widget(
        Paragraph::new(truncate_width(
            &notice.message,
            width.saturating_sub(4) as usize,
        ))
        .style(theme.surface_style())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(color))
                .style(theme.surface_style()),
        )
        .alignment(Alignment::Center),
        rect,
    );
}

fn render_modal<'a>(
    frame: &mut Frame<'_>,
    rect: Rect,
    title: &'a str,
    body: Paragraph<'a>,
    border: Color,
    theme: Theme,
) {
    clear_area(frame, rect, theme);
    frame.render_widget(
        body.block(
            Block::default()
                .title(Span::styled(
                    title,
                    Style::default().fg(border).add_modifier(Modifier::BOLD),
                ))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border))
                .style(theme.surface_style()),
        ),
        rect,
    );
}

fn clear_area(frame: &mut Frame<'_>, area: Rect, theme: Theme) {
    // Clear adjacent cells too: a wide glyph can start outside the overlay and occupy its border.
    let bounds = frame.area();
    let x = area.x.saturating_sub(1).max(bounds.x);
    let right = area.right().saturating_add(1).min(bounds.right());
    let clear = Rect::new(x, area.y, right.saturating_sub(x), area.height);
    frame.render_widget(Clear, clear);
    frame.render_widget(Block::default().style(theme.surface_style()), clear);
}

fn catalog_summary(model: &CatalogModel, language: Language) -> String {
    let context = model.config["contextWindow"]
        .as_u64()
        .expect("validated catalog context window");
    let max_tokens = model.config["maxTokens"]
        .as_u64()
        .expect("validated catalog max tokens");
    let input = model.config["cost"]["input"]
        .as_f64()
        .expect("validated catalog input cost");
    let output = model.config["cost"]["output"]
        .as_f64()
        .expect("validated catalog output cost");
    format!(
        "ctx {}  max {}  $/M {} {input} {} {output}",
        format_token_count(context),
        format_token_count(max_tokens),
        language.pick("in", "输入"),
        language.pick("out", "输出"),
    )
}

fn format_token_count(value: u64) -> String {
    if value >= 1_000_000 {
        "1M".into()
    } else if value >= 100_000 {
        format!("{}k", value / 1_000)
    } else {
        value.to_string()
    }
}

fn modal_rect(area: Rect, desired_width: u16, desired_height: u16) -> Rect {
    let width = desired_width.min(area.width.saturating_sub(2).max(1));
    let height = desired_height.min(area.height.saturating_sub(2).max(1));
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

/// Near-full-screen rect leaving a 1-cell margin on every side. Used by the
/// model-import overlay so the catalog list has room to breathe instead of
/// sitting in a small centered modal.
fn near_full_rect(area: Rect) -> Rect {
    let width = area.width.saturating_sub(2).max(1);
    let height = area.height.saturating_sub(2).max(1);
    Rect::new(area.x + 1, area.y + 1, width, height)
}
