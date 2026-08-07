use super::snapshot::{
    clear_default_for_provider, lock_provider_documents, write_provider_changes,
};
use super::*;

pub fn save_provider(
    paths: &Paths,
    previous_id: Option<&str>,
    draft: &ProviderDraft,
) -> Result<()> {
    validate_draft(draft)?;
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

    let mut settings = read_document(&paths.settings, json!({}))?;
    let mut settings_changed = false;
    if let Some(old) = previous_id {
        if was_in_pi && !draft.in_pi {
            settings_changed = clear_default_for_provider(&mut settings, paths, old)?;
        } else if old != draft.id
            && string_field(&settings, "defaultProvider")?.as_deref() == Some(old)
        {
            root_object_mut(&mut settings, &paths.settings)?
                .insert("defaultProvider".into(), Value::String(draft.id.clone()));
            settings_changed = true;
        }
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
    let mut settings = read_document(&paths.settings, json!({}))?;
    let settings_changed = !in_pi && clear_default_for_provider(&mut settings, paths, id)?;
    write_provider_changes(
        paths,
        &lock,
        Some(&models),
        settings_changed.then_some(&settings),
        &library,
    )
}

pub fn remove_provider(paths: &Paths, id: &str) -> Result<()> {
    let (lock, mut library, mut models) = lock_provider_documents(paths)?;
    if providers_object_mut(&mut library)?.remove(id).is_none() {
        return Err(AppError::Invalid(format!(
            "provider '{id}' no longer exists"
        )));
    }
    providers_object_mut(&mut models)?.remove(id);
    let mut settings = read_document(&paths.settings, json!({}))?;
    let settings_changed = clear_default_for_provider(&mut settings, paths, id)?;
    write_provider_changes(
        paths,
        &lock,
        Some(&models),
        settings_changed.then_some(&settings),
        &library,
    )
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
    validate_model_draft(draft)?;
    let (lock, mut library, mut pi_models) = lock_provider_documents(paths)?;
    let models = provider_models_mut(&mut library, provider_id)?;
    if models.iter().any(|model| {
        model.get("id").and_then(Value::as_str) == Some(draft.id.as_str())
            && previous_id != Some(draft.id.as_str())
    }) {
        return Err(AppError::Invalid(format!(
            "model '{}' already exists in provider '{provider_id}'",
            draft.id
        )));
    }
    if let Some(previous_id) = previous_id {
        let model = models
            .iter_mut()
            .find(|model| model.get("id").and_then(Value::as_str) == Some(previous_id))
            .ok_or_else(|| {
                AppError::Invalid(format!(
                    "model '{previous_id}' no longer exists in provider '{provider_id}'"
                ))
            })?;
        patch_model(
            model
                .as_object_mut()
                .ok_or_else(|| AppError::Invalid("model entry must be an object".into()))?,
            draft,
        );
    } else {
        let mut model = Map::new();
        patch_model(&mut model, draft);
        models.push(Value::Object(model));
    }
    sync_library_provider_to_pi(&library, &mut pi_models, provider_id)?;
    let mut settings = read_document(&paths.settings, json!({}))?;
    let settings_changed = if let Some(old) = previous_id.filter(|old| *old != draft.id) {
        if string_field(&settings, "defaultProvider")?.as_deref() == Some(provider_id)
            && string_field(&settings, "defaultModel")?.as_deref() == Some(old)
        {
            root_object_mut(&mut settings, &paths.settings)?
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
    let mut settings = read_document(&paths.settings, json!({}))?;
    let selected = string_field(&settings, "defaultProvider")?.as_deref() == Some(provider_id)
        && string_field(&settings, "defaultModel")?.as_deref() == Some(model_id);
    if selected {
        let object = root_object_mut(&mut settings, &paths.settings)?;
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
    let models = read_document(&paths.models, json!({ "providers": {} }))?;
    let provider = providers_object(&models)?
        .get(provider_id)
        .ok_or_else(|| AppError::Invalid(format!("provider '{provider_id}' is not added to Pi")))?;
    let provider = provider_view(provider_id, provider)?;
    if !provider.models.iter().any(|model| model.id == model_id) {
        return Err(AppError::Invalid(format!(
            "model '{model_id}' does not belong to provider '{provider_id}'"
        )));
    }
    let mut settings = read_document(&paths.settings, json!({}))?;
    let object = root_object_mut(&mut settings, &paths.settings)?;
    object.insert("defaultProvider".into(), Value::String(provider_id.into()));
    object.insert("defaultModel".into(), Value::String(model_id.into()));
    write_document(paths, &lock, &paths.settings, &settings).map(|_| ())
}
