use super::*;

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
    let mut command = if cfg!(target_os = "windows") {
        let mut cmd = std::process::Command::new("cmd");
        cmd.args(["/c", "npm", "install", "-g", "@oldsuns/pi-switch"]);
        cmd
    } else {
        let mut cmd = std::process::Command::new("npm");
        cmd.args(["install", "-g", "@oldsuns/pi-switch"]);
        cmd
    };
    let output = command
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
