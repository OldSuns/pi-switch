mod network;
mod opencode;
mod schema;
mod sessions;
mod storage;

use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};
use thiserror::Error;

use schema::{
    patch_model, patch_provider, provider_models_mut, provider_view, unique_copy_id,
    validate_draft, validate_model_draft, validate_model_id, validate_provider_view,
};
use storage::{
    archive_corrupt_provider_store, providers_object, providers_object_mut, read_document,
    root_object_mut, string_field, write_document, write_initial_document, WriteLock,
};

pub use network::fetch_models;
pub use opencode::{apply_opencode_import, list_opencode_providers, prepare_opencode_import};
pub use sessions::{
    delete_session, format_session_time, list_sessions, load_preview, session_display_title,
    session_matches, DeleteMethod, PreviewMessage, SessionSummary,
};
pub use storage::{list_backups, restore_backup};

pub const API_TYPES: [&str; 4] = [
    "openai-completions",
    "openai-responses",
    "anthropic-messages",
    "google-generative-ai",
];
pub const USER_AGENT_HEADER: &str = "User-Agent";

pub const PI_DEFAULT_CONTEXT_WINDOW: u64 = 128_000;
pub const PI_DEFAULT_MAX_TOKENS: u64 = 16_384;

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
    pub providers: PathBuf,
    pub models: PathBuf,
    pub settings: PathBuf,
    pub opencode: PathBuf,
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
            providers: home.join(".pi-switch/providers.json"),
            models: home.join(".pi/agent/models.json"),
            settings: home.join(".pi/agent/settings.json"),
            opencode: home.join(".config/opencode/opencode.json"),
            backups: home.join(".pi-switch/backups"),
            lock: home.join(".pi-switch/write.lock"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub providers_path: String,
    pub models_path: String,
    pub settings_path: String,
    pub providers: Vec<ProviderView>,
    pub default_provider: Option<String>,
    pub default_model: Option<String>,
    pub language: String,
    pub fetch_model_metadata: bool,
    pub model_defaults: ModelDefaults,
    pub warning: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ProviderView {
    pub id: String,
    pub in_pi: bool,
    pub base_url: String,
    pub api: String,
    pub api_key: String,
    pub auth_header: bool,
    pub models: Vec<ModelView>,
    pub raw: Value,
}

#[derive(Clone, Debug)]
pub struct ModelView {
    pub id: String,
    pub name: Option<String>,
    pub api: Option<String>,
    pub reasoning: bool,
    pub input: Vec<String>,
    pub context_window: Option<u64>,
    pub max_tokens: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct ProviderDraft {
    pub id: String,
    pub in_pi: bool,
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

#[derive(Clone, Debug)]
pub struct CatalogModel {
    pub id: String,
    pub config: Value,
}

/// Per-model cost computed from NewAPI `/api/ratio_config`. Costs are per 1M
/// tokens in USD (`ratio × 2`, since `1 USD = 500,000 quota`).
#[derive(Clone, Debug)]
pub struct RatioCost {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write: f64,
}

impl RatioCost {
    pub fn to_cost_json(&self) -> Value {
        json!({
            "input": self.input,
            "output": self.output,
            "cacheRead": self.cache_read,
            "cacheWrite": self.cache_write
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct ModelCatalog {
    providers: std::collections::BTreeMap<String, Vec<CatalogModel>>,
}

impl ModelCatalog {
    pub fn resolve(&self, provider_id: &str, model_id: &str) -> Option<&CatalogModel> {
        if let Some(model) = self
            .providers
            .get(provider_id)
            .and_then(|models| models.iter().find(|model| model.id == model_id))
        {
            return Some(model);
        }

        let mut matches = self
            .providers
            .values()
            .flatten()
            .filter(|model| model.id == model_id);
        let first = matches.next()?;
        matches
            .all(|model| model.config == first.config)
            .then_some(first)
    }

    pub fn ambiguous_candidates(&self, provider_id: &str, model_id: &str) -> Vec<CatalogCandidate> {
        if self.resolve(provider_id, model_id).is_some() {
            return Vec::new();
        }
        self.providers
            .iter()
            .flat_map(|(source_provider_id, models)| {
                models
                    .iter()
                    .filter(move |model| model.id == model_id)
                    .map(|model| CatalogCandidate {
                        provider_id: source_provider_id.clone(),
                        model: model.clone(),
                    })
            })
            .collect()
    }

    fn insert(&mut self, provider_id: String, models: Vec<CatalogModel>) {
        self.providers.insert(provider_id, models);
    }

    // thinkingLevelMap is a model capability, not a per-listing value. Each
    // provider listing exposes a different (often incomplete) reasoning_options;
    // the model's real thinking capability is the most detailed one. After
    // parsing, unify every listing of a model to the most detailed thinkingLevelMap
    // found across its siblings, so imports don't depend on which listing is chosen.
    fn enrich_reasoning(&mut self) {
        let mut best: std::collections::BTreeMap<String, (usize, Value)> =
            std::collections::BTreeMap::new();
        for models in self.providers.values() {
            for model in models {
                if model.config.get("reasoning").and_then(Value::as_bool) != Some(true) {
                    continue;
                }
                if let Some(map) = model.config.get("thinkingLevelMap") {
                    let score = map
                        .as_object()
                        .map(|object| object.values().filter(|value| value.is_string()).count())
                        .unwrap_or(0);
                    if best.get(&model.id).map(|(score, _)| *score).unwrap_or(0) < score {
                        best.insert(model.id.clone(), (score, map.clone()));
                    }
                }
            }
        }
        for models in self.providers.values_mut() {
            for model in models {
                if model.config.get("reasoning").and_then(Value::as_bool) != Some(true) {
                    continue;
                }
                if let Some((_, map)) = best.get(&model.id).cloned() {
                    if let Some(object) = model.config.as_object_mut() {
                        object.insert("thinkingLevelMap".into(), map);
                    }
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct CatalogCandidate {
    pub provider_id: String,
    pub model: CatalogModel,
}

#[derive(Clone, Debug)]
pub struct CatalogAmbiguity {
    pub provider_id: String,
    pub model_id: String,
    pub candidates: Vec<CatalogCandidate>,
}

#[derive(Clone, Debug)]
pub struct CatalogFetch {
    pub models: Vec<CatalogModel>,
    pub ambiguous: Vec<CatalogAmbiguity>,
    pub unavailable: usize,
    /// Per-model costs from `/api/ratio_config`, keyed by model id. Applied on
    /// top of catalog metadata so displayed and imported prices reflect the
    /// gateway's own ratios when available.
    pub ratio_prices: std::collections::BTreeMap<String, RatioCost>,
    pub ratio_config_used: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelImportSummary {
    pub added: usize,
    pub updated: usize,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModelDefaults {
    pub context_window: Option<u64>,
    pub max_tokens: Option<u64>,
    pub input_cost: Option<f64>,
    pub output_cost: Option<f64>,
    pub cache_read_cost: Option<f64>,
    pub cache_write_cost: Option<f64>,
}

impl ModelDefaults {
    pub fn model(&self, id: &str) -> CatalogModel {
        CatalogModel {
            id: id.into(),
            config: json!({
                "id": id,
                "contextWindow": self.context_window.unwrap_or(PI_DEFAULT_CONTEXT_WINDOW),
                "maxTokens": self.max_tokens.unwrap_or(PI_DEFAULT_MAX_TOKENS),
                "cost": {
                    "input": self.input_cost.unwrap_or(0.0),
                    "output": self.output_cost.unwrap_or(0.0),
                    "cacheRead": self.cache_read_cost.unwrap_or(0.0),
                    "cacheWrite": self.cache_write_cost.unwrap_or(0.0)
                }
            }),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ImportOptions {
    pub fetch_metadata: bool,
    pub defaults: ModelDefaults,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ImportSummary {
    pub providers: usize,
    pub models: usize,
    pub metadata: usize,
    pub defaults: usize,
    pub unresolved: usize,
    pub changed: bool,
}

#[derive(Debug)]
pub struct OpenCodeImportPlan {
    source: Value,
    provider_ids: Vec<String>,
    options: ImportOptions,
    catalog: Option<ModelCatalog>,
    pub ambiguous: Vec<CatalogAmbiguity>,
}

#[derive(Clone, Debug)]
pub struct Backup {
    pub path: String,
    pub name: String,
}

#[cfg_attr(not(test), napi_derive::napi(object))]
#[derive(Clone, Debug)]
pub struct DoctorCheck {
    pub ok: bool,
    pub label: String,
    pub detail: String,
}

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

fn validate_local_library(value: &Value) -> Result<()> {
    if value.get("version").and_then(Value::as_u64) != Some(1) {
        return Err(AppError::Invalid("providers.json version must be 1".into()));
    }
    validate_provider_document(value)
}

fn validate_provider_document(value: &Value) -> Result<()> {
    for (id, provider) in providers_object(value)? {
        provider_view(id, provider)?;
    }
    Ok(())
}

fn validate_settings_document(settings: &Value, models: &Value) -> Result<()> {
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

fn lock_provider_documents(paths: &Paths) -> Result<(WriteLock, Value, Value)> {
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

fn clear_default_for_provider(settings: &mut Value, paths: &Paths, id: &str) -> Result<bool> {
    if string_field(settings, "defaultProvider")?.as_deref() != Some(id) {
        return Ok(false);
    }
    let object = root_object_mut(settings, &paths.settings)?;
    object.remove("defaultProvider");
    object.remove("defaultModel");
    Ok(true)
}

fn write_provider_changes(
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

fn pi_switch_object(settings: &Value) -> Result<Option<&Map<String, Value>>> {
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

pub fn set_language(paths: &Paths, language: &str) -> Result<()> {
    if !matches!(language, "en" | "zh-CN") {
        return Err(AppError::Invalid(format!(
            "unsupported pi-switch language '{language}'"
        )));
    }
    update_pi_switch(paths, |settings| {
        settings.insert("language".into(), Value::String(language.into()));
    })
}

pub fn set_fetch_model_metadata(paths: &Paths, enabled: bool) -> Result<()> {
    update_pi_switch(paths, |settings| {
        settings.insert("fetchModelMetadata".into(), Value::Bool(enabled));
    })
}

pub fn set_model_defaults(paths: &Paths, defaults: &ModelDefaults) -> Result<()> {
    if defaults.context_window == Some(0) || defaults.max_tokens == Some(0) {
        return Err(AppError::Invalid(
            "default context window and max tokens must be positive".into(),
        ));
    }
    for (field, value) in [
        ("input cost", defaults.input_cost),
        ("output cost", defaults.output_cost),
        ("cache read cost", defaults.cache_read_cost),
        ("cache write cost", defaults.cache_write_cost),
    ] {
        if value.is_some_and(|value| !value.is_finite() || value < 0.0) {
            return Err(AppError::Invalid(format!(
                "default {field} must be a non-negative number"
            )));
        }
    }
    let mut value = Map::new();
    for (field, item) in [
        ("contextWindow", defaults.context_window.map(Value::from)),
        ("maxTokens", defaults.max_tokens.map(Value::from)),
        ("inputCost", defaults.input_cost.map(Value::from)),
        ("outputCost", defaults.output_cost.map(Value::from)),
        ("cacheReadCost", defaults.cache_read_cost.map(Value::from)),
        ("cacheWriteCost", defaults.cache_write_cost.map(Value::from)),
    ] {
        if let Some(item) = item {
            value.insert(field.into(), item);
        }
    }
    update_pi_switch(paths, |settings| {
        if value.is_empty() {
            settings.remove("modelDefaults");
        } else {
            settings.insert("modelDefaults".into(), Value::Object(value));
        }
    })
}

fn update_pi_switch(paths: &Paths, update: impl FnOnce(&mut Map<String, Value>)) -> Result<()> {
    let _lock = WriteLock::acquire(paths)?;
    let mut settings = read_document(&paths.settings, json!({}))?;
    let root = root_object_mut(&mut settings, &paths.settings)?;
    let pi_switch = root
        .entry("piSwitch")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| AppError::Invalid("settings piSwitch must be an object".into()))?;
    update(pi_switch);
    write_document(paths, &_lock, &paths.settings, &settings).map(|_| ())
}

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

pub fn doctor(paths: &Paths) -> Vec<DoctorCheck> {
    let mut checks = Vec::new();
    checks.push(check(
        paths.providers.exists(),
        "providers.json",
        paths.providers.display().to_string(),
    ));
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
            let enabled = snapshot
                .providers
                .iter()
                .filter(|provider| provider.in_pi)
                .count();
            checks.push(check(
                true,
                "Provider library",
                format!(
                    "{} provider(s), {enabled} added to Pi; JSON and projection are valid",
                    snapshot.providers.len()
                ),
            ));
            let default_ok = match (&snapshot.default_provider, &snapshot.default_model) {
                (None, None) => true,
                (Some(provider), Some(model)) => snapshot.providers.iter().any(|item| {
                    item.in_pi
                        && item.id == *provider
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
                    "defaultProvider/defaultModel is incomplete or references a provider not added to Pi"
                        .into()
                },
            ));
            for provider in snapshot.providers {
                let valid = validate_provider_view(&provider).is_ok();
                checks.push(check(
                    valid,
                    format!("Provider {}", provider.id),
                    validate_provider_view(&provider)
                        .map(|_| {
                            format!(
                                "{} model(s), {}, {}",
                                provider.models.len(),
                                provider.api,
                                if provider.in_pi {
                                    "added to Pi"
                                } else {
                                    "local only"
                                }
                            )
                        })
                        .unwrap_or_else(|error| error.to_string()),
                ));
            }
        }
        Err(error) => checks.push(check(false, "Provider documents", error.to_string())),
    }
    let (legacy, corrupt) = backup_diagnostics(paths);
    checks.push(check(
        legacy == 0,
        "Legacy backups",
        format!("{legacy} unsupported legacy backup(s)"),
    ));
    checks.push(check(
        corrupt == 0,
        "Corrupt provider archives",
        format!("{corrupt} archived provider file(s)"),
    ));
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

fn backup_diagnostics(paths: &Paths) -> (usize, usize) {
    let Ok(entries) = std::fs::read_dir(&paths.backups) else {
        return (0, 0);
    };
    let mut legacy = 0;
    let mut corrupt = 0;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with("corrupt-providers-") && name.ends_with(".json") {
            corrupt += 1;
        } else if name.starts_with("backup-") && name.ends_with(".json") {
            let version = std::fs::read(entry.path())
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                .and_then(|value| value.get("version").and_then(Value::as_u64));
            if version != Some(2) {
                legacy += 1;
            }
        }
    }
    (legacy, corrupt)
}

fn check(ok: bool, label: impl Into<String>, detail: impl Into<String>) -> DoctorCheck {
    DoctorCheck {
        ok,
        label: label.into(),
        detail: detail.into(),
    }
}

#[cfg(test)]
use network::{
    fetch_models_for_test, find_ratio, parse_models_dev_catalog, parse_pricing,
    parse_provider_catalog, parse_ratio_config, resolve_secret,
};
#[cfg(test)]
use opencode::{import_opencode_with_catalog, prepare_opencode_with_catalog};
#[cfg(test)]
use storage::now_millis;

#[cfg(test)]
mod tests;
