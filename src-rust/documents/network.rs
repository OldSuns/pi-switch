use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    time::Duration,
};

use reqwest::{blocking::Client, Url};
use serde_json::{json, Value};

use super::{
    schema::{model_view, validate_model_id, validate_provider_view, validate_url},
    AppError, CatalogAmbiguity, CatalogFetch, CatalogModel, ImportOptions, ModelCatalog,
    ProviderView, RatioCost, Result,
};

const MODELS_DEV_CATALOG_URL: &str = "https://models.dev/api.json";
const QUOTA_PER_USD: f64 = 500_000.0;
const TOKENS_PER_COST: f64 = 1_000_000.0;
const COST_FACTOR: f64 = TOKENS_PER_COST / QUOTA_PER_USD;

pub fn fetch_catalog() -> Result<ModelCatalog> {
    let client = http_client()?;
    fetch_catalog_from(&client, MODELS_DEV_CATALOG_URL)
}

pub fn fetch_models(provider: ProviderView, options: ImportOptions) -> Result<CatalogFetch> {
    fetch_models_from(provider, options, MODELS_DEV_CATALOG_URL)
}

fn fetch_models_from(
    provider: ProviderView,
    options: ImportOptions,
    metadata_catalog_url: &str,
) -> Result<CatalogFetch> {
    validate_provider_view(&provider)?;
    if provider.base_url.is_empty() || provider.api.is_empty() {
        return Err(AppError::Invalid(
            "fetching models requires provider baseUrl and api".into(),
        ));
    }
    let key = resolve_secret(&provider.api_key)?;
    let mut url = catalog_url(&provider)?;
    if provider.api == "google-generative-ai" {
        if let Some(key) = key.as_deref() {
            url.query_pairs_mut().append_pair("key", key);
        }
    }

    let client = http_client()?;

    // ratio_config — best-effort; failure or malformed payload → no ratio prices.
    let ratios = fetch_ratio_config(&client, &provider);
    let ratio_config_used = ratios.is_some();

    let mut request = client.get(url);
    for (name, value) in provider_headers(&provider)? {
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
    let ids = parse_provider_catalog(&provider.api, &body)?;
    let ratio_prices = ratios
        .as_ref()
        .map(|ratios| compute_ratio_prices(&ids, ratios))
        .unwrap_or_default();
    if !options.fetch_metadata {
        return Ok(CatalogFetch {
            models: ids.iter().map(|id| options.defaults.model(id)).collect(),
            ambiguous: Vec::new(),
            unavailable: 0,
            ratio_prices,
            ratio_config_used,
        });
    }
    let catalog = fetch_catalog_from(&client, metadata_catalog_url)?;
    let mut models = Vec::new();
    let mut ambiguous = Vec::new();
    let mut unavailable = 0;
    for id in ids {
        if let Some(model) = catalog.resolve(&provider.id, &id) {
            models.push(model.clone());
            continue;
        }
        let candidates = catalog.ambiguous_candidates(&provider.id, &id);
        if candidates.is_empty() {
            unavailable += 1;
        } else {
            ambiguous.push(CatalogAmbiguity {
                provider_id: provider.id.clone(),
                model_id: id,
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
// NewAPI ratio_config — best-effort price source. NewAPI gateways expose
// /api/ratio_config at the gateway root (i.e. baseUrl with any trailing /v1
// stripped). Failure is silent: the caller falls back to models.dev prices.
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

/// Fetch /api/ratio_config from the gateway root. Any error (unreachable,
/// non-2xx, malformed JSON) returns None so the caller falls back silently.
fn fetch_ratio_config(client: &Client, provider: &ProviderView) -> Option<Ratios> {
    let result = (|| -> Result<Option<Ratios>> {
        let root = gateway_root_url(&provider.base_url);
        let url = Url::parse(&format!("{root}/api/ratio_config"))
            .map_err(|error| AppError::Http(format!("invalid ratio_config url: {error}")))?;
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
        let body: Value = response
            .json()
            .map_err(|error| AppError::Http(format!("invalid ratio_config JSON: {error}")))?;
        Ok(Some(parse_ratio_config(&body)))
    })();
    result.ok().flatten()
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
    let config = json!({
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
    model_view("models.dev", 0, &config)?;
    Ok(Some(CatalogModel {
        id: id.into(),
        config,
    }))
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
    fetch_models_from(provider, options, catalog_url)
}
