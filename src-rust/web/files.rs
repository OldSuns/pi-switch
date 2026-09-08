use std::{
    fs,
    path::{Component, Path, PathBuf},
};

use chrono::{DateTime, Utc};
use serde_json::{json, Value};

use crate::documents::{self, AppError, DeleteMethod, Result, SessionSummary};

use super::{invalid, output, WebCore};

impl WebCore {
    pub(super) fn list_backups(&self) -> Result<Value> {
        Ok(json!({
            "backups": documents::list_backups(&self.paths)?.iter().map(|backup| json!({
                "name": backup.name, "path": backup.path,
            })).collect::<Vec<_>>()
        }))
    }

    pub(super) fn restore_backup(&mut self, name: &str) -> Result<Value> {
        require_relative_id(name)?;
        if name.contains('/') {
            return Err(invalid(
                "backup name must be a file name without a directory",
            ));
        }
        let backup = documents::list_backups(&self.paths)?
            .into_iter()
            .find(|backup| backup.name == name)
            .ok_or_else(|| invalid("backup does not exist or is not a valid pi-switch backup"))?;
        contained_file(&self.paths.backups, Path::new(&backup.path))?;
        documents::restore_backup(&self.paths, &backup)?;
        self.fetched_models.clear();
        self.opencode_plan = None;
        self.snapshot_result()
    }

    pub(super) fn list_sessions(&self) -> Result<Value> {
        let sessions = documents::list_sessions_in(&self.sessions_root)?;
        let sessions = sessions
            .iter()
            .map(|session| self.session_json(session))
            .collect::<Result<Vec<_>>>()?;
        Ok(json!({
            "root": self.sessions_root.display().to_string(),
            "sessions": sessions,
        }))
    }

    fn session_json(&self, session: &SessionSummary) -> Result<Value> {
        Ok(json!({
            "id": session_id(&self.sessions_root, &session.path)?,
            "sessionId": session.id,
            "title": documents::session_display_title(session),
            "name": session.name,
            "cwd": session.cwd,
            "createdAt": DateTime::<Utc>::from(session.created).to_rfc3339(),
            "modifiedAt": DateTime::<Utc>::from(session.modified).to_rfc3339(),
            "messageCount": session.message_count,
            "firstMessage": session.first_message,
            "searchText": session.search_text,
        }))
    }

    pub(super) fn preview_session(&self, id: &str, user_only: bool) -> Result<Value> {
        let session = self.find_session(id)?;
        let preview = documents::load_preview(&session.path, user_only)?;
        Ok(output::preview(id, &preview))
    }

    pub(super) fn delete_session(&self, id: &str) -> Result<Value> {
        let session = self.find_session(id)?;
        let method = documents::delete_session_in(&session.path, &self.sessions_root)?;
        Ok(json!({
            "id": id,
            "method": match method { DeleteMethod::Trash => "trash", DeleteMethod::Unlink => "unlink" },
        }))
    }

    fn find_session(&self, id: &str) -> Result<SessionSummary> {
        require_relative_id(id)?;
        // Resolve only IDs emitted by the shared session reader, so a regular
        // file in this directory cannot be turned into a preview/delete target.
        for session in documents::list_sessions_in(&self.sessions_root)? {
            if session_id(&self.sessions_root, &session.path)? == id {
                contained_file(&self.sessions_root, &session.path)?;
                return Ok(session);
            }
        }
        Err(invalid(
            "session no longer exists or is not a valid Pi session",
        ))
    }
}

fn require_relative_id(id: &str) -> Result<()> {
    if id.is_empty()
        || id.contains(['\\', ':'])
        || id.chars().any(char::is_control)
        || id.split('/').any(|part| matches!(part, "" | "." | ".."))
        || Path::new(id)
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(invalid(
            "file ID must be a relative path without traversal components",
        ));
    }
    Ok(())
}

fn session_id(root: &Path, path: &Path) -> Result<String> {
    path.strip_prefix(root)
        .map_err(|_| invalid("session path is outside the sessions directory"))?
        .components()
        .map(|component| match component {
            Component::Normal(part) => part
                .to_str()
                .map(str::to_owned)
                .ok_or_else(|| invalid("session file name is not valid UTF-8")),
            _ => Err(invalid("session path contains an invalid component")),
        })
        .collect::<Result<Vec<_>>>()
        .map(|parts| parts.join("/"))
}

fn contained_file(root: &Path, path: &Path) -> Result<PathBuf> {
    let canonical_root = canonicalize(root)?;
    let canonical_path = canonicalize(path)?;
    if !canonical_path.starts_with(&canonical_root) || !canonical_path.is_file() {
        return Err(invalid(
            "file path is outside its configured directory or is not a file",
        ));
    }
    Ok(canonical_path)
}

fn canonicalize(path: &Path) -> Result<PathBuf> {
    fs::canonicalize(path).map_err(|source| AppError::Io {
        path: path.into(),
        source,
    })
}
