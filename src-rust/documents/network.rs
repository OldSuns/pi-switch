use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    path::Path,
    time::Duration,
};

use reqwest::{blocking::Client, Url};
use semver::Version;
use serde_json::{json, Map, Value};

use super::{
    schema::{model_view, validate_model_id, validate_provider_view, validate_url},
    AppError, CatalogAmbiguity, CatalogFetch, CatalogModel, ImportOptions, ModelCatalog,
    ProviderView, RatioCost, Result,
};

const MODELS_DEV_CATALOG_URL: &str = "https://models.dev/api.json";
const QUOTA_PER_USD: f64 = 500_000.0;
const TOKENS_PER_COST: f64 = 1_000_000.0;
const COST_FACTOR: f64 = TOKENS_PER_COST / QUOTA_PER_USD;

/// npm registry endpoint for the `@oldsuns/pi-switch` `latest` version. The
/// scope is `%2F`-encoded so the whole package name sits in a single path
/// segment; `/latest` returns a lightweight manifest (just the dist-tag's
/// publish) instead of the full packument.
const NPM_LATEST_URL: &str = "https://registry.npmjs.org/@oldsuns%2Fpi-switch/latest";
const UPDATE_CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// Check npm for a newer published version of `@oldsuns/pi-switch`.
///
/// A result file at `cache_path` records `{ lastCheck, latest }`. While the
/// recorded `lastCheck` is within `UPDATE_CACHE_TTL` of now the cached `latest`
/// is reused without any network access. Once the TTL elapses the npm
/// registry is queried for the current `latest` dist-tag and the cache is
/// rewritten.
///
/// Returns `Some(latest)` only when the registry/cache `latest` is strictly
/// greater than the compiled-in `CARGO_PKG_VERSION`. Any error — unreachable
/// registry, malformed response, unreadable/writable cache — is swallowed and
/// `Ok(None)` is returned: update checks are best-effort and must never
/// disturb the UI.
pub fn check_npm_update(cache_path: &Path) -> Result<Option<String>> {
    let current = env!("CARGO_PKG_VERSION");
    let now = now_millis();

    // Reuse the cached `latest` while it is still fresh, avoiding a network
    // round-trip on every launch.
    if let Some(cached) = read_update_cache(cache_path) {
        if now.saturating_sub(cached.last_check) < UPDATE_CACHE_TTL.as_millis() {
            return Ok(newer_version(current, &cached.latest));
        }
    }

    let client = http_client()?;
    let latest = match fetch_npm_latest(&client) {
        Some(latest) => latest,
        None => return Ok(None),
    };
    // Persist the fresh result so subsequent launches within the TTL skip the
    // network call. A write failure must not surface as an error.
    let _ = write_update_cache(cache_path, now, &latest);
    Ok(newer_version(current, &latest))
}

/// Compare two version strings and return the `latest` when it is strictly
/// greater than `current`. Both must parse as semver; any parse failure yields
/// `None` (the npm `latest` tag is always plain semver, but defend against
/// unexpected metadata by failing safe rather than panicking).
pub fn newer_version(current: &str, latest: &str) -> Option<String> {
    let current = Version::parse(current).ok()?;
    let latest = Version::parse(latest).ok()?;
    (latest > current).then(|| latest.to_string())
}

/// Fetch the npm `latest` manifest and extract its `version` field. Returns
/// `None` on any transport, status, or parsing failure.
fn fetch_npm_latest(client: &Client) -> Option<String> {
    let response = client
        .get(NPM_LATEST_URL)
        .header("accept", "application/json")
        .send()
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let body: Value = response.json().ok()?;
    body.get("version")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

struct CachedUpdate {
    last_check: u128,
    latest: String,
    dismissed: Option<String>,
}

/// Read `{ lastCheck, latest, dismissed? }` from the cache file. Any error returns `None`.
fn read_update_cache(path: &Path) -> Option<CachedUpdate> {
    let bytes = std::fs::read(path).ok()?;
    let value: Value = serde_json::from_slice(&bytes).ok()?;
    let object = value.as_object()?;
    let last_check = object.get("lastCheck").and_then(Value::as_u64)? as u128;
    let latest = object.get("latest").and_then(Value::as_str)?.to_owned();
    let dismissed = object
        .get("dismissed")
        .and_then(Value::as_str)
        .map(str::to_owned);
    Some(CachedUpdate {
        last_check,
        latest,
        dismissed,
    })
}

/// Write `{ lastCheck, latest, dismissed? }` to the cache file, creating parent dirs.
fn write_update_cache(path: &Path, last_check: u128, latest: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let body = json!({ "lastCheck": last_check as u64, "latest": latest });
    std::fs::write(path, serde_json::to_vec(&body)?)
}

/// Write the cache with a `dismissed` field recording which version the user
/// skipped, so the auto-check popup doesn't reappear for that version.
fn write_update_cache_with_dismiss(
    path: &Path,
    last_check: u128,
    latest: &str,
    dismissed: &str,
) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let body = json!({
        "lastCheck": last_check as u64,
        "latest": latest,
        "dismissed": dismissed
    });
    std::fs::write(path, serde_json::to_vec(&body)?)
}

/// Read the version the user previously dismissed (skipped), if any.
pub fn read_dismissed_update(cache_path: &Path) -> Option<String> {
    read_update_cache(cache_path).and_then(|c| c.dismissed)
}

/// Record that the user dismissed the install prompt for `version`, so the
/// auto-check on subsequent launches shows only the banner without popping up
/// the confirmation dialog again for this version.
pub fn dismiss_update(cache_path: &Path, version: &str) {
    let existing = read_update_cache(cache_path);
    let last_check = existing
        .as_ref()
        .map(|c| c.last_check)
        .unwrap_or_else(now_millis);
    let latest = existing
        .as_ref()
        .map(|c| c.latest.clone())
        .unwrap_or_else(|| version.to_owned());
    let _ = write_update_cache_with_dismiss(cache_path, last_check, &latest, version);
}

/// Install the latest version of `@oldsuns/pi-switch` globally via npm.
/// Runs `npm install -g @oldsuns/pi-switch` and returns an error if the
/// command cannot be found or exits with a non-zero status.
pub fn install_update() -> Result<()> {
    let output = std::process::Command::new("npm")
        .args(["install", "-g", "@oldsuns/pi-switch"])
        .output()
        .map_err(|e| AppError::Http(format!("failed to run npm: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Http(format!("npm install failed: {stderr}")));
    }
    Ok(())
}

/// Milliseconds since the Unix epoch, matching the timestamp used for backups.
fn now_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

pub fn fetch_catalog() -> Result<ModelCatalog> {
    let client = http_client()?;
    fetch_catalog_from(&client, MODELS_DEV_CATALOG_URL)
}

/// No models.dev catalog is touched. Used to present the selection list before
/// metadata is resolved for the chosen models.
pub fn fetch_model_ids(provider: &ProviderView) -> Result<Vec<String>> {
    fetch_provider_ids(provider)
}

/// Phase 2 of the import flow: resolve models.dev metadata (and ratio_config
/// pricing) for an already-selected set of model IDs. Models without any
/// models.dev match fall back to `options.defaults` so the user's explicit
/// selection is never silently dropped.
pub fn resolve_metadata(
    provider: ProviderView,
    ids: Vec<String>,
    options: ImportOptions,
) -> Result<CatalogFetch> {
    let client = http_client()?;
    resolve_ids_against_catalog(&client, &provider, &ids, options, MODELS_DEV_CATALOG_URL)
}

/// Validates the provider and requests its `/models` endpoint, returning the
/// parsed model ID list. Shared by the one-shot and two-phase import paths.
fn fetch_provider_ids(provider: &ProviderView) -> Result<Vec<String>> {
    let client = http_client()?;
    fetch_provider_ids_with(&client, provider)
}

fn fetch_provider_ids_with(client: &Client, provider: &ProviderView) -> Result<Vec<String>> {
    validate_provider_view(provider)?;
    if provider.base_url.is_empty() || provider.api.is_empty() {
        return Err(AppError::Invalid(
            "fetching models requires provider baseUrl and api".into(),
        ));
    }
    let key = resolve_secret(&provider.api_key)?;
    let mut url = catalog_url(provider)?;
    if provider.api == "google-generative-ai" {
        if let Some(key) = key.as_deref() {
            url.query_pairs_mut().append_pair("key", key);
        }
    }

    let mut request = client.get(url);
    for (name, value) in provider_headers(provider)? {
        request = request.header(name, value);
    }
    if provider.auth_header {
        if let Some(key) = key.as_deref() {
            request = match provider.api.as_str() {
                "anthropic-messages" => request.header("x-api-key", key),
                "google-generative-ai" => request,
                _ => request.bearer_auth(key),
            };
        }
    }
    if provider.api == "anthropic-messages" {
        request = request.header("anthropic-version", "2023-06-01");
    }

    let response = request
        .send()
        .map_err(|error| AppError::Http(error.to_string()))?;
    let status = response.status();
    if !status.is_success() {
        return Err(AppError::Http(format!("HTTP {status}")));
    }
    let body: Value = response
        .json()
        .map_err(|error| AppError::Http(format!("invalid JSON response: {error}")))?;
    parse_provider_catalog(&provider.api, &body)
}

/// Resolves a set of model IDs against the models.dev catalog, applying
/// ratio_config pricing on top. IDs with no catalog match fall back to
/// `options.defaults`; IDs with multiple matches are collected as ambiguities
/// for the caller to resolve interactively.
fn resolve_ids_against_catalog(
    client: &Client,
    provider: &ProviderView,
    ids: &[String],
    options: ImportOptions,
    metadata_catalog_url: &str,
) -> Result<CatalogFetch> {
    // ratio_config — best-effort; failure or malformed payload → no ratio prices.
    let ratios = fetch_ratio_config(client, provider);
    let ratio_config_used = ratios.is_some();
    let ratio_prices = ratios
        .as_ref()
        .map(|ratios| compute_ratio_prices(ids, ratios))
        .unwrap_or_default();

    // models.dev catalog — best-effort; if unreachable, fall back to an empty
    // catalog so every selected model is imported with default metadata rather
    // than aborting the entire flow.
    let (catalog, catalog_unreachable) = match fetch_catalog_from(client, metadata_catalog_url) {
        Ok(catalog) => (catalog, false),
        Err(_) => (ModelCatalog::default(), true),
    };
    let mut models = Vec::new();
    let mut ambiguous = Vec::new();
    let mut unavailable = 0;
    for id in ids {
        if let Some(model) = catalog.resolve(&provider.id, id) {
            models.push(model.clone());
            continue;
        }
        let candidates = catalog.ambiguous_candidates(&provider.id, id);
        if candidates.is_empty() {
            // No models.dev metadata: fall back to defaults so an explicit
            // user selection is still imported rather than silently dropped.
            unavailable += 1;
            models.push(options.defaults.model(id));
        } else {
            ambiguous.push(CatalogAmbiguity {
                provider_id: provider.id.clone(),
                model_id: id.clone(),
                candidates,
            });
        }
    }
    if models.is_empty() && ambiguous.is_empty() {
        return Err(AppError::Http(format!(
            "models.dev has no usable metadata for provider '{}' model IDs",
            provider.id
        )));
    }
    Ok(CatalogFetch {
        models,
        ambiguous,
        unavailable,
        ratio_prices,
        ratio_config_used,
        catalog_unreachable,
    })
}

fn http_client() -> Result<Client> {
    Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent(concat!("pi-switch/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| AppError::Http(error.to_string()))
}

// ---------------------------------------------------------------------------
// NewAPI gateway pricing — best-effort price source. NewAPI gateways expose
// two endpoints at the gateway root (baseUrl with any trailing /v1 stripped):
//   /api/ratio_config — admin-only; maps of model_ratio/completion_ratio/
//     cache_ratio/create_cache_ratio. Returns 403 with a regular user key.
//   /api/pricing — works with regular user keys; same ratios as an array of
//     { model_name, model_ratio, completion_ratio, cache_ratio }.
// We try ratio_config first (it may include create_cache_ratio), then fall
// back to /api/pricing. Any failure is silent: the caller uses models.dev.
// ---------------------------------------------------------------------------

#[derive(Default)]
pub(super) struct Ratios {
    pub(super) model_ratio: BTreeMap<String, f64>,
    pub(super) completion_ratio: BTreeMap<String, f64>,
    pub(super) cache_ratio: BTreeMap<String, f64>,
    pub(super) create_cache_ratio: BTreeMap<String, f64>,
}

fn gateway_root_url(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    trimmed
        .strip_suffix("/v1")
        .map(str::to_owned)
        .unwrap_or_else(|| trimmed.to_owned())
}

/// GET a JSON endpoint at the gateway root with the provider's auth. Returns
/// None on any error (unreachable, non-2xx, malformed JSON) so callers fall
/// back silently.
fn get_gateway_json(client: &Client, provider: &ProviderView, path: &str) -> Option<Value> {
    let result = (|| -> Result<Option<Value>> {
        let root = gateway_root_url(&provider.base_url);
        let url = Url::parse(&format!("{root}{path}"))
            .map_err(|error| AppError::Http(format!("invalid gateway url: {error}")))?;
        let key = resolve_secret(&provider.api_key)?;
        let headers = provider_headers(provider)?;
        let mut request = client.get(url);
        for (name, value) in headers {
            request = request.header(name, value);
        }
        if provider.auth_header {
            if let Some(key) = key.as_deref() {
                request = match provider.api.as_str() {
                    "anthropic-messages" => request.header("x-api-key", key),
                    "google-generative-ai" => request,
                    _ => request.bearer_auth(key),
                };
            }
        }
        let response = request
            .send()
            .map_err(|error| AppError::Http(error.to_string()))?;
        if !response.status().is_success() {
            return Ok(None);
        }
        response
            .json::<Value>()
            .map(Some)
            .map_err(|error| AppError::Http(format!("invalid gateway JSON: {error}")))
    })();
    result.ok().flatten()
}

/// Fetch gateway pricing. Tries /api/ratio_config first (admin-only, may have
/// create_cache_ratio), then falls back to /api/pricing (regular user key).
/// Returns None only if both fail, so the caller falls back to models.dev.
fn fetch_ratio_config(client: &Client, provider: &ProviderView) -> Option<Ratios> {
    if let Some(body) = get_gateway_json(client, provider, "/api/ratio_config") {
        let ratios = parse_ratio_config(&body);
        if !ratios.model_ratio.is_empty() {
            return Some(ratios);
        }
    }
    if let Some(body) = get_gateway_json(client, provider, "/api/pricing") {
        let ratios = parse_pricing(&body);
        if !ratios.model_ratio.is_empty() {
            return Some(ratios);
        }
    }
    None
}

/// Parse a /api/ratio_config payload defensively. Returns empty maps on any
/// shape issue or an explicit `success: false`. The payload may be wrapped in
/// a `{ success, data }` envelope or be the ratio object directly.
pub(super) fn parse_ratio_config(body: &Value) -> Ratios {
    let Some(root) = body.as_object() else {
        return Ratios::default();
    };
    if root.get("success").and_then(Value::as_bool) == Some(false) {
        return Ratios::default();
    }
    let data = root.get("data").unwrap_or(body);
    Ratios {
        model_ratio: as_ratio_map(data.get("model_ratio")),
        completion_ratio: as_ratio_map(data.get("completion_ratio")),
        cache_ratio: as_ratio_map(data.get("cache_ratio")),
        create_cache_ratio: as_ratio_map(data.get("create_cache_ratio")),
    }
}

/// Parse a /api/pricing payload defensively. Same ratios as ratio_config but
/// as an array of per-model objects: `{ success, data: [{ model_name,
/// model_ratio, completion_ratio, cache_ratio }] }`. `create_cache_ratio` is
/// usually absent, so cache_write defaults to 0.
pub(super) fn parse_pricing(body: &Value) -> Ratios {
    let Some(root) = body.as_object() else {
        return Ratios::default();
    };
    if root.get("success").and_then(Value::as_bool) == Some(false) {
        return Ratios::default();
    }
    let Some(entries) = root.get("data").and_then(Value::as_array) else {
        return Ratios::default();
    };
    let mut ratios = Ratios::default();
    for entry in entries {
        let Some(name) = entry.get("model_name").and_then(Value::as_str) else {
            continue;
        };
        if let Some(value) = entry
            .get("model_ratio")
            .and_then(Value::as_f64)
            .filter(|v| v.is_finite())
        {
            ratios.model_ratio.insert(name.into(), value);
        }
        if let Some(value) = entry
            .get("completion_ratio")
            .and_then(Value::as_f64)
            .filter(|v| v.is_finite())
        {
            ratios.completion_ratio.insert(name.into(), value);
        }
        if let Some(value) = entry
            .get("cache_ratio")
            .and_then(Value::as_f64)
            .filter(|v| v.is_finite())
        {
            ratios.cache_ratio.insert(name.into(), value);
        }
        if let Some(value) = entry
            .get("create_cache_ratio")
            .and_then(Value::as_f64)
            .filter(|v| v.is_finite())
        {
            ratios.create_cache_ratio.insert(name.into(), value);
        }
    }
    ratios
}

fn as_ratio_map(value: Option<&Value>) -> BTreeMap<String, f64> {
    let Some(object) = value.and_then(Value::as_object) else {
        return BTreeMap::new();
    };
    object
        .iter()
        .filter_map(|(key, value)| {
            value
                .as_f64()
                .filter(|value| value.is_finite())
                .map(|value| (key.clone(), value))
        })
        .collect()
}

/// Resolve a model's ratio, tolerating version tags or casing differences:
/// exact match, then case-insensitive, then prefix match.
pub(super) fn find_ratio(model_id: &str, ratios: &BTreeMap<String, f64>) -> Option<f64> {
    if let Some(&value) = ratios.get(model_id) {
        return Some(value);
    }
    let lower = model_id.to_lowercase();
    for (key, &value) in ratios {
        if key.to_lowercase() == lower {
            return Some(value);
        }
    }
    for (key, &value) in ratios {
        if lower.starts_with(&key.to_lowercase()) {
            return Some(value);
        }
    }
    None
}

/// Build the per-model price map from NewAPI ratios. Only models present in
/// `model_ratio` get a ratio-derived price; the rest stay with their catalog
/// (models.dev or defaults) cost. `1 USD = 500,000 quota`; `cost = ratio × 2`
/// per 1M tokens.
fn compute_ratio_prices(ids: &[String], ratios: &Ratios) -> BTreeMap<String, RatioCost> {
    let mut prices = BTreeMap::new();
    for id in ids {
        let Some(model_rate) = find_ratio(id, &ratios.model_ratio) else {
            continue;
        };
        let completion_rate = find_ratio(id, &ratios.completion_ratio).unwrap_or(1.0);
        let cache_rate = find_ratio(id, &ratios.cache_ratio).unwrap_or(0.0);
        let create_cache_rate = find_ratio(id, &ratios.create_cache_ratio).unwrap_or(0.0);
        prices.insert(
            id.clone(),
            RatioCost {
                input: model_rate * COST_FACTOR,
                output: model_rate * completion_rate * COST_FACTOR,
                cache_read: model_rate * cache_rate * COST_FACTOR,
                cache_write: model_rate * create_cache_rate * COST_FACTOR,
            },
        );
    }
    prices
}

fn fetch_catalog_from(client: &Client, url: &str) -> Result<ModelCatalog> {
    let response = client
        .get(url)
        .header("accept", "application/json")
        .send()
        .map_err(|error| AppError::Http(format!("models.dev catalog: {error}")))?;
    let status = response.status();
    if !status.is_success() {
        return Err(AppError::Http(format!(
            "models.dev catalog returned HTTP {status}"
        )));
    }
    let body: Value = response
        .json()
        .map_err(|error| AppError::Http(format!("invalid models.dev catalog JSON: {error}")))?;
    parse_models_dev_catalog(&body)
}

pub(super) fn catalog_url(provider: &ProviderView) -> Result<Url> {
    let base = provider.base_url.trim_end_matches('/');
    let value = match provider.api.as_str() {
        "anthropic-messages" if base.ends_with("/v1") => format!("{base}/models"),
        "anthropic-messages" => format!("{base}/v1/models"),
        _ => format!("{base}/models"),
    };
    validate_url(&value)
}

pub(super) fn provider_headers(provider: &ProviderView) -> Result<Vec<(String, String)>> {
    let Some(headers) = provider.raw.get("headers").and_then(Value::as_object) else {
        return Ok(Vec::new());
    };
    headers
        .iter()
        .map(|(name, value)| {
            let raw = value
                .as_str()
                .ok_or_else(|| AppError::Invalid(format!("header '{name}' must be a string")))?;
            let resolved = resolve_secret(raw)?
                .ok_or_else(|| AppError::Invalid(format!("header '{name}' is empty")))?;
            Ok((name.clone(), resolved))
        })
        .collect()
}

pub(super) fn resolve_secret(value: &str) -> Result<Option<String>> {
    if value.is_empty() {
        return Ok(None);
    }
    if value.starts_with('!') {
        return Err(AppError::Invalid(
            "!command secret references are preserved but not executed".into(),
        ));
    }
    let chars = value.chars().collect::<Vec<_>>();
    let mut output = String::with_capacity(value.len());
    let mut index = 0;
    while index < chars.len() {
        if chars[index] != '$' {
            output.push(chars[index]);
            index += 1;
            continue;
        }
        match chars.get(index + 1) {
            Some('$') => {
                output.push('$');
                index += 2;
            }
            Some('!') => {
                output.push('!');
                index += 2;
            }
            Some('{') => {
                let end = (index + 2..chars.len())
                    .find(|candidate| chars[*candidate] == '}')
                    .ok_or_else(|| {
                        AppError::Invalid("unterminated environment reference".into())
                    })?;
                let name = chars[index + 2..end].iter().collect::<String>();
                output.push_str(&environment_value(&name)?);
                index = end + 1;
            }
            Some(next) if next.is_ascii_alphabetic() || *next == '_' => {
                let end = (index + 2..chars.len())
                    .find(|candidate| {
                        !chars[*candidate].is_ascii_alphanumeric() && chars[*candidate] != '_'
                    })
                    .unwrap_or(chars.len());
                let name = chars[index + 1..end].iter().collect::<String>();
                output.push_str(&environment_value(&name)?);
                index = end;
            }
            _ => {
                output.push('$');
                index += 1;
            }
        }
    }
    Ok(Some(output))
}

pub(super) fn environment_value(name: &str) -> Result<String> {
    if name.is_empty() {
        return Err(AppError::Invalid(
            "environment variable name is empty".into(),
        ));
    }
    env::var(name)
        .map_err(|_| AppError::Invalid(format!("environment variable '{name}' is not set")))
}

pub(super) fn parse_provider_catalog(api: &str, body: &Value) -> Result<Vec<String>> {
    let entries = if api == "google-generative-ai" {
        body.get("models")
    } else {
        body.get("data")
    }
    .and_then(Value::as_array)
    .ok_or_else(|| AppError::Http("response does not contain a model array".into()))?;
    let models = entries
        .iter()
        .filter_map(|entry| {
            let value = if api == "google-generative-ai" {
                entry.get("name")
            } else {
                entry.get("id")
            }?
            .as_str()?;
            Some(value.strip_prefix("models/").unwrap_or(value).to_owned())
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if models.is_empty() {
        return Err(AppError::Http("model array contains no IDs".into()));
    }
    Ok(models)
}

pub(super) fn parse_models_dev_catalog(body: &Value) -> Result<ModelCatalog> {
    let providers = body
        .as_object()
        .ok_or_else(|| AppError::Http("models.dev catalog must be an object".into()))?;
    let mut catalog = ModelCatalog::default();
    for (provider_id, value) in providers {
        let provider = value.as_object().ok_or_else(|| {
            AppError::Http(format!(
                "models.dev provider '{provider_id}' must be an object"
            ))
        })?;
        if provider.get("id").and_then(Value::as_str) != Some(provider_id) {
            return Err(AppError::Http(format!(
                "models.dev provider key '{provider_id}' does not match its ID"
            )));
        }
        let entries = provider
            .get("models")
            .and_then(Value::as_object)
            .ok_or_else(|| {
                AppError::Http(format!(
                    "models.dev provider '{provider_id}' models must be an object"
                ))
            })?;
        let models = entries
            .iter()
            .map(|(key, value)| {
                let source_id = value.get("id").and_then(Value::as_str).ok_or_else(|| {
                    AppError::Http("models.dev model entry must have a string ID".into())
                })?;
                if source_id != key {
                    return Err(AppError::Http(format!(
                        "models.dev provider '{provider_id}' model key '{key}' does not match ID '{source_id}'"
                    )));
                }
                let model = catalog_model(value)?;
                Ok(model)
            })
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect();
        catalog.insert(provider_id.clone(), models);
    }
    if providers.is_empty() {
        return Err(AppError::Http(
            "models.dev catalog contains no providers".into(),
        ));
    }
    catalog.enrich_reasoning();
    Ok(catalog)
}

fn catalog_model(value: &Value) -> Result<Option<CatalogModel>> {
    let source = value
        .as_object()
        .ok_or_else(|| AppError::Http("models.dev model entry must be an object".into()))?;
    let id = source
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::Http("models.dev model entry must have a string ID".into()))?;
    validate_model_id(id)?;
    let name = source
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::Http(format!("models.dev model '{id}' name must be a string")))?;
    let reasoning = source
        .get("reasoning")
        .and_then(Value::as_bool)
        .ok_or_else(|| {
            AppError::Http(format!(
                "models.dev model '{id}' reasoning must be a boolean"
            ))
        })?;

    let limit = match source.get("limit") {
        None | Some(Value::Null) => return Ok(None),
        Some(Value::Object(value)) => value,
        Some(_) => {
            return Err(AppError::Http(format!(
                "models.dev model '{id}' limit must be an object"
            )))
        }
    };
    let Some(context_window) = positive_u64(limit.get("context"), id, "limit.context")? else {
        return Ok(None);
    };
    let Some(max_tokens) = positive_u64(limit.get("output"), id, "limit.output")? else {
        return Ok(None);
    };

    let modalities = match source.get("modalities") {
        None | Some(Value::Null) => return Ok(None),
        Some(Value::Object(value)) => value,
        Some(_) => {
            return Err(AppError::Http(format!(
                "models.dev model '{id}' modalities must be an object"
            )))
        }
    };
    let inputs = modalities
        .get("input")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            AppError::Http(format!(
                "models.dev model '{id}' modalities.input must be an array"
            ))
        })?;
    let inputs = inputs
        .iter()
        .map(|value| {
            value.as_str().ok_or_else(|| {
                AppError::Http(format!(
                    "models.dev model '{id}' modalities.input must contain strings"
                ))
            })
        })
        .collect::<Result<Vec<_>>>()?;
    if !inputs.contains(&"text") {
        return Ok(None);
    }
    let input = if inputs.contains(&"image") {
        json!(["text", "image"])
    } else {
        json!(["text"])
    };

    let cost = match source.get("cost") {
        None | Some(Value::Null) => None,
        Some(Value::Object(value)) => Some(value),
        Some(_) => {
            return Err(AppError::Http(format!(
                "models.dev model '{id}' cost must be an object"
            )))
        }
    };
    let cost_value = |field| nonnegative_f64(cost.and_then(|value| value.get(field)), id, field);
    let mut config = json!({
        "id": id,
        "name": name,
        "reasoning": reasoning,
        "input": input,
        "cost": {
            "input": cost_value("input")?,
            "output": cost_value("output")?,
            "cacheRead": cost_value("cache_read")?,
            "cacheWrite": cost_value("cache_write")?
        },
        "contextWindow": context_window,
        "maxTokens": max_tokens
    });
    if let Some(thinking_level_map) = thinking_level_map(source, reasoning) {
        config["thinkingLevelMap"] = thinking_level_map;
    }
    model_view("models.dev", 0, &config)?;
    Ok(Some(CatalogModel {
        id: id.into(),
        config,
    }))
}

fn thinking_level_map(source: &Map<String, Value>, reasoning: bool) -> Option<Value> {
    if !reasoning {
        return None;
    }
    let options = source.get("reasoning_options")?.as_array()?;
    let effort_values = options.iter().find_map(|option| {
        let object = option.as_object()?;
        if object.get("type").and_then(Value::as_str) != Some("effort") {
            return None;
        }
        object.get("values")?.as_array()
    })?;
    let graded = ["minimal", "low", "medium", "high", "xhigh", "max"];
    let contains = |name: &str| {
        effort_values
            .iter()
            .any(|value| value.as_str() == Some(name))
    };
    if !graded.iter().any(|level| contains(level)) {
        return None;
    }
    let mut map = Map::new();
    if contains("none") {
        map.insert("off".into(), Value::String("none".into()));
    }
    for level in graded {
        map.insert(
            level.into(),
            if contains(level) {
                Value::String(level.into())
            } else {
                Value::Null
            },
        );
    }
    Some(Value::Object(map))
}

fn positive_u64(value: Option<&Value>, id: &str, field: &str) -> Result<Option<u64>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .map(|value| (value > 0).then_some(value))
            .ok_or_else(|| {
                AppError::Http(format!(
                    "models.dev model '{id}' {field} must be an integer"
                ))
            }),
    }
}

fn nonnegative_f64(value: Option<&Value>, id: &str, field: &str) -> Result<f64> {
    match value {
        None | Some(Value::Null) => Ok(0.0),
        Some(value) => value.as_f64().filter(|value| *value >= 0.0).ok_or_else(|| {
            AppError::Http(format!(
                "models.dev model '{id}' cost.{field} must be a non-negative number"
            ))
        }),
    }
}

#[cfg(test)]
pub(super) fn fetch_models_for_test(
    provider: ProviderView,
    options: ImportOptions,
    catalog_url: &str,
) -> Result<CatalogFetch> {
    let client = http_client()?;
    let ids = fetch_provider_ids_with(&client, &provider)?;
    if !options.fetch_metadata {
        let ratios = fetch_ratio_config(&client, &provider);
        let ratio_config_used = ratios.is_some();
        let ratio_prices = ratios
            .as_ref()
            .map(|ratios| compute_ratio_prices(&ids, ratios))
            .unwrap_or_default();
        return Ok(CatalogFetch {
            models: ids.iter().map(|id| options.defaults.model(id)).collect(),
            ambiguous: Vec::new(),
            unavailable: 0,
            ratio_prices,
            ratio_config_used,
            catalog_unreachable: false,
        });
    }
    resolve_ids_against_catalog(&client, &provider, &ids, options, catalog_url)
}
