mod auth;
mod diagnostics;
mod network;
mod opencode;
mod ordering;
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

pub use auth::{credential_ids, credential_key, has_credential};
pub use diagnostics::doctor;
pub use network::check_npm_update;
pub(crate) use network::check_npm_update_strict;
pub use network::fetch_model_ids;
pub use network::resolve_metadata;
pub use network::{dismiss_update, install_update, read_dismissed_update};
pub use opencode::{apply_opencode_import, list_opencode_providers, prepare_opencode_import};
pub use ordering::{reorder_profiles, set_profile_sort, ProfileList, ProfileOrdering, ProfileSort};
pub use providers::{
    duplicate_model, duplicate_provider, import_models, remove_model, remove_provider, save_model,
    save_provider, save_provider_overwriting_credential, set_default, set_provider_in_pi,
};
pub use session_tree::{load_preview, PreviewMessage, PreviewTreePosition, SessionPreview};
pub use sessions::{
    delete_session, format_session_time, session_display_title, session_matches, DeleteMethod,
    SessionSummary,
};
pub(crate) use sessions::{delete_session_in, list_sessions_in, sessions_root};
#[cfg(test)]
use settings::check_updates_field;
pub use settings::{
    set_check_updates, set_fetch_model_metadata, set_key_storage, set_language, set_model_defaults,
    KeyStorage,
};
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
    #[error("auth.json already has a credential for '{0}'")]
    CredentialOverwriteRequired(String),
    #[error("another pi-switch process is writing configuration ({0})")]
    Busy(PathBuf),
    #[error("provider update completed, but settings update failed: {0}")]
    Partial(String),
    #[error("{0}")]
    Http(String),
}

#[derive(Clone, Debug)]
pub struct Paths {
    pub providers: PathBuf,
    pub pi_models: PathBuf,
    pub pi_settings: PathBuf,
    pub pi_auth: PathBuf,
    pub app_settings: PathBuf,
    pub opencode: PathBuf,
    pub backups: PathBuf,
    pub update: PathBuf,
    lock: PathBuf,
}

impl Paths {
    pub fn discover() -> Result<Self> {
        let home = dirs::home_dir()
            .ok_or_else(|| AppError::Invalid("home directory is unavailable".into()))?;
        let agent_dir = pi_agent_dir(&home);
        Ok(Self::from_roots(&home, &agent_dir))
    }

    #[cfg(test)]
    pub(crate) fn from_home(home: &Path) -> Self {
        Self::from_roots(home, &home.join(".pi/agent"))
    }

    pub(crate) fn from_roots(home: &Path, agent_dir: &Path) -> Self {
        Self {
            providers: home.join(".pi-switch/providers.json"),
            pi_models: agent_dir.join("models.json"),
            pi_settings: agent_dir.join("settings.json"),
            pi_auth: agent_dir.join("auth.json"),
            app_settings: home.join(".pi-switch/settings.json"),
            opencode: home.join(".config/opencode/opencode.json"),
            backups: home.join(".pi-switch/backups"),
            update: home.join(".pi-switch/update.json"),
            lock: home.join(".pi-switch/write.lock"),
        }
    }
}

pub(crate) fn pi_agent_dir(home: &Path) -> PathBuf {
    pi_agent_dir_from(home, std::env::var_os("PI_CODING_AGENT_DIR"))
}

pub(crate) fn pi_agent_dir_from(home: &Path, configured: Option<std::ffi::OsString>) -> PathBuf {
    configured
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".pi/agent"))
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub providers_path: String,
    pub pi_models_path: String,
    pub pi_settings_path: String,
    pub app_settings_path: String,
    pub providers: Vec<ProviderView>,
    pub ordering: ProfileOrdering,
    pub default_provider: Option<String>,
    pub default_model: Option<String>,
    pub language: String,
    pub fetch_model_metadata: bool,
    pub key_storage: KeyStorage,
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
    pub fn resolve(&self, provider_id: &str, model_id: &str) -> Option<CatalogModel> {
        match self.resolution(provider_id, model_id) {
            CatalogResolution::Resolved(model) => Some(rebind_model(&model, model_id)),
            CatalogResolution::Ambiguous(_) | CatalogResolution::Missing => None,
        }
    }

    pub fn ambiguous_candidates(&self, provider_id: &str, model_id: &str) -> Vec<CatalogCandidate> {
        match self.resolution(provider_id, model_id) {
            CatalogResolution::Ambiguous(candidates) => candidates,
            CatalogResolution::Resolved(_) | CatalogResolution::Missing => Vec::new(),
        }
    }

    pub fn selected_candidate(candidate: &CatalogCandidate, model_id: &str) -> CatalogModel {
        rebind_model(&candidate.model, model_id)
    }

    fn resolution(&self, provider_id: &str, model_id: &str) -> CatalogResolution {
        let strong_key = strong_model_key(model_id);
        let suffix_key = suffix_fallback_key(&strong_key);
        let compact_key = compact_model_id(&strong_key);
        let unknown_prefix_keys = unknown_prefix_keys(&strong_key);

        for candidates in [
            self.candidates(Some(provider_id), |id| id == model_id),
            self.candidates(None, |id| id == model_id),
            self.candidates(Some(provider_id), |id| strong_model_key(id) == strong_key),
            self.candidates(None, |id| strong_model_key(id) == strong_key),
        ] {
            match resolve_candidates(candidates) {
                CatalogResolution::Missing => {}
                resolution => return resolution,
            }
        }

        if let Some(suffix_key) = suffix_key {
            for candidates in [
                self.candidates(Some(provider_id), |id| strong_model_key(id) == suffix_key),
                self.candidates(None, |id| strong_model_key(id) == suffix_key),
            ] {
                match resolve_candidates(candidates) {
                    CatalogResolution::Missing => {}
                    resolution => return resolution,
                }
            }
        }

        for candidates in [
            self.candidates(Some(provider_id), |id| {
                compact_model_id(&strong_model_key(id)) == compact_key
            }),
            self.candidates(None, |id| {
                compact_model_id(&strong_model_key(id)) == compact_key
            }),
        ] {
            if candidates.len() == 1 {
                return CatalogResolution::Resolved(candidates[0].model.clone());
            }
        }

        for key in unknown_prefix_keys {
            for candidates in [
                self.candidates(Some(provider_id), |id| strong_model_key(id) == key),
                self.candidates(None, |id| strong_model_key(id) == key),
            ] {
                if candidates.len() == 1 {
                    return CatalogResolution::Resolved(candidates[0].model.clone());
                }
            }
        }
        CatalogResolution::Missing
    }

    fn candidates(
        &self,
        provider_id: Option<&str>,
        matches: impl Fn(&str) -> bool + Copy,
    ) -> Vec<CatalogCandidate> {
        self.providers
            .iter()
            .filter(|(source_provider_id, _)| {
                provider_id.is_none_or(|provider_id| source_provider_id.as_str() == provider_id)
            })
            .flat_map(|(source_provider_id, models)| {
                models
                    .iter()
                    .filter(move |model| matches(&model.id))
                    .map(move |model| CatalogCandidate {
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
enum CatalogResolution {
    Resolved(CatalogModel),
    Ambiguous(Vec<CatalogCandidate>),
    Missing,
}

fn resolve_candidates(candidates: Vec<CatalogCandidate>) -> CatalogResolution {
    let Some(first) = candidates.first() else {
        return CatalogResolution::Missing;
    };
    if candidates.len() == 1
        || candidates
            .iter()
            .skip(1)
            .all(|candidate| same_metadata(&candidate.model.config, &first.model.config))
    {
        CatalogResolution::Resolved(first.model.clone())
    } else {
        CatalogResolution::Ambiguous(candidates)
    }
}

fn same_metadata(left: &Value, right: &Value) -> bool {
    let mut left = left.clone();
    let mut right = right.clone();
    if let Some(object) = left.as_object_mut() {
        object.remove("id");
    }
    if let Some(object) = right.as_object_mut() {
        object.remove("id");
    }
    left == right
}

fn rebind_model(model: &CatalogModel, model_id: &str) -> CatalogModel {
    let mut model = model.clone();
    model.id = model_id.into();
    if let Some(config) = model.config.as_object_mut() {
        config.insert("id".into(), Value::String(model_id.into()));
    }
    model
}

fn suffix_fallback_key(model_id: &str) -> Option<String> {
    let mut fallback = model_id.to_owned();
    let mut stripped_any = false;
    while let Some(stripped) = strip_safe_suffix(&fallback) {
        fallback.truncate(stripped);
        stripped_any = true;
    }
    (stripped_any && !fallback.is_empty()).then_some(fallback)
}

fn unknown_prefix_keys(model_id: &str) -> Vec<String> {
    let segments = model_id.split('-').collect::<Vec<_>>();
    (1..=segments.len().saturating_sub(1).min(2))
        .filter_map(|count| {
            let stripped = segments[count..].join("-");
            (!stripped.is_empty()).then_some(stripped)
        })
        .collect()
}

fn strong_model_key(model_id: &str) -> String {
    let model_id = model_id
        .rsplit('/')
        .next()
        .unwrap_or(model_id)
        .to_lowercase();
    let model_id = ["z-ai-", "zai-org-", "zai-"]
        .iter()
        .find_map(|prefix| model_id.strip_prefix(prefix))
        .unwrap_or(&model_id);

    let mut canonical = String::with_capacity(model_id.len());
    let chars = model_id.chars().collect::<Vec<_>>();
    for (index, character) in chars.iter().copied().enumerate() {
        let previous_is_digit = index > 0 && chars[index - 1].is_ascii_digit();
        let next_is_digit = chars
            .get(index + 1)
            .is_some_and(|next| next.is_ascii_digit());
        let replacement = match character {
            '_' => Some('-'),
            '.' | ',' if previous_is_digit && next_is_digit => Some('-'),
            'p' if previous_is_digit && next_is_digit => Some('-'),
            _ => None,
        };
        if let Some(replacement) = replacement {
            if !canonical.ends_with(replacement) {
                canonical.push(replacement);
            }
        } else {
            canonical.push(character);
        }
    }

    canonical
}

fn strip_safe_suffix(model_id: &str) -> Option<usize> {
    for suffix in ["-fp8", "-int4", "-gguf"] {
        if let Some(stripped) = model_id.strip_suffix(suffix) {
            return Some(stripped.len());
        }
    }
    let (base, suffix) = model_id.rsplit_once('-')?;
    is_date_snapshot(suffix).then_some(base.len())
}

fn is_date_snapshot(value: &str) -> bool {
    if !matches!(value.len(), 4 | 8) || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return false;
    }
    if value.len() == 4 {
        let first = value[..2].parse::<u8>().unwrap_or(0);
        let second = value[2..].parse::<u8>().unwrap_or(0);
        return valid_month_day(first, second, None)
            || (1..=99).contains(&first) && (1..=12).contains(&second);
    }
    let year = value[..4].parse::<u16>().unwrap_or(0);
    let month = value[4..6].parse::<u8>().unwrap_or(0);
    let day = value[6..].parse::<u8>().unwrap_or(0);
    year > 0 && valid_month_day(month, day, Some(year))
}

fn valid_month_day(month: u8, day: u8, year: Option<u16>) -> bool {
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year.is_none_or(|year| year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)) => 29,
        2 => 28,
        _ => return false,
    };
    (1..=max_day).contains(&day)
}

fn compact_model_id(model_id: &str) -> String {
    model_id
        .chars()
        .filter(|character| character.is_alphanumeric())
        .collect()
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

impl CatalogFetch {
    /// Gateway ratio prices take precedence over the shared metadata catalog,
    /// including candidates that still need an explicit user selection.
    pub fn apply_ratio_prices(&mut self) {
        for model in &mut self.models {
            if let Some(cost) = self.ratio_prices.get(&model.id) {
                if let Some(object) = model.config.as_object_mut() {
                    object.insert("cost".into(), cost.to_cost_json());
                }
            }
        }
        for ambiguity in &mut self.ambiguous {
            if let Some(cost) = self.ratio_prices.get(&ambiguity.model_id) {
                for candidate in &mut ambiguity.candidates {
                    if let Some(object) = candidate.model.config.as_object_mut() {
                        object.insert("cost".into(), cost.to_cost_json());
                    }
                }
            }
        }
    }
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

#[derive(Clone, Debug)]
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
    resolve_metadata_with_timeout_for_test, resolve_secret, round_price, Ratios,
};
#[cfg(test)]
use opencode::{import_opencode_with_catalog, prepare_opencode_with_catalog};
#[cfg(test)]
use storage::now_millis;

#[cfg(test)]
mod tests;
