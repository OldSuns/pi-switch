use std::{collections::BTreeSet, env, time::Duration};

use reqwest::{Client, Url};
use serde_json::Value;

use super::{
    schema::{validate_provider_view, validate_url},
    AppError, ProviderView, Result,
};

pub async fn fetch_models(provider: ProviderView) -> Result<Vec<String>> {
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

    let client = Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|error| AppError::Http(error.to_string()))?;
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
    parse_catalog(&provider.api, &body)
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

pub(super) fn parse_catalog(api: &str, body: &Value) -> Result<Vec<String>> {
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
