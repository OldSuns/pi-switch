use std::{collections::BTreeMap, fs};

use serde_json::{json, Map, Value};

use super::{
    lock_provider_documents,
    network::fetch_catalog,
    schema::{minimal_model, provider_view, validate_draft, validate_model_id},
    storage::{io_error, providers_object, providers_object_mut},
    write_provider_changes, AppError, CatalogAmbiguity, CatalogModel, ImportOptions, ImportSummary,
    ModelCatalog, OpenCodeImportPlan, Paths, ProviderDraft, Result,
};

pub fn list_opencode_providers(paths: &Paths) -> Result<Vec<String>> {
    let source = read_source(paths)?;
    let mut providers = source_providers(&source)?
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    providers.sort_by_key(|value| value.to_lowercase());
    Ok(providers)
}

pub fn prepare_opencode_import(
    paths: &Paths,
    provider_ids: &[String],
    options: ImportOptions,
) -> Result<OpenCodeImportPlan> {
    let source = read_source(paths)?;
    validate_provider_ids(&source, provider_ids)?;
    let catalog = if options.fetch_metadata {
        Some(fetch_catalog()?)
    } else {
        None
    };
    let ambiguous = catalog
        .as_ref()
        .map(|catalog| catalog_ambiguities(&source, provider_ids, catalog))
        .transpose()?
        .unwrap_or_default();
    Ok(OpenCodeImportPlan {
        source,
        provider_ids: provider_ids.to_vec(),
        options,
        catalog,
        ambiguous,
    })
}

pub fn apply_opencode_import(
    paths: &Paths,
    plan: OpenCodeImportPlan,
    candidate_indices: &[usize],
) -> Result<ImportSummary> {
    if candidate_indices.len() != plan.ambiguous.len() {
        return Err(AppError::Invalid(
            "every ambiguous OpenCode model requires a catalog selection".into(),
        ));
    }
    let mut selections = CatalogSelections::new();
    for (ambiguity, index) in plan.ambiguous.iter().zip(candidate_indices) {
        let candidate = ambiguity.candidates.get(*index).ok_or_else(|| {
            AppError::Invalid(format!(
                "catalog selection for '{}/{}' is out of range",
                ambiguity.provider_id, ambiguity.model_id
            ))
        })?;
        selections
            .entry(ambiguity.provider_id.clone())
            .or_default()
            .insert(ambiguity.model_id.clone(), candidate.model.clone());
    }
    import_source(
        paths,
        &plan.source,
        plan.catalog.as_ref(),
        &plan.provider_ids,
        &plan.options,
        &selections,
    )
}

#[cfg(test)]
pub(super) fn import_opencode_with_catalog(
    paths: &Paths,
    catalog: &ModelCatalog,
) -> Result<ImportSummary> {
    let source = read_source(paths)?;
    let provider_ids = source_providers(&source)?
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    import_source(
        paths,
        &source,
        Some(catalog),
        &provider_ids,
        &ImportOptions {
            fetch_metadata: true,
            defaults: Default::default(),
        },
        &CatalogSelections::new(),
    )
}

#[cfg(test)]
pub(super) fn prepare_opencode_with_catalog(
    paths: &Paths,
    catalog: ModelCatalog,
    provider_ids: &[String],
) -> Result<OpenCodeImportPlan> {
    let source = read_source(paths)?;
    validate_provider_ids(&source, provider_ids)?;
    let ambiguous = catalog_ambiguities(&source, provider_ids, &catalog)?;
    Ok(OpenCodeImportPlan {
        source,
        provider_ids: provider_ids.to_vec(),
        options: ImportOptions {
            fetch_metadata: true,
            defaults: Default::default(),
        },
        catalog: Some(catalog),
        ambiguous,
    })
}

type CatalogSelections = BTreeMap<String, BTreeMap<String, CatalogModel>>;

fn read_source(paths: &Paths) -> Result<Value> {
    let bytes = fs::read(&paths.opencode).map_err(|source| io_error(&paths.opencode, source))?;
    serde_json::from_slice(&bytes).map_err(|source| AppError::Json {
        path: paths.opencode.clone(),
        source,
    })
}

fn source_providers(source: &Value) -> Result<&Map<String, Value>> {
    let providers = source
        .as_object()
        .and_then(|root| root.get("provider"))
        .and_then(Value::as_object)
        .ok_or_else(|| AppError::Invalid("OpenCode provider must be an object".into()))?;
    if providers.is_empty() {
        return Err(AppError::Invalid(
            "OpenCode configuration contains no providers".into(),
        ));
    }
    Ok(providers)
}

fn validate_provider_ids(source: &Value, provider_ids: &[String]) -> Result<()> {
    let providers = source_providers(source)?;
    if provider_ids.is_empty() {
        return Err(AppError::Invalid(
            "select at least one OpenCode provider".into(),
        ));
    }
    for id in provider_ids {
        if !providers.contains_key(id) {
            return Err(AppError::Invalid(format!(
                "OpenCode provider '{id}' no longer exists"
            )));
        }
    }
    Ok(())
}

fn catalog_ambiguities(
    source: &Value,
    provider_ids: &[String],
    catalog: &ModelCatalog,
) -> Result<Vec<CatalogAmbiguity>> {
    let providers = source_providers(source)?;
    let empty = Map::new();
    let mut ambiguous = Vec::new();
    for provider_id in provider_ids {
        let provider = providers[provider_id].as_object().ok_or_else(|| {
            AppError::Invalid(format!(
                "OpenCode provider '{provider_id}' must be an object"
            ))
        })?;
        let models = optional_object(provider, "models", provider_id)?.unwrap_or(&empty);
        for model_id in models.keys() {
            if catalog.resolve(provider_id, model_id).is_some() {
                continue;
            }
            let candidates = catalog.ambiguous_candidates(provider_id, model_id);
            if !candidates.is_empty() {
                ambiguous.push(CatalogAmbiguity {
                    provider_id: provider_id.clone(),
                    model_id: model_id.clone(),
                    candidates,
                });
            }
        }
    }
    Ok(ambiguous)
}

fn import_source(
    paths: &Paths,
    source: &Value,
    catalog: Option<&ModelCatalog>,
    provider_ids: &[String],
    options: &ImportOptions,
    selections: &CatalogSelections,
) -> Result<ImportSummary> {
    let source_providers = source_providers(source)?;
    validate_provider_ids(source, provider_ids)?;

    let (lock, mut library, mut models) = lock_provider_documents(paths)?;
    let before_library = library.clone();
    let before_models = models.clone();
    let providers = providers_object_mut(&mut library)?;
    let mut summary = ImportSummary {
        providers: 0,
        models: 0,
        metadata: 0,
        defaults: 0,
        unresolved: 0,
        changed: false,
    };

    for id in provider_ids {
        let source = &source_providers[id];
        let (models, metadata, defaults, unresolved) =
            merge_provider(id, source, providers, catalog, options, selections)?;
        summary.models += models;
        summary.metadata += metadata;
        summary.defaults += defaults;
        summary.unresolved += unresolved;
        summary.providers += 1;
    }

    let local = providers_object(&library)?;
    let enabled = providers_object_mut(&mut models)?;
    for id in provider_ids {
        enabled.insert(id.clone(), local[id].clone());
    }
    summary.changed = library != before_library || models != before_models;
    if summary.changed {
        write_provider_changes(paths, &lock, Some(&models), None, &library)?;
    }
    Ok(summary)
}

fn merge_provider(
    id: &str,
    source: &Value,
    providers: &mut Map<String, Value>,
    catalog: Option<&ModelCatalog>,
    import_options: &ImportOptions,
    selections: &CatalogSelections,
) -> Result<(usize, usize, usize, usize)> {
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
        in_pi: true,
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
    let mut metadata = 0;
    let mut defaults = 0;
    let mut unresolved = 0;
    for (model_id, model) in source_models {
        let (from_metadata, from_defaults, is_unresolved) = merge_model(
            id,
            model_id,
            model,
            target_models,
            catalog,
            import_options,
            selections,
        )?;
        metadata += usize::from(from_metadata);
        defaults += usize::from(from_defaults);
        unresolved += usize::from(is_unresolved);
    }
    provider_view(id, &Value::Object(target.clone()))?;
    Ok((source_models.len(), metadata, defaults, unresolved))
}

fn merge_model(
    provider_id: &str,
    id: &str,
    source: &Value,
    models: &mut Vec<Value>,
    catalog: Option<&ModelCatalog>,
    options: &ImportOptions,
    selections: &CatalogSelections,
) -> Result<(bool, bool, bool)> {
    validate_model_id(id)?;
    let source = source.as_object().ok_or_else(|| {
        AppError::Invalid(format!(
            "OpenCode provider '{provider_id}' model '{id}' must be an object"
        ))
    })?;
    let index = models
        .iter()
        .position(|model| model.get("id").and_then(Value::as_str) == Some(id));
    let (index, is_new) = match index {
        Some(index) => (index, false),
        None => {
            models.push(minimal_model(id));
            (models.len() - 1, true)
        }
    };
    let target = models[index]
        .as_object_mut()
        .ok_or_else(|| AppError::Invalid("Pi model entry must be an object".into()))?;

    let catalog_model = if options.fetch_metadata {
        selections
            .get(provider_id)
            .and_then(|models| models.get(id))
            .cloned()
            .or_else(|| catalog.and_then(|catalog| catalog.resolve(provider_id, id).cloned()))
    } else {
        Some(options.defaults.model(id))
    };
    if options.fetch_metadata || is_new {
        if let Some(model) = catalog_model.as_ref() {
            target.extend(
                model
                    .config
                    .as_object()
                    .expect("validated catalog model")
                    .clone(),
            );
        }
    }

    if let Some(name) = optional_string(source, "name", id)?.filter(|name| !name.is_empty()) {
        target.insert("name".into(), Value::String(name.into()));
    }
    if target.get("name").and_then(Value::as_str) == Some("") {
        target.remove("name");
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
    Ok((
        options.fetch_metadata && catalog_model.is_some(),
        !options.fetch_metadata && is_new,
        options.fetch_metadata && catalog_model.is_none(),
    ))
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
