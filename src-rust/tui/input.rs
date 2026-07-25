use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::documents;

use super::API_TYPES;

pub(super) fn mask_secret(value: &str) -> String {
    if value.is_empty() {
        "not set".into()
    } else if value.starts_with('$') || value.starts_with('!') {
        value.into()
    } else {
        format!(
            "{}...{}",
            value.chars().take(4).collect::<String>(),
            value
                .chars()
                .rev()
                .take(4)
                .collect::<String>()
                .chars()
                .rev()
                .collect::<String>()
        )
    }
}

pub(super) fn truncate_width(value: &str, max_width: usize) -> String {
    if UnicodeWidthStr::width(value) <= max_width {
        return value.into();
    }
    if max_width <= 3 {
        return ".".repeat(max_width);
    }
    let mut output = String::new();
    let limit = max_width - 3;
    let mut width = 0;
    for character in value.chars() {
        let char_width = character.width().unwrap_or_default();
        if width + char_width > limit {
            break;
        }
        output.push(character);
        width += char_width;
    }
    output.push_str("...");
    output
}

pub(super) fn pad_width(value: &str, width: usize) -> String {
    format!(
        "{value}{}",
        " ".repeat(width.saturating_sub(UnicodeWidthStr::width(value)))
    )
}

pub(super) fn wrap_width(value: &str, max_width: usize) -> Vec<String> {
    let max_width = max_width.max(1);
    let mut lines = Vec::new();
    let mut line = String::new();
    let mut width = 0;
    for character in value.chars() {
        let character_width = character.width().unwrap_or_default();
        if !line.is_empty() && width + character_width > max_width {
            lines.push(line);
            line = String::new();
            width = 0;
        }
        line.push(character);
        width += character_width;
    }
    lines.push(line);
    lines
}

pub(super) fn moved(current: usize, delta: isize, len: usize) -> usize {
    if len == 0 {
        0
    } else if delta < 0 {
        current.saturating_sub(delta.unsigned_abs())
    } else {
        current.saturating_add(delta as usize).min(len - 1)
    }
}

pub(super) fn api_label(index: usize) -> &'static str {
    index
        .checked_sub(1)
        .and_then(|index| API_TYPES.get(index))
        .copied()
        .unwrap_or("inherit")
}

pub(super) fn api_from_index(index: usize) -> Option<String> {
    index
        .checked_sub(1)
        .and_then(|index| API_TYPES.get(index))
        .map(|api| (*api).into())
}

pub(super) fn parse_optional_object(
    value: &str,
    field: &str,
) -> documents::Result<Option<serde_json::Value>> {
    if value.trim().is_empty() {
        return Ok(None);
    }
    let parsed: serde_json::Value = serde_json::from_str(value)
        .map_err(|error| documents::AppError::Invalid(format!("invalid {field} JSON: {error}")))?;
    if !parsed.is_object() {
        return Err(documents::AppError::Invalid(format!(
            "{field} must be a JSON object"
        )));
    }
    Ok(Some(parsed))
}

pub(super) fn parse_positive_u64(value: &str, field: &str) -> documents::Result<u64> {
    let parsed = value
        .trim()
        .parse::<u64>()
        .map_err(|_| documents::AppError::Invalid(format!("{field} must be a positive integer")))?;
    if parsed == 0 {
        return Err(documents::AppError::Invalid(format!(
            "{field} must be greater than zero"
        )));
    }
    Ok(parsed)
}

pub(super) fn char_len(value: &str) -> usize {
    value.chars().count()
}

pub(super) fn byte_index(value: &str, char_index: usize) -> usize {
    value
        .char_indices()
        .nth(char_index)
        .map(|(index, _)| index)
        .unwrap_or(value.len())
}

pub(super) fn insert_char(value: &mut String, char_index: usize, character: char) {
    value.insert(byte_index(value, char_index), character);
}

pub(super) fn remove_char(value: &mut String, char_index: usize) {
    if char_index >= char_len(value) {
        return;
    }
    let start = byte_index(value, char_index);
    let end = byte_index(value, char_index + 1);
    value.replace_range(start..end, "");
}

pub(super) fn edit_text_key(value: &mut String, cursor: &mut usize, key: KeyEvent) {
    match key.code {
        KeyCode::Left => *cursor = cursor.saturating_sub(1),
        KeyCode::Right => *cursor = (*cursor + 1).min(char_len(value)),
        KeyCode::Home => *cursor = 0,
        KeyCode::End => *cursor = char_len(value),
        KeyCode::Backspace if *cursor > 0 => {
            *cursor -= 1;
            remove_char(value, *cursor);
        }
        KeyCode::Delete => remove_char(value, *cursor),
        KeyCode::Char(character) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            insert_char(value, *cursor, character);
            *cursor += 1;
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            value.clear();
            *cursor = 0;
        }
        _ => {}
    }
}

pub(super) fn with_cursor(value: &str, char_index: usize) -> String {
    let mut output = value.to_owned();
    output.insert(byte_index(value, char_index), '|');
    output
}
