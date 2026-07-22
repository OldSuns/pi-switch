use std::fs;

use serde_json::{json, Map, Value};

use super::{
    schema::{default_model, provider_view, validate_draft, validate_model_id},
    storage::{io_error, providers_object_mut, read_document, write_document, WriteLock},
    AppError, ImportSummary, Paths, ProviderDraft, Result,
};

pub fn import_opencode(paths: &Paths) -> Result<ImportSummary> {
    let bytes = fs::read(&paths.opencode).map_err(|source| io_error(&paths.opencode, source))?;
    let source: Value = serde_json::from_slice(&bytes).map_err(|source| AppError::Json {
        path: paths.opencode.clone(),
        source,
    })?;
    let source_providers = source
        .as_object()
        .and_then(|root| root.get("provider"))
        .and_then(Value::as_object)
        .ok_or_else(|| AppError::Invalid("OpenCode provider must be an object".into()))?;
    if source_providers.is_empty() {
        return Err(AppError::Invalid(
            "OpenCode configuration contains no providers".into(),
        ));
    }

    let _lock = WriteLock::acquire(paths)?;
    let mut root = read_document(&paths.models, json!({ "providers": {} }))?;
    let before = root.clone();
    let providers = providers_object_mut(&mut root)?;
    let mut summary = ImportSummary {
        providers: 0,
        models: 0,
        changed: false,
    };

    for (id, source) in source_providers {
        summary.models += merge_provider(id, source, providers)?;
        summary.providers += 1;
    }

    summary.changed = root != before;
    if summary.changed {
        write_document(paths, &paths.models, "models", &root)?;
    }
    Ok(summary)
}

fn merge_provider(id: &str, source: &Value, providers: &mut Map<String, Value>) -> Result<usize> {
    let source = source
        .as_object()
        .ok_or_else(|| AppError::Invalid(format!("OpenCode provider '{id}' must be an object")))?;
    let empty = Map::new();
    let options = optional_object(source, "options", id)?.unwrap_or(&empty);
    let api = provider_api(id, optional_string(source, "npm", id)?)?;
    let base_url = optional_string(options, "baseURL", id)?;
    let api_key = optional_string(options, "apiKey", id)?
        .map(translate_secret)
        .transpose()?;
    let headers = optional_object(options, "headers", id)?
        .map(|headers| translate_strings(&Value::Object(headers.clone())))
        .transpose()?;
    validate_draft(&ProviderDraft {
        id: id.into(),
        base_url: base_url.unwrap_or_default().into(),
        api: Some(api.into()),
        api_key: api_key.clone().unwrap_or_default(),
        auth_header: true,
        headers: headers.clone(),
        compat: None,
    })?;

    let is_new = !providers.contains_key(id);
    let target = providers.entry(id).or_insert_with(|| json!({}));
    let target = target
        .as_object_mut()
        .ok_or_else(|| AppError::Invalid(format!("Pi provider '{id}' must be an object")))?;
    if let Some(base_url) = base_url {
        target.insert("baseUrl".into(), Value::String(base_url.into()));
    }
    target.insert("api".into(), Value::String(api.into()));
    if let Some(api_key) = api_key {
        target.insert("apiKey".into(), Value::String(api_key));
    }
    if let Some(headers) = headers {
        target.insert("headers".into(), headers);
    }
    if is_new {
        target.insert("authHeader".into(), Value::Bool(true));
    }

    let source_models = optional_object(source, "models", id)?.unwrap_or(&empty);
    let target_models = target
        .entry("models")
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or_else(|| AppError::Invalid(format!("Pi provider '{id}' models must be an array")))?;
    for (model_id, model) in source_models {
        merge_model(id, model_id, model, target_models)?;
    }
    provider_view(id, &Value::Object(target.clone()))?;
    Ok(source_models.len())
}

fn merge_model(provider_id: &str, id: &str, source: &Value, models: &mut Vec<Value>) -> Result<()> {
    validate_model_id(id)?;
    let source = source.as_object().ok_or_else(|| {
        AppError::Invalid(format!(
            "OpenCode provider '{provider_id}' model '{id}' must be an object"
        ))
    })?;
    let index = models
        .iter()
        .position(|model| model.get("id").and_then(Value::as_str) == Some(id));
    let index = match index {
        Some(index) => index,
        None => {
            models.push(default_model(id));
            models.len() - 1
        }
    };
    let target = models[index]
        .as_object_mut()
        .ok_or_else(|| AppError::Invalid("Pi model entry must be an object".into()))?;

    if let Some(name) = optional_string(source, "name", id)? {
        target.insert("name".into(), Value::String(name.into()));
    }
    if let Some(reasoning) = optional_bool(source, "reasoning", id)? {
        if reasoning {
            target.insert("reasoning".into(), Value::Bool(true));
        } else {
            target.remove("reasoning");
        }
    }
    if let Some(limit) = optional_object(source, "limit", id)? {
        set_positive_u64(target, "contextWindow", limit.get("context"), id)?;
        set_positive_u64(target, "maxTokens", limit.get("output"), id)?;
    }
    if let Some(modalities) = optional_object(source, "modalities", id)? {
        if let Some(input) = modalities.get("input") {
            target.insert("input".into(), Value::Array(model_input(input, id)?));
        }
    }
    Ok(())
}

fn provider_api(id: &str, npm: Option<&str>) -> Result<&'static str> {
    match npm {
        Some("@ai-sdk/openai-compatible") => Ok("openai-completions"),
        Some("@ai-sdk/openai") => Ok("openai-responses"),
        Some("@ai-sdk/anthropic") => Ok("anthropic-messages"),
        Some("@ai-sdk/google") => Ok("google-generative-ai"),
        Some(package) => Err(AppError::Invalid(format!(
            "OpenCode provider '{id}' uses unsupported package '{package}'"
        ))),
        None => match id {
            "openai" => Ok("openai-responses"),
            "anthropic" => Ok("anthropic-messages"),
            "google" | "gemini" => Ok("google-generative-ai"),
            _ => Err(AppError::Invalid(format!(
                "OpenCode provider '{id}' must declare a supported npm package"
            ))),
        },
    }
}

fn optional_object<'a>(
    object: &'a Map<String, Value>,
    field: &str,
    context: &str,
) -> Result<Option<&'a Map<String, Value>>> {
    match object.get(field) {
        None => Ok(None),
        Some(Value::Object(value)) => Ok(Some(value)),
        Some(_) => Err(AppError::Invalid(format!(
            "OpenCode '{context}' {field} must be an object"
        ))),
    }
}

fn optional_string<'a>(
    object: &'a Map<String, Value>,
    field: &str,
    context: &str,
) -> Result<Option<&'a str>> {
    match object.get(field) {
        None => Ok(None),
        Some(Value::String(value)) => Ok(Some(value)),
        Some(_) => Err(AppError::Invalid(format!(
            "OpenCode '{context}' {field} must be a string"
        ))),
    }
}

fn optional_bool(object: &Map<String, Value>, field: &str, context: &str) -> Result<Option<bool>> {
    match object.get(field) {
        None => Ok(None),
        Some(Value::Bool(value)) => Ok(Some(*value)),
        Some(_) => Err(AppError::Invalid(format!(
            "OpenCode '{context}' {field} must be a boolean"
        ))),
    }
}

fn set_positive_u64(
    target: &mut Map<String, Value>,
    field: &str,
    value: Option<&Value>,
    model_id: &str,
) -> Result<()> {
    let Some(value) = value else {
        return Ok(());
    };
    let value = value.as_u64().filter(|value| *value > 0).ok_or_else(|| {
        AppError::Invalid(format!(
            "OpenCode model '{model_id}' {field} must be a positive integer"
        ))
    })?;
    target.insert(field.into(), Value::from(value));
    Ok(())
}

fn model_input(value: &Value, model_id: &str) -> Result<Vec<Value>> {
    let values = value.as_array().ok_or_else(|| {
        AppError::Invalid(format!(
            "OpenCode model '{model_id}' input modalities must be an array"
        ))
    })?;
    let mut image = false;
    for value in values {
        match value.as_str() {
            Some("text") => {}
            Some("image") => image = true,
            Some(other) => {
                return Err(AppError::Invalid(format!(
                    "OpenCode model '{model_id}' has unsupported input modality '{other}'"
                )))
            }
            None => {
                return Err(AppError::Invalid(format!(
                    "OpenCode model '{model_id}' input modalities must contain strings"
                )))
            }
        }
    }
    let mut input = vec![Value::String("text".into())];
    if image {
        input.push(Value::String("image".into()));
    }
    Ok(input)
}

fn translate_strings(value: &Value) -> Result<Value> {
    match value {
        Value::String(value) => Ok(Value::String(translate_secret(value)?)),
        Value::Array(values) => values
            .iter()
            .map(translate_strings)
            .collect::<Result<Vec<_>>>()
            .map(Value::Array),
        Value::Object(values) => values
            .iter()
            .map(|(key, value)| Ok((key.clone(), translate_strings(value)?)))
            .collect::<Result<Map<_, _>>>()
            .map(Value::Object),
        value => Ok(value.clone()),
    }
}

fn translate_secret(value: &str) -> Result<String> {
    if value.contains("{file:") {
        return Err(AppError::Invalid(
            "OpenCode {file:...} secrets cannot be imported safely".into(),
        ));
    }
    let mut output = String::new();
    let mut rest = value;
    while let Some(start) = rest.find("{env:") {
        output.push_str(&rest[..start]);
        let variable = &rest[start + 5..];
        let end = variable.find('}').ok_or_else(|| {
            AppError::Invalid("OpenCode environment variable reference is incomplete".into())
        })?;
        let name = &variable[..end];
        if !valid_env_name(name) {
            return Err(AppError::Invalid(
                "OpenCode environment variable name is invalid".into(),
            ));
        }
        output.push_str("${");
        output.push_str(name);
        output.push('}');
        rest = &variable[end + 1..];
    }
    output.push_str(rest);
    Ok(output)
}

fn valid_env_name(value: &str) -> bool {
    let mut chars = value.chars();
    chars
        .next()
        .is_some_and(|character| character == '_' || character.is_ascii_alphabetic())
        && chars.all(|character| character == '_' || character.is_ascii_alphanumeric())
}
