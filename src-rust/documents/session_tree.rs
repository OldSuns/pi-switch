use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{BufRead, BufReader},
    path::Path,
};

use chrono::DateTime;
use serde_json::Value;

use super::{
    sessions::{extract_text_content, truncate_chars},
    AppError, Result,
};

const PREVIEW_TEXT_LIMIT: usize = 4_096;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TreeGutter {
    pub position: usize,
    pub show: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PreviewTreePosition {
    pub parent_id: Option<String>,
    pub level: usize,
    pub indent: usize,
    pub show_connector: bool,
    pub is_last: bool,
    pub gutters: Vec<TreeGutter>,
    pub active_path: bool,
    pub has_children: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreviewMessage {
    pub id: String,
    pub role: String,
    pub text: String,
    pub label: Option<String>,
    pub tree: PreviewTreePosition,
}

impl PreviewMessage {
    #[cfg(test)]
    pub(crate) fn new(
        id: impl Into<String>,
        parent_id: Option<String>,
        role: impl Into<String>,
        text: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            role: role.into(),
            text: text.into(),
            label: None,
            tree: PreviewTreePosition {
                parent_id,
                ..Default::default()
            },
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionPreview {
    pub messages: Vec<PreviewMessage>,
    pub active_leaf_id: Option<String>,
    pub active_message_id: Option<String>,
    pub branch_points: usize,
}

impl SessionPreview {
    pub(crate) fn active_index(&self) -> Option<usize> {
        self.active_message_id
            .as_ref()
            .and_then(|id| self.message_index(id))
    }

    pub(crate) fn message_index(&self, id: &str) -> Option<usize> {
        self.messages.iter().position(|message| message.id == id)
    }

    pub(crate) fn parent_index(&self, index: usize) -> Option<usize> {
        let parent_id = self.messages.get(index)?.tree.parent_id.as_deref()?;
        self.message_index(parent_id)
    }

    pub(crate) fn parent_branch(&self, index: usize) -> Option<(usize, usize)> {
        let indent = self.messages.get(index)?.tree.indent;
        if indent == 0 {
            return None;
        }
        let mut branch_root = index;
        let mut cursor = index;
        while let Some(parent) = self.parent_index(cursor) {
            if self.messages[parent].tree.indent < indent {
                return Some((parent, branch_root));
            }
            branch_root = parent;
            cursor = parent;
        }
        None
    }

    pub(crate) fn child_branch_index(
        &self,
        index: usize,
        preferred_id: Option<&str>,
    ) -> Option<usize> {
        let child_indent = self.messages.get(index)?.tree.indent + 1;
        let is_child_branch = |candidate: usize| {
            self.messages
                .get(candidate)
                .is_some_and(|entry| entry.tree.indent == child_indent)
                && self.is_descendant_of(candidate, index)
        };
        if let Some(candidate) = preferred_id.and_then(|id| self.message_index(id)) {
            if is_child_branch(candidate) {
                return Some(candidate);
            }
        }
        ((index + 1)..self.messages.len()).find(|candidate| is_child_branch(*candidate))
    }

    fn is_descendant_of(&self, candidate: usize, ancestor: usize) -> bool {
        let mut cursor = Some(candidate);
        while let Some(index) = cursor {
            if index == ancestor {
                return true;
            }
            cursor = self.parent_index(index);
        }
        false
    }

    pub(crate) fn branch_position(&self, index: usize) -> Option<(usize, usize)> {
        let (siblings, position) = self.branch_context(index)?;
        Some((position + 1, siblings.len()))
    }

    pub(crate) fn adjacent_branch_index(&self, index: usize, delta: isize) -> Option<usize> {
        let (siblings, position) = self.branch_context(index)?;
        let next = position as isize + delta;
        if !(0..siblings.len() as isize).contains(&next) {
            return None;
        }
        siblings.get(next as usize).copied()
    }

    pub(crate) fn direct_child_count(&self, index: usize) -> usize {
        let Some(message) = self.messages.get(index) else {
            return 0;
        };
        self.messages
            .iter()
            .filter(|item| item.tree.parent_id.as_deref() == Some(message.id.as_str()))
            .count()
    }

    pub(crate) fn descendant_count(&self, index: usize) -> usize {
        let Some(message) = self.messages.get(index) else {
            return 0;
        };
        self.messages
            .iter()
            .skip(index + 1)
            .take_while(|item| item.tree.level > message.tree.level)
            .count()
    }

    fn branch_context(&self, index: usize) -> Option<(Vec<usize>, usize)> {
        let by_id = self
            .messages
            .iter()
            .enumerate()
            .map(|(index, message)| (message.id.as_str(), index))
            .collect::<HashMap<_, _>>();
        let mut by_parent = HashMap::<Option<&str>, Vec<usize>>::new();
        for (index, message) in self.messages.iter().enumerate() {
            by_parent
                .entry(message.tree.parent_id.as_deref())
                .or_default()
                .push(index);
        }

        let mut branch = index;
        loop {
            let parent_id = self.messages.get(branch)?.tree.parent_id.as_deref();
            let siblings = by_parent.get(&parent_id)?;
            if siblings.len() > 1 {
                let position = siblings.iter().position(|candidate| *candidate == branch)?;
                return Some((siblings.clone(), position));
            }
            branch = *by_id.get(parent_id?)?;
        }
    }

    #[cfg(test)]
    pub(crate) fn from_messages(messages: Vec<PreviewMessage>) -> Self {
        let active_leaf_id = messages.last().map(|message| message.id.clone());
        let active_message_id = active_leaf_id.clone();
        Self {
            messages,
            active_leaf_id,
            active_message_id,
            branch_points: 0,
        }
    }
}

struct RawEntry {
    id: String,
    parent_id: Option<String>,
    order: usize,
    timestamp: Option<i64>,
    visible: Option<VisibleEntry>,
}

struct VisibleEntry {
    role: String,
    text: String,
}

pub fn load_preview(path: &Path, user_only: bool) -> Result<SessionPreview> {
    let file = fs::File::open(path).map_err(|source| AppError::Io {
        path: path.into(),
        source,
    })?;
    let reader = BufReader::new(file);
    let mut version = 1u64;
    let mut raw_entries = Vec::new();
    let mut labels = HashMap::<String, String>::new();
    let mut previous_id = None;
    let mut used_ids = HashSet::new();

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
        let entry_type = entry
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if entry_type == "session" {
            version = entry.get("version").and_then(Value::as_u64).unwrap_or(1);
            continue;
        }

        let order = raw_entries.len();
        let id = if let Some(id) = entry
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
        {
            let id = id.to_owned();
            if !used_ids.insert(id.clone()) {
                continue;
            }
            id
        } else {
            unique_id(&mut used_ids, order)
        };
        let parent_id = if version < 2 {
            previous_id.clone()
        } else {
            entry
                .get("parentId")
                .and_then(Value::as_str)
                .filter(|id| {
                    !id.is_empty()
                        && *id != entry.get("id").and_then(Value::as_str).unwrap_or_default()
                })
                .map(str::to_owned)
        };
        let visible = visible_entry(&entry, user_only);
        if entry_type == "label" {
            if let Some(target) = entry.get("targetId").and_then(Value::as_str) {
                if let Some(label) = entry
                    .get("label")
                    .and_then(Value::as_str)
                    .filter(|label| !label.is_empty())
                {
                    labels.insert(target.to_owned(), label.to_owned());
                } else {
                    labels.remove(target);
                }
            }
        }
        previous_id = Some(id.clone());
        raw_entries.push(RawEntry {
            id,
            parent_id,
            order,
            timestamp: timestamp_key(entry.get("timestamp")),
            visible,
        });
    }

    build_preview(raw_entries, labels)
}

fn unique_id(used_ids: &mut HashSet<String>, order: usize) -> String {
    let base = format!("legacy-{order}");
    if used_ids.insert(base.clone()) {
        return base;
    }
    let mut suffix = 1;
    loop {
        let candidate = format!("{base}-{suffix}");
        if used_ids.insert(candidate.clone()) {
            return candidate;
        }
        suffix += 1;
    }
}

fn timestamp_key(value: Option<&Value>) -> Option<i64> {
    value.and_then(Value::as_i64).or_else(|| {
        value
            .and_then(Value::as_str)
            .and_then(|text| DateTime::parse_from_rfc3339(text).ok())
            .map(|time| time.timestamp_millis())
    })
}

fn visible_entry(entry: &Value, user_only: bool) -> Option<VisibleEntry> {
    match entry.get("type").and_then(Value::as_str) {
        Some("message") => {
            let message = entry.get("message")?;
            let role = message.get("role").and_then(Value::as_str)?;
            let keep_role = match role {
                "user" => true,
                "assistant" => !user_only,
                _ => false,
            };
            if !keep_role {
                return None;
            }
            let text = extract_text_content(message.get("content"))?;
            (!text.trim().is_empty()).then(|| VisibleEntry {
                role: role.into(),
                text,
            })
        }
        Some("branch_summary") if !user_only => entry
            .get("summary")
            .and_then(Value::as_str)
            .filter(|text| !text.trim().is_empty())
            .map(|text| VisibleEntry {
                role: "branchSummary".into(),
                text: text.into(),
            }),
        Some("compaction") if !user_only => entry
            .get("summary")
            .and_then(Value::as_str)
            .filter(|text| !text.trim().is_empty())
            .map(|text| VisibleEntry {
                role: "compaction".into(),
                text: text.into(),
            }),
        Some("custom_message")
            if !user_only
                && entry
                    .get("display")
                    .and_then(Value::as_bool)
                    .unwrap_or(false) =>
        {
            let text = extract_text_content(entry.get("content"))?;
            (!text.trim().is_empty()).then(|| VisibleEntry {
                role: "custom".into(),
                text,
            })
        }
        _ => None,
    }
}

fn build_preview(raw: Vec<RawEntry>, labels: HashMap<String, String>) -> Result<SessionPreview> {
    if raw.is_empty() {
        return Ok(SessionPreview::default());
    }
    let by_id = raw
        .iter()
        .enumerate()
        .map(|(index, entry)| (entry.id.as_str(), index))
        .collect::<HashMap<_, _>>();
    let leaf_index = raw
        .iter()
        .rposition(|entry| entry.visible.is_some())
        .unwrap_or(raw.len() - 1);
    let mut active_path = HashSet::new();
    let mut cursor = Some(leaf_index);
    while let Some(index) = cursor {
        if !active_path.insert(index) {
            break;
        }
        cursor = raw[index]
            .parent_id
            .as_deref()
            .and_then(|id| by_id.get(id).copied());
    }

    let visible_raw = raw
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| entry.visible.as_ref().map(|_| index))
        .collect::<Vec<_>>();
    let visible_set = visible_raw.iter().copied().collect::<HashSet<_>>();
    let mut visible_parent = vec![None; raw.len()];
    for &index in &visible_raw {
        let mut parent = raw[index]
            .parent_id
            .as_deref()
            .and_then(|id| by_id.get(id).copied());
        let mut nearest = None;
        let mut seen = HashSet::from([index]);
        let mut cycle = false;
        while let Some(parent_index) = parent {
            if !seen.insert(parent_index) {
                cycle = true;
                break;
            }
            if nearest.is_none() && visible_set.contains(&parent_index) {
                nearest = Some(parent_index);
            }
            parent = raw[parent_index]
                .parent_id
                .as_deref()
                .and_then(|id| by_id.get(id).copied());
        }
        if !cycle {
            visible_parent[index] = nearest;
        }
    }
    let mut visible_children = vec![Vec::<usize>::new(); raw.len()];
    let mut visible_roots = Vec::new();
    for &index in &visible_raw {
        if let Some(parent) = visible_parent[index] {
            visible_children[parent].push(index);
        } else {
            visible_roots.push(index);
        }
    }
    let order = |a: &usize, b: &usize| {
        active_path
            .contains(b)
            .cmp(&active_path.contains(a))
            .then_with(|| raw[*a].timestamp.cmp(&raw[*b].timestamp))
            .then_with(|| raw[*a].order.cmp(&raw[*b].order))
    };
    visible_roots.sort_by(order);
    for &index in &visible_raw {
        visible_children[index].sort_by(order);
    }

    let active_message_raw = nearest_visible(leaf_index, &raw, &by_id, &visible_set);
    let mut messages = Vec::with_capacity(visible_raw.len());
    type StackItem = (usize, usize, usize, bool, bool, Vec<TreeGutter>);
    let mut stack: Vec<StackItem> = Vec::new();
    for index in (0..visible_roots.len()).rev() {
        let is_last = index == visible_roots.len() - 1;
        stack.push((visible_roots[index], 0, 0, false, is_last, Vec::new()));
    }
    while let Some((index, indent, level, show_connector, is_last, gutters)) = stack.pop() {
        let parent_id = visible_parent[index].map(|parent| raw[parent].id.clone());
        let child_indices = &visible_children[index];
        let multiple_children = child_indices.len() > 1;
        let child_gutters = if show_connector {
            let mut next = gutters.clone();
            next.push(TreeGutter {
                position: indent.saturating_sub(1),
                show: !is_last,
            });
            next
        } else {
            gutters.clone()
        };
        let visible = raw[index]
            .visible
            .as_ref()
            .expect("visible index has content");
        messages.push(PreviewMessage {
            id: raw[index].id.clone(),
            role: visible.role.clone(),
            text: truncate_chars(&visible.text, PREVIEW_TEXT_LIMIT),
            label: labels.get(&raw[index].id).cloned(),
            tree: PreviewTreePosition {
                parent_id,
                level,
                indent,
                show_connector,
                is_last,
                gutters,
                active_path: active_path.contains(&index),
                has_children: !child_indices.is_empty(),
            },
        });

        let child_indent = indent + usize::from(multiple_children);
        for child_index in (0..child_indices.len()).rev() {
            let child_is_last = child_index == child_indices.len() - 1;
            stack.push((
                child_indices[child_index],
                child_indent,
                level + 1,
                multiple_children,
                child_is_last,
                child_gutters.clone(),
            ));
        }
    }

    let active_message_id = active_message_raw.map(|index| raw[index].id.clone());
    let branch_points = visible_children
        .iter()
        .filter(|children| children.len() > 1)
        .count();
    Ok(SessionPreview {
        messages,
        active_leaf_id: Some(raw[leaf_index].id.clone()),
        active_message_id,
        branch_points,
    })
}

fn nearest_visible(
    start: usize,
    raw: &[RawEntry],
    by_id: &HashMap<&str, usize>,
    visible: &HashSet<usize>,
) -> Option<usize> {
    let mut current = Some(start);
    let mut seen = HashSet::new();
    while let Some(index) = current {
        if !seen.insert(index) {
            return None;
        }
        if visible.contains(&index) {
            return Some(index);
        }
        current = raw[index]
            .parent_id
            .as_deref()
            .and_then(|id| by_id.get(id).copied());
    }
    None
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    fn fixture(content: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "pi-switch-tree-{}.jsonl",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn builds_active_first_tree_with_branch_connectors_and_labels() {
        let path = fixture(
            r#"{"type":"session","version":3,"id":"s","timestamp":"2026-01-01T00:00:00Z","cwd":"/tmp"}
{"type":"message","id":"u1","parentId":null,"timestamp":"2026-01-01T00:00:01Z","message":{"role":"user","content":"root"}}
{"type":"message","id":"a1","parentId":"u1","timestamp":"2026-01-01T00:00:02Z","message":{"role":"assistant","content":[{"type":"text","text":"reply"}]}}
{"type":"message","id":"u-old","parentId":"a1","timestamp":"2026-01-01T00:00:03Z","message":{"role":"user","content":"old"}}
{"type":"message","id":"a-old","parentId":"u-old","timestamp":"2026-01-01T00:00:04Z","message":{"role":"assistant","content":[{"type":"text","text":"old reply"}]}}
{"type":"message","id":"u-new","parentId":"a1","timestamp":"2026-01-01T00:00:05Z","message":{"role":"user","content":"active"}}
{"type":"message","id":"a-new","parentId":"u-new","timestamp":"2026-01-01T00:00:06Z","message":{"role":"assistant","content":[{"type":"text","text":"active reply"}]}}
{"type":"label","id":"l1","parentId":"a-new","timestamp":"2026-01-01T00:00:07Z","targetId":"u-old","label":"alternative"}
"#,
        );
        let preview = load_preview(&path, false).unwrap();
        let ids = preview
            .messages
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ids, ["u1", "a1", "u-new", "a-new", "u-old", "a-old"]);
        assert_eq!(preview.active_message_id.as_deref(), Some("a-new"));
        assert_eq!(preview.active_leaf_id.as_deref(), Some("a-new"));
        assert_eq!(preview.branch_points, 1);
        assert!(!preview.messages[1].tree.show_connector);
        assert!(preview.messages[2].tree.show_connector);
        assert!(!preview.messages[3].tree.show_connector);
        assert_eq!(preview.messages[1].tree.indent, 0);
        assert_eq!(preview.messages[2].tree.indent, 1);
        assert_eq!(preview.messages[3].tree.indent, 1);
        assert_eq!(preview.messages[4].tree.indent, 1);
        assert_eq!(preview.messages[5].tree.indent, 1);
        assert!(preview.messages[4].tree.is_last);
        assert_eq!(preview.messages[4].label.as_deref(), Some("alternative"));
        assert_eq!(preview.parent_index(3), Some(2));
        assert_eq!(preview.parent_branch(3), Some((1, 2)));
        assert_eq!(preview.parent_branch(5), Some((1, 4)));
        assert_eq!(preview.child_branch_index(1, None), Some(2));
        assert_eq!(preview.child_branch_index(1, Some("u-old")), Some(4));
        assert_eq!(preview.child_branch_index(4, None), None);
        assert_eq!(preview.descendant_count(1), 4);
        assert_eq!(preview.branch_position(3), Some((1, 2)));
        assert_eq!(preview.adjacent_branch_index(3, 1), Some(4));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn user_filter_reconnects_children_to_nearest_visible_parent() {
        let path = fixture(
            r#"{"type":"session","version":3,"id":"s","timestamp":"2026-01-01T00:00:00Z","cwd":"/tmp"}
{"type":"message","id":"u1","parentId":null,"timestamp":"2026-01-01T00:00:01Z","message":{"role":"user","content":"root"}}
{"type":"message","id":"a1","parentId":"u1","timestamp":"2026-01-01T00:00:02Z","message":{"role":"assistant","content":[{"type":"text","text":"reply"}]}}
{"type":"message","id":"u2","parentId":"a1","timestamp":"2026-01-01T00:00:03Z","message":{"role":"user","content":"next"}}
"#,
        );
        let preview = load_preview(&path, true).unwrap();
        assert_eq!(preview.messages.len(), 2);
        assert_eq!(preview.messages[1].tree.parent_id.as_deref(), Some("u1"));
        assert_eq!(preview.messages[1].tree.level, 1);
        assert_eq!(preview.messages[0].tree.indent, 0);
        assert_eq!(preview.messages[1].tree.indent, 0);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn legacy_sessions_are_treated_as_a_linear_tree() {
        let path = fixture(
            r#"{"type":"session","id":"s","timestamp":"2026-01-01T00:00:00Z","cwd":"/tmp"}
{"type":"message","message":{"role":"user","content":"one"}}
{"type":"message","message":{"role":"assistant","content":[{"type":"text","text":"two"}]}}
"#,
        );
        let preview = load_preview(&path, false).unwrap();
        assert_eq!(preview.messages.len(), 2);
        assert_eq!(
            preview.messages[1].tree.parent_id.as_deref(),
            Some("legacy-0")
        );
        let _ = fs::remove_file(path);
    }
}
