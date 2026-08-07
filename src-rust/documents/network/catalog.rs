use super::*;

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

pub(super) fn fetch_provider_ids_with(
    client: &Client,
    provider: &ProviderView,
) -> Result<Vec<String>> {
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
pub(super) fn resolve_ids_against_catalog(
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

pub(super) fn http_client() -> Result<Client> {
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
pub(in crate::documents) struct Ratios {
    pub(in crate::documents) model_ratio: BTreeMap<String, f64>,
    pub(in crate::documents) completion_ratio: BTreeMap<String, f64>,
    pub(in crate::documents) cache_ratio: BTreeMap<String, f64>,
    pub(in crate::documents) create_cache_ratio: BTreeMap<String, f64>,
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
pub(super) fn get_gateway_json(
    client: &Client,
    provider: &ProviderView,
    path: &str,
) -> Option<Value> {
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
pub(super) fn fetch_ratio_config(client: &Client, provider: &ProviderView) -> Option<Ratios> {
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
