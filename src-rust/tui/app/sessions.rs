use super::*;

impl App {
    pub(in crate::tui) fn visible_sessions(&self) -> Vec<usize> {
        self.sessions
            .iter()
            .enumerate()
            .filter(|(_, session)| {
                documents::session_matches(session, &self.session_filter, self.named_only)
            })
            .map(|(index, _)| index)
            .collect()
    }

    pub(in crate::tui) fn selected_session(&self) -> Option<&SessionSummary> {
        let visible = self.visible_sessions();
        visible
            .get(self.session_cursor)
            .and_then(|index| self.sessions.get(*index))
    }

    pub(in crate::tui) fn ensure_sessions_loaded(&mut self) {
        if self.sessions_loaded {
            return;
        }
        self.reload_sessions(None);
    }

    pub(in crate::tui) fn reload_sessions(&mut self, message: Option<&str>) {
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

    pub(in crate::tui) fn clamp_session_selection(&mut self) {
        let count = self.visible_sessions().len();
        self.session_cursor = if count == 0 {
            0
        } else {
            self.session_cursor.min(count - 1)
        };
    }

    pub(in crate::tui) fn refresh_preview(&mut self) {
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

    pub(in crate::tui) fn in_sessions(&self) -> bool {
        self.page == Page::Sessions && self.focus == Focus::Content
    }

    pub(in crate::tui) fn in_session_list(&self) -> bool {
        self.page == Page::Sessions && self.focus == Focus::Content
    }

    pub(in crate::tui) fn in_session_preview(&self) -> bool {
        self.page == Page::Sessions && self.focus == Focus::SessionPreview
    }

    pub(in crate::tui) fn preview_message_count(&self) -> usize {
        self.preview
            .as_ref()
            .map(|messages| messages.len())
            .unwrap_or(0)
    }

    pub(in crate::tui) fn selected_preview_message(&self) -> Option<&PreviewMessage> {
        let messages = self.preview.as_ref()?;
        messages.get(self.preview_message_cursor)
    }

    pub(in crate::tui) fn clamp_preview_message_cursor(&mut self) {
        let count = self.preview_message_count();
        self.preview_message_cursor = if count == 0 {
            0
        } else {
            self.preview_message_cursor.min(count - 1)
        };
    }

    pub(in crate::tui) fn ensure_preview_message_visible(&mut self) {
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

    pub(in crate::tui) fn focus_session_preview(&mut self) {
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

    pub(in crate::tui) fn move_preview_message(&mut self, delta: isize) {
        let count = self.preview_message_count() as isize;
        if count == 0 {
            self.preview_message_cursor = 0;
            return;
        }
        let next = (self.preview_message_cursor as isize + delta).clamp(0, count - 1);
        self.preview_message_cursor = next as usize;
        self.ensure_preview_message_visible();
    }

    pub(in crate::tui) fn scroll_preview_lines(&mut self, delta: isize) {
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

    pub(in crate::tui) fn copy_selected_preview_message(&mut self) {
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
            Err(error) => self.notice(NoticeKind::Warning, error),
        }
    }

    pub(in crate::tui) fn on_mouse(&mut self, mouse: crossterm::event::MouseEvent) {
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

    pub(super) fn on_session_preview_key(&mut self, key: KeyEvent) {
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
}
