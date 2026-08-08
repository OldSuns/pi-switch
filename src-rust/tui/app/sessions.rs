use super::*;
use std::cmp::Reverse;

impl App {
    pub(in crate::tui) fn session_groups(&self) -> Vec<SessionGroup> {
        let mut groups: Vec<SessionGroup> = Vec::new();
        for (index, session) in self.sessions.iter().enumerate() {
            if !documents::session_matches(session, &self.session_filter, self.named_only) {
                continue;
            }
            let cwd = session.cwd.clone();
            if let Some(group) = groups.iter_mut().find(|g| g.cwd == cwd) {
                group.sessions.push(index);
            } else {
                groups.push(SessionGroup {
                    cwd,
                    sessions: vec![index],
                });
            }
        }
        // Order groups by the most-recently-modified session within each group (descending).
        groups.sort_by_key(|group| {
            let max_modified = group
                .sessions
                .iter()
                .filter_map(|&i| self.sessions.get(i))
                .map(|s| s.modified)
                .max();
            Reverse(max_modified)
        });
        groups
    }

    pub(in crate::tui) fn visible_sessions(&self) -> Vec<usize> {
        self.session_groups()
            .into_iter()
            .flat_map(|group| group.sessions)
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
            self.preview_visible.clear();
            self.preview_collapsed.clear();
            self.preview_child_history.clear();
            self.preview_layout = None;
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
            Ok(preview) => {
                self.preview = Some(preview);
                self.preview_visible.clear();
                self.preview_collapsed.clear();
                self.preview_child_history.clear();
                self.preview_layout = None;
                self.preview_path = Some(path);
                self.preview_scroll = 0;
                self.rebuild_preview_visibility();
            }
            Err(error) => {
                self.preview = None;
                self.preview_visible.clear();
                self.preview_collapsed.clear();
                self.preview_child_history.clear();
                self.preview_layout = None;
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
        self.preview_visible.len()
    }

    pub(in crate::tui) fn selected_preview_tree_index(&self) -> Option<usize> {
        self.preview_visible
            .get(self.preview_message_cursor)
            .copied()
    }

    pub(in crate::tui) fn selected_preview_message(&self) -> Option<&PreviewMessage> {
        let preview = self.preview.as_ref()?;
        preview.messages.get(self.selected_preview_tree_index()?)
    }

    pub(in crate::tui) fn clamp_preview_message_cursor(&mut self) {
        let count = self.preview_message_count();
        self.preview_message_cursor = if count == 0 {
            0
        } else {
            self.preview_message_cursor.min(count - 1)
        };
    }

    fn rebuild_preview_visibility(&mut self) {
        let preferred_id = self
            .selected_preview_message()
            .map(|message| message.id.clone());
        self.preview_visible.clear();
        let Some(preview) = self.preview.as_ref() else {
            self.preview_message_cursor = 0;
            self.preview_layout = None;
            return;
        };

        let mut hidden_level = None;
        for (index, message) in preview.messages.iter().enumerate() {
            if let Some(level) = hidden_level {
                if message.tree.level > level {
                    continue;
                }
                hidden_level = None;
            }
            self.preview_visible.push(index);
            if self.preview_collapsed.contains(&message.id) {
                hidden_level = Some(message.tree.level);
            }
        }
        self.preview_message_cursor = preferred_id
            .as_deref()
            .and_then(|id| {
                self.preview_visible.iter().position(|index| {
                    preview
                        .messages
                        .get(*index)
                        .is_some_and(|message| message.id == id)
                })
            })
            .or_else(|| {
                preview.active_index().and_then(|active| {
                    self.preview_visible
                        .iter()
                        .position(|index| *index == active)
                })
            })
            .unwrap_or(0);
        self.preview_layout = None;
    }

    pub(in crate::tui) fn toggle_preview_fold(&mut self) {
        let Some(index) = self.selected_preview_tree_index() else {
            return;
        };
        let Some(message) = self
            .preview
            .as_ref()
            .and_then(|preview| preview.messages.get(index))
        else {
            return;
        };
        if !message.tree.has_children {
            self.notice(
                NoticeKind::Warning,
                self.language
                    .pick("Leaf nodes cannot be folded", "叶节点没有可折叠的下级"),
            );
            return;
        }
        let id = message.id.clone();
        let descendants = self
            .preview
            .as_ref()
            .map_or(0, |preview| preview.descendant_count(index));
        let collapsed = if self.preview_collapsed.insert(id.clone()) {
            true
        } else {
            self.preview_collapsed.remove(&id);
            false
        };
        self.rebuild_preview_visibility();
        self.ensure_preview_message_visible();
        self.notice(
            NoticeKind::Success,
            if collapsed {
                format!(
                    "{} {descendants} {}",
                    self.language.pick("Collapsed", "已折叠"),
                    self.language.pick("descendants", "个下级节点")
                )
            } else {
                format!(
                    "{} {descendants} {}",
                    self.language.pick("Expanded", "已展开"),
                    self.language.pick("descendants", "个下级节点")
                )
            },
        );
    }

    pub(in crate::tui) fn move_preview_parent(&mut self) {
        let Some(index) = self.selected_preview_tree_index() else {
            return;
        };
        let target = self.preview.as_ref().and_then(|preview| {
            let (parent, branch_root) = preview.parent_branch(index)?;
            Some((
                preview.messages.get(parent)?.id.clone(),
                preview.messages.get(branch_root)?.id.clone(),
            ))
        });
        if let Some((parent_id, branch_id)) = target {
            self.preview_child_history
                .insert(parent_id.clone(), branch_id);
            self.select_visible_preview_id(&parent_id);
        } else {
            self.notice(
                NoticeKind::Warning,
                self.language
                    .pick("Already at the root branch", "当前已经是最上层分支"),
            );
        }
    }

    pub(in crate::tui) fn move_preview_child(&mut self) {
        let Some(index) = self.selected_preview_tree_index() else {
            return;
        };
        let Some((current_id, target)) = self.preview.as_ref().and_then(|preview| {
            let current = preview.messages.get(index)?;
            let child = preview.child_branch_index(
                index,
                self.preview_child_history
                    .get(&current.id)
                    .map(String::as_str),
            )?;
            Some((current.id.clone(), preview.messages.get(child)?.id.clone()))
        }) else {
            self.notice(
                NoticeKind::Warning,
                self.language
                    .pick("No child branch", "当前分支没有下一级分支"),
            );
            return;
        };
        if self.preview_collapsed.remove(&current_id) {
            self.rebuild_preview_visibility();
        }
        self.select_visible_preview_id(&target);
    }

    pub(in crate::tui) fn move_preview_branch(&mut self, delta: isize) {
        let Some(index) = self.selected_preview_tree_index() else {
            return;
        };
        let target = self
            .preview
            .as_ref()
            .and_then(|preview| preview.adjacent_branch_index(index, delta))
            .and_then(|target| self.preview.as_ref()?.messages.get(target))
            .map(|message| message.id.clone());
        if let Some(target) = target {
            self.select_visible_preview_id(&target);
        } else {
            self.notice(
                NoticeKind::Warning,
                self.language
                    .pick("No adjacent branch that way", "该方向没有相邻分支"),
            );
        }
    }

    fn select_visible_preview_id(&mut self, id: &str) {
        if let Some(cursor) = self.preview_visible.iter().position(|index| {
            self.preview
                .as_ref()
                .and_then(|preview| preview.messages.get(*index))
                .is_some_and(|message| message.id == id)
        }) {
            self.preview_message_cursor = cursor;
            self.remember_selected_preview_child();
            self.ensure_preview_message_visible();
        }
    }

    fn remember_selected_preview_child(&mut self) {
        let relation = self.preview.as_ref().and_then(|preview| {
            let index = self.selected_preview_tree_index()?;
            let (parent, branch_root) = preview.parent_branch(index)?;
            Some((
                preview.messages.get(parent)?.id.clone(),
                preview.messages.get(branch_root)?.id.clone(),
            ))
        });
        if let Some((parent, child)) = relation {
            self.preview_child_history.insert(parent, child);
        }
    }

    pub(in crate::tui) fn set_preview_geometry(&mut self, width: usize, height: u16) {
        self.preview_viewport_height = height.max(1);
        self.ensure_preview_layout(width);
        self.clamp_preview_message_cursor();
        let max_scroll = self
            .preview_total_line_count_from_layout()
            .saturating_sub(self.preview_viewport_height as usize);
        self.preview_scroll = (self.preview_scroll as usize)
            .min(max_scroll)
            .min(u16::MAX as usize) as u16;
    }

    pub(in crate::tui) fn ensure_preview_layout(&mut self, width: usize) {
        let width = width.max(1);
        self.preview_wrap_width = width;
        let message_count = self.preview_visible.len();
        let rebuild = self
            .preview_layout
            .as_ref()
            .is_none_or(|layout| layout.width != width || layout.messages.len() != message_count);
        if rebuild {
            let preview = self.preview.as_ref();
            let visible = self.preview_visible.clone();
            self.preview_layout = preview.map(|preview| {
                PreviewLayout::new(
                    visible
                        .iter()
                        .filter_map(|index| preview.messages.get(*index)),
                    width,
                )
            });
        }
    }

    pub(in crate::tui) fn ensure_preview_message_visible(&mut self) {
        self.ensure_preview_layout(self.preview_wrap_width);
        let Some(layout) = self.preview_layout.as_ref() else {
            self.preview_scroll = 0;
            return;
        };
        if layout.messages.is_empty() {
            self.preview_scroll = 0;
            return;
        }
        let mut offset = self.preview_header_line_count();
        for index in 0..layout.messages.len() {
            let block_lines = layout.message_height(index);
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
        0
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
        self.remember_selected_preview_child();
        self.ensure_preview_message_visible();
    }

    pub(in crate::tui) fn scroll_preview_lines(&mut self, delta: isize) {
        self.ensure_preview_layout(self.preview_wrap_width);
        let max_scroll = self
            .preview_total_line_count_from_layout()
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
            self.remember_selected_preview_child();
        }
    }

    fn preview_total_line_count_from_layout(&self) -> usize {
        let mut lines = self.preview_header_line_count();
        match self.preview_layout.as_ref() {
            Some(layout) if !layout.messages.is_empty() => {
                lines += (0..layout.messages.len())
                    .map(|index| layout.message_height(index))
                    .sum::<usize>();
            }
            _ => lines += 1,
        }
        lines
    }

    fn preview_message_cursor_at_line(&self, line: usize) -> Option<usize> {
        let layout = self.preview_layout.as_ref()?;
        if layout.messages.is_empty() {
            return None;
        }
        let mut offset = self.preview_header_line_count();
        for index in 0..layout.messages.len() {
            let block_lines = layout.message_height(index);
            if line < offset + block_lines {
                return Some(index);
            }
            offset += block_lines;
        }
        Some(layout.messages.len() - 1)
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

    pub(in crate::tui) fn move_session_selection(&mut self, delta: isize) {
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

    pub(in crate::tui) fn open_delete_session(&mut self) {
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

    pub(super) fn on_session_preview_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Left if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.move_preview_parent()
            }
            KeyCode::Right if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.move_preview_child()
            }
            KeyCode::Left if key.modifiers.contains(KeyModifiers::ALT) => {
                self.move_preview_branch(-1)
            }
            KeyCode::Right if key.modifiers.contains(KeyModifiers::ALT) => {
                self.move_preview_branch(1)
            }
            KeyCode::Tab => self.toggle_preview_fold(),
            KeyCode::Up | KeyCode::Char('k') => self.move_preview_message(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_preview_message(1),
            KeyCode::PageUp => {
                let page = self.preview_viewport_height.saturating_sub(1).max(1) as isize;
                self.scroll_preview_lines(-page);
            }
            KeyCode::PageDown => {
                let page = self.preview_viewport_height.saturating_sub(1).max(1) as isize;
                self.scroll_preview_lines(page);
            }
            KeyCode::Left => self.focus = Focus::Content,
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
