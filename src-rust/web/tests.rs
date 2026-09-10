use std::{
    env, fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process,
    sync::atomic::{AtomicUsize, Ordering},
    thread,
    time::{Duration, Instant},
};

use serde_json::{json, Value};

use crate::documents::{
    CatalogAmbiguity, CatalogCandidate, CatalogFetch, ModelDefaults, RatioCost,
};

use super::{
    imports::{FetchedModels, PreparedModelImport},
    *,
};

static FIXTURE_ID: AtomicUsize = AtomicUsize::new(0);

struct Fixture {
    root: PathBuf,
    core: WebCore,
}

impl Fixture {
    fn new() -> Self {
        let root = env::temp_dir().join(format!(
            "pi-switch-web-test-{}-{}",
            process::id(),
            FIXTURE_ID.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&root).unwrap();
        fs::create_dir_all(root.join(".pi/agent/sessions")).unwrap();
        let paths = Paths::from_home(&root);
        let sessions_root = root.join(".pi/agent/sessions");
        Self {
            root,
            core: WebCore::new(paths, sessions_root),
        }
    }

    fn call(&mut self, request: Value) -> Value {
        self.core.dispatch(&request).unwrap()
    }

    fn create_provider(&mut self, id: &str, in_pi: bool) {
        self.call(json!({ "action": "provider.save", "draft": provider_draft(id, in_pi) }));
    }

    fn create_model(&mut self, provider_id: &str, id: &str) {
        self.call(
            json!({ "action": "model.save", "providerId": provider_id, "draft": model_draft(id) }),
        );
    }

    fn seed_candidates(&mut self, provider_id: &str, ids: &[&str]) {
        let snapshot = documents::load_snapshot(&self.core.paths).unwrap();
        let provider = snapshot
            .providers
            .iter()
            .find(|provider| provider.id == provider_id)
            .unwrap();
        self.core.fetched_models.insert(
            provider_id.into(),
            FetchedModels {
                provider: provider.raw.clone(),
                ids: ids.iter().map(|id| (*id).into()).collect(),
                prepared: None,
            },
        );
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let root = self.root.canonicalize().unwrap();
        let temporary_root = env::temp_dir().canonicalize().unwrap();
        assert!(root.starts_with(&temporary_root) && root != temporary_root);
        assert!(root
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("pi-switch-web-test-"));
        fs::remove_dir_all(root).unwrap();
    }
}

fn provider_draft(id: &str, in_pi: bool) -> Value {
    json!({
        "id": id,
        "inPi": in_pi,
        "baseUrl": "https://example.test/v1",
        "api": "openai-completions",
        "apiKey": "${TEST_API_KEY}",
        "authHeader": true,
        "headers": null,
        "compat": null,
    })
}

fn model_draft(id: &str) -> Value {
    json!({
        "id": id, "name": null, "api": null, "reasoning": false, "input": ["text"],
        "contextWindow": 128_000, "maxTokens": 16_384,
        "inputCost": null, "outputCost": null, "cacheReadCost": null, "cacheWriteCost": null,
        "thinkingLevelMap": null,
    })
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn write_json(path: &Path, value: &Value) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, value.to_string()).unwrap();
}

#[test]
fn provider_sync_and_default_follow_document_invariants() {
    let mut fixture = Fixture::new();
    fixture.create_provider("local", false);
    fixture.create_model("local", "chat");
    let set_default =
        json!({ "action": "model.default", "providerId": "local", "modelId": "chat" });
    assert!(fixture
        .core
        .dispatch(&set_default)
        .unwrap_err()
        .to_string()
        .contains("not added to Pi"));

    fixture.call(json!({ "action": "provider.sync", "providerId": "local", "inPi": true }));
    let selected = fixture.call(set_default);
    assert_eq!(selected["snapshot"]["defaultProvider"], "local");
    assert_eq!(selected["snapshot"]["defaultModel"], "chat");

    let renamed = fixture.call(json!({
        "action": "provider.save", "previousId": "local", "draft": provider_draft("renamed", true),
    }));
    assert_eq!(renamed["snapshot"]["defaultProvider"], "renamed");
    assert!(read_json(&fixture.core.paths.pi_models)["providers"]
        .get("local")
        .is_none());

    let disabled =
        fixture.call(json!({ "action": "provider.sync", "providerId": "renamed", "inPi": false }));
    assert!(disabled["snapshot"]["defaultProvider"].is_null());
    assert!(disabled["snapshot"]["defaultModel"].is_null());
    assert_eq!(
        disabled["snapshot"]["providers"][0]["models"][0]["id"],
        "chat"
    );
    assert!(read_json(&fixture.core.paths.pi_models)["providers"]
        .get("renamed")
        .is_none());
    assert!(read_json(&fixture.core.paths.providers)["providers"]
        .get("renamed")
        .is_some());
}

#[test]
fn model_rename_and_removal_update_the_default_without_losing_other_settings() {
    let mut fixture = Fixture::new();
    fixture.create_provider("local", true);
    fixture.create_model("local", "chat");
    write_json(
        &fixture.core.paths.pi_settings,
        &json!({ "theme": "mocha" }),
    );
    fixture.call(json!({ "action": "model.default", "providerId": "local", "modelId": "chat" }));
    let renamed = fixture.call(json!({
        "action": "model.save", "providerId": "local", "previousId": "chat", "draft": model_draft("chat-v2"),
    }));
    assert_eq!(renamed["snapshot"]["defaultModel"], "chat-v2");
    let removed = fixture
        .call(json!({ "action": "model.remove", "providerId": "local", "modelId": "chat-v2" }));
    assert!(removed["snapshot"]["defaultModel"].is_null());
    assert_eq!(
        read_json(&fixture.core.paths.pi_settings),
        json!({ "theme": "mocha" })
    );
}

#[test]
fn provider_and_model_edits_preserve_unknown_fields_and_sync_the_same_value() {
    let mut fixture = Fixture::new();
    write_json(
        &fixture.core.paths.pi_models,
        &json!({
            "futureRoot": true,
            "providers": { "local": {
                "api": "openai-completions", "baseUrl": "https://example.test/v1",
                "futureProvider": { "keep": 7 },
                "headers": { "x-custom": "keep" }, "compat": { "futureCompat": "keep" },
                "models": [{ "id": "chat", "futureModel": [1, 2], "cost": { "input": 1, "futureCost": 9 } }],
            }},
        }),
    );
    let mut draft = provider_draft("local", true);
    draft["baseUrl"] = json!("https://updated.test/v1");
    draft["headers"] = json!({ "x-custom": "keep" });
    draft["compat"] = json!({ "futureCompat": "keep" });
    fixture.call(json!({ "action": "provider.save", "previousId": "local", "draft": draft }));
    let mut model = model_draft("chat");
    model["inputCost"] = json!(2.5);
    fixture.call(json!({ "action": "model.save", "providerId": "local", "previousId": "chat", "draft": model }));
    let pi_models = read_json(&fixture.core.paths.pi_models);
    let library = read_json(&fixture.core.paths.providers);
    let provider = &pi_models["providers"]["local"];
    assert_eq!(*provider, library["providers"]["local"]);
    assert_eq!(pi_models["futureRoot"], true);
    assert_eq!(provider["futureProvider"]["keep"], 7);
    assert_eq!(provider["compat"]["futureCompat"], "keep");
    assert_eq!(provider["models"][0]["futureModel"], json!([1, 2]));
    assert_eq!(provider["models"][0]["cost"]["futureCost"], 9);
    assert_eq!(provider["models"][0]["cost"]["input"], 2.5);

    let duplicate = fixture.call(json!({ "action": "provider.duplicate", "providerId": "local" }));
    assert_eq!(duplicate["providerId"], "local-copy");
    assert_eq!(
        read_json(&fixture.core.paths.pi_models)["providers"]["local-copy"],
        *provider
    );
}

#[test]
fn model_duplication_preserves_extensions_source_default_and_sync_state() {
    for in_pi in [false, true] {
        let mut fixture = Fixture::new();
        fixture.create_provider("local", in_pi);
        let source = json!({
            "id": "chat", "name": "Original",
            "cost": { "input": 1, "output": 3, "futureCost": { "tier": [1, 2] } },
            "headers": { "X-Model": "keep" },
            "compat": { "futureOption": true },
            "futureModel": { "nested": ["keep", { "value": 7 }] },
        });
        let mut library = read_json(&fixture.core.paths.providers);
        library["providers"]["local"]["models"] = json!([source]);
        write_json(&fixture.core.paths.providers, &library);
        if in_pi {
            write_json(
                &fixture.core.paths.pi_models,
                &json!({ "providers": { "local": library["providers"]["local"] } }),
            );
        }
        write_json(
            &fixture.core.paths.pi_settings,
            &json!({ "theme": "mocha" }),
        );
        if in_pi {
            fixture.call(json!({
                "action": "model.default", "providerId": "local", "modelId": "chat",
            }));
        }
        let settings_before = fs::read(&fixture.core.paths.pi_settings).unwrap();
        let backups_before = documents::list_backups(&fixture.core.paths).unwrap().len();
        let mut draft = model_draft("chat-copy");
        draft["name"] = json!("Edited copy");
        draft["inputCost"] = json!(2.5);
        let result = fixture.call(json!({
            "action": "model.duplicate", "providerId": "local",
            "sourceModelId": "chat", "draft": draft,
        }));
        let library = read_json(&fixture.core.paths.providers);
        let provider = &library["providers"]["local"];
        assert_eq!(provider["models"].as_array().unwrap().len(), 2);
        assert_eq!(provider["models"][0], source);
        let copied = &provider["models"][1];
        assert_eq!(copied["id"], "chat-copy");
        assert_eq!(copied["name"], "Edited copy");
        assert_eq!(copied["contextWindow"], 128_000);
        assert_eq!(copied["cost"]["input"], 2.5);
        assert_eq!(copied["cost"]["output"], source["cost"]["output"]);
        assert_eq!(copied["cost"]["futureCost"], source["cost"]["futureCost"]);
        for field in ["headers", "compat", "futureModel"] {
            assert_eq!(copied[field], source[field]);
        }
        let pi_models = read_json(&fixture.core.paths.pi_models);
        if in_pi {
            assert_eq!(pi_models["providers"]["local"], *provider);
            assert_eq!(result["snapshot"]["defaultModel"], "chat");
        } else {
            assert!(pi_models["providers"].get("local").is_none());
            assert!(result["snapshot"]["defaultModel"].is_null());
        }
        assert_eq!(result["snapshot"]["providers"][0]["inPi"], in_pi);
        assert_eq!(
            fs::read(&fixture.core.paths.pi_settings).unwrap(),
            settings_before
        );
        assert_eq!(
            documents::list_backups(&fixture.core.paths).unwrap().len(),
            backups_before + 1
        );
    }
}

#[test]
fn model_duplication_rejects_missing_sources_collisions_and_bad_requests_without_writing() {
    let mut fixture = Fixture::new();
    fixture.create_provider("local", true);
    fixture.create_model("local", "chat");
    fixture.call(json!({ "action": "model.default", "providerId": "local", "modelId": "chat" }));
    let paths = [
        fixture.core.paths.providers.clone(),
        fixture.core.paths.pi_models.clone(),
        fixture.core.paths.pi_settings.clone(),
    ];
    let before: Vec<_> = paths.iter().map(|path| fs::read(path).unwrap()).collect();
    let backups_before = documents::list_backups(&fixture.core.paths).unwrap().len();
    for (source_id, target_id, expected) in [
        ("missing", "copy", "no longer exists"),
        ("chat", "chat", "already exists"),
    ] {
        let error = fixture
            .core
            .dispatch(&json!({
                "action": "model.duplicate", "providerId": "local",
                "sourceModelId": source_id, "draft": model_draft(target_id),
            }))
            .unwrap_err();
        assert!(error.to_string().contains(expected), "{error}");
    }
    for request in [
        json!({ "action": "model.duplicate", "providerId": "missing", "sourceModelId": "chat", "draft": model_draft("copy") }),
        json!({ "action": "model.duplicate", "providerId": "local", "sourceModelId": null, "draft": model_draft("copy") }),
        json!({ "action": "model.duplicate", "providerId": "local", "sourceModelId": "chat", "previousId": "chat", "draft": model_draft("copy") }),
        json!({ "action": "model.duplicate", "providerId": "local", "sourceModelId": "chat", "draft": { "id": "copy", "input": [] } }),
    ] {
        assert!(
            fixture.core.dispatch(&request).is_err(),
            "accepted {request}"
        );
    }
    for (path, contents) in paths.iter().zip(before) {
        assert_eq!(fs::read(path).unwrap(), contents);
    }
    assert_eq!(
        documents::list_backups(&fixture.core.paths).unwrap().len(),
        backups_before
    );
}

#[test]
fn profile_ordering_is_persisted_without_changing_pi_or_provider_configs() {
    let mut fixture = Fixture::new();
    fixture.create_provider("zeta", true);
    fixture.create_provider("alpha", false);
    fixture.create_model("zeta", "first");
    fixture.create_model("zeta", "second");
    fixture.call(json!({ "action": "model.default", "providerId": "zeta", "modelId": "first" }));
    let pi_before = fs::read(&fixture.core.paths.pi_models).unwrap();
    let settings_before = fs::read(&fixture.core.paths.pi_settings).unwrap();
    let providers_before = read_json(&fixture.core.paths.providers)["providers"].clone();
    let initial = fixture.call(json!({ "action": "snapshot" }));
    let original_date = initial["ordering"]["models"]["zeta"]["addedAt"]["first"].clone();
    assert!(original_date.is_string());
    for mode in ["name-asc", "name-desc", "added-asc", "added-desc", "custom"] {
        let result = fixture.call(json!({ "action": "providers.sort", "value": mode }));
        assert_eq!(result["snapshot"]["ordering"]["providers"]["sort"], mode);
        let result =
            fixture.call(json!({ "action": "models.sort", "providerId": "zeta", "value": mode }));
        assert_eq!(
            result["snapshot"]["ordering"]["models"]["zeta"]["sort"],
            mode
        );
    }
    fixture.call(json!({ "action": "providers.reorder", "ids": ["zeta", "alpha"] }));
    fixture.call(
        json!({ "action": "models.reorder", "providerId": "zeta", "ids": ["second", "first"] }),
    );
    let mut reloaded = WebCore::new(
        fixture.core.paths.clone(),
        fixture.core.sessions_root.clone(),
    );
    let result = reloaded.dispatch(&json!({ "action": "snapshot" })).unwrap();
    assert_eq!(
        result["ordering"]["providers"]["order"],
        json!(["zeta", "alpha"])
    );
    assert_eq!(
        result["ordering"]["models"]["zeta"]["order"],
        json!(["second", "first"])
    );
    assert_eq!(
        result["ordering"]["models"]["zeta"]["addedAt"]["first"],
        original_date
    );
    assert_eq!(fs::read(&fixture.core.paths.pi_models).unwrap(), pi_before);
    assert_eq!(
        fs::read(&fixture.core.paths.pi_settings).unwrap(),
        settings_before
    );
    assert_eq!(
        read_json(&fixture.core.paths.providers)["providers"],
        providers_before
    );
}

#[test]
fn profile_ordering_keeps_legacy_and_external_dates_unknown() {
    let mut fixture = Fixture::new();
    let provider = json!({ "models": [{ "id": "legacy" }] });
    write_json(
        &fixture.core.paths.providers,
        &json!({ "version": 1, "providers": { "old": provider } }),
    );
    write_json(
        &fixture.core.paths.pi_models,
        &json!({ "providers": { "old": provider } }),
    );
    let before = fs::read(&fixture.core.paths.providers).unwrap();
    let initial = fixture.call(json!({ "action": "snapshot" }));
    assert!(initial["ordering"]["providers"]["addedAt"]["old"].is_null());
    assert!(initial["ordering"]["models"]["old"]["addedAt"]["legacy"].is_null());
    assert_eq!(fs::read(&fixture.core.paths.providers).unwrap(), before);
    fixture.call(json!({ "action": "model.save", "providerId": "old", "previousId": "legacy", "draft": model_draft("renamed") }));
    let result = fixture.call(json!({ "action": "provider.save", "previousId": "old", "draft": provider_draft("new-name", true) }));
    assert!(result["snapshot"]["ordering"]["providers"]["addedAt"]["new-name"].is_null());
    assert!(result["snapshot"]["ordering"]["models"]["new-name"]["addedAt"]["renamed"].is_null());
    let mut pi_models = read_json(&fixture.core.paths.pi_models);
    pi_models["providers"]["external"] = json!({ "models": [{ "id": "from-pi" }] });
    write_json(&fixture.core.paths.pi_models, &pi_models);
    let result = fixture.call(json!({ "action": "snapshot" }));
    assert!(result["ordering"]["providers"]["addedAt"]["external"].is_null());
    assert!(result["ordering"]["models"]["external"]["addedAt"]["from-pi"].is_null());
}

#[test]
fn profile_ordering_tracks_renames_copies_removals_and_backup_restore() {
    let mut fixture = Fixture::new();
    fixture.create_provider("local", true);
    fixture.create_model("local", "first");
    fixture.create_model("local", "second");
    fixture.call(
        json!({ "action": "models.reorder", "providerId": "local", "ids": ["second", "first"] }),
    );
    let date = json!("2001-02-03T04:05:06.000Z");
    let mut library = read_json(&fixture.core.paths.providers);
    library["ordering"]["providers"]["addedAt"]["local"] = date.clone();
    library["ordering"]["models"]["local"]["addedAt"]["first"] = date.clone();
    library["ordering"]["futureField"] = json!({ "keep": true });
    write_json(&fixture.core.paths.providers, &library);
    fixture.call(json!({ "action": "model.save", "providerId": "local", "previousId": "first", "draft": model_draft("renamed") }));
    let renamed = fixture.call(json!({ "action": "provider.save", "previousId": "local", "draft": provider_draft("renamed-provider", true) }));
    let ordering = &renamed["snapshot"]["ordering"];
    assert_eq!(ordering["providers"]["addedAt"]["renamed-provider"], date);
    assert_eq!(
        ordering["models"]["renamed-provider"]["addedAt"]["renamed"],
        date
    );
    assert_eq!(
        ordering["models"]["renamed-provider"]["order"],
        json!(["second", "renamed"])
    );
    assert!(ordering["models"].get("local").is_none());
    assert_eq!(
        read_json(&fixture.core.paths.providers)["ordering"]["futureField"],
        json!({ "keep": true })
    );
    let copied = fixture.call(json!({ "action": "model.duplicate", "providerId": "renamed-provider", "sourceModelId": "renamed", "draft": model_draft("copied") }));
    let copied_date =
        &copied["snapshot"]["ordering"]["models"]["renamed-provider"]["addedAt"]["copied"];
    assert!(copied_date.is_string());
    assert_ne!(*copied_date, date);
    let copied =
        fixture.call(json!({ "action": "provider.duplicate", "providerId": "renamed-provider" }));
    assert_ne!(
        copied["snapshot"]["ordering"]["providers"]["addedAt"]["renamed-provider-copy"],
        date
    );
    assert_ne!(
        copied["snapshot"]["ordering"]["models"]["renamed-provider-copy"]["addedAt"]["renamed"],
        date
    );
    let restore_library = read_json(&fixture.core.paths.providers);
    let restore_pi = read_json(&fixture.core.paths.pi_models);
    let backup_name = "backup-ordering-fixture.json";
    write_json(
        &fixture.core.paths.backups.join(backup_name),
        &json!({
            "version": 3, "providers": restore_library, "models": restore_pi, "piSettings": {}, "appSettings": {},
        }),
    );
    let removed = fixture.call(
        json!({ "action": "model.remove", "providerId": "renamed-provider", "modelId": "renamed" }),
    );
    assert!(
        removed["snapshot"]["ordering"]["models"]["renamed-provider"]["addedAt"]
            .get("renamed")
            .is_none()
    );
    let removed =
        fixture.call(json!({ "action": "provider.remove", "providerId": "renamed-provider-copy" }));
    assert!(removed["snapshot"]["ordering"]["models"]
        .get("renamed-provider-copy")
        .is_none());
    let restored = fixture.call(json!({ "action": "backups.restore", "name": backup_name }));
    assert_eq!(
        restored["snapshot"]["ordering"]["models"]["renamed-provider"]["order"],
        json!(["second", "renamed", "copied"])
    );
    assert_eq!(read_json(&fixture.core.paths.providers), restore_library);
}

#[test]
fn profile_ordering_dates_are_added_only_for_new_imported_items() {
    let mut fixture = Fixture::new();
    fixture.create_provider("local", false);
    fixture.create_model("local", "existing");
    let initial = fixture.call(json!({ "action": "settings.metadata", "value": false }));
    let existing_date =
        initial["snapshot"]["ordering"]["models"]["local"]["addedAt"]["existing"].clone();
    fixture.seed_candidates("local", &["existing", "online"]);
    let imported = fixture.call(json!({ "action": "models.import", "providerId": "local", "ids": ["existing", "online"], "updateExisting": true }));
    assert_eq!(
        imported["snapshot"]["ordering"]["models"]["local"]["addedAt"]["existing"],
        existing_date
    );
    assert!(imported["snapshot"]["ordering"]["models"]["local"]["addedAt"]["online"].is_string());
    write_json(
        &fixture.core.paths.opencode,
        &json!({ "provider": {
        "local": { "npm": "@ai-sdk/openai-compatible", "options": { "baseURL": "https://example.test/v1" }, "models": { "existing": {}, "opencode": {} } },
    } }),
    );
    let prepared = fixture.call(json!({ "action": "opencode.prepare", "providerIds": ["local"] }));
    let imported = fixture.call(json!({ "action": "opencode.import", "planId": prepared["planId"], "candidateIndices": [] }));
    assert_eq!(
        imported["snapshot"]["ordering"]["models"]["local"]["addedAt"]["existing"],
        existing_date
    );
    assert!(imported["snapshot"]["ordering"]["models"]["local"]["addedAt"]["opencode"].is_string());
}

#[test]
fn profile_ordering_rejects_stale_orders_and_invalid_metadata_without_writing() {
    let mut fixture = Fixture::new();
    fixture.create_provider("local", true);
    fixture.create_model("local", "first");
    fixture.create_model("local", "second");
    let before = fs::read(&fixture.core.paths.providers).unwrap();
    let pi_before = fs::read(&fixture.core.paths.pi_models).unwrap();
    for request in [
        json!({ "action": "providers.sort", "value": "random" }),
        json!({ "action": "models.sort", "providerId": "missing", "value": "name-asc" }),
        json!({ "action": "providers.reorder", "ids": [] }),
        json!({ "action": "models.reorder", "providerId": "local", "ids": ["first", "first"] }),
        json!({ "action": "models.reorder", "providerId": "local", "ids": ["first"] }),
        json!({ "action": "models.reorder", "providerId": "local", "ids": ["first", "missing"] }),
    ] {
        assert!(
            fixture.core.dispatch(&request).is_err(),
            "accepted {request}"
        );
    }
    assert_eq!(fs::read(&fixture.core.paths.providers).unwrap(), before);
    for (field, value) in [
        ("sort", json!("unknown")),
        ("order", json!(["local", "local"])),
        ("addedAt", json!({ "local": "invalid date" })),
    ] {
        let mut malformed: Value = serde_json::from_slice(&before).unwrap();
        malformed["ordering"]["providers"][field] = value;
        write_json(&fixture.core.paths.providers, &malformed);
        let bytes = fs::read(&fixture.core.paths.providers).unwrap();
        let error = fixture
            .core
            .dispatch(&json!({ "action": "snapshot" }))
            .unwrap_err();
        assert!(error.to_string().contains("ordering"), "{error}");
        assert_eq!(fs::read(&fixture.core.paths.providers).unwrap(), bytes);
    }
    assert_eq!(fs::read(&fixture.core.paths.pi_models).unwrap(), pi_before);
}

#[test]
fn malformed_boundary_types_and_unknown_fields_fail_before_writing() {
    let mut fixture = Fixture::new();
    fixture.create_provider("local", true);
    fixture.create_model("local", "chat");
    let before = fs::read(&fixture.core.paths.providers).unwrap();
    let before_pi = fs::read(&fixture.core.paths.pi_models).unwrap();
    for (field, value) in [
        ("id", json!(7)),
        ("inPi", json!("false")),
        ("baseUrl", json!(false)),
        ("api", json!([])),
        ("apiKey", json!(123)),
        ("authHeader", json!(null)),
        ("headers", json!([])),
        ("compat", json!("{}")),
    ] {
        let mut draft = provider_draft("local", true);
        draft[field] = value;
        let error = fixture
            .core
            .dispatch(&json!({ "action": "provider.save", "previousId": "local", "draft": draft }))
            .unwrap_err();
        assert!(error.to_string().contains(field), "{error}");
    }
    for (field, value) in [
        ("name", json!(false)),
        ("api", json!(9)),
        ("reasoning", json!("true")),
        ("input", json!(["text", 2])),
        ("contextWindow", json!(1.5)),
        ("maxTokens", json!(-2)),
        ("inputCost", json!(-1)),
        ("outputCost", json!("2")),
        ("thinkingLevelMap", json!([])),
    ] {
        let mut draft = model_draft("chat");
        draft[field] = value;
        let error = fixture.core.dispatch(&json!({ "action": "model.save", "providerId": "local", "previousId": "chat", "draft": draft })).unwrap_err();
        assert!(error.to_string().contains(field), "{error}");
    }
    for request in [
        json!({ "action": "settings.metadata", "value": "false" }),
        json!({ "action": "settings.updates", "value": 0 }),
        json!({ "action": "settings.defaults", "value": { "contextWindow": "100" } }),
        json!({ "action": "provider.save", "previousId": 5, "draft": provider_draft("local", true) }),
        json!({ "action": "provider.sync", "providerId": "local", "inPi": 1 }),
        json!({ "action": "model.default", "providerId": "local", "modelId": "chat", "other": true }),
        json!({ "action": "models.import", "providerId": "local", "ids": ["chat"], "updateExisting": true, "config": {} }),
    ] {
        assert!(
            fixture.core.dispatch(&request).is_err(),
            "accepted {request}"
        );
    }
    assert_eq!(fs::read(&fixture.core.paths.providers).unwrap(), before);
    assert_eq!(fs::read(&fixture.core.paths.pi_models).unwrap(), before_pi);
}

#[test]
fn malformed_json_and_unknown_actions_return_errors() {
    let mut fixture = Fixture::new();
    assert!(fixture
        .core
        .request("{")
        .unwrap_err()
        .to_string()
        .contains("invalid Web request JSON"));
    for input in [
        json!([]),
        json!(null),
        json!({}),
        json!({ "action": 2 }),
        json!({ "action": "no-such-action" }),
    ] {
        assert!(fixture.core.dispatch(&input).is_err());
    }
    assert!(!fixture.core.paths.providers.exists());
}

#[test]
fn settings_round_trip_through_the_shared_settings_documents() {
    let mut fixture = Fixture::new();
    fixture.call(json!({ "action": "settings.language", "value": "zh-CN" }));
    fixture.call(json!({ "action": "settings.metadata", "value": false }));
    fixture.call(json!({ "action": "settings.updates", "value": false }));
    let result = fixture.call(json!({
        "action": "settings.defaults",
        "value": { "contextWindow": 200_000, "maxTokens": 32_000, "inputCost": 0.2 },
    }));
    let snapshot = &result["snapshot"];
    assert_eq!(snapshot["language"], "zh-CN");
    assert_eq!(snapshot["fetchModelMetadata"], false);
    assert_eq!(snapshot["checkUpdates"], false);
    assert_eq!(snapshot["modelDefaults"]["contextWindow"], 200_000);
    assert_eq!(snapshot["modelDefaults"]["inputCost"], 0.2);
    assert!(snapshot["modelDefaults"]["outputCost"].is_null());
    assert_eq!(
        snapshot["paths"]["sessions"],
        fixture.core.sessions_root.display().to_string()
    );
    assert!(!fixture.core.paths.pi_settings.exists());
}

fn write_session(path: &Path) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let entries = [
        json!({ "type": "session", "version": 3, "id": "session-1", "cwd": "/project", "timestamp": "2026-09-08T12:00:00Z" }),
        json!({ "type": "session_info", "name": "Review Web UI" }),
        json!({ "type": "message", "id": "u1", "parentId": null, "message": { "role": "user", "content": "Review the web page" } }),
        json!({ "type": "message", "id": "a1", "parentId": "u1", "message": { "role": "assistant", "content": "## Findings\nLooks good." } }),
    ];
    fs::write(
        path,
        entries
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .unwrap();
}

#[test]
fn sessions_list_and_preview_preserve_titles_timestamps_and_branch_data() {
    let mut fixture = Fixture::new();
    write_session(&fixture.core.sessions_root.join("project/session.jsonl"));
    fs::write(
        fixture.core.sessions_root.join("not-a-session.jsonl"),
        "{\"private\":true}",
    )
    .unwrap();
    let listed = fixture.call(json!({ "action": "sessions.list" }));
    let sessions = listed["sessions"].as_array().unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0]["id"], "project/session.jsonl");
    assert_eq!(sessions[0]["sessionId"], "session-1");
    assert_eq!(sessions[0]["title"], "Review Web UI");
    assert_eq!(sessions[0]["messageCount"], 2);
    assert!(
        chrono::DateTime::parse_from_rfc3339(sessions[0]["modifiedAt"].as_str().unwrap()).is_ok()
    );
    let preview =
        fixture.call(json!({ "action": "sessions.preview", "id": "project/session.jsonl" }));
    assert_eq!(preview["messages"].as_array().unwrap().len(), 2);
    assert_eq!(preview["messages"][1]["text"], "## Findings\nLooks good.");
    assert_eq!(preview["messages"][1]["tree"]["parentId"], "u1");
    let user_only = fixture.call(
        json!({ "action": "sessions.preview", "id": "project/session.jsonl", "userOnly": true }),
    );
    assert_eq!(user_only["messages"].as_array().unwrap().len(), 1);
    assert_eq!(user_only["messages"][0]["role"], "user");
}

#[test]
fn session_ids_cannot_escape_the_root_or_select_regular_files() {
    let mut fixture = Fixture::new();
    let outside = fixture.root.join("outside.jsonl");
    write_session(&outside);
    let arbitrary = fixture.core.sessions_root.join("private.jsonl");
    fs::write(&arbitrary, "{\"private\":true}").unwrap();
    for id in [
        "../outside.jsonl",
        "project/../../outside.jsonl",
        "..\\outside.jsonl",
        "/outside.jsonl",
        "C:/outside.jsonl",
        "\\\\server\\outside.jsonl",
        "project//session.jsonl",
        "private.jsonl",
        "%2e%2e/outside.jsonl",
        outside.to_str().unwrap(),
    ] {
        for action in ["sessions.preview", "sessions.delete"] {
            assert!(
                fixture
                    .core
                    .dispatch(&json!({ "action": action, "id": id }))
                    .is_err(),
                "accepted {id}"
            );
        }
    }
    assert!(outside.exists());
    assert!(arbitrary.exists());
}

#[test]
fn backup_restore_uses_only_a_valid_listed_basename() {
    let mut fixture = Fixture::new();
    fixture.call(json!({ "action": "settings.language", "value": "en" }));
    fixture.call(json!({ "action": "settings.language", "value": "zh-CN" }));
    let backups = fixture.call(json!({ "action": "backups.list" }));
    let newest = &backups["backups"][0];
    let name = newest["name"].as_str().unwrap().to_owned();
    let path = newest["path"].as_str().unwrap().to_owned();
    let before = fs::read(&fixture.core.paths.app_settings).unwrap();
    for invalid_name in [
        "../backup.json",
        "..\\backup.json",
        "nested/backup.json",
        "backup-missing.json",
        path.as_str(),
    ] {
        assert!(fixture
            .core
            .dispatch(&json!({ "action": "backups.restore", "name": invalid_name }))
            .is_err());
    }
    assert_eq!(fs::read(&fixture.core.paths.app_settings).unwrap(), before);
    let restored = fixture.call(json!({ "action": "backups.restore", "name": name }));
    assert_eq!(restored["snapshot"]["language"], "en");
}

#[cfg(unix)]
#[test]
fn backup_symlinks_and_session_directory_symlinks_cannot_escape_their_roots() {
    use std::os::unix::fs::symlink;
    let mut fixture = Fixture::new();
    fixture.call(json!({ "action": "settings.language", "value": "en" }));
    fixture.call(json!({ "action": "settings.language", "value": "zh-CN" }));
    let backup = documents::list_backups(&fixture.core.paths)
        .unwrap()
        .remove(0);
    let outside = fixture.root.join("outside-backup.json");
    fs::copy(backup.path, &outside).unwrap();
    symlink(
        &outside,
        fixture.core.paths.backups.join("backup-link.json"),
    )
    .unwrap();
    let error = fixture
        .core
        .dispatch(&json!({ "action": "backups.restore", "name": "backup-link.json" }))
        .unwrap_err();
    assert!(error.to_string().contains("outside"));

    let external_sessions = fixture.root.join("external-sessions");
    write_session(&external_sessions.join("private.jsonl"));
    symlink(
        &external_sessions,
        fixture.core.sessions_root.join("linked"),
    )
    .unwrap();
    assert!(fixture
        .core
        .dispatch(&json!({ "action": "sessions.preview", "id": "linked/private.jsonl" }))
        .is_err());
    assert!(external_sessions.join("private.jsonl").exists());
}

const TEST_HTTP_ACCEPT_TIMEOUT: Duration = Duration::from_secs(4);
const TEST_HTTP_IO_TIMEOUT: Duration = Duration::from_secs(2);
const TEST_HTTP_HEADER_LIMIT: usize = 16 * 1024;

fn read_http_headers(stream: &mut TcpStream) -> Vec<u8> {
    let deadline = Instant::now() + TEST_HTTP_IO_TIMEOUT;
    let mut request = Vec::new();
    let mut chunk = [0_u8; 4096];
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(
            !remaining.is_zero(),
            "timed out reading complete HTTP request headers"
        );
        stream.set_read_timeout(Some(remaining)).unwrap();
        let count = match stream.read(&mut chunk) {
            Ok(count) => count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => panic!("failed to read complete HTTP request headers: {error}"),
        };
        assert!(
            count > 0,
            "connection closed before HTTP request headers were complete"
        );
        request.extend_from_slice(&chunk[..count]);
        assert!(
            request.len() <= TEST_HTTP_HEADER_LIMIT,
            "HTTP request headers exceeded the test limit"
        );
        if request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
            return request;
        }
    }
}

fn model_endpoint() -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let worker = thread::spawn(move || {
        let deadline = Instant::now() + TEST_HTTP_ACCEPT_TIMEOUT;
        loop {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    // Windows accepted sockets can inherit the listener's mode;
                    // only accept polling is nonblocking in this test server.
                    stream.set_nonblocking(false).unwrap();
                    stream
                        .set_write_timeout(Some(TEST_HTTP_IO_TIMEOUT))
                        .unwrap();
                    let request = read_http_headers(&mut stream);
                    assert!(request.starts_with(b"GET /v1/models "));
                    let body = r#"{"data":[{"id":"chat"},{"id":"vision"}]}"#;
                    write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
                    return;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        Instant::now() < deadline,
                        "model endpoint was never requested"
                    );
                    thread::sleep(Duration::from_millis(2));
                }
                Err(error) => panic!("model endpoint failed: {error}"),
            }
        }
    });
    (format!("http://{address}/v1"), worker)
}

#[test]
fn model_endpoint_waits_for_a_complete_fragmented_http_request() {
    let (endpoint, worker) = model_endpoint();
    let address = endpoint
        .strip_prefix("http://")
        .unwrap()
        .strip_suffix("/v1")
        .unwrap();
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_write_timeout(Some(TEST_HTTP_IO_TIMEOUT))
        .unwrap();
    stream
        .set_read_timeout(Some(Duration::from_millis(100)))
        .unwrap();
    stream.write_all(b"GET /v1/").unwrap();
    let mut premature_response = [0_u8; 1];
    let result = stream.read(&mut premature_response);
    assert!(
        matches!(result, Err(ref error) if matches!(
            error.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
        )),
        "server responded or closed before the request was complete: {result:?}"
    );

    stream
        .write_all(b"models HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .unwrap();
    stream.set_read_timeout(Some(TEST_HTTP_IO_TIMEOUT)).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    worker.join().unwrap();
    assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
    assert!(response.ends_with(r#"{"data":[{"id":"chat"},{"id":"vision"}]}"#));
}

#[test]
fn fetched_model_ids_can_be_imported_but_browser_supplied_ids_cannot() {
    let mut fixture = Fixture::new();
    let (endpoint, worker) = model_endpoint();
    let mut draft = provider_draft("local", true);
    draft["baseUrl"] = json!(endpoint);
    draft["apiKey"] = json!("");
    fixture.call(json!({ "action": "provider.save", "draft": draft }));
    fixture.create_model("local", "chat");
    fixture.call(json!({ "action": "settings.metadata", "value": false }));
    let fetched = fixture.call(json!({ "action": "models.fetch", "providerId": "local" }));
    worker.join().unwrap();
    assert_eq!(fetched["total"], 2);
    assert_eq!(
        fetched["models"][0],
        json!({ "id": "chat", "existing": true })
    );
    assert!(fixture.core.dispatch(&json!({
        "action": "models.import", "providerId": "local", "ids": ["injected"], "updateExisting": true,
    })).unwrap_err().to_string().contains("not returned"));
    let imported = fixture.call(json!({
        "action": "models.import", "providerId": "local", "ids": ["vision"], "updateExisting": false,
    }));
    assert_eq!(imported["summary"], json!({ "added": 1, "updated": 0 }));
    assert_eq!(
        imported["snapshot"]["providers"][0]["models"][1]["id"],
        "vision"
    );
    assert!(fixture.core.dispatch(&json!({
        "action": "models.import", "providerId": "local", "ids": ["vision"], "updateExisting": false,
    })).unwrap_err().to_string().contains("expired"));
}

#[test]
fn changed_provider_configuration_rejects_stale_model_candidates() {
    let mut fixture = Fixture::new();
    fixture.create_provider("local", true);
    fixture.seed_candidates("local", &["chat"]);
    let mut changed = provider_draft("local", true);
    changed["baseUrl"] = json!("https://changed.test/v1");
    fixture.call(json!({ "action": "provider.save", "previousId": "local", "draft": changed }));
    let error = fixture.core.dispatch(&json!({
        "action": "models.import", "providerId": "local", "ids": ["chat"], "updateExisting": false,
    })).unwrap_err();
    assert!(error.to_string().contains("configuration changed"));
}

#[test]
fn catalog_selection_tokens_and_indices_protect_pending_imports() {
    let mut fixture = Fixture::new();
    fixture.create_provider("local", true);
    fixture.seed_candidates("local", &["chat"]);
    let mut fetched = CatalogFetch {
        models: Vec::new(),
        ambiguous: vec![CatalogAmbiguity {
            provider_id: "local".into(),
            model_id: "chat".into(),
            candidates: vec![CatalogCandidate {
                provider_id: "catalog".into(),
                model: ModelDefaults::default().model("original-id"),
            }],
        }],
        unavailable: 0,
        ratio_prices: [(
            "chat".into(),
            RatioCost {
                input: 1.25,
                output: 3.5,
                cache_read: 0.1,
                cache_write: 0.0,
            },
        )]
        .into(),
        ratio_config_used: true,
        catalog_unreachable: false,
    };
    fetched.apply_ratio_prices();
    fixture
        .core
        .fetched_models
        .get_mut("local")
        .unwrap()
        .prepared = Some(PreparedModelImport {
        selection_id: 7,
        ids: vec!["chat".into()],
        update_existing: false,
        fetched,
    });
    let request = json!({
        "action": "models.import", "providerId": "local", "ids": ["chat"], "updateExisting": false,
        "selectionId": 7, "candidateIndices": [0],
    });
    let mut stale = request.clone();
    stale["selectionId"] = json!(6);
    assert!(fixture
        .core
        .dispatch(&stale)
        .unwrap_err()
        .to_string()
        .contains("expired"));
    let mut invalid_index = request.clone();
    invalid_index["candidateIndices"] = json!([1]);
    assert!(fixture
        .core
        .dispatch(&invalid_index)
        .unwrap_err()
        .to_string()
        .contains("out of range"));
    let imported = fixture.call(request);
    let model = &imported["snapshot"]["providers"][0]["models"][0];
    assert_eq!(model["id"], "chat");
    assert_eq!(model["inputCost"], 1.25);
    assert_eq!(model["outputCost"], 3.5);
}

#[test]
fn opencode_import_uses_a_matching_plan_and_shared_mapping_rules() {
    let mut fixture = Fixture::new();
    fixture.call(json!({ "action": "settings.metadata", "value": false }));
    write_json(
        &fixture.core.paths.opencode,
        &json!({
            "provider": { "gateway": {
                "npm": "@ai-sdk/openai-compatible",
                "options": { "baseURL": "https://gateway.test/v1", "apiKey": "{env:GATEWAY_KEY}" },
                "models": { "chat": { "name": "Chat model" } },
            }},
        }),
    );
    let providers = fixture.call(json!({ "action": "opencode.list" }));
    assert_eq!(providers["providerIds"], json!(["gateway"]));
    let first = fixture.call(json!({ "action": "opencode.prepare", "providerIds": ["gateway"] }));
    let second = fixture.call(json!({ "action": "opencode.prepare", "providerIds": ["gateway"] }));
    assert!(fixture
        .core
        .dispatch(&json!({
            "action": "opencode.import", "planId": first["planId"], "candidateIndices": [],
        }))
        .unwrap_err()
        .to_string()
        .contains("expired"));
    assert!(fixture
        .core
        .dispatch(&json!({
            "action": "opencode.import", "planId": second["planId"], "candidateIndices": [0],
        }))
        .is_err());
    let imported = fixture.call(json!({
        "action": "opencode.import", "planId": second["planId"], "candidateIndices": [],
    }));
    assert_eq!(imported["summary"]["providers"], 1);
    assert_eq!(imported["summary"]["models"], 1);
    let provider = &imported["snapshot"]["providers"][0];
    assert_eq!(provider["apiKey"], "${GATEWAY_KEY}");
    assert_eq!(provider["models"][0]["name"], "Chat model");
    assert_eq!(provider["inPi"], true);
}

#[test]
fn provider_save_asks_before_replacing_a_pi_credential() {
    let mut fixture = Fixture::new();
    let auth = fixture.core.paths.pi_auth.clone();
    fs::write(
        &auth,
        r#"{"pi-login":{"type":"oauth","access":"a","refresh":"r","expires":9}}"#,
    )
    .unwrap();
    let draft = provider_draft("pi-login", true);

    let response = fixture.call(json!({ "action": "provider.save", "draft": draft.clone() }));
    assert_eq!(response["requiresCredentialOverwrite"], json!(true));
    assert_eq!(read_json(&auth)["pi-login"]["type"], json!("oauth"));

    let response = fixture
        .call(json!({ "action": "provider.save", "draft": draft, "overwriteCredential": true }));
    assert!(response.get("snapshot").is_some());
}
