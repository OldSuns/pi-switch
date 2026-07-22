mod network;
mod schema;
mod storage;

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use serde::Serialize;
use serde_json::{json, Map, Value};
use thiserror::Error;

use schema::{
    default_model, patch_model, patch_provider, provider_models_mut, provider_view, unique_copy_id,
    validate_draft, validate_model_draft, validate_model_id, validate_provider_view,
};
use storage::{
    providers_object, providers_object_mut, read_document, rename_default_provider,
    root_object_mut, string_field, update_default_model, write_document, WriteLock,
};

pub use network::fetch_models;
pub use storage::{list_backups, restore_backup};

const API_TYPES: [&str; 4] = [
    "openai-completions",
    "openai-responses",
    "anthropic-messages",
    "google-generative-ai",
];

pub type Result<T> = std::result::Result<T, AppError>;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid JSON in {path}: {source}")]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("invalid configuration: {0}")]
    Invalid(String),
    #[error("another pi-switch process is writing configuration ({0})")]
    Busy(PathBuf),
    #[error("provider update completed, but settings update failed: {0}")]
    Partial(String),
    #[error("model catalog request failed: {0}")]
    Http(String),
}

#[derive(Clone, Debug)]
pub struct Paths {
    pub models: PathBuf,
    pub settings: PathBuf,
    pub backups: PathBuf,
    lock: PathBuf,
}

impl Paths {
    pub fn discover() -> Result<Self> {
        let home = dirs::home_dir()
            .ok_or_else(|| AppError::Invalid("home directory is unavailable".into()))?;
        Ok(Self::from_home(&home))
    }

    pub(crate) fn from_home(home: &Path) -> Self {
        Self {
            models: home.join(".pi/agent/models.json"),
            settings: home.join(".pi/agent/settings.json"),
            backups: home.join(".pi-switch/backups"),
            lock: home.join(".pi-switch/write.lock"),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub models_path: String,
    pub settings_path: String,
    pub providers: Vec<ProviderView>,
    pub default_provider: Option<String>,
    pub default_model: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderView {
    pub id: String,
    pub base_url: String,
    pub api: String,
    pub api_key: String,
    pub auth_header: bool,
    pub models: Vec<ModelView>,
    #[serde(skip)]
    pub raw: Value,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelView {
    pub id: String,
    pub name: Option<String>,
    pub api: Option<String>,
    pub reasoning: bool,
    pub input: Vec<String>,
    pub context_window: u64,
    pub max_tokens: u64,
}

#[derive(Clone, Debug)]
pub struct ProviderDraft {
    pub id: String,
    pub base_url: String,
    pub api: Option<String>,
    pub api_key: String,
    pub auth_header: bool,
    pub headers: Option<Value>,
    pub compat: Option<Value>,
}

#[derive(Clone, Debug)]
pub struct ModelDraft {
    pub id: String,
    pub name: Option<String>,
    pub api: Option<String>,
    pub reasoning: bool,
    pub input: Vec<String>,
    pub context_window: u64,
    pub max_tokens: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct Backup {
    pub path: String,
    pub name: String,
    pub target: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct DoctorCheck {
    pub ok: bool,
    pub label: String,
    pub detail: String,
}

pub fn load_snapshot(paths: &Paths) -> Result<Snapshot> {
    let models = read_document(&paths.models, json!({ "providers": {} }))?;
    let settings = read_document(&paths.settings, json!({}))?;
    let providers = providers_object(&models)?;

    let mut views = providers
        .iter()
        .map(|(id, value)| provider_view(id, value))
        .collect::<Result<Vec<_>>>()?;
    views.sort_by(|a, b| a.id.to_lowercase().cmp(&b.id.to_lowercase()));

    Ok(Snapshot {
        models_path: paths.models.display().to_string(),
        settings_path: paths.settings.display().to_string(),
        providers: views,
        default_provider: string_field(&settings, "defaultProvider")?,
        default_model: string_field(&settings, "defaultModel")?,
    })
}

pub fn save_provider(
    paths: &Paths,
    previous_id: Option<&str>,
    draft: &ProviderDraft,
) -> Result<()> {
    validate_draft(draft)?;
    let _lock = WriteLock::acquire(paths)?;
    let mut models = read_document(&paths.models, json!({ "providers": {} }))?;
    let providers = providers_object_mut(&mut models)?;

    if let Some(old) = previous_id {
        if !providers.contains_key(old) {
            return Err(AppError::Invalid(format!(
                "provider '{old}' no longer exists"
            )));
        }
    }

    if previous_id != Some(draft.id.as_str()) && providers.contains_key(&draft.id) {
        return Err(AppError::Invalid(format!(
            "provider '{}' already exists",
            draft.id
        )));
    }

    let mut provider = previous_id
        .and_then(|id| providers.get(id).cloned())
        .unwrap_or_else(|| json!({}));
    patch_provider(&mut provider, draft)?;
    provider_view(&draft.id, &provider)?;
    if let Some(old) = previous_id.filter(|old| *old != draft.id) {
        if providers.remove(old).is_none() {
            return Err(AppError::Invalid(format!(
                "provider '{old}' no longer exists"
            )));
        }
    }
    providers.insert(draft.id.clone(), provider);
    write_document(paths, &paths.models, "models", &models)?;

    if let Some(old) = previous_id.filter(|old| *old != draft.id) {
        rename_default_provider(paths, old, &draft.id)
            .map_err(|error| AppError::Partial(error.to_string()))?;
    }
    Ok(())
}

pub fn remove_provider(paths: &Paths, id: &str) -> Result<()> {
    let _lock = WriteLock::acquire(paths)?;
    let mut models = read_document(&paths.models, json!({ "providers": {} }))?;
    if providers_object_mut(&mut models)?.remove(id).is_none() {
        return Err(AppError::Invalid(format!(
            "provider '{id}' no longer exists"
        )));
    }
    write_document(paths, &paths.models, "models", &models)?;

    let mut settings = read_document(&paths.settings, json!({}))?;
    if string_field(&settings, "defaultProvider")?.as_deref() == Some(id) {
        let object = root_object_mut(&mut settings, &paths.settings)?;
        object.remove("defaultProvider");
        object.remove("defaultModel");
        write_document(paths, &paths.settings, "settings", &settings)
            .map_err(|error| AppError::Partial(error.to_string()))?;
    }
    Ok(())
}

pub fn duplicate_provider(paths: &Paths, source_id: &str) -> Result<String> {
    let _lock = WriteLock::acquire(paths)?;
    let mut models = read_document(&paths.models, json!({ "providers": {} }))?;
    let providers = providers_object_mut(&mut models)?;
    let provider = providers
        .get(source_id)
        .cloned()
        .ok_or_else(|| AppError::Invalid(format!("provider '{source_id}' no longer exists")))?;
    provider_view(source_id, &provider)?;
    let copy_id = unique_copy_id(source_id, |candidate| providers.contains_key(candidate));
    providers.insert(copy_id.clone(), provider);
    write_document(paths, &paths.models, "models", &models)?;
    Ok(copy_id)
}

pub fn import_models(paths: &Paths, provider_id: &str, model_ids: &[String]) -> Result<usize> {
    for model_id in model_ids {
        validate_model_id(model_id)?;
    }
    let _lock = WriteLock::acquire(paths)?;
    let mut root = read_document(&paths.models, json!({ "providers": {} }))?;
    let models = provider_models_mut(&mut root, provider_id)?;
    let mut known = models
        .iter()
        .filter_map(|model| model.get("id").and_then(Value::as_str))
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    let mut added = 0;
    for model_id in model_ids {
        if known.insert(model_id.clone()) {
            models.push(default_model(model_id));
            added += 1;
        }
    }
    if added > 0 {
        write_document(paths, &paths.models, "models", &root)?;
    }
    Ok(added)
}

pub fn save_model(
    paths: &Paths,
    provider_id: &str,
    previous_id: Option<&str>,
    draft: &ModelDraft,
) -> Result<()> {
    validate_model_draft(draft)?;
    let _lock = WriteLock::acquire(paths)?;
    let mut root = read_document(&paths.models, json!({ "providers": {} }))?;
    let models = provider_models_mut(&mut root, provider_id)?;

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
        let object = model
            .as_object_mut()
            .ok_or_else(|| AppError::Invalid("model entry must be an object".into()))?;
        patch_model(object, draft);
    } else {
        let mut model = Map::new();
        patch_model(&mut model, draft);
        models.push(Value::Object(model));
    }

    write_document(paths, &paths.models, "models", &root)?;
    if let Some(previous_id) = previous_id.filter(|previous_id| *previous_id != draft.id) {
        update_default_model(paths, provider_id, previous_id, Some(&draft.id))
            .map_err(|error| AppError::Partial(error.to_string()))?;
    }
    Ok(())
}

pub fn remove_model(paths: &Paths, provider_id: &str, model_id: &str) -> Result<()> {
    let _lock = WriteLock::acquire(paths)?;
    let mut root = read_document(&paths.models, json!({ "providers": {} }))?;
    let models = provider_models_mut(&mut root, provider_id)?;
    let index = models
        .iter()
        .position(|model| model.get("id").and_then(Value::as_str) == Some(model_id))
        .ok_or_else(|| {
            AppError::Invalid(format!(
                "model '{model_id}' no longer exists in provider '{provider_id}'"
            ))
        })?;
    models.remove(index);
    write_document(paths, &paths.models, "models", &root)?;
    update_default_model(paths, provider_id, model_id, None)
        .map_err(|error| AppError::Partial(error.to_string()))
}

pub fn set_default(paths: &Paths, provider_id: &str, model_id: &str) -> Result<()> {
    let _lock = WriteLock::acquire(paths)?;
    let snapshot = load_snapshot(paths)?;
    let provider = snapshot
        .providers
        .iter()
        .find(|provider| provider.id == provider_id)
        .ok_or_else(|| AppError::Invalid(format!("provider '{provider_id}' does not exist")))?;
    if !provider.models.iter().any(|model| model.id == model_id) {
        return Err(AppError::Invalid(format!(
            "model '{model_id}' does not belong to provider '{provider_id}'"
        )));
    }

    let mut settings = read_document(&paths.settings, json!({}))?;
    let object = root_object_mut(&mut settings, &paths.settings)?;
    object.insert("defaultProvider".into(), Value::String(provider_id.into()));
    object.insert("defaultModel".into(), Value::String(model_id.into()));
    write_document(paths, &paths.settings, "settings", &settings)
}

pub fn doctor(paths: &Paths) -> Vec<DoctorCheck> {
    let mut checks = Vec::new();
    checks.push(check(
        paths.models.exists(),
        "models.json",
        if paths.models.exists() {
            paths.models.display().to_string()
        } else {
            format!(
                "not found at {}; Pi may not be initialized",
                paths.models.display()
            )
        },
    ));
    match load_snapshot(paths) {
        Ok(snapshot) => {
            checks.push(check(
                true,
                "Pi documents",
                format!("{} provider(s), JSON is valid", snapshot.providers.len()),
            ));
            let default_ok = match (&snapshot.default_provider, &snapshot.default_model) {
                (None, None) => true,
                (Some(provider), Some(model)) => snapshot.providers.iter().any(|item| {
                    item.id == *provider
                        && item.models.iter().any(|candidate| candidate.id == *model)
                }),
                _ => false,
            };
            checks.push(check(
                default_ok,
                "Default model",
                if default_ok {
                    snapshot
                        .default_provider
                        .zip(snapshot.default_model)
                        .map(|(provider, model)| format!("{provider}/{model}"))
                        .unwrap_or_else(|| "not explicitly configured".into())
                } else {
                    "defaultProvider/defaultModel is incomplete or references missing data".into()
                },
            ));
            for provider in snapshot.providers {
                let valid = validate_provider_view(&provider).is_ok();
                checks.push(check(
                    valid,
                    format!("Provider {}", provider.id),
                    validate_provider_view(&provider)
                        .map(|_| format!("{} model(s), {}", provider.models.len(), provider.api))
                        .unwrap_or_else(|error| error.to_string()),
                ));
            }
        }
        Err(error) => checks.push(check(false, "Pi documents", error.to_string())),
    }
    checks.push(check(
        !paths.lock.exists(),
        "Write lock",
        if paths.lock.exists() {
            format!(
                "lock exists at {}; remove it after confirming no writer is active",
                paths.lock.display()
            )
        } else {
            "available".into()
        },
    ));
    checks
}

fn check(ok: bool, label: impl Into<String>, detail: impl Into<String>) -> DoctorCheck {
    DoctorCheck {
        ok,
        label: label.into(),
        detail: detail.into(),
    }
}

#[cfg(test)]
use network::{parse_catalog, resolve_secret};
#[cfg(test)]
use storage::now_millis;

#[cfg(test)]
mod tests;
