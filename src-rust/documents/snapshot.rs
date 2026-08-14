use super::settings::{
    check_updates_field, fetch_model_metadata_field, language_field, load_settings,
    model_defaults_field,
};
use super::*;

pub fn load_snapshot(paths: &Paths) -> Result<Snapshot> {
    let settings = load_settings(paths)?;
    let (library, models, provider_warning) = load_provider_documents(paths)?;
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
        pi_models_path: paths.pi_models.display().to_string(),
        pi_settings_path: paths.pi_settings.display().to_string(),
        app_settings_path: paths.app_settings.display().to_string(),
        providers: views,
        default_provider: string_field(&settings.pi, "defaultProvider")?,
        default_model: string_field(&settings.pi, "defaultModel")?,
        language: language_field(&settings.app)?,
        fetch_model_metadata: fetch_model_metadata_field(&settings.app)?,
        check_updates: check_updates_field(&settings.app)?,
        model_defaults: model_defaults_field(&settings.app)?,
        warning: merge_warnings(provider_warning, settings.warning),
    })
}

fn load_provider_documents(paths: &Paths) -> Result<(Value, Value, Option<String>)> {
    let models = read_document(&paths.pi_models, json!({ "providers": {} }))?;
    validate_provider_document(&models)?;
    if !paths.providers.exists() {
        let lock = WriteLock::acquire(paths)?;
        if !paths.providers.exists() {
            let models = read_document(&paths.pi_models, json!({ "providers": {} }))?;
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
            let models = read_document(&paths.pi_models, json!({ "providers": {} }))?;
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
        let models = read_document(&paths.pi_models, json!({ "providers": {} }))?;
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

pub(super) fn validate_pi_settings(settings: &Value, models: &Value) -> Result<()> {
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
    let models = read_document(&paths.pi_models, json!({ "providers": {} }))?;
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
    let object = root_object_mut(settings, &paths.pi_settings)?;
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
        .map(|value| write_document(paths, lock, &paths.pi_models, value))
        .transpose()?
        .unwrap_or(false);
    let settings_changed = settings
        .map(|value| write_document(paths, lock, &paths.pi_settings, value))
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

fn merge_warnings(first: Option<String>, second: Option<String>) -> Option<String> {
    match (first, second) {
        (Some(first), Some(second)) => Some(format!("{first}\n\n{second}")),
        (Some(warning), None) | (None, Some(warning)) => Some(warning),
        (None, None) => None,
    }
}
