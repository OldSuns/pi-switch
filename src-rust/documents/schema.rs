use std::collections::BTreeSet;

use reqwest::Url;
use serde_json::{json, Map, Value};

use super::{
    storage::{provider_string, providers_object_mut},
    AppError, ModelDraft, ModelView, ProviderDraft, ProviderView, Result, API_TYPES,
};

pub(super) fn provider_view(id: &str, value: &Value) -> Result<ProviderView> {
    let object = value
        .as_object()
        .ok_or_else(|| AppError::Invalid(format!("provider '{id}' must be an object")))?;
    let models = match object.get("models") {
        None => Vec::new(),
        Some(Value::Array(items)) => {
            validate_model_entries(items, id)?;
            items
                .iter()
                .enumerate()
                .map(|(index, model)| model_view(id, index, model))
                .collect::<Result<Vec<_>>>()?
        }
        Some(_) => {
            return Err(AppError::Invalid(format!(
                "provider '{id}' models must be an array"
            )))
        }
    };
    let auth_header = match object.get("authHeader") {
        None => true,
        Some(Value::Bool(value)) => *value,
        Some(_) => {
            return Err(AppError::Invalid(format!(
                "provider '{id}' authHeader must be a boolean"
            )))
        }
    };
    Ok(ProviderView {
        id: id.into(),
        base_url: provider_string(object, id, "baseUrl")?,
        api: provider_string(object, id, "api")?,
        api_key: provider_string(object, id, "apiKey")?,
        auth_header,
        models,
        raw: value.clone(),
    })
}

pub(super) fn model_view(provider_id: &str, index: usize, value: &Value) -> Result<ModelView> {
    let object = value.as_object().ok_or_else(|| {
        AppError::Invalid(format!(
            "provider '{provider_id}' model at index {index} must be an object"
        ))
    })?;
    let id = object
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            AppError::Invalid(format!(
                "provider '{provider_id}' model at index {index} must have a string ID"
            ))
        })?
        .to_owned();
    let name = optional_object_string(object, provider_id, index, "name")?;
    let api = optional_object_string(object, provider_id, index, "api")?;
    if let Some(api) = api.as_deref().filter(|api| !API_TYPES.contains(api)) {
        return Err(AppError::Invalid(format!(
            "provider '{provider_id}' model '{id}' has unsupported API type '{api}'"
        )));
    }
    let reasoning = optional_object_bool(object, provider_id, index, "reasoning")?.unwrap_or(false);
    let input = match object.get("input") {
        None => vec!["text".into()],
        Some(Value::Array(values)) => values
            .iter()
            .map(|value| {
                value.as_str().map(str::to_owned).ok_or_else(|| {
                    AppError::Invalid(format!(
                        "provider '{provider_id}' model '{id}' input must contain strings"
                    ))
                })
            })
            .collect::<Result<Vec<_>>>()?,
        Some(_) => {
            return Err(AppError::Invalid(format!(
                "provider '{provider_id}' model '{id}' input must be an array"
            )))
        }
    };
    let context_window = optional_object_u64(object, provider_id, index, "contextWindow")?;
    let max_tokens = optional_object_u64(object, provider_id, index, "maxTokens")?;
    if context_window == Some(0) || max_tokens == Some(0) {
        return Err(AppError::Invalid(format!(
            "provider '{provider_id}' model '{id}' contextWindow and maxTokens must be positive"
        )));
    }
    let view = ModelView {
        id,
        name,
        api,
        reasoning,
        input,
        context_window,
        max_tokens,
    };
    validate_model_id(&view.id)?;
    if view.input != ["text"] && view.input != ["text", "image"] {
        return Err(AppError::Invalid(format!(
            "provider '{provider_id}' model '{}' input must be text or text + image",
            view.id
        )));
    }
    Ok(view)
}

pub(super) fn optional_object_string(
    object: &Map<String, Value>,
    provider_id: &str,
    index: usize,
    field: &str,
) -> Result<Option<String>> {
    match object.get(field) {
        None => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(AppError::Invalid(format!(
            "provider '{provider_id}' model at index {index} {field} must be a string"
        ))),
    }
}

pub(super) fn optional_object_bool(
    object: &Map<String, Value>,
    provider_id: &str,
    index: usize,
    field: &str,
) -> Result<Option<bool>> {
    match object.get(field) {
        None => Ok(None),
        Some(Value::Bool(value)) => Ok(Some(*value)),
        Some(_) => Err(AppError::Invalid(format!(
            "provider '{provider_id}' model at index {index} {field} must be a boolean"
        ))),
    }
}

pub(super) fn optional_object_u64(
    object: &Map<String, Value>,
    provider_id: &str,
    index: usize,
    field: &str,
) -> Result<Option<u64>> {
    match object.get(field) {
        None => Ok(None),
        Some(Value::Number(value)) => value.as_u64().map(Some).ok_or_else(|| {
            AppError::Invalid(format!(
                "provider '{provider_id}' model at index {index} {field} must be a positive integer"
            ))
        }),
        Some(_) => Err(AppError::Invalid(format!(
            "provider '{provider_id}' model at index {index} {field} must be an integer"
        ))),
    }
}

pub(super) fn patch_provider(provider: &mut Value, draft: &ProviderDraft) -> Result<()> {
    let object = provider
        .as_object_mut()
        .ok_or_else(|| AppError::Invalid("provider data must be an object".into()))?;
    set_optional_string(
        object,
        "baseUrl",
        (!draft.base_url.is_empty()).then_some(&draft.base_url),
    );
    set_optional_string(object, "api", draft.api.as_ref());
    set_optional_string(
        object,
        "apiKey",
        (!draft.api_key.is_empty()).then_some(&draft.api_key),
    );
    object.insert("authHeader".into(), Value::Bool(draft.auth_header));
    set_optional_value(object, "headers", draft.headers.clone());
    set_optional_value(object, "compat", draft.compat.clone());
    Ok(())
}

pub(super) fn patch_model(object: &mut Map<String, Value>, draft: &ModelDraft) {
    object.insert("id".into(), Value::String(draft.id.clone()));
    set_optional_string(object, "name", draft.name.as_ref());
    set_optional_string(object, "api", draft.api.as_ref());
    if draft.reasoning {
        object.insert("reasoning".into(), Value::Bool(true));
    } else {
        object.remove("reasoning");
    }
    if draft.input == ["text"] {
        object.remove("input");
    } else {
        object.insert(
            "input".into(),
            Value::Array(draft.input.iter().cloned().map(Value::String).collect()),
        );
    }
    object.insert("contextWindow".into(), Value::from(draft.context_window));
    object.insert("maxTokens".into(), Value::from(draft.max_tokens));
}

pub(super) fn set_optional_string(
    object: &mut Map<String, Value>,
    field: &str,
    value: Option<&String>,
) {
    match value {
        Some(value) => {
            object.insert(field.into(), Value::String(value.clone()));
        }
        None => {
            object.remove(field);
        }
    }
}

pub(super) fn set_optional_value(
    object: &mut Map<String, Value>,
    field: &str,
    value: Option<Value>,
) {
    match value {
        Some(value) => {
            object.insert(field.into(), value);
        }
        None => {
            object.remove(field);
        }
    }
}

pub(super) fn minimal_model(id: &str) -> Value {
    json!({ "id": id })
}

pub(super) fn validate_draft(draft: &ProviderDraft) -> Result<()> {
    let id = draft.id.trim();
    if id.is_empty() || id.chars().any(char::is_control) {
        return Err(AppError::Invalid(
            "provider ID is empty or contains control characters".into(),
        ));
    }
    if !draft.base_url.is_empty() {
        validate_url(&draft.base_url)?;
    }
    if let Some(api) = draft.api.as_deref().filter(|api| !API_TYPES.contains(api)) {
        return Err(AppError::Invalid(format!("unsupported API type '{}'", api)));
    }
    for (field, value) in [("headers", &draft.headers), ("compat", &draft.compat)] {
        if value.as_ref().is_some_and(|value| !value.is_object()) {
            return Err(AppError::Invalid(format!(
                "provider {field} must be a JSON object"
            )));
        }
    }
    Ok(())
}

pub(super) fn validate_model_draft(draft: &ModelDraft) -> Result<()> {
    validate_model_id(&draft.id)?;
    if let Some(api) = draft.api.as_deref().filter(|api| !API_TYPES.contains(api)) {
        return Err(AppError::Invalid(format!("unsupported API type '{api}'")));
    }
    if draft.input != ["text"] && draft.input != ["text", "image"] {
        return Err(AppError::Invalid(
            "model input must be text or text + image".into(),
        ));
    }
    if draft.context_window == 0 || draft.max_tokens == 0 {
        return Err(AppError::Invalid(
            "context window and max tokens must be greater than zero".into(),
        ));
    }
    Ok(())
}

pub(super) fn validate_model_id(model_id: &str) -> Result<()> {
    if model_id.trim().is_empty() || model_id.chars().any(char::is_control) {
        return Err(AppError::Invalid(
            "model ID is empty or contains control characters".into(),
        ));
    }
    Ok(())
}

pub(super) fn provider_models_mut<'a>(
    root: &'a mut Value,
    provider_id: &str,
) -> Result<&'a mut Vec<Value>> {
    let provider = providers_object_mut(root)?
        .get_mut(provider_id)
        .ok_or_else(|| AppError::Invalid(format!("provider '{provider_id}' no longer exists")))?;
    let object = provider
        .as_object_mut()
        .ok_or_else(|| AppError::Invalid(format!("provider '{provider_id}' must be an object")))?;
    if !object.contains_key("models") {
        object.insert("models".into(), Value::Array(Vec::new()));
    }
    let models = object
        .get_mut("models")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| {
            AppError::Invalid(format!("provider '{provider_id}' models must be an array"))
        })?;
    validate_model_entries(models, provider_id)?;
    Ok(models)
}

pub(super) fn validate_model_entries(models: &[Value], provider_id: &str) -> Result<()> {
    let mut ids = BTreeSet::new();
    for (index, model) in models.iter().enumerate() {
        let model = model_view(provider_id, index, model)?;
        if !ids.insert(model.id.clone()) {
            return Err(AppError::Invalid(format!(
                "provider '{provider_id}' contains duplicate model ID '{}'",
                model.id
            )));
        }
    }
    Ok(())
}

pub(super) fn unique_copy_id(source_id: &str, exists: impl Fn(&str) -> bool) -> String {
    let base = format!("{source_id}-copy");
    if !exists(&base) {
        return base;
    }
    let mut suffix = 2;
    loop {
        let candidate = format!("{base}-{suffix}");
        if !exists(&candidate) {
            return candidate;
        }
        suffix += 1;
    }
}

pub(super) fn validate_provider_view(provider: &ProviderView) -> Result<()> {
    if !provider.base_url.is_empty() {
        validate_url(&provider.base_url)?;
    }
    if !provider.api.is_empty() && !API_TYPES.contains(&provider.api.as_str()) {
        return Err(AppError::Invalid(format!(
            "unsupported API type '{}'",
            provider.api
        )));
    }
    Ok(())
}

pub(super) fn validate_url(value: &str) -> Result<Url> {
    let url = Url::parse(value)
        .map_err(|error| AppError::Invalid(format!("invalid baseUrl: {error}")))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(AppError::Invalid("baseUrl must use http or https".into()));
    }
    Ok(url)
}
