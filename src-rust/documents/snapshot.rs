use super::settings::check_updates_field;
use super::*;

pub fn load_snapshot(paths: &Paths) -> Result<Snapshot> {
    let (library, models, warning) = load_provider_documents(paths)?;
    let settings = read_document(&paths.settings, json!({}))?;
    let enabled = providers_object(&models)?;
    let mut views = providers_object(&library)?
        .iter()
        .map(|(id, value)| {
            let mut view = provider_view(id, value)?;
            view.in_pi = enabled.contains_key(id);
            Ok(view)
        })
        .collect::<Result<Vec<_>>>()?;
    views.sort_by_key(|a| a.id.to_lowercase());

    Ok(Snapshot {
        providers_path: paths.providers.display().to_string(),
        models_path: paths.models.display().to_string(),
        settings_path: paths.settings.display().to_string(),
        providers: views,
        default_provider: string_field(&settings, "defaultProvider")?,
        default_model: string_field(&settings, "defaultModel")?,
        language: language_field(&settings)?,
        fetch_model_metadata: fetch_model_metadata_field(&settings)?,
        check_updates: check_updates_field(&settings)?,
        model_defaults: model_defaults_field(&settings)?,
        warning,
    })
}

fn load_provider_documents(paths: &Paths) -> Result<(Value, Value, Option<String>)> {
    let models = read_document(&paths.models, json!({ "providers": {} }))?;
    validate_provider_document(&models)?;
    if !paths.providers.exists() {
        let lock = WriteLock::acquire(paths)?;
        if !paths.providers.exists() {
            let models = read_document(&paths.models, json!({ "providers": {} }))?;
            validate_provider_document(&models)?;
            let library = local_library_from_models(&models);
            write_initial_document(&paths.providers, &library)?;
            return Ok((library, models, None));
        }
        drop(lock);
        return load_provider_documents(paths);
    }

    let library = read_document(&paths.providers, json!({}));
    let mut library = match library.and_then(|value| {
        validate_local_library(&value)?;
        Ok(value)
    }) {
        Ok(value) => value,
        Err(error) => {
            let lock = WriteLock::acquire(paths)?;
            if read_document(&paths.providers, json!({}))
                .and_then(|value| validate_local_library(&value))
                .is_ok()
            {
                drop(lock);
                return load_provider_documents(paths);
            }
            let models = read_document(&paths.models, json!({ "providers": {} }))?;
            validate_provider_document(&models)?;
            let archived = archive_corrupt_provider_store(paths)?;
            let rebuilt = local_library_from_models(&models);
            write_initial_document(&paths.providers, &rebuilt)?;
            return Ok((
                rebuilt,
                models,
                Some(format!(
                    "Local provider library was invalid ({error}). The original was archived at {} and rebuilt from Pi.",
                    archived.display()
                )),
            ));
        }
    };

    let pi_providers = providers_object(&models)?;
    let local_providers = providers_object_mut(&mut library)?;
    let mut changed = false;
    for (id, provider) in pi_providers {
        if local_providers.get(id) != Some(provider) {
            provider_view(id, provider)?;
            local_providers.insert(id.clone(), provider.clone());
            changed = true;
        }
    }
    if changed {
        let lock = WriteLock::acquire(paths)?;
        let models = read_document(&paths.models, json!({ "providers": {} }))?;
        let mut library = read_document(&paths.providers, json!({}))?;
        validate_provider_document(&models)?;
        validate_local_library(&library)?;
        let local = providers_object_mut(&mut library)?;
        for (id, provider) in providers_object(&models)? {
            if local.get(id) != Some(provider) {
                local.insert(id.clone(), provider.clone());
            }
        }
        write_document(paths, &lock, &paths.providers, &library)?;
        return Ok((library, models, None));
    }
    Ok((library, models, None))
}

fn local_library_from_models(models: &Value) -> Value {
    json!({
        "version": 1,
        "providers": models.get("providers").cloned().unwrap_or_else(|| json!({}))
    })
}

pub(super) fn validate_local_library(value: &Value) -> Result<()> {
    if value.get("version").and_then(Value::as_u64) != Some(1) {
        return Err(AppError::Invalid("providers.json version must be 1".into()));
    }
    validate_provider_document(value)
}

pub(super) fn validate_provider_document(value: &Value) -> Result<()> {
    for (id, provider) in providers_object(value)? {
        provider_view(id, provider)?;
    }
    Ok(())
}

pub(super) fn validate_settings_document(settings: &Value, models: &Value) -> Result<()> {
    language_field(settings)?;
    fetch_model_metadata_field(settings)?;
    model_defaults_field(settings)?;
    let provider = string_field(settings, "defaultProvider")?;
    let model = string_field(settings, "defaultModel")?;
    match (provider, model) {
        (None, None) => Ok(()),
        (Some(provider_id), Some(model_id)) => {
            let provider = providers_object(models)?.get(&provider_id).ok_or_else(|| {
                AppError::Invalid(format!(
                    "default provider '{provider_id}' is not present in models.json"
                ))
            })?;
            let provider = provider_view(&provider_id, provider)?;
            if provider.models.iter().any(|item| item.id == model_id) {
                Ok(())
            } else {
                Err(AppError::Invalid(format!(
                    "default model '{model_id}' is not present in provider '{provider_id}'"
                )))
            }
        }
        _ => Err(AppError::Invalid(
            "defaultProvider and defaultModel must both be set or both be absent".into(),
        )),
    }
}

pub(super) fn lock_provider_documents(paths: &Paths) -> Result<(WriteLock, Value, Value)> {
    // Initialize or repair the library before taking the operation lock, then re-read while locked.
    let _ = load_provider_documents(paths)?;
    let lock = WriteLock::acquire(paths)?;
    let mut library = read_document(&paths.providers, json!({}))?;
    let models = read_document(&paths.models, json!({ "providers": {} }))?;
    validate_local_library(&library)?;
    validate_provider_document(&models)?;
    let local = providers_object_mut(&mut library)?;
    for (id, provider) in providers_object(&models)? {
        if local.get(id) != Some(provider) {
            local.insert(id.clone(), provider.clone());
        }
    }
    Ok((lock, library, models))
}

pub(super) fn clear_default_for_provider(
    settings: &mut Value,
    paths: &Paths,
    id: &str,
) -> Result<bool> {
    if string_field(settings, "defaultProvider")?.as_deref() != Some(id) {
        return Ok(false);
    }
    let object = root_object_mut(settings, &paths.settings)?;
    object.remove("defaultProvider");
    object.remove("defaultModel");
    Ok(true)
}

pub(super) fn write_provider_changes(
    paths: &Paths,
    lock: &WriteLock,
    models: Option<&Value>,
    settings: Option<&Value>,
    library: &Value,
) -> Result<()> {
    let models_changed = models
        .map(|value| write_document(paths, lock, &paths.models, value))
        .transpose()?
        .unwrap_or(false);
    let settings_changed = settings
        .map(|value| write_document(paths, lock, &paths.settings, value))
        .transpose()
        .map_err(|error| {
            if models_changed {
                AppError::Partial(format!(
                    "models.json updated; settings.json failed: {error}"
                ))
            } else {
                error
            }
        })?
        .unwrap_or(false);
    write_document(paths, lock, &paths.providers, library)
        .map(|_| ())
        .map_err(|error| {
            if models_changed || settings_changed {
                AppError::Partial(format!(
                    "Pi configuration updated; providers.json failed: {error}"
                ))
            } else {
                error
            }
        })
}

fn language_field(settings: &Value) -> Result<String> {
    let Some(value) = settings.get("piSwitch") else {
        return Ok("en".into());
    };
    let object = value
        .as_object()
        .ok_or_else(|| AppError::Invalid("settings piSwitch must be an object".into()))?;
    match object.get("language") {
        None => Ok("en".into()),
        Some(Value::String(value)) if matches!(value.as_str(), "en" | "zh-CN") => Ok(value.clone()),
        Some(_) => Err(AppError::Invalid(
            "settings piSwitch.language must be 'en' or 'zh-CN'".into(),
        )),
    }
}

pub(super) fn pi_switch_object(settings: &Value) -> Result<Option<&Map<String, Value>>> {
    settings
        .get("piSwitch")
        .map(|value| {
            value
                .as_object()
                .ok_or_else(|| AppError::Invalid("settings piSwitch must be an object".into()))
        })
        .transpose()
}

fn fetch_model_metadata_field(settings: &Value) -> Result<bool> {
    match pi_switch_object(settings)?.and_then(|value| value.get("fetchModelMetadata")) {
        None => Ok(true),
        Some(Value::Bool(value)) => Ok(*value),
        Some(_) => Err(AppError::Invalid(
            "settings piSwitch.fetchModelMetadata must be a boolean".into(),
        )),
    }
}

fn model_defaults_field(settings: &Value) -> Result<ModelDefaults> {
    let Some(value) = pi_switch_object(settings)?.and_then(|value| value.get("modelDefaults"))
    else {
        return Ok(ModelDefaults::default());
    };
    let object = value.as_object().ok_or_else(|| {
        AppError::Invalid("settings piSwitch.modelDefaults must be an object".into())
    })?;
    Ok(ModelDefaults {
        context_window: optional_positive_u64(object, "contextWindow")?,
        max_tokens: optional_positive_u64(object, "maxTokens")?,
        input_cost: optional_nonnegative_f64(object, "inputCost")?,
        output_cost: optional_nonnegative_f64(object, "outputCost")?,
        cache_read_cost: optional_nonnegative_f64(object, "cacheReadCost")?,
        cache_write_cost: optional_nonnegative_f64(object, "cacheWriteCost")?,
    })
}

fn optional_positive_u64(object: &Map<String, Value>, field: &str) -> Result<Option<u64>> {
    match object.get(field) {
        None => Ok(None),
        Some(value) => value
            .as_u64()
            .filter(|value| *value > 0)
            .map(Some)
            .ok_or_else(|| {
                AppError::Invalid(format!(
                    "settings piSwitch.modelDefaults.{field} must be a positive integer"
                ))
            }),
    }
}

fn optional_nonnegative_f64(object: &Map<String, Value>, field: &str) -> Result<Option<f64>> {
    match object.get(field) {
        None => Ok(None),
        Some(value) => value
            .as_f64()
            .filter(|value| *value >= 0.0)
            .map(Some)
            .ok_or_else(|| {
                AppError::Invalid(format!(
                    "settings piSwitch.modelDefaults.{field} must be a non-negative number"
                ))
            }),
    }
}
