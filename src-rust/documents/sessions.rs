use std::{
    env, fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::Command,
    time::SystemTime,
};

use chrono::{DateTime, Local};
use serde_json::Value;

use super::{AppError, Result};

const SEARCH_TEXT_LIMIT: usize = 8_192;
const PREVIEW_TEXT_LIMIT: usize = 4_096;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionSummary {
    pub path: PathBuf,
    pub id: String,
    pub cwd: String,
    pub name: Option<String>,
    pub created: SystemTime,
    pub modified: SystemTime,
    pub message_count: usize,
    pub first_message: String,
    pub search_text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreviewMessage {
    pub role: String,
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeleteMethod {
    Trash,
    Unlink,
}

pub fn sessions_root() -> Result<PathBuf> {
    if let Ok(dir) = env::var("PI_CODING_AGENT_SESSION_DIR") {
        let path = PathBuf::from(dir);
        if !path.as_os_str().is_empty() {
            return Ok(path);
        }
    }
    let agent_dir = if let Ok(dir) = env::var("PI_CODING_AGENT_DIR") {
        PathBuf::from(dir)
    } else {
        let home = dirs::home_dir()
            .ok_or_else(|| AppError::Invalid("home directory is unavailable".into()))?;
        home.join(".pi/agent")
    };
    Ok(agent_dir.join("sessions"))
}

pub fn list_sessions() -> Result<Vec<SessionSummary>> {
    list_sessions_in(&sessions_root()?)
}

pub fn list_sessions_in(root: &Path) -> Result<Vec<SessionSummary>> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    collect_session_files(root, &mut files)?;
    let mut sessions = files
        .into_iter()
        .filter_map(|path| read_session_summary(&path).ok().flatten())
        .collect::<Vec<_>>();
    sessions.sort_by_key(|b| std::cmp::Reverse(b.modified));
    Ok(sessions)
}

pub fn load_preview(path: &Path, user_only: bool) -> Result<Vec<PreviewMessage>> {
    let file = fs::File::open(path).map_err(|source| AppError::Io {
        path: path.into(),
        source,
    })?;
    let reader = BufReader::new(file);
    let mut messages = Vec::new();
    for line in reader.lines() {
        let line = line.map_err(|source| AppError::Io {
            path: path.into(),
            source,
        })?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(entry) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };
        if entry.get("type").and_then(Value::as_str) != Some("message") {
            continue;
        }
        let Some(message) = entry.get("message") else {
            continue;
        };
        let Some(role) = message.get("role").and_then(Value::as_str) else {
            continue;
        };
        if role != "user" && role != "assistant" {
            continue;
        }
        if user_only && role != "user" {
            continue;
        }
        let Some(text) = extract_text_content(message.get("content")) else {
            continue;
        };
        if text.trim().is_empty() {
            continue;
        }
        messages.push(PreviewMessage {
            role: role.into(),
            text: truncate_chars(&text, PREVIEW_TEXT_LIMIT),
        });
    }
    Ok(messages)
}

pub fn delete_session(path: &Path) -> Result<DeleteMethod> {
    delete_session_in(path, &sessions_root()?)
}

pub fn delete_session_in(path: &Path, root: &Path) -> Result<DeleteMethod> {
    let canonical_root = canonicalize_existing(root)?;
    let canonical_path = canonicalize_existing(path)?;
    if !canonical_path.starts_with(&canonical_root) {
        return Err(AppError::Invalid(
            "session path is outside the sessions directory".into(),
        ));
    }
    if !canonical_path.is_file() {
        return Err(AppError::Invalid(format!(
            "session file not found: {}",
            path.display()
        )));
    }

    if try_trash(&canonical_path) && !canonical_path.exists() {
        return Ok(DeleteMethod::Trash);
    }

    fs::remove_file(&canonical_path).map_err(|source| AppError::Io {
        path: canonical_path,
        source,
    })?;
    Ok(DeleteMethod::Unlink)
}

pub fn format_session_time(time: SystemTime) -> String {
    let datetime: DateTime<Local> = time.into();
    let now: DateTime<Local> = Local::now();
    if datetime.format("%Y").to_string() == now.format("%Y").to_string() {
        datetime.format("%m-%d %H:%M").to_string()
    } else {
        datetime.format("%Y-%m-%d").to_string()
    }
}

pub fn session_display_title(session: &SessionSummary) -> &str {
    session
        .name
        .as_deref()
        .filter(|name| !name.is_empty())
        .unwrap_or(session.first_message.as_str())
}

pub fn session_matches(session: &SessionSummary, query: &str, named_only: bool) -> bool {
    if named_only
        && session
            .name
            .as_ref()
            .is_none_or(|name| name.trim().is_empty())
    {
        return false;
    }
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return true;
    }
    session.search_text.to_lowercase().contains(&needle)
}

fn collect_session_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let entries = fs::read_dir(dir).map_err(|source| AppError::Io {
        path: dir.into(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| AppError::Io {
            path: dir.into(),
            source,
        })?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|source| AppError::Io {
            path: path.clone(),
            source,
        })?;
        if file_type.is_dir() {
            collect_session_files(&path, out)?;
        } else if file_type.is_file()
            && path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("jsonl"))
        {
            out.push(path);
        }
    }
    Ok(())
}

fn read_session_summary(path: &Path) -> Result<Option<SessionSummary>> {
    let file = fs::File::open(path).map_err(|source| AppError::Io {
        path: path.into(),
        source,
    })?;
    let metadata = file.metadata().map_err(|source| AppError::Io {
        path: path.into(),
        source,
    })?;
    let mtime = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
    let reader = BufReader::new(file);

    let mut header_id = None;
    let mut header_cwd = String::new();
    let mut created = None;
    let mut name = None;
    let mut message_count = 0usize;
    let mut first_message = String::new();
    let mut last_activity: Option<SystemTime> = None;
    let mut search_parts = Vec::new();
    let mut search_len = 0usize;
    let mut saw_header = false;

    for line in reader.lines() {
        let line = line.map_err(|source| AppError::Io {
            path: path.into(),
            source,
        })?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(entry) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };
        if !saw_header {
            if entry.get("type").and_then(Value::as_str) != Some("session") {
                return Ok(None);
            }
            saw_header = true;
            header_id = entry.get("id").and_then(Value::as_str).map(str::to_owned);
            header_cwd = entry
                .get("cwd")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            created = entry
                .get("timestamp")
                .and_then(Value::as_str)
                .and_then(parse_iso_time);
            if let Some(id) = header_id.as_deref() {
                push_search(&mut search_parts, &mut search_len, id);
            }
            push_search(&mut search_parts, &mut search_len, &header_cwd);
            continue;
        }

        match entry.get("type").and_then(Value::as_str) {
            Some("session_info") => {
                name = entry
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned);
                if let Some(name) = name.as_deref() {
                    push_search(&mut search_parts, &mut search_len, name);
                }
            }
            Some("message") => {
                message_count += 1;
                let Some(message) = entry.get("message") else {
                    continue;
                };
                if let Some(activity) = message_activity_time(&entry, message) {
                    last_activity = Some(match last_activity {
                        Some(previous) => previous.max(activity),
                        None => activity,
                    });
                }
                let Some(role) = message.get("role").and_then(Value::as_str) else {
                    continue;
                };
                if role != "user" && role != "assistant" {
                    continue;
                }
                let Some(text) = extract_text_content(message.get("content")) else {
                    continue;
                };
                if text.trim().is_empty() {
                    continue;
                }
                push_search(&mut search_parts, &mut search_len, &text);
                if first_message.is_empty() && role == "user" {
                    first_message = text;
                }
            }
            _ => {}
        }
    }

    if !saw_header {
        return Ok(None);
    }
    let Some(id) = header_id else {
        return Ok(None);
    };
    let created = created.unwrap_or(mtime);
    let modified = last_activity.unwrap_or(created.max(mtime));
    let first_message = if first_message.is_empty() {
        "(no messages)".into()
    } else {
        first_message
    };
    if name.is_none() {
        // keep first_message searchable even when unnamed
    }
    push_search(&mut search_parts, &mut search_len, &first_message);

    Ok(Some(SessionSummary {
        path: path.to_path_buf(),
        id,
        cwd: header_cwd,
        name,
        created,
        modified,
        message_count,
        first_message,
        search_text: search_parts.join(" "),
    }))
}

fn message_activity_time(entry: &Value, message: &Value) -> Option<SystemTime> {
    if let Some(ms) = message.get("timestamp").and_then(Value::as_i64) {
        return unix_ms(ms);
    }
    if let Some(ms) = message.get("timestamp").and_then(Value::as_u64) {
        return unix_ms(ms as i64);
    }
    entry
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(parse_iso_time)
}

fn extract_text_content(content: Option<&Value>) -> Option<String> {
    let content = content?;
    match content {
        Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(text.clone())
            }
        }
        Value::Array(parts) => {
            let mut texts = Vec::new();
            for part in parts {
                if part.get("type").and_then(Value::as_str) == Some("text") {
                    if let Some(text) = part.get("text").and_then(Value::as_str) {
                        if !text.trim().is_empty() {
                            texts.push(text);
                        }
                    }
                }
            }
            if texts.is_empty() {
                None
            } else {
                Some(texts.join("\n"))
            }
        }
        _ => None,
    }
}

fn push_search(parts: &mut Vec<String>, used: &mut usize, text: &str) {
    if *used >= SEARCH_TEXT_LIMIT {
        return;
    }
    let remaining = SEARCH_TEXT_LIMIT - *used;
    let piece = truncate_chars(text, remaining);
    *used += piece.chars().count();
    parts.push(piece);
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_owned();
    }
    text.chars().take(max_chars).collect()
}

fn parse_iso_time(value: &str) -> Option<SystemTime> {
    DateTime::parse_from_rfc3339(value).ok().map(|dt| {
        let millis = dt.timestamp_millis();
        if millis >= 0 {
            SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(millis as u64)
        } else {
            SystemTime::UNIX_EPOCH
        }
    })
}

fn unix_ms(ms: i64) -> Option<SystemTime> {
    if ms < 0 {
        return None;
    }
    Some(SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(ms as u64))
}

fn canonicalize_existing(path: &Path) -> Result<PathBuf> {
    path.canonicalize().map_err(|source| AppError::Io {
        path: path.into(),
        source,
    })
}

fn try_trash(path: &Path) -> bool {
    let path_str = path.to_string_lossy();
    let args = if path_str.starts_with('-') {
        vec!["--".to_owned(), path_str.into_owned()]
    } else {
        vec![path_str.into_owned()]
    };
    match Command::new("trash").args(&args).output() {
        Ok(output) => output.status.success() || !path.exists(),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        env, fs, process,
        sync::atomic::{AtomicUsize, Ordering},
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    static FIXTURE_ID: AtomicUsize = AtomicUsize::new(0);

    fn temp_root() -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        env::temp_dir().join(format!(
            "pi-switch-sessions-{}-{}-{}",
            process::id(),
            stamp,
            FIXTURE_ID.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn write_session(dir: &Path, name: &str, body: &str) -> PathBuf {
        fs::create_dir_all(dir).unwrap();
        let path = dir.join(name);
        fs::write(&path, body).unwrap();
        path
    }

    #[test]
    fn parses_summary_name_first_message_and_counts() {
        let root = temp_root();
        let project = root.join("--proj--");
        let path = write_session(
            &project,
            "a.jsonl",
            r#"{"type":"session","version":3,"id":"sess-1","timestamp":"2026-01-01T00:00:00.000Z","cwd":"/tmp/proj"}
{"type":"session_info","id":"i1","parentId":null,"timestamp":"2026-01-01T00:00:01.000Z","name":"  Auth refactor  "}
{"type":"model_change","id":"m1","parentId":"i1","timestamp":"2026-01-01T00:00:02.000Z","provider":"x","modelId":"y"}
{"type":"message","id":"u1","parentId":"m1","timestamp":"2026-01-01T00:01:00.000Z","message":{"role":"user","content":"Hello world","timestamp":1704067260000}}
{"type":"message","id":"a1","parentId":"u1","timestamp":"2026-01-01T00:02:00.000Z","message":{"role":"assistant","content":[{"type":"text","text":"Hi there"}],"timestamp":1704067320000}}
{"type":"message","id":"t1","parentId":"a1","timestamp":"2026-01-01T00:02:01.000Z","message":{"role":"toolResult","content":[{"type":"text","text":"ignored"}]}}
"#,
        );

        let summary = read_session_summary(&path).unwrap().unwrap();
        assert_eq!(summary.id, "sess-1");
        assert_eq!(summary.cwd, "/tmp/proj");
        assert_eq!(summary.name.as_deref(), Some("Auth refactor"));
        assert_eq!(summary.first_message, "Hello world");
        assert_eq!(summary.message_count, 3);
        assert!(summary.search_text.contains("Auth refactor"));
        assert!(summary.search_text.contains("Hello world"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn list_filters_and_preview_user_only() {
        let root = temp_root();
        let project = root.join("--proj--");
        write_session(
            &project,
            "named.jsonl",
            r#"{"type":"session","version":3,"id":"n1","timestamp":"2026-01-02T00:00:00.000Z","cwd":"/work/alpha"}
{"type":"session_info","id":"i1","parentId":null,"timestamp":"2026-01-02T00:00:01.000Z","name":"Named Task"}
{"type":"message","id":"u1","parentId":"i1","timestamp":"2026-01-02T00:01:00.000Z","message":{"role":"user","content":[{"type":"text","text":"named user"}],"timestamp":1}}
{"type":"message","id":"a1","parentId":"u1","timestamp":"2026-01-02T00:02:00.000Z","message":{"role":"assistant","content":[{"type":"text","text":"named assistant"}],"timestamp":2}}
"#,
        );
        write_session(
            &project,
            "plain.jsonl",
            r#"{"type":"session","version":3,"id":"p1","timestamp":"2026-01-01T00:00:00.000Z","cwd":"/work/beta"}
{"type":"message","id":"u1","parentId":null,"timestamp":"2026-01-01T00:01:00.000Z","message":{"role":"user","content":"plain user","timestamp":1}}
"#,
        );
        write_session(
            &project,
            "broken.jsonl",
            "not-json\n{\"type\":\"message\"}\n",
        );

        let sessions = list_sessions_in(&root).unwrap();
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].id, "n1"); // newer modified first
        assert!(session_matches(&sessions[0], "named", false));
        assert!(session_matches(&sessions[0], "ALPHA", false));
        assert!(!session_matches(&sessions[1], "named", true));
        assert!(session_matches(&sessions[0], "", true));

        let preview = load_preview(&sessions[0].path, false).unwrap();
        assert_eq!(preview.len(), 2);
        assert_eq!(preview[0].role, "user");
        assert_eq!(preview[0].text, "named user");
        assert_eq!(preview[1].role, "assistant");

        let user_only = load_preview(&sessions[0].path, true).unwrap();
        assert_eq!(user_only.len(), 1);
        assert_eq!(user_only[0].role, "user");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn delete_rejects_outside_root_and_removes_inside() {
        let root = temp_root();
        let project = root.join("--proj--");
        let path = write_session(
            &project,
            "del.jsonl",
            r#"{"type":"session","version":3,"id":"d1","timestamp":"2026-01-01T00:00:00.000Z","cwd":"/tmp"}
{"type":"message","id":"u1","parentId":null,"timestamp":"2026-01-01T00:01:00.000Z","message":{"role":"user","content":"bye"}}
"#,
        );
        let outside = env::temp_dir().join(format!("pi-switch-outside-{}.jsonl", process::id()));
        fs::write(&outside, "x").unwrap();

        let err = delete_session_in(&outside, &root).unwrap_err().to_string();
        assert!(err.contains("outside"));

        let method = delete_session_in(&path, &root).unwrap();
        assert!(matches!(method, DeleteMethod::Trash | DeleteMethod::Unlink));
        assert!(!path.exists());
        let _ = fs::remove_file(outside);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn no_messages_uses_placeholder() {
        let root = temp_root();
        let path = write_session(
            &root,
            "empty.jsonl",
            r#"{"type":"session","version":3,"id":"e1","timestamp":"2026-01-01T00:00:00.000Z","cwd":"/tmp"}
{"type":"model_change","id":"m1","parentId":null,"timestamp":"2026-01-01T00:00:01.000Z","provider":"x","modelId":"y"}
"#,
        );
        let summary = read_session_summary(&path).unwrap().unwrap();
        assert_eq!(summary.first_message, "(no messages)");
        assert_eq!(summary.message_count, 0);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn format_time_is_non_empty() {
        let text = format_session_time(SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000));
        assert!(!text.is_empty());
    }
}
