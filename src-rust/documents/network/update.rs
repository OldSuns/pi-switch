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
    // Automatic TUI checks retain their best-effort contract. User-requested
    // Web checks use the strict entry point so failures stay distinguishable.
    check_npm_update_with(cache_path, UpdateCheck::Automatic, fetch_npm_latest).or(Ok(None))
}

pub(crate) fn check_npm_update_strict(cache_path: &Path) -> Result<Option<String>> {
    check_npm_update_with(cache_path, UpdateCheck::Manual, fetch_npm_latest)
}

#[derive(Clone, Copy)]
enum UpdateCheck {
    Automatic,
    Manual,
}

fn check_npm_update_with(
    cache_path: &Path,
    mode: UpdateCheck,
    fetch: impl FnOnce() -> Result<String>,
) -> Result<Option<String>> {
    let current = env!("CARGO_PKG_VERSION");
    let now = now_millis();

    // Reuse the cached `latest` while it is still fresh, avoiding a network
    // round-trip on every launch.
    if matches!(mode, UpdateCheck::Automatic) {
        if let Some(cached) = read_update_cache(cache_path) {
            if now.saturating_sub(cached.last_check) < UPDATE_CACHE_TTL.as_millis() {
                return strict_newer_version(current, &cached.latest);
            }
        }
    }

    let latest = fetch()?;
    let available = strict_newer_version(current, &latest)?;
    // Preserve a previous dismissal, so a manual check does not make the
    // skipped version pop up in the TUI again.
    let dismissed = read_update_cache(cache_path).and_then(|cached| cached.dismissed);
    let written = match dismissed.as_deref() {
        Some(dismissed) => write_update_cache_with_dismiss(cache_path, now, &latest, dismissed),
        None => write_update_cache(cache_path, now, &latest),
    };
    // An automatic check must still announce a fetched update if its cache is
    // unwritable. Manual checks report that persistence failure to the caller.
    if let (UpdateCheck::Manual, Err(source)) = (mode, written) {
        return Err(AppError::Io {
            path: cache_path.into(),
            source,
        });
    }
    Ok(available)
}

fn strict_newer_version(current: &str, latest: &str) -> Result<Option<String>> {
    let current = Version::parse(current)
        .map_err(|error| AppError::Http(format!("invalid installed version: {error}")))?;
    let latest = Version::parse(latest)
        .map_err(|error| AppError::Http(format!("invalid npm version: {error}")))?;
    Ok((latest > current).then(|| latest.to_string()))
}

/// Compare two version strings and return the `latest` when it is strictly
/// greater than `current`. Both must parse as semver; any parse failure yields
/// `None` (the npm `latest` tag is always plain semver, but defend against
/// unexpected metadata by failing safe rather than panicking).
#[cfg(test)]
pub fn newer_version(current: &str, latest: &str) -> Option<String> {
    strict_newer_version(current, latest).ok().flatten()
}

/// Fetch the npm `latest` manifest, preserving transport and manifest failures.
fn fetch_npm_latest() -> Result<String> {
    let response = http_client()?
        .get(NPM_LATEST_URL)
        .header("accept", "application/json")
        .send()
        .map_err(update_request_error)?;
    if !response.status().is_success() {
        return Err(AppError::Http(format!(
            "npm update check: HTTP {}",
            response.status()
        )));
    }
    let body: Value = response
        .json()
        .map_err(|error| AppError::Http(format!("invalid npm manifest: {error}")))?;
    body.get("version")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| AppError::Http("npm manifest is missing a string version".into()))
}

fn update_request_error(error: reqwest::Error) -> AppError {
    let mut message = format!("npm update check: {error}");
    let mut source = std::error::Error::source(&error);
    while let Some(cause) = source {
        message.push_str(": ");
        message.push_str(&cause.to_string());
        source = cause.source();
    }
    AppError::Http(message)
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

#[cfg(test)]
mod strict_tests {
    use super::*;

    #[test]
    fn manual_update_checks_surface_fetch_errors() {
        let error = check_npm_update_with(
            Path::new("unused-update-cache"),
            UpdateCheck::Manual,
            || Err(AppError::Http("registry unavailable".into())),
        )
        .unwrap_err();
        assert!(error.to_string().contains("registry unavailable"));
    }

    #[test]
    fn malformed_registry_versions_are_errors_for_manual_checks() {
        let error = check_npm_update_with(
            Path::new("unused-update-cache"),
            UpdateCheck::Manual,
            || Ok("not-a-version".into()),
        )
        .unwrap_err();
        assert!(error.to_string().contains("invalid npm version"));
    }

    #[test]
    fn manual_update_checks_bypass_fresh_cache() {
        let cache = std::env::temp_dir().join(format!(
            "pi-switch-web-update-test-{}-{}.json",
            std::process::id(),
            now_millis(),
        ));
        write_update_cache(&cache, now_millis(), "999.0.0").unwrap();
        let automatic = check_npm_update_with(&cache, UpdateCheck::Automatic, || {
            panic!("automatic check should use a fresh cache")
        })
        .unwrap();
        assert_eq!(automatic.as_deref(), Some("999.0.0"));
        let manual = check_npm_update_with(&cache, UpdateCheck::Manual, || {
            Err(AppError::Http("forced network error".into()))
        });
        std::fs::remove_file(cache).unwrap();
        assert!(manual
            .unwrap_err()
            .to_string()
            .contains("forced network error"));
    }

    #[test]
    fn manual_checks_preserve_a_dismissed_version() {
        let cache = std::env::temp_dir().join(format!(
            "pi-switch-web-update-dismiss-test-{}-{}.json",
            std::process::id(),
            now_millis(),
        ));
        write_update_cache_with_dismiss(&cache, now_millis(), "1.0.0", "1.0.0").unwrap();
        check_npm_update_with(&cache, UpdateCheck::Manual, || Ok("2.0.0".into())).unwrap();
        let preserved = read_dismissed_update(&cache);
        std::fs::remove_file(cache).unwrap();
        assert_eq!(preserved.as_deref(), Some("1.0.0"));
    }

    #[test]
    fn automatic_checks_keep_fetched_updates_when_cache_writes_fail() {
        let cache = std::env::temp_dir().join(format!(
            "pi-switch-web-unwritable-cache-{}-{}",
            std::process::id(),
            now_millis(),
        ));
        // A directory used as the cache file fails consistently without
        // changing OS permissions or relying on the test runner's privileges.
        std::fs::create_dir(&cache).unwrap();
        let automatic =
            check_npm_update_with(&cache, UpdateCheck::Automatic, || Ok("999.0.0".into()));
        let manual = check_npm_update_with(&cache, UpdateCheck::Manual, || Ok("999.0.0".into()));
        std::fs::remove_dir(cache).unwrap();
        assert_eq!(automatic.unwrap().as_deref(), Some("999.0.0"));
        assert!(matches!(manual, Err(AppError::Io { .. })));
    }
}
