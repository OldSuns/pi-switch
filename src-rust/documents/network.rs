use std::{collections::BTreeSet, env, time::Duration};

use reqwest::{Client, Url};
use serde_json::Value;

use super::{
    schema::{model_view, validate_model_id, validate_provider_view, validate_url},
    AppError, CatalogFetch, CatalogModel, ImportOptions, ModelCatalog, ProviderView, Result,
};

const PI_CATALOG_URL: &str = "https://pi.dev/api/models";

pub async fn fetch_catalog() -> Result<ModelCatalog> {
    let client = http_client()?;
    fetch_catalog_from(&client, PI_CATALOG_URL).await
}

pub async fn fetch_models(provider: ProviderView, options: ImportOptions) -> Result<CatalogFetch> {
    fetch_models_from(provider, options, PI_CATALOG_URL).await
}

async fn fetch_models_from(
    provider: ProviderView,
    options: ImportOptions,
    pi_catalog_url: &str,
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
        .await
        .map_err(|error| AppError::Http(error.to_string()))?;
    let status = response.status();
    if !status.is_success() {
        return Err(AppError::Http(format!("HTTP {status}")));
    }
    let body: Value = response
        .json()
        .await
        .map_err(|error| AppError::Http(format!("invalid JSON response: {error}")))?;
    let ids = parse_provider_catalog(&provider.api, &body)?;
    if !options.fetch_metadata {
        return Ok(CatalogFetch {
            models: ids.iter().map(|id| options.defaults.model(id)).collect(),
            unavailable: 0,
        });
    }
    let catalog = fetch_catalog_from(&client, pi_catalog_url).await?;
    let models = ids
        .iter()
        .filter_map(|id| catalog.resolve(&provider.id, id).cloned())
        .collect::<Vec<_>>();
    let unavailable = ids.len() - models.len();
    if models.is_empty() {
        return Err(AppError::Http(format!(
            "pi.dev has no unambiguous metadata for provider '{}' model IDs",
            provider.id
        )));
    }
    Ok(CatalogFetch {
        models,
        unavailable,
    })
}

fn http_client() -> Result<Client> {
    Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent(concat!("pi-switch/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| AppError::Http(error.to_string()))
}

async fn fetch_catalog_from(client: &Client, url: &str) -> Result<ModelCatalog> {
    let response = client
        .get(url)
        .header("accept", "application/json")
        .send()
        .await
        .map_err(|error| AppError::Http(format!("pi.dev catalog: {error}")))?;
    let status = response.status();
    if !status.is_success() {
        return Err(AppError::Http(format!(
            "pi.dev catalog returned HTTP {status}"
        )));
    }
    let body: Value = response
        .json()
        .await
        .map_err(|error| AppError::Http(format!("invalid pi.dev catalog JSON: {error}")))?;
    parse_pi_catalog(&body)
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

pub(super) fn parse_pi_catalog(body: &Value) -> Result<ModelCatalog> {
    let providers = body
        .as_object()
        .ok_or_else(|| AppError::Http("pi.dev catalog must be an object".into()))?;
    let mut catalog = ModelCatalog::default();
    for (provider_id, value) in providers {
        let entries = value.as_object().ok_or_else(|| {
            AppError::Http(format!(
                "pi.dev provider '{provider_id}' catalog must be an object"
            ))
        })?;
        let models = entries
            .iter()
            .map(|(key, value)| {
                let model = catalog_model(value)?;
                if model.id != *key {
                    return Err(AppError::Http(format!(
                        "pi.dev provider '{provider_id}' model key '{key}' does not match ID '{}'",
                        model.id
                    )));
                }
                Ok(model)
            })
            .collect::<Result<Vec<_>>>()?;
        catalog.insert(provider_id.clone(), models);
    }
    if providers.is_empty() {
        return Err(AppError::Http(
            "pi.dev catalog contains no providers".into(),
        ));
    }
    Ok(catalog)
}

fn catalog_model(value: &Value) -> Result<CatalogModel> {
    let source = value
        .as_object()
        .ok_or_else(|| AppError::Http("pi.dev model entry must be an object".into()))?;
    let id = source
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::Http("pi.dev model entry must have a string ID".into()))?;
    validate_model_id(id)?;
    for field in ["contextWindow", "maxTokens"] {
        if source
            .get(field)
            .and_then(Value::as_u64)
            .filter(|value| *value > 0)
            .is_none()
        {
            return Err(AppError::Http(format!(
                "pi.dev model '{id}' {field} must be a positive integer"
            )));
        }
    }
    let cost = source
        .get("cost")
        .and_then(Value::as_object)
        .ok_or_else(|| AppError::Http(format!("pi.dev model '{id}' cost must be an object")))?;
    for field in ["input", "output", "cacheRead", "cacheWrite"] {
        if !cost
            .get(field)
            .and_then(Value::as_f64)
            .is_some_and(|value| value >= 0.0)
        {
            return Err(AppError::Http(format!(
                "pi.dev model '{id}' cost.{field} must be a non-negative number"
            )));
        }
    }

    let mut config = serde_json::Map::new();
    for field in [
        "id",
        "name",
        "reasoning",
        "input",
        "cost",
        "contextWindow",
        "maxTokens",
    ] {
        if let Some(value) = source.get(field) {
            config.insert(field.into(), value.clone());
        }
    }
    let config = Value::Object(config);
    model_view("pi.dev", 0, &config)?;
    Ok(CatalogModel {
        id: id.into(),
        config,
    })
}

#[cfg(test)]
pub(super) async fn fetch_models_for_test(
    provider: ProviderView,
    options: ImportOptions,
    catalog_url: &str,
) -> Result<CatalogFetch> {
    fetch_models_from(provider, options, catalog_url).await
}
