use crate::documents::{
    self, AppError, ModelDefaults, ModelDraft, ModelView, ProviderDraft, ProviderView,
    USER_AGENT_HEADER,
};
use serde_json::{Map, Value};

const SEND_SESSION_AFFINITY_HEADERS: &str = "sendSessionAffinityHeaders";
const REQUIRES_REASONING_CONTENT: &str = "requiresReasoningContentOnAssistantMessages";
const THINKING_FORMAT: &str = "thinkingFormat";
const SUPPORTS_LONG_CACHE_RETENTION: &str = "supportsLongCacheRetention";
const SUPPORTS_STORE: &str = "supportsStore";
const SUPPORTS_DEVELOPER_ROLE: &str = "supportsDeveloperRole";
const SUPPORTS_REASONING_EFFORT: &str = "supportsReasoningEffort";
const MAX_TOKENS_FIELD: &str = "maxTokensField";
const SUPPORTS_STRICT_MODE: &str = "supportsStrictMode";

/// thinkingFormat cycle values (index 0 = inherit / do not write).
pub(super) const THINKING_FORMATS: [&str; 7] = [
    "",
    "openai",
    "openrouter",
    "deepseek",
    "together",
    "zai",
    "qwen",
];

/// maxTokensField cycle values (index 0 = inherit / do not write).
pub(super) const MAX_TOKENS_FIELDS: [&str; 3] = ["", "max_completion_tokens", "max_tokens"];

/// Preset cycle labels (index 0 = inherit / none).
pub(super) const PRESETS: [&str; 5] = ["", "deepseek", "qwen", "zai", "openrouter"];

use super::{
    input::{api_from_index, char_len, parse_optional_object, parse_positive_u64},
    API_TYPES,
};

#[derive(Clone)]
pub(super) struct FormState {
    pub(super) previous_id: Option<String>,
    pub(super) id: String,
    pub(super) base_url: String,
    pub(super) api: usize,
    pub(super) api_key: String,
    pub(super) auth_header: bool,
    pub(super) in_pi: bool,
    pub(super) user_agent: String,
    pub(super) headers_json: String,
    pub(super) editing_headers: bool,
    pub(super) headers_field: usize,
    // compat secondary menu
    pub(super) editing_compat: bool,
    pub(super) compat_field: usize,
    pub(super) preset: usize,
    pub(super) requires_reasoning_content: Option<bool>,
    pub(super) thinking_format: usize,
    pub(super) supports_long_cache_retention: Option<bool>,
    pub(super) supports_store: Option<bool>,
    pub(super) supports_developer_role: Option<bool>,
    pub(super) supports_reasoning_effort: Option<bool>,
    pub(super) max_tokens_field: usize,
    pub(super) supports_strict_mode: Option<bool>,
    pub(super) send_session_affinity_headers: bool,
    pub(super) other_compat_json: String,
    pub(super) field: usize,
    pub(super) cursor: usize,
    pub(super) show_help: bool,
}

impl FormState {
    pub(super) fn add() -> Self {
        Self {
            previous_id: None,
            id: String::new(),
            base_url: String::new(),
            api: 1,
            api_key: String::new(),
            auth_header: true,
            in_pi: true,
            user_agent: String::new(),
            headers_json: String::new(),
            editing_headers: false,
            headers_field: 0,
            editing_compat: false,
            compat_field: 0,
            preset: 0,
            requires_reasoning_content: None,
            thinking_format: 0,
            supports_long_cache_retention: None,
            supports_store: None,
            supports_developer_role: None,
            supports_reasoning_effort: None,
            max_tokens_field: 0,
            supports_strict_mode: None,
            send_session_affinity_headers: true,
            other_compat_json: String::new(),
            field: 0,
            cursor: 0,
            show_help: false,
        }
    }

    pub(super) fn edit(provider: &ProviderView) -> Self {
        let (user_agent, headers_json) = split_headers(provider);
        let compat = split_compat_fields(provider);
        let mut form = Self {
            previous_id: Some(provider.id.clone()),
            id: provider.id.clone(),
            base_url: provider.base_url.clone(),
            api: API_TYPES
                .iter()
                .position(|api| *api == provider.api)
                .map(|index| index + 1)
                .unwrap_or_default(),
            api_key: provider.api_key.clone(),
            auth_header: provider.auth_header,
            in_pi: provider.in_pi,
            user_agent,
            headers_json,
            editing_headers: false,
            headers_field: 0,
            editing_compat: false,
            compat_field: 0,
            preset: compat.preset,
            requires_reasoning_content: compat.requires_reasoning_content,
            thinking_format: compat.thinking_format,
            supports_long_cache_retention: compat.supports_long_cache_retention,
            supports_store: compat.supports_store,
            supports_developer_role: compat.supports_developer_role,
            supports_reasoning_effort: compat.supports_reasoning_effort,
            max_tokens_field: compat.max_tokens_field,
            supports_strict_mode: compat.supports_strict_mode,
            send_session_affinity_headers: compat.send_session_affinity_headers,
            other_compat_json: compat.other_json,
            field: 0,
            cursor: 0,
            show_help: false,
        };
        form.cursor = form.current_len();
        form
    }

    pub(super) fn current_len(&self) -> usize {
        self.current_text().map(char_len).unwrap_or_default()
    }

    pub(super) fn current_text(&self) -> Option<&str> {
        if self.editing_compat {
            return (self.compat_field == 10).then_some(&self.other_compat_json);
        }
        match self.field {
            0 => Some(&self.id),
            1 => Some(&self.base_url),
            3 => Some(&self.api_key),
            _ => None,
        }
    }

    pub(super) fn current_text_mut(&mut self) -> Option<&mut String> {
        if self.editing_compat {
            return (self.compat_field == 10).then_some(&mut self.other_compat_json);
        }
        match self.field {
            0 => Some(&mut self.id),
            1 => Some(&mut self.base_url),
            3 => Some(&mut self.api_key),
            _ => None,
        }
    }

    pub(super) fn select_field(&mut self, next: usize) {
        self.field = next % 8;
        self.cursor = self.current_len();
    }

    pub(super) fn select_headers_field(&mut self, next: usize) {
        self.headers_field = next % 2;
        self.cursor = char_len(if self.headers_field == 0 {
            &self.user_agent
        } else {
            &self.headers_json
        });
    }

    pub(super) fn select_compat_field(&mut self, next: usize) {
        self.compat_field = next % 11;
        self.cursor = if self.compat_field == 10 {
            char_len(&self.other_compat_json)
        } else {
            0
        };
    }

    pub(super) fn header_names(&self) -> Result<Vec<String>, ()> {
        let mut names = Vec::new();
        if !self.user_agent.trim().is_empty() {
            names.push(USER_AGENT_HEADER.into());
        }
        if !self.headers_json.trim().is_empty() {
            let value: Value = serde_json::from_str(&self.headers_json).map_err(|_| ())?;
            let headers = value.as_object().ok_or(())?;
            let mut others = headers
                .keys()
                .filter(|name| !name.eq_ignore_ascii_case(USER_AGENT_HEADER))
                .cloned()
                .collect::<Vec<_>>();
            others.sort_by_key(|name| name.to_ascii_lowercase());
            names.extend(others);
        }
        Ok(names)
    }

    pub(super) fn draft(&self) -> documents::Result<ProviderDraft> {
        let headers = merge_user_agent(
            parse_optional_object(&self.headers_json, "headers")?,
            &self.user_agent,
        )?;
        Ok(ProviderDraft {
            id: self.id.trim().into(),
            in_pi: self.in_pi,
            base_url: self.base_url.trim().into(),
            api: api_from_index(self.api),
            api_key: self.api_key.trim().into(),
            auth_header: self.auth_header,
            headers,
            compat: self.merge_compat()?,
        })
    }

    /// Apply the currently-selected preset to the structured compat fields.
    /// Overwrites only the fields the preset mentions; leaves others untouched.
    /// Selecting preset index 0 (default/none) resets all structured fields
    /// to their default (inherit/None) state, but preserves session affinity
    /// and other compat JSON.
    pub(super) fn apply_preset(&mut self) {
        let Some(preset) = PRESETS.get(self.preset).copied() else {
            return;
        };
        match preset {
            "" => {
                self.requires_reasoning_content = None;
                self.thinking_format = 0;
                self.supports_long_cache_retention = None;
                self.supports_store = None;
                self.supports_developer_role = None;
                self.supports_reasoning_effort = None;
                self.max_tokens_field = 0;
                self.supports_strict_mode = None;
            }
            "deepseek" => {
                self.requires_reasoning_content = Some(true);
                self.thinking_format = THINKING_FORMATS
                    .iter()
                    .position(|value| *value == "deepseek")
                    .unwrap_or(0);
                self.supports_store = Some(false);
                self.supports_developer_role = Some(false);
            }
            "qwen" => {
                self.thinking_format = THINKING_FORMATS
                    .iter()
                    .position(|value| *value == "qwen")
                    .unwrap_or(0);
                self.supports_developer_role = Some(false);
                self.supports_store = Some(false);
                self.supports_reasoning_effort = Some(false);
            }
            "zai" => {
                self.thinking_format = THINKING_FORMATS
                    .iter()
                    .position(|value| *value == "zai")
                    .unwrap_or(0);
                self.max_tokens_field = MAX_TOKENS_FIELDS
                    .iter()
                    .position(|value| *value == "max_tokens")
                    .unwrap_or(0);
                self.supports_store = Some(false);
                self.supports_developer_role = Some(false);
                self.supports_reasoning_effort = Some(false);
            }
            "openrouter" => {
                self.thinking_format = THINKING_FORMATS
                    .iter()
                    .position(|value| *value == "openrouter")
                    .unwrap_or(0);
                self.supports_developer_role = Some(false);
            }
            _ => {}
        }
    }

    /// Build the merged compat JSON object from structured fields + raw JSON.
    fn merge_compat(&self) -> documents::Result<Option<Value>> {
        let mut compat = parse_optional_object(&self.other_compat_json, "compat")?
            .map(|value| match value {
                Value::Object(map) => map,
                _ => unreachable!("parse_optional_object only returns objects"),
            })
            .unwrap_or_default();
        // Reject structured keys present in the raw JSON to avoid conflicts.
        let structured_keys = [
            SEND_SESSION_AFFINITY_HEADERS,
            REQUIRES_REASONING_CONTENT,
            THINKING_FORMAT,
            SUPPORTS_LONG_CACHE_RETENTION,
            SUPPORTS_STORE,
            SUPPORTS_DEVELOPER_ROLE,
            SUPPORTS_REASONING_EFFORT,
            MAX_TOKENS_FIELD,
            SUPPORTS_STRICT_MODE,
        ];
        for key in structured_keys {
            if compat.contains_key(key) {
                return Err(AppError::Invalid(format!(
                    "`{key}` is managed by the compat fields above; remove it from Other compat JSON"
                )));
            }
        }
        compat.insert(
            SEND_SESSION_AFFINITY_HEADERS.into(),
            Value::Bool(self.send_session_affinity_headers),
        );
        if let Some(value) = self.requires_reasoning_content {
            compat.insert(REQUIRES_REASONING_CONTENT.into(), Value::Bool(value));
        }
        if let Some(format) = THINKING_FORMATS.get(self.thinking_format).copied() {
            if !format.is_empty() {
                compat.insert(THINKING_FORMAT.into(), Value::String(format.into()));
            }
        }
        if let Some(value) = self.supports_long_cache_retention {
            compat.insert(SUPPORTS_LONG_CACHE_RETENTION.into(), Value::Bool(value));
        }
        if let Some(value) = self.supports_store {
            compat.insert(SUPPORTS_STORE.into(), Value::Bool(value));
        }
        if let Some(value) = self.supports_developer_role {
            compat.insert(SUPPORTS_DEVELOPER_ROLE.into(), Value::Bool(value));
        }
        if let Some(value) = self.supports_reasoning_effort {
            compat.insert(SUPPORTS_REASONING_EFFORT.into(), Value::Bool(value));
        }
        if let Some(field) = MAX_TOKENS_FIELDS.get(self.max_tokens_field).copied() {
            if !field.is_empty() {
                compat.insert(MAX_TOKENS_FIELD.into(), Value::String(field.into()));
            }
        }
        if let Some(value) = self.supports_strict_mode {
            compat.insert(SUPPORTS_STRICT_MODE.into(), Value::Bool(value));
        }
        Ok(Some(Value::Object(compat)))
    }
}

/// Structured compat fields parsed from a provider's raw compat object.
struct CompatFields {
    preset: usize,
    requires_reasoning_content: Option<bool>,
    thinking_format: usize,
    supports_long_cache_retention: Option<bool>,
    supports_store: Option<bool>,
    supports_developer_role: Option<bool>,
    supports_reasoning_effort: Option<bool>,
    max_tokens_field: usize,
    supports_strict_mode: Option<bool>,
    send_session_affinity_headers: bool,
    other_json: String,
}

impl Default for CompatFields {
    fn default() -> Self {
        Self {
            preset: 0,
            requires_reasoning_content: None,
            thinking_format: 0,
            supports_long_cache_retention: None,
            supports_store: None,
            supports_developer_role: None,
            supports_reasoning_effort: None,
            max_tokens_field: 0,
            supports_strict_mode: None,
            send_session_affinity_headers: true,
            other_json: String::new(),
        }
    }
}

fn split_compat_fields(provider: &ProviderView) -> CompatFields {
    let mut fields = CompatFields::default();
    let Some(compat) = provider.raw.get("compat").and_then(Value::as_object) else {
        return fields;
    };
    fields.send_session_affinity_headers = compat
        .get(SEND_SESSION_AFFINITY_HEADERS)
        .and_then(Value::as_bool)
        .unwrap_or(true);
    fields.requires_reasoning_content = compat
        .get(REQUIRES_REASONING_CONTENT)
        .and_then(Value::as_bool);
    fields.thinking_format = compat
        .get(THINKING_FORMAT)
        .and_then(Value::as_str)
        .and_then(|value| {
            THINKING_FORMATS
                .iter()
                .position(|candidate| *candidate == value)
        })
        .unwrap_or(0);
    fields.supports_long_cache_retention = compat
        .get(SUPPORTS_LONG_CACHE_RETENTION)
        .and_then(Value::as_bool);
    fields.supports_store = compat.get(SUPPORTS_STORE).and_then(Value::as_bool);
    fields.supports_developer_role = compat.get(SUPPORTS_DEVELOPER_ROLE).and_then(Value::as_bool);
    fields.supports_reasoning_effort = compat
        .get(SUPPORTS_REASONING_EFFORT)
        .and_then(Value::as_bool);
    fields.max_tokens_field = compat
        .get(MAX_TOKENS_FIELD)
        .and_then(Value::as_str)
        .and_then(|value| {
            MAX_TOKENS_FIELDS
                .iter()
                .position(|candidate| *candidate == value)
        })
        .unwrap_or(0);
    fields.supports_strict_mode = compat.get(SUPPORTS_STRICT_MODE).and_then(Value::as_bool);
    let mut others = compat.clone();
    for key in [
        SEND_SESSION_AFFINITY_HEADERS,
        REQUIRES_REASONING_CONTENT,
        THINKING_FORMAT,
        SUPPORTS_LONG_CACHE_RETENTION,
        SUPPORTS_STORE,
        SUPPORTS_DEVELOPER_ROLE,
        SUPPORTS_REASONING_EFFORT,
        MAX_TOKENS_FIELD,
        SUPPORTS_STRICT_MODE,
    ] {
        others.remove(key);
    }
    fields.other_json = if others.is_empty() {
        String::new()
    } else {
        Value::Object(others).to_string()
    };
    fields
}

fn split_headers(provider: &ProviderView) -> (String, String) {
    let Some(headers) = provider.raw.get("headers").and_then(Value::as_object) else {
        return (String::new(), String::new());
    };
    let user_agent = headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(USER_AGENT_HEADER))
        .map(|(_, value)| {
            value
                .as_str()
                .expect("provider header values are validated strings")
                .to_owned()
        })
        .unwrap_or_default();
    let mut others = headers.clone();
    others.retain(|name, _| !name.eq_ignore_ascii_case(USER_AGENT_HEADER));
    let headers_json = if others.is_empty() {
        String::new()
    } else {
        Value::Object(others).to_string()
    };
    (user_agent, headers_json)
}

fn merge_user_agent(headers: Option<Value>, user_agent: &str) -> documents::Result<Option<Value>> {
    let mut headers = match headers {
        None => Map::new(),
        Some(Value::Object(headers)) => headers,
        Some(_) => unreachable!("parse_optional_object only returns objects"),
    };
    headers.retain(|name, _| !name.eq_ignore_ascii_case(USER_AGENT_HEADER));
    let user_agent = user_agent.trim();
    if user_agent.chars().any(char::is_control) {
        return Err(AppError::Invalid(
            "User-Agent must not contain control characters".into(),
        ));
    }
    if !user_agent.is_empty() {
        headers.insert(USER_AGENT_HEADER.into(), Value::String(user_agent.into()));
    }
    Ok((!headers.is_empty()).then_some(Value::Object(headers)))
}

pub(super) struct ModelFormState {
    pub(super) provider_id: String,
    pub(super) previous_id: Option<String>,
    pub(super) id: String,
    pub(super) name: String,
    pub(super) api: usize,
    pub(super) reasoning: bool,
    pub(super) image_input: bool,
    pub(super) context_window: String,
    pub(super) max_tokens: String,
    pub(super) input_cost: String,
    pub(super) output_cost: String,
    pub(super) cache_read_cost: String,
    pub(super) cache_write_cost: String,
    pub(super) limits_expanded: bool,
    pub(super) pricing_expanded: bool,
    pub(super) field: usize,
    pub(super) cursor: usize,
}

impl ModelFormState {
    pub(super) fn add(provider_id: &str) -> Self {
        Self {
            provider_id: provider_id.into(),
            previous_id: None,
            id: String::new(),
            name: String::new(),
            api: 0,
            reasoning: false,
            image_input: false,
            context_window: String::new(),
            max_tokens: String::new(),
            input_cost: String::new(),
            output_cost: String::new(),
            cache_read_cost: String::new(),
            cache_write_cost: String::new(),
            limits_expanded: false,
            pricing_expanded: false,
            field: 0,
            cursor: 0,
        }
    }

    pub(super) fn edit(provider_id: &str, model: &ModelView) -> Self {
        Self {
            provider_id: provider_id.into(),
            previous_id: Some(model.id.clone()),
            id: model.id.clone(),
            name: model.name.clone().unwrap_or_default(),
            api: model
                .api
                .as_deref()
                .and_then(|api| API_TYPES.iter().position(|candidate| *candidate == api))
                .map(|index| index + 1)
                .unwrap_or_default(),
            reasoning: model.reasoning,
            image_input: model.input.iter().any(|input| input == "image"),
            context_window: model
                .context_window
                .map(|value| value.to_string())
                .unwrap_or_default(),
            max_tokens: model
                .max_tokens
                .map(|value| value.to_string())
                .unwrap_or_default(),
            input_cost: optional_number(model.input_cost),
            output_cost: optional_number(model.output_cost),
            cache_read_cost: optional_number(model.cache_read_cost),
            cache_write_cost: optional_number(model.cache_write_cost),
            limits_expanded: false,
            pricing_expanded: false,
            field: 0,
            cursor: char_len(&model.id),
        }
    }

    pub(super) fn copy(provider_id: &str, model: &ModelView) -> Self {
        let mut form = Self::edit(provider_id, model);
        form.id = format!("{}-copy", model.id);
        form.previous_id = None;
        form.cursor = char_len(&form.id);
        form
    }

    pub(super) fn visible_fields(&self) -> Vec<usize> {
        let mut fields = vec![0, 1, 2, 3, 4, 5];
        if self.limits_expanded {
            fields.extend([6, 7]);
        }
        fields.push(8);
        if self.pricing_expanded {
            fields.extend([9, 10, 11, 12]);
        }
        fields
    }

    pub(super) fn current_field_id(&self) -> usize {
        self.visible_fields().get(self.field).copied().unwrap_or(0)
    }

    fn field_text(&self, field_id: usize) -> Option<&str> {
        match field_id {
            0 => Some(&self.id),
            1 => Some(&self.name),
            6 => Some(&self.context_window),
            7 => Some(&self.max_tokens),
            9 => Some(&self.input_cost),
            10 => Some(&self.output_cost),
            11 => Some(&self.cache_read_cost),
            12 => Some(&self.cache_write_cost),
            _ => None,
        }
    }

    fn field_text_mut(&mut self, field_id: usize) -> Option<&mut String> {
        match field_id {
            0 => Some(&mut self.id),
            1 => Some(&mut self.name),
            6 => Some(&mut self.context_window),
            7 => Some(&mut self.max_tokens),
            9 => Some(&mut self.input_cost),
            10 => Some(&mut self.output_cost),
            11 => Some(&mut self.cache_read_cost),
            12 => Some(&mut self.cache_write_cost),
            _ => None,
        }
    }

    pub(super) fn current_text(&self) -> Option<&str> {
        let field_id = self.current_field_id();
        self.field_text(field_id)
    }

    pub(super) fn current_text_mut(&mut self) -> Option<&mut String> {
        let field_id = self.current_field_id();
        self.field_text_mut(field_id)
    }

    pub(super) fn current_len(&self) -> usize {
        self.current_text().map(char_len).unwrap_or_default()
    }

    pub(super) fn select_field(&mut self, next: usize) {
        let count = self.visible_fields().len();
        if count == 0 {
            self.field = 0;
        } else {
            self.field = next % count;
        }
        self.cursor = self.current_len();
    }

    pub(super) fn draft(&self) -> documents::Result<ModelDraft> {
        Ok(ModelDraft {
            id: self.id.trim().into(),
            name: (!self.name.trim().is_empty()).then(|| self.name.trim().into()),
            api: api_from_index(self.api),
            reasoning: self.reasoning,
            input: if self.image_input {
                vec!["text".into(), "image".into()]
            } else {
                vec!["text".into()]
            },
            context_window: parse_positive_u64(&self.context_window, "context window")?,
            max_tokens: parse_positive_u64(&self.max_tokens, "max tokens")?,
            input_cost: parse_optional_nonnegative_f64(&self.input_cost, "input cost")?,
            output_cost: parse_optional_nonnegative_f64(&self.output_cost, "output cost")?,
            cache_read_cost: parse_optional_nonnegative_f64(
                &self.cache_read_cost,
                "cache read cost",
            )?,
            cache_write_cost: parse_optional_nonnegative_f64(
                &self.cache_write_cost,
                "cache write cost",
            )?,
        })
    }
}

pub(super) struct ModelDefaultsFormState {
    pub(super) context_window: String,
    pub(super) max_tokens: String,
    pub(super) input_cost: String,
    pub(super) output_cost: String,
    pub(super) cache_read_cost: String,
    pub(super) cache_write_cost: String,
    pub(super) field: usize,
    pub(super) cursor: usize,
}

impl ModelDefaultsFormState {
    pub(super) fn new(defaults: &ModelDefaults) -> Self {
        Self {
            context_window: optional_number(defaults.context_window),
            max_tokens: optional_number(defaults.max_tokens),
            input_cost: optional_number(defaults.input_cost),
            output_cost: optional_number(defaults.output_cost),
            cache_read_cost: optional_number(defaults.cache_read_cost),
            cache_write_cost: optional_number(defaults.cache_write_cost),
            field: 0,
            cursor: 0,
        }
    }

    pub(super) fn current_text(&self) -> &str {
        match self.field {
            0 => &self.context_window,
            1 => &self.max_tokens,
            2 => &self.input_cost,
            3 => &self.output_cost,
            4 => &self.cache_read_cost,
            _ => &self.cache_write_cost,
        }
    }

    pub(super) fn current_text_mut(&mut self) -> &mut String {
        match self.field {
            0 => &mut self.context_window,
            1 => &mut self.max_tokens,
            2 => &mut self.input_cost,
            3 => &mut self.output_cost,
            4 => &mut self.cache_read_cost,
            _ => &mut self.cache_write_cost,
        }
    }

    pub(super) fn select_field(&mut self, next: usize) {
        self.field = next % 6;
        self.cursor = char_len(self.current_text());
    }

    pub(super) fn draft(&self) -> documents::Result<ModelDefaults> {
        Ok(ModelDefaults {
            context_window: parse_optional_positive_u64(&self.context_window, "context window")?,
            max_tokens: parse_optional_positive_u64(&self.max_tokens, "max tokens")?,
            input_cost: parse_optional_nonnegative_f64(&self.input_cost, "input cost")?,
            output_cost: parse_optional_nonnegative_f64(&self.output_cost, "output cost")?,
            cache_read_cost: parse_optional_nonnegative_f64(
                &self.cache_read_cost,
                "cache read cost",
            )?,
            cache_write_cost: parse_optional_nonnegative_f64(
                &self.cache_write_cost,
                "cache write cost",
            )?,
        })
    }
}

fn optional_number(value: Option<impl ToString>) -> String {
    value.map(|value| value.to_string()).unwrap_or_default()
}

fn parse_optional_positive_u64(value: &str, field: &str) -> documents::Result<Option<u64>> {
    if value.trim().is_empty() {
        return Ok(None);
    }
    parse_positive_u64(value, field).map(Some)
}

fn parse_optional_nonnegative_f64(value: &str, field: &str) -> documents::Result<Option<f64>> {
    if value.trim().is_empty() {
        return Ok(None);
    }
    value
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map(Some)
        .ok_or_else(|| AppError::Invalid(format!("{field} must be a non-negative number")))
}
