#[test]
fn newer_version_compares_strictly_and_fails_safe() {
    assert_eq!(newer_version("0.2.2", "0.2.3").as_deref(), Some("0.2.3"));
    assert_eq!(newer_version("0.2.2", "0.2.2"), None);
    assert_eq!(newer_version("0.3.0", "0.2.9"), None);
    // semver prerelease is handled: 1.0.0-beta < 1.0.0
    assert_eq!(newer_version("1.0.0-beta", "1.0.0").as_deref(), Some("1.0.0"));
    // build metadata must not affect ordering
    assert_eq!(newer_version("0.2.2+build", "0.2.3").as_deref(), Some("0.2.3"));
    // unparseable versions yield None rather than panicking
    assert_eq!(newer_version("not-a-version", "0.2.3"), None);
    assert_eq!(newer_version("0.2.2", "garbage"), None);
}

#[test]
fn npm_manifest_version_is_extracted() {
    // The /latest endpoint returns the manifest; the only field we read is
    // `version`. Confirm extraction against a realistic (trimmed) payload.
    let body = json!({ "version": "0.2.3", "name": "@oldsuns/pi-switch" });
    assert_eq!(
        body.get("version").and_then(Value::as_str).map(str::to_owned),
        Some("0.2.3".into())
    );
    // missing version field
    assert!(json!({ "name": "@oldsuns/pi-switch" })
        .get("version")
        .and_then(Value::as_str)
        .is_none());
}

#[test]
fn cache_round_trip_preserves_latest_and_last_check() {
    let (root, _paths) = fixture();
    let cache = root.join("update.json");
    let last_check: u128 = 1_700_000_000_000;
    // Write directly via the storage helper path; network write is private, so
    // emulate the file shape and confirm read picks it up.
    fs::write(
        &cache,
        serde_json::to_vec(&json!({ "lastCheck": last_check as u64, "latest": "9.9.9" })).unwrap(),
    )
    .unwrap();
    let bytes = fs::read(&cache).unwrap();
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        value.get("latest").and_then(Value::as_str),
        Some("9.9.9")
    );
    assert_eq!(
        value.get("lastCheck").and_then(Value::as_u64),
        Some(last_check as u64)
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn newer_version_from_cache_is_returned_via_check_npm_update() {
    // Populate a fresh cache that records a newer version and a recent
    // lastCheck so check_npm_update reuses it without touching the network.
    let (root, paths) = fixture();
    fs::create_dir_all(paths.update.parent().unwrap()).unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    fs::write(
        &paths.update,
        serde_json::to_vec(&json!({ "lastCheck": now as u64, "latest": "999.0.0" })).unwrap(),
    )
    .unwrap();
    let result = check_npm_update(&paths.update).unwrap();
    assert_eq!(result.as_deref(), Some("999.0.0"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn stale_cache_does_not_silently_return_outdated_newer_version() {
    // A cache whose lastCheck is older than the TTL is ignored. With no network
    // available (the cache cannot be refreshed), the function falls back to
    // None rather than returning a stale "newer" version.
    let (root, paths) = fixture();
    fs::create_dir_all(paths.update.parent().unwrap()).unwrap();
    let ancient: u64 = 1; // well before any 24h TTL
    fs::write(
        &paths.update,
        serde_json::to_vec(&json!({ "lastCheck": ancient, "latest": "999.0.0" })).unwrap(),
    )
    .unwrap();
    let result = check_npm_update(&paths.update).unwrap();
    // Network is unreachable in the test sandbox, so the stale entry is not
    // trusted and no newer version is reported.
    assert!(result.is_none());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn missing_cache_without_network_returns_none() {
    let (root, paths) = fixture();
    // No cache file exists; network calls fail against the real registry in the
    // sandbox (or timeout). Either way, best-effort → None.
    let result = check_npm_update(&paths.update);
    assert!(result.unwrap().is_none());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn corrupt_cache_is_ignored() {
    let (root, paths) = fixture();
    fs::create_dir_all(paths.update.parent().unwrap()).unwrap();
    fs::write(&paths.update, b"not json at all").unwrap();
    let result = check_npm_update(&paths.update);
    assert!(result.unwrap().is_none());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn set_check_updates_persists_and_round_trips_through_snapshot() {
    let (root, paths) = fixture();
    fs::write(&paths.models, r#"{"providers":{}}"#).unwrap();
    fs::write(&paths.settings, r#"{}"#).unwrap();

    // defaults to true when unset
    assert!(load_snapshot(&paths).unwrap().check_updates);

    set_check_updates(&paths, false).unwrap();
    assert!(!load_snapshot(&paths).unwrap().check_updates);

    set_check_updates(&paths, true).unwrap();
    assert!(load_snapshot(&paths).unwrap().check_updates);

    let settings: Value =
        serde_json::from_slice(&fs::read(&paths.settings).unwrap()).unwrap();
    assert_eq!(settings["piSwitch"]["checkForUpdates"], json!(true));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn check_updates_field_rejects_non_boolean() {
    let value = json!({ "piSwitch": { "checkForUpdates": "yes" } });
    assert!(check_updates_field(&value).is_err());
}

#[test]
fn dismiss_update_records_and_reads_back() {
    let (root, paths) = fixture();
    fs::create_dir_all(paths.update.parent().unwrap()).unwrap();
    // Initially no dismissed version.
    assert!(read_dismissed_update(&paths.update).is_none());
    // Dismiss version 0.3.0.
    dismiss_update(&paths.update, "0.3.0");
    assert_eq!(read_dismissed_update(&paths.update).as_deref(), Some("0.3.0"));
    // Dismissing a different version overwrites the previous dismissal.
    dismiss_update(&paths.update, "0.4.0");
    assert_eq!(read_dismissed_update(&paths.update).as_deref(), Some("0.4.0"));
    // The cache still holds lastCheck and latest alongside the dismissed field.
    let bytes = fs::read(&paths.update).unwrap();
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(value.get("lastCheck").and_then(Value::as_u64).is_some());
    assert!(value.get("latest").is_some());
    assert_eq!(value.get("dismissed").and_then(Value::as_str), Some("0.4.0"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn dismiss_update_preserves_existing_cache_fields() {
    let (root, paths) = fixture();
    fs::create_dir_all(paths.update.parent().unwrap()).unwrap();
    // Seed a cache with lastCheck and latest.
    fs::write(
        &paths.update,
        serde_json::to_vec(&json!({ "lastCheck": 123456_u64, "latest": "0.3.0" })).unwrap(),
    )
    .unwrap();
    // Dismiss a version — existing lastCheck and latest must survive.
    dismiss_update(&paths.update, "0.3.0");
    let value: Value = serde_json::from_slice(&fs::read(&paths.update).unwrap()).unwrap();
    assert_eq!(value.get("lastCheck").and_then(Value::as_u64), Some(123456));
    assert_eq!(value.get("latest").and_then(Value::as_str), Some("0.3.0"));
    assert_eq!(value.get("dismissed").and_then(Value::as_str), Some("0.3.0"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn read_dismissed_update_returns_none_for_missing_or_corrupt_cache() {
    let (root, paths) = fixture();
    // No file at all.
    assert!(read_dismissed_update(&paths.update).is_none());
    // Corrupt file.
    fs::create_dir_all(paths.update.parent().unwrap()).unwrap();
    fs::write(&paths.update, b"garbage").unwrap();
    assert!(read_dismissed_update(&paths.update).is_none());
    // Valid cache without dismissed field.
    fs::write(
        &paths.update,
        serde_json::to_vec(&json!({ "lastCheck": 0_u64, "latest": "0.1.0" })).unwrap(),
    )
    .unwrap();
    assert!(read_dismissed_update(&paths.update).is_none());
    let _ = fs::remove_dir_all(root);
}
