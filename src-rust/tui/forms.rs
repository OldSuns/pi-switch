use crate::documents::{
    self, AppError, ModelDefaults, ModelDraft, ModelView, ProviderDraft, ProviderView,
    USER_AGENT_HEADER,
};
use serde_json::{Map, Value};

const SEND_SESSION_AFFINITY_HEADERS: &str = "sendSessionAffinityHeaders";

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
    pub(super) send_session_affinity_headers: bool,
    pub(super) compat_json: String,
    pub(super) editing_headers: bool,
    pub(super) headers_field: usize,
    pub(super) field: usize,
    pub(super) cursor: usize,
    pub(super) show_help: bool,
}

impl FormState {
    pub(super) fn add() -> Self {
        Self {
            previous_id: None,
            id: String::new(),
            base_url: "https://api.openai.com/v1".into(),
            api: 1,
            api_key: "$OPENAI_API_KEY".into(),
            auth_header: true,
            in_pi: true,
            user_agent: String::new(),
            headers_json: String::new(),
            send_session_affinity_headers: true,
            compat_json: String::new(),
            editing_headers: false,
            headers_field: 0,
            field: 0,
            cursor: 0,
            show_help: false,
        }
    }

    pub(super) fn edit(provider: &ProviderView) -> Self {
        let (user_agent, headers_json) = split_headers(provider);
        let (send_session_affinity_headers, compat_json) = split_compat(provider);
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
            send_session_affinity_headers,
            compat_json,
            editing_headers: false,
            headers_field: 0,
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
        match self.field {
            0 => Some(&self.id),
            1 => Some(&self.base_url),
            3 => Some(&self.api_key),
            5 if self.editing_headers => Some(&self.headers_json),
            7 => Some(&self.compat_json),
            _ => None,
        }
    }

    pub(super) fn current_text_mut(&mut self) -> Option<&mut String> {
        match self.field {
            0 => Some(&mut self.id),
            1 => Some(&mut self.base_url),
            3 => Some(&mut self.api_key),
            5 if self.editing_headers => Some(&mut self.headers_json),
            7 => Some(&mut self.compat_json),
            _ => None,
        }
    }

    pub(super) fn select_field(&mut self, next: usize) {
        self.field = next % 9;
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
            compat: merge_send_session_affinity_headers(
                parse_optional_object(&self.compat_json, "compat")?,
                self.send_session_affinity_headers,
            )?,
        })
    }
}

fn split_compat(provider: &ProviderView) -> (bool, String) {
    let Some(compat) = provider.raw.get("compat").and_then(Value::as_object) else {
        return (true, String::new());
    };
    let send_session_affinity_headers = compat
        .get(SEND_SESSION_AFFINITY_HEADERS)
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let mut others = compat.clone();
    others.remove(SEND_SESSION_AFFINITY_HEADERS);
    let compat_json = if others.is_empty() {
        String::new()
    } else {
        Value::Object(others).to_string()
    };
    (send_session_affinity_headers, compat_json)
}

fn merge_send_session_affinity_headers(
    compat: Option<Value>,
    send_session_affinity_headers: bool,
) -> documents::Result<Option<Value>> {
    let mut compat = match compat {
        None => Map::new(),
        Some(Value::Object(compat)) => compat,
        Some(_) => unreachable!("parse_optional_object only returns objects"),
    };
    if compat.contains_key(SEND_SESSION_AFFINITY_HEADERS) {
        return Err(AppError::Invalid(format!(
            "{SEND_SESSION_AFFINITY_HEADERS} is managed by Session affinity; remove it from Other compat JSON"
        )));
    }
    compat.insert(
        SEND_SESSION_AFFINITY_HEADERS.into(),
        Value::Bool(send_session_affinity_headers),
    );
    Ok(Some(Value::Object(compat)))
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

    pub(super) fn current_text(&self) -> Option<&str> {
        match self.field {
            0 => Some(&self.id),
            1 => Some(&self.name),
            5 => Some(&self.context_window),
            6 => Some(&self.max_tokens),
            _ => None,
        }
    }

    pub(super) fn current_text_mut(&mut self) -> Option<&mut String> {
        match self.field {
            0 => Some(&mut self.id),
            1 => Some(&mut self.name),
            5 => Some(&mut self.context_window),
            6 => Some(&mut self.max_tokens),
            _ => None,
        }
    }

    pub(super) fn current_len(&self) -> usize {
        self.current_text().map(char_len).unwrap_or_default()
    }

    pub(super) fn select_field(&mut self, next: usize) {
        self.field = next % 7;
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
