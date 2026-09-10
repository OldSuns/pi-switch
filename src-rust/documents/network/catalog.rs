use std::{sync::mpsc, thread, time::Instant};

use super::*;

const METADATA_TIMEOUT: Duration = Duration::from_secs(10);

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
    resolve_metadata_with_timeout(
        provider,
        ids,
        options,
        MODELS_DEV_CATALOG_URL,
        METADATA_TIMEOUT,
    )
}

fn resolve_metadata_with_timeout(
    provider: ProviderView,
    ids: Vec<String>,
    options: ImportOptions,
    metadata_catalog_url: &str,
    timeout: Duration,
) -> Result<CatalogFetch> {
    let client = http_client()?;
    // Pricing requests carry the provider key, so they must not follow redirects.
    let pricing_client = credential_client()?;
    let deadline = Instant::now() + timeout;
    let (sender, receiver) = mpsc::channel();

    let catalog_client = client.clone();
    let catalog_url = metadata_catalog_url.to_owned();
    let catalog_sender = sender.clone();
    thread::spawn(move || {
        let result = fetch_catalog_from_until(&catalog_client, &catalog_url, deadline);
        let _ = catalog_sender.send(TimedMetadataResult {
            completed_at: Instant::now(),
            result: MetadataResult::Catalog(result),
        });
    });

    let pricing_provider = provider.clone();
    thread::spawn(move || {
        let ratios = fetch_ratio_config_until(&pricing_client, &pricing_provider, deadline);
        let _ = sender.send(TimedMetadataResult {
            completed_at: Instant::now(),
            result: MetadataResult::Ratios(ratios),
        });
    });

    let mut catalog = ModelCatalog::default();
    let mut catalog_unreachable = true;
    let mut ratios = None;
    let mut received = 0;
    while received < 2 {
        let Some(remaining) = deadline
            .checked_duration_since(Instant::now())
            .filter(|remaining| !remaining.is_zero())
        else {
            break;
        };
        match receiver.recv_timeout(remaining) {
            Ok(result) => {
                apply_metadata_result(
                    result,
                    deadline,
                    &mut catalog,
                    &mut catalog_unreachable,
                    &mut ratios,
                );
                received += 1;
            }
            Err(_) => break,
        }
    }
    while let Ok(result) = receiver.try_recv() {
        apply_metadata_result(
            result,
            deadline,
            &mut catalog,
            &mut catalog_unreachable,
            &mut ratios,
        );
    }

    build_catalog_fetch(
        &provider,
        &ids,
        options,
        catalog,
        catalog_unreachable,
        ratios,
    )
}

#[cfg(test)]
pub(in crate::documents) fn resolve_metadata_with_timeout_for_test(
    provider: ProviderView,
    ids: Vec<String>,
    options: ImportOptions,
    metadata_catalog_url: &str,
    timeout: Duration,
) -> Result<CatalogFetch> {
    resolve_metadata_with_timeout(provider, ids, options, metadata_catalog_url, timeout)
}

struct TimedMetadataResult {
    completed_at: Instant,
    result: MetadataResult,
}

enum MetadataResult {
    Catalog(Result<ModelCatalog>),
    Ratios(Option<Ratios>),
}

fn apply_metadata_result(
    timed: TimedMetadataResult,
    deadline: Instant,
    catalog: &mut ModelCatalog,
    catalog_unreachable: &mut bool,
    ratios: &mut Option<Ratios>,
) {
    if timed.completed_at > deadline {
        return;
    }
    match timed.result {
        MetadataResult::Catalog(Ok(fetched)) => {
            *catalog = fetched;
            *catalog_unreachable = false;
        }
        MetadataResult::Catalog(Err(_)) => {}
        MetadataResult::Ratios(fetched) => *ratios = fetched,
    }
}

/// Validates the provider and requests its `/models` endpoint, returning the
/// parsed model ID list. Shared by the one-shot and two-phase import paths.
fn fetch_provider_ids(provider: &ProviderView) -> Result<Vec<String>> {
    let client = credential_client()?;
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
    if key.is_some() {
        require_secure_transport(&url)?;
    }
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
        .map_err(|error| AppError::Http(error.without_url().to_string()))?;
    let status = response.status();
    if !status.is_success() {
        return Err(AppError::Http(format!("HTTP {status}")));
    }
    let body: Value = response
        .json()
        .map_err(|error| AppError::Http(format!("invalid JSON response: {error}")))?;
    parse_provider_catalog(&provider.api, &body)
}

fn build_catalog_fetch(
    provider: &ProviderView,
    ids: &[String],
    options: ImportOptions,
    catalog: ModelCatalog,
    catalog_unreachable: bool,
    ratios: Option<Ratios>,
) -> Result<CatalogFetch> {
    let ratio_config_used = ratios.is_some();
    let ratio_prices = ratios
        .as_ref()
        .map(|ratios| compute_ratio_prices(ids, ratios))
        .unwrap_or_default();
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

/// Client for requests that carry a provider credential: it must not follow
/// redirects, which would hand the key to whatever host the response names.
fn credential_client() -> Result<Client> {
    Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent(concat!("pi-switch/", env!("CARGO_PKG_VERSION")))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| AppError::Http(error.to_string()))
}

/// A provider credential may only be sent over HTTPS, or to the machine itself
/// or its local network — a self-hosted gateway is normally plain HTTP on a
/// LAN, but a key must never travel in clear text beyond it.
fn require_secure_transport(url: &Url) -> Result<()> {
    let host = url.host_str().unwrap_or_default();
    if url.scheme() == "https" || is_local_host(host) {
        return Ok(());
    }
    Err(AppError::Invalid(format!(
        "refusing to send a provider credential to {}://{}: use https or a local address",
        url.scheme(),
        url.host_str().unwrap_or_default()
    )))
}

fn is_local_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    match host.trim_start_matches('[').trim_end_matches(']').parse() {
        Ok(std::net::IpAddr::V4(ip)) => ip.is_loopback() || ip.is_private() || ip.is_link_local(),
        Ok(std::net::IpAddr::V6(ip)) => ip.is_loopback(),
        Err(_) => false,
    }
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
fn get_gateway_json_until(
    client: &Client,
    provider: &ProviderView,
    path: &str,
    deadline: Option<Instant>,
) -> Option<Value> {
    let result = (|| -> Result<Option<Value>> {
        let root = gateway_root_url(&provider.base_url);
        let url = Url::parse(&format!("{root}{path}"))
            .map_err(|error| AppError::Http(format!("invalid gateway url: {error}")))?;
        let key = resolve_secret(&provider.api_key)?;
        if key.is_some() {
            require_secure_transport(&url)?;
        }
        let headers = provider_headers(provider)?;
        let mut request = client.get(url);
        if let Some(deadline) = deadline {
            let timeout = deadline
                .checked_duration_since(Instant::now())
                .filter(|timeout| !timeout.is_zero())
                .ok_or_else(|| AppError::Http("gateway pricing timed out".into()))?;
            request = request.timeout(timeout);
        }
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
            .map_err(|error| AppError::Http(error.without_url().to_string()))?;
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
#[cfg(test)]
pub(super) fn fetch_ratio_config(client: &Client, provider: &ProviderView) -> Option<Ratios> {
    fetch_ratio_config_before(client, provider, None)
}

fn fetch_ratio_config_until(
    client: &Client,
    provider: &ProviderView,
    deadline: Instant,
) -> Option<Ratios> {
    fetch_ratio_config_before(client, provider, Some(deadline))
}

fn fetch_ratio_config_before(
    client: &Client,
    provider: &ProviderView,
    deadline: Option<Instant>,
) -> Option<Ratios> {
    if let Some(body) = get_gateway_json_until(client, provider, "/api/ratio_config", deadline) {
        let ratios = parse_ratio_config(&body);
        if !ratios.model_ratio.is_empty() {
            return Some(ratios);
        }
    }
    if let Some(body) = get_gateway_json_until(client, provider, "/api/pricing", deadline) {
        let ratios = parse_pricing(&body);
        if !ratios.model_ratio.is_empty() {
            return Some(ratios);
        }
    }
    None
}

#[cfg(test)]
mod transport_tests {
    use super::*;

    fn url(value: &str) -> Url {
        Url::parse(value).unwrap()
    }

    #[test]
    fn credentials_need_https_or_a_local_address() {
        for allowed in [
            "https://api.example.test/v1",
            "http://localhost:8080/v1",
            "http://127.0.0.1:8080/v1",
            "http://[::1]:8080/v1",
            "http://192.168.1.10:8080/v1",
            "http://10.0.0.5/v1",
        ] {
            assert!(require_secure_transport(&url(allowed)).is_ok(), "{allowed}");
        }
        for rejected in [
            "http://api.example.test/v1",
            "http://8.8.8.8/v1",
            "ftp://api.example.test/v1",
        ] {
            assert!(
                require_secure_transport(&url(rejected)).is_err(),
                "{rejected}"
            );
        }
        // A rejected URL is echoed as scheme://host only, so a key that sits in
        // its query or userinfo cannot reach an error message.
        let error = require_secure_transport(&url(
            "http://user:sk-secret@api.example.test/v1?key=sk-secret",
        ))
        .unwrap_err()
        .to_string();
        assert!(error.contains("http://api.example.test"), "{error}");
        assert!(!error.contains("sk-secret"), "{error}");
    }
}
