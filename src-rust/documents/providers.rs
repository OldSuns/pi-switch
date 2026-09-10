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
    let mut draft = draft.clone();
    // In auth.json mode a given key moves to auth.json; provider JSON (both the
    // local library and models.json) never stores apiKey.
    let key_storage = settings::key_storage_setting(paths)?;
    let auth_key = match key_storage {
        KeyStorage::AuthJson if !draft.api_key.is_empty() => {
            Some(std::mem::take(&mut draft.api_key))
        }
        _ => None,
    };
    // A key that stays in the provider JSON must not leave an auth.json entry
    // behind: Pi prefers auth.json and would keep using the stale one.
    let key_in_provider_json =
        matches!(key_storage, KeyStorage::ModelsJson) && !draft.api_key.is_empty();
    let renamed_from = previous_id.filter(|old| *old != draft.id);
    let moves_credential = renamed_from.filter(|_| !key_in_provider_json);
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
    patch_provider(&mut provider, draft)?;
    provider_view(&draft.id, &provider)?;
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
        enabled.insert(draft.id.clone(), provider);
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
    // Credentials move last, once every provider-side validation passed: a
    // rejected edit must never rename or overwrite an auth.json entry. This
    // keeps the provider-then-auth lock order used by the other writers.
    if auth_key.is_some() || renamed_from.is_some() || key_in_provider_json {
        let provider_id = draft.id.clone();
        auth::edit(paths, |credentials| {
            // What this save would leave under the target id; the typed key wins
            // over a credential moved by a rename, exactly like the writes below.
            let current = credentials.get(&provider_id);
            let next = auth_key
                .as_ref()
                .map(|key| auth::api_key_entry(current, key))
                .or_else(|| renamed_from.and_then(|old| credentials.get(old)).cloned());
            // An `api_key` under the id being saved is this provider's own, even
            // when its provider document was removed and is only now re-created.
            let own_key =
                renamed_from.is_none() && current.is_some_and(|entry| entry["type"] == "api_key");
            // Retiring the entry of a models.json key is ours to do unless it also
            // carries provider config; replacing any other entry never is.
            let carries_config = current.is_some_and(auth::has_provider_config);
            let retires = key_in_provider_json && current.is_some() && (!own_key || carries_config);
            let replaces = !key_in_provider_json
                && !own_key
                && current
                    .zip(next.as_ref())
                    .is_some_and(|(current, next)| current != next);
            if !overwrite_credential && (retires || replaces) {
                return Err(AppError::CredentialOverwriteRequired(provider_id.clone()));
            }
            if key_in_provider_json {
                // Pi prefers auth.json, so the entry has to go or it would keep
                // shadowing the key that now lives in the provider JSON.
                credentials.remove(&provider_id);
            }
            if let Some(old) = moves_credential {
                if let Some(credential) = credentials.remove(old) {
                    credentials.insert(provider_id.clone(), credential);
                }
            }
            if let Some(ref key) = auth_key {
                let entry = auth::api_key_entry(credentials.get(&provider_id), key);
                credentials.insert(provider_id.clone(), entry);
            }
            Ok(())
        })?;
    }
    write_provider_changes(
        paths,
        &lock,
        Some(&models),
        settings_changed.then_some(&settings),
        &library,
    )
}

pub fn set_provider_in_pi(paths: &Paths, id: &str, in_pi: bool) -> Result<()> {
    let (lock, library, mut models) = lock_provider_documents(paths)?;
    let provider = providers_object(&library)?
        .get(id)
        .cloned()
        .ok_or_else(|| AppError::Invalid(format!("provider '{id}' no longer exists")))?;
    let enabled = providers_object_mut(&mut models)?;
    if in_pi {
        enabled.insert(id.into(), provider);
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
    )
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
        enabled.insert(copy_id.clone(), provider);
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
        sync_library_provider_to_pi(&library, &mut pi_models, provider_id)?;
        write_provider_changes(paths, &lock, Some(&pi_models), None, &library)?;
    }
    Ok(summary)
}

fn sync_library_provider_to_pi(
    library: &Value,
    models: &mut Value,
    provider_id: &str,
) -> Result<()> {
    let enabled = providers_object_mut(models)?;
    if enabled.contains_key(provider_id) {
        let provider = providers_object(library)?
            .get(provider_id)
            .cloned()
            .ok_or_else(|| {
                AppError::Invalid(format!("provider '{provider_id}' no longer exists"))
            })?;
        enabled.insert(provider_id.into(), provider);
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
    sync_library_provider_to_pi(&library, &mut pi_models, provider_id)?;
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
    sync_library_provider_to_pi(&library, &mut pi_models, provider_id)?;
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
