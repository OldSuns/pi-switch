mod diagnostics;
mod network;
mod opencode;
mod providers;
mod schema;
mod session_tree;
mod sessions;
mod settings;
mod snapshot;
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

pub use diagnostics::doctor;
pub use network::check_npm_update;
pub use network::fetch_model_ids;
pub use network::resolve_metadata;
pub use network::{dismiss_update, install_update, read_dismissed_update};
pub use opencode::{apply_opencode_import, list_opencode_providers, prepare_opencode_import};
pub use providers::{
    duplicate_provider, import_models, remove_model, remove_provider, save_model, save_provider,
    set_default, set_provider_in_pi,
};
pub use session_tree::{load_preview, PreviewMessage, PreviewTreePosition, SessionPreview};
pub use sessions::{
    delete_session, format_session_time, list_sessions, session_display_title, session_matches,
    DeleteMethod, SessionSummary,
};
#[cfg(test)]
use settings::check_updates_field;
pub use settings::{set_check_updates, set_fetch_model_metadata, set_language, set_model_defaults};
pub use snapshot::load_snapshot;
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
    pub update: PathBuf,
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
            update: home.join(".pi-switch/update.json"),
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
    pub check_updates: bool,
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
    pub input_cost: Option<f64>,
    pub output_cost: Option<f64>,
    pub cache_read_cost: Option<f64>,
    pub cache_write_cost: Option<f64>,
    pub thinking_level_map: Option<Map<String, Value>>,
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
    pub context_window: Option<u64>,
    pub max_tokens: Option<u64>,
    pub input_cost: Option<f64>,
    pub output_cost: Option<f64>,
    pub cache_read_cost: Option<f64>,
    pub cache_write_cost: Option<f64>,
    /// `Some(map)` replaces the model's thinkingLevelMap; `None` removes the
    /// key entirely. Unlike cost, an all-empty map has no "keep" reading, so
    /// None always means removal.
    pub thinking_level_map: Option<Map<String, Value>>,
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
        // Defensive rounding: even if a RatioCost is constructed from a source
        // that bypasses compute_ratio_prices, the serialized value must be free
        // of IEEE 754 artifacts before it lands in a config file.
        json!({
            "input": network::round_price(self.input),
            "output": network::round_price(self.output),
            "cacheRead": network::round_price(self.cache_read),
            "cacheWrite": network::round_price(self.cache_write)
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
    #[allow(dead_code)]
    pub ratio_config_used: bool,
    /// true when the models.dev catalog could not be fetched at all (network
    /// error, non-2xx, malformed JSON). All models fell back to defaults.
    pub catalog_unreachable: bool,
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

#[cfg(test)]
use network::{
    compute_ratio_prices, fetch_models_for_test, find_ratio, newer_version,
    parse_models_dev_catalog, parse_pricing, parse_provider_catalog, parse_ratio_config,
    resolve_secret, round_price, Ratios,
};
#[cfg(test)]
use opencode::{import_opencode_with_catalog, prepare_opencode_with_catalog};
#[cfg(test)]
use storage::now_millis;

#[cfg(test)]
mod tests;
