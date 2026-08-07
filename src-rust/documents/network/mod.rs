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
const NPM_LATEST_URL: &str = "https://registry.npmjs.org/@oldsuns%2Fpi-switch/latest";
const UPDATE_CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);

mod catalog;
mod pricing;
mod update;

pub(super) use catalog::fetch_catalog;
use catalog::http_client;
#[cfg(test)]
pub(super) use catalog::Ratios;
pub use catalog::{fetch_model_ids, resolve_metadata};
#[cfg(test)]
use catalog::{fetch_provider_ids_with, fetch_ratio_config, resolve_ids_against_catalog};
use pricing::{catalog_url, fetch_catalog_from, provider_headers};
pub(super) use pricing::{
    compute_ratio_prices, parse_pricing, parse_provider_catalog, parse_ratio_config,
    resolve_secret, round_price,
};
#[cfg(test)]
pub(super) use pricing::{fetch_models_for_test, find_ratio, parse_models_dev_catalog};
#[cfg(test)]
pub(super) use update::newer_version;
pub use update::{check_npm_update, dismiss_update, install_update, read_dismissed_update};
