use super::snapshot::{
    clear_default_for_provider, lock_provider_documents, write_provider_changes,
};
use super::*;

pub fn save_provider(
    paths: &Paths,
    previous_id: Option<&str>,
    draft: &ProviderDraft,
) -> Result<()> {
    save(paths, previous_id, draft, false)
}

/// [`save_provider`] for a caller whose user has already confirmed that an
/// existing `auth.json` entry under the target id may be replaced.
pub fn save_provider_overwriting_credential(
    paths: &Paths,
    previous_id: Option<&str>,
    draft: &ProviderDraft,
) -> Result<()> {
    save(paths, previous_id, draft, true)
}

fn save(
    paths: &Paths,
    previous_id: Option<&str>,
    draft: &ProviderDraft,
    overwrite_credential: bool,
) -> Result<()> {
    validate_draft(draft)?;
    let draft = draft.clone();
    let key_storage = settings::key_storage_setting(paths)?;
    let renamed_from = previous_id.filter(|old| *old != draft.id);
    let draft = &draft;
    let (lock, mut library, mut models) = lock_provider_documents(paths)?;
    let local = providers_object_mut(&mut library)?;
    if let Some(old) = previous_id.filter(|old| !local.contains_key(*old)) {
        return Err(AppError::Invalid(format!(
            "provider '{old}' no longer exists"
        )));
    }
    if previous_id != Some(draft.id.as_str()) && local.contains_key(&draft.id) {
        return Err(AppError::Invalid(format!(
            "provider '{}' already exists",
            draft.id
        )));
    }
    let mut provider = previous_id
        .and_then(|id| local.get(id).cloned())
        .unwrap_or_else(|| json!({}));
    let existing_key = provider.get("apiKey").cloned();
    patch_provider(&mut provider, draft)?;
    if matches!(key_storage, KeyStorage::AuthJson) && draft.api_key.is_empty() {
        if let Some(key) = existing_key {
            provider
                .as_object_mut()
                .ok_or_else(|| AppError::Invalid("provider data must be an object".into()))?
                .insert("apiKey".into(), key);
        }
    }
    provider_view(&draft.id, &provider)?;
    let provider_for_pi = provider_for_pi(&provider, key_storage)?;
    if let Some(old) = previous_id.filter(|old| *old != draft.id) {
        local.remove(old);
    }
    local.insert(draft.id.clone(), provider.clone());
    if let Some(old) = previous_id.filter(|old| *old != draft.id) {
        ordering::rename_provider(&mut library, old, &draft.id);
    }

    let enabled = providers_object_mut(&mut models)?;
    let was_in_pi = previous_id.is_some_and(|id| enabled.contains_key(id));
    if let Some(old) = previous_id.filter(|old| *old != draft.id) {
        enabled.remove(old);
    }
    if draft.in_pi {
        enabled.insert(draft.id.clone(), provider_for_pi);
    } else if let Some(old) = previous_id {
        enabled.remove(old);
    }

    let mut settings = read_document(&paths.pi_settings, json!({}))?;
    let mut settings_changed = false;
    if let Some(old) = previous_id {
        if was_in_pi && !draft.in_pi {
            settings_changed = clear_default_for_provider(&mut settings, paths, old)?;
        } else if old != draft.id
            && string_field(&settings, "defaultProvider")?.as_deref() == Some(old)
        {
            root_object_mut(&mut settings, &paths.pi_settings)?
                .insert("defaultProvider".into(), Value::String(draft.id.clone()));
            settings_changed = true;
        }
    }
    // `providers.json` is pi-switch's local copy; auth.json is only the Pi
    // projection for enabled providers. Keep the local key even when a
    // provider is not currently synced.
    let local_key = provider
        .get("apiKey")
        .and_then(Value::as_str)
        .filter(|key| !key.is_empty())
        .map(str::to_owned);
    let credentials_changed = match key_storage {
        KeyStorage::AuthJson => {
            let target = draft.id.clone();
            let source = renamed_from.map(str::to_owned);
            if draft.in_pi || was_in_pi || source.is_some() {
                auth::edit(paths, |credentials| {
                    if !draft.in_pi {
                        for id in std::iter::once(&target).chain(source.as_ref()) {
                            if credentials.get(id).is_some_and(auth::is_api_key) {
                                credentials.remove(id);
                            }
                        }
                        return Ok(());
                    }
                    if let Some(key) = local_key.as_deref() {
                        let base = source
                            .as_ref()
                            .and_then(|old| credentials.get(old))
                            .filter(|entry| auth::is_api_key(entry))
                            .or_else(|| credentials.get(&target));
                        let next = auth::api_key_entry(base, key);
                        let replaces = credentials
                            .get(&target)
                            .is_some_and(|current| current != &next);
                        let own = source.as_deref().is_none_or(|source| source == target)
                            && credentials.get(&target).is_some_and(auth::is_api_key);
                        if replaces && !own && !overwrite_credential {
                            return Err(AppError::CredentialOverwriteRequired(target.clone()));
                        }
                        if let Some(old) = source.as_deref() {
                            if credentials.get(old).is_some_and(auth::is_api_key) {
                                credentials.remove(old);
                            }
                        }
                        credentials.insert(target.clone(), next);
                    }
                    Ok(())
                })?
            } else {
                false
            }
        }
        KeyStorage::ModelsJson if local_key.is_some() => {
            let target = draft.id.clone();
            let source = renamed_from.map(str::to_owned);
            auth::edit(paths, |credentials| {
                // Pi resolves auth.json first, so stale entries would shadow
                // the key in models.json. Plain API-key entries are ours to retire.
                for id in std::iter::once(&target).chain(source.as_ref()) {
                    let Some(entry) = credentials.get(id) else {
                        continue;
                    };
                    if !(auth::is_plain_api_key(entry) || overwrite_credential) {
                        return Err(AppError::CredentialOverwriteRequired(id.clone()));
                    }
                    credentials.remove(id);
                }
                Ok(())
            })?
        }
        _ => false,
    };
    write_provider_changes(
        paths,
        &lock,
        Some(&models),
        settings_changed.then_some(&settings),
        &library,
    )
    .map_err(|error| {
        if credentials_changed {
            AppError::Partial(format!(
                "auth.json was updated, but the provider documents were not: {error}"
            ))
        } else {
            error
        }
    })
}

pub(super) fn provider_for_pi(provider: &Value, key_storage: KeyStorage) -> Result<Value> {
    let mut provider = provider.clone();
    if matches!(key_storage, KeyStorage::AuthJson) {
        provider
            .as_object_mut()
            .ok_or_else(|| AppError::Invalid("provider data must be an object".into()))?
            .remove("apiKey");
    }
    Ok(provider)
}

pub fn set_provider_in_pi(paths: &Paths, id: &str, in_pi: bool) -> Result<()> {
    let key_storage = settings::key_storage_setting(paths)?;
    let (lock, mut library, mut models) = lock_provider_documents(paths)?;
    let local = providers_object_mut(&mut library)?;
    let mut provider = local
        .get(id)
        .cloned()
        .ok_or_else(|| AppError::Invalid(format!("provider '{id}' no longer exists")))?;
    let original_provider = provider.clone();
    let mut local_key = provider
        .get("apiKey")
        .and_then(Value::as_str)
        .filter(|key| !key.is_empty())
        .map(str::to_owned);
    let auth_credentials = if matches!(key_storage, KeyStorage::AuthJson) {
        Some(auth::read_credentials(paths)?)
    } else {
        None
    };
    if local_key.is_none() {
        local_key = auth_credentials
            .as_ref()
            .and_then(|credentials| credentials.get(id))
            .filter(|entry| auth::is_api_key(entry))
            .and_then(|entry| entry.get("key"))
            .and_then(Value::as_str)
            .filter(|key| !key.is_empty())
            .map(str::to_owned);
        if let Some(key) = local_key.as_deref() {
            provider
                .as_object_mut()
                .ok_or_else(|| AppError::Invalid("provider data must be an object".into()))?
                .insert("apiKey".into(), Value::String(key.into()));
        }
    }
    if in_pi && matches!(key_storage, KeyStorage::AuthJson) {
        if let Some(key) = local_key.as_deref() {
            let next = auth::api_key_entry(
                auth_credentials
                    .as_ref()
                    .and_then(|credentials| credentials.get(id)),
                key,
            );
            if auth_credentials
                .as_ref()
                .and_then(|credentials| credentials.get(id))
                .is_some_and(|current| current != &next && !auth::is_api_key(current))
            {
                return Err(AppError::CredentialOverwriteRequired(id.into()));
            }
        }
    }
    let provider_for_pi = provider_for_pi(&provider, key_storage)?;
    if provider != original_provider {
        providers_object_mut(&mut library)?.insert(id.into(), provider);
    }
    let enabled = providers_object_mut(&mut models)?;
    if in_pi {
        enabled.insert(id.into(), provider_for_pi);
    } else {
        enabled.remove(id);
    }
    let mut settings = read_document(&paths.pi_settings, json!({}))?;
    let settings_changed = !in_pi && clear_default_for_provider(&mut settings, paths, id)?;
    write_provider_changes(
        paths,
        &lock,
        Some(&models),
        settings_changed.then_some(&settings),
        &library,
    )?;
    if matches!(key_storage, KeyStorage::AuthJson) {
        auth::edit(paths, |credentials| {
            if in_pi {
                if let Some(key) = local_key.as_deref() {
                    let next = auth::api_key_entry(credentials.get(id), key);
                    credentials.insert(id.into(), next);
                }
            } else if credentials.get(id).is_some_and(auth::is_api_key) {
                credentials.remove(id);
            }
            Ok(())
        })
        .map_err(|error| {
            AppError::Partial(format!(
                "provider documents updated, but auth.json update failed: {error}"
            ))
        })?;
    }
    Ok(())
}

pub fn remove_provider(paths: &Paths, id: &str, remove_auth: bool) -> Result<()> {
    let (lock, mut library, mut models) = lock_provider_documents(paths)?;
    if providers_object_mut(&mut library)?.remove(id).is_none() {
        return Err(AppError::Invalid(format!(
            "provider '{id}' no longer exists"
        )));
    }
    providers_object_mut(&mut models)?.remove(id);
    let mut settings = read_document(&paths.pi_settings, json!({}))?;
    let settings_changed = clear_default_for_provider(&mut settings, paths, id)?;
    write_provider_changes(
        paths,
        &lock,
        Some(&models),
        settings_changed.then_some(&settings),
        &library,
    )?;
    if remove_auth {
        auth::edit(paths, |credentials| {
            // Only `api_key` entries are ours to delete: OAuth and any other
            // Pi-managed credential stays behind.
            let is_api_key = credentials
                .get(id)
                .and_then(Value::as_object)
                .and_then(|credential| credential.get("type"))
                .and_then(Value::as_str)
                == Some("api_key");
            if is_api_key {
                credentials.remove(id);
            }
            Ok(())
        })
        .map_err(|error| {
            AppError::Partial(format!(
                "provider '{id}' removed; auth.json update failed: {error}"
            ))
        })?;
    }
    Ok(())
}

pub fn duplicate_provider(paths: &Paths, source_id: &str) -> Result<String> {
    let key_storage = settings::key_storage_setting(paths)?;
    let (lock, mut library, mut models) = lock_provider_documents(paths)?;
    let local = providers_object_mut(&mut library)?;
    let provider = local
        .get(source_id)
        .cloned()
        .ok_or_else(|| AppError::Invalid(format!("provider '{source_id}' no longer exists")))?;
    provider_view(source_id, &provider)?;
    let copy_id = unique_copy_id(source_id, |candidate| local.contains_key(candidate));
    local.insert(copy_id.clone(), provider.clone());
    let enabled = providers_object_mut(&mut models)?;
    if enabled.contains_key(source_id) {
        enabled.insert(copy_id.clone(), provider_for_pi(&provider, key_storage)?);
    }
    write_provider_changes(paths, &lock, Some(&models), None, &library)?;
    Ok(copy_id)
}

pub fn import_models(
    paths: &Paths,
    provider_id: &str,
    catalog_models: &[CatalogModel],
    update_existing: bool,
) -> Result<ModelImportSummary> {
    for model in catalog_models {
        validate_model_id(&model.id)?;
    }
    let key_storage = settings::key_storage_setting(paths)?;
    let (lock, mut library, mut pi_models) = lock_provider_documents(paths)?;
    let models = provider_models_mut(&mut library, provider_id)?;
    let mut summary = ModelImportSummary {
        added: 0,
        updated: 0,
    };
    for catalog_model in catalog_models {
        if let Some(existing) = models
            .iter_mut()
            .find(|model| model.get("id").and_then(Value::as_str) == Some(&catalog_model.id))
        {
            if !update_existing {
                continue;
            }
            let object = existing
                .as_object_mut()
                .ok_or_else(|| AppError::Invalid("model entry must be an object".into()))?;
            let before = object.clone();
            object.extend(
                catalog_model
                    .config
                    .as_object()
                    .expect("validated catalog model")
                    .clone(),
            );
            summary.updated += usize::from(*object != before);
        } else {
            models.push(catalog_model.config.clone());
            summary.added += 1;
        }
    }
    if summary.added + summary.updated > 0 {
        sync_library_provider_to_pi(&library, &mut pi_models, provider_id, key_storage)?;
        write_provider_changes(paths, &lock, Some(&pi_models), None, &library)?;
    }
    Ok(summary)
}

fn sync_library_provider_to_pi(
    library: &Value,
    models: &mut Value,
    provider_id: &str,
    key_storage: KeyStorage,
) -> Result<()> {
    let enabled = providers_object_mut(models)?;
    if enabled.contains_key(provider_id) {
        let provider = providers_object(library)?
            .get(provider_id)
            .cloned()
            .ok_or_else(|| {
                AppError::Invalid(format!("provider '{provider_id}' no longer exists"))
            })?;
        enabled.insert(provider_id.into(), provider_for_pi(&provider, key_storage)?);
    }
    Ok(())
}

pub fn save_model(
    paths: &Paths,
    provider_id: &str,
    previous_id: Option<&str>,
    draft: &ModelDraft,
) -> Result<()> {
    let source = previous_id.map_or(ModelSource::New, ModelSource::Edit);
    write_model(paths, provider_id, source, draft)
}

pub fn duplicate_model(
    paths: &Paths,
    provider_id: &str,
    source_id: &str,
    draft: &ModelDraft,
) -> Result<()> {
    write_model(paths, provider_id, ModelSource::Copy(source_id), draft)
}

enum ModelSource<'a> {
    New,
    Edit(&'a str),
    Copy(&'a str),
}

fn write_model(
    paths: &Paths,
    provider_id: &str,
    source: ModelSource<'_>,
    draft: &ModelDraft,
) -> Result<()> {
    validate_model_draft(draft)?;
    let key_storage = settings::key_storage_setting(paths)?;
    let (lock, mut library, mut pi_models) = lock_provider_documents(paths)?;
    let models = provider_models_mut(&mut library, provider_id)?;
    let (source_id, previous_id) = match source {
        ModelSource::New => (None, None),
        ModelSource::Edit(id) => (Some(id), Some(id)),
        ModelSource::Copy(id) => (Some(id), None),
    };
    if models.iter().any(|model| {
        model.get("id").and_then(Value::as_str) == Some(draft.id.as_str())
            && previous_id != Some(draft.id.as_str())
    }) {
        return Err(AppError::Invalid(format!(
            "model '{}' already exists in provider '{provider_id}'",
            draft.id
        )));
    }
    let source_index = source_id
        .map(|id| {
            models
                .iter()
                .position(|model| model.get("id").and_then(Value::as_str) == Some(id))
                .ok_or_else(|| {
                    AppError::Invalid(format!(
                        "model '{id}' no longer exists in provider '{provider_id}'"
                    ))
                })
        })
        .transpose()?;
    // Clone the stored object, not the UI projection, to retain extension fields.
    let mut model = match source_index {
        Some(index) => models[index].clone(),
        None => Value::Object(Map::new()),
    };
    patch_model(
        model
            .as_object_mut()
            .ok_or_else(|| AppError::Invalid("model entry must be an object".into()))?,
        draft,
    );
    if let Some(index) = source_index.filter(|_| previous_id.is_some()) {
        models[index] = model;
    } else {
        models.push(model);
    }
    if let Some(old) = previous_id.filter(|old| *old != draft.id) {
        ordering::rename_model(&mut library, provider_id, old, &draft.id);
    }
    sync_library_provider_to_pi(&library, &mut pi_models, provider_id, key_storage)?;
    let mut settings = read_document(&paths.pi_settings, json!({}))?;
    let settings_changed = if let Some(old) = previous_id.filter(|old| *old != draft.id) {
        if string_field(&settings, "defaultProvider")?.as_deref() == Some(provider_id)
            && string_field(&settings, "defaultModel")?.as_deref() == Some(old)
        {
            root_object_mut(&mut settings, &paths.pi_settings)?
                .insert("defaultModel".into(), Value::String(draft.id.clone()));
            true
        } else {
            false
        }
    } else {
        false
    };
    write_provider_changes(
        paths,
        &lock,
        Some(&pi_models),
        settings_changed.then_some(&settings),
        &library,
    )
}

pub fn remove_model(paths: &Paths, provider_id: &str, model_id: &str) -> Result<()> {
    let key_storage = settings::key_storage_setting(paths)?;
    let (lock, mut library, mut pi_models) = lock_provider_documents(paths)?;
    let models = provider_models_mut(&mut library, provider_id)?;
    let index = models
        .iter()
        .position(|model| model.get("id").and_then(Value::as_str) == Some(model_id))
        .ok_or_else(|| {
            AppError::Invalid(format!(
                "model '{model_id}' no longer exists in provider '{provider_id}'"
            ))
        })?;
    models.remove(index);
    sync_library_provider_to_pi(&library, &mut pi_models, provider_id, key_storage)?;
    let mut settings = read_document(&paths.pi_settings, json!({}))?;
    let selected = string_field(&settings, "defaultProvider")?.as_deref() == Some(provider_id)
        && string_field(&settings, "defaultModel")?.as_deref() == Some(model_id);
    if selected {
        let object = root_object_mut(&mut settings, &paths.pi_settings)?;
        object.remove("defaultProvider");
        object.remove("defaultModel");
    }
    write_provider_changes(
        paths,
        &lock,
        Some(&pi_models),
        selected.then_some(&settings),
        &library,
    )
}

pub fn set_default(paths: &Paths, provider_id: &str, model_id: &str) -> Result<()> {
    let lock = WriteLock::acquire(paths)?;
    let models = read_document(&paths.pi_models, json!({ "providers": {} }))?;
    let provider = providers_object(&models)?
        .get(provider_id)
        .ok_or_else(|| AppError::Invalid(format!("provider '{provider_id}' is not added to Pi")))?;
    let provider = provider_view(provider_id, provider)?;
    if !provider.models.iter().any(|model| model.id == model_id) {
        return Err(AppError::Invalid(format!(
            "model '{model_id}' does not belong to provider '{provider_id}'"
        )));
    }
    let mut settings = read_document(&paths.pi_settings, json!({}))?;
    let object = root_object_mut(&mut settings, &paths.pi_settings)?;
    object.insert("defaultProvider".into(), Value::String(provider_id.into()));
    object.insert("defaultModel".into(), Value::String(model_id.into()));
    write_document(paths, &lock, &paths.pi_settings, &settings).map(|_| ())
}
