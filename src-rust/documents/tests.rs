use super::*;
use std::{
    env, fs,
    path::PathBuf,
    process,
    sync::atomic::{AtomicUsize, Ordering},
};

static FIXTURE_ID: AtomicUsize = AtomicUsize::new(0);

fn fixture() -> (PathBuf, Paths) {
    let root = env::temp_dir().join(format!(
        "pi-switch-test-{}-{}-{}",
        process::id(),
        now_millis(),
        FIXTURE_ID.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(root.join(".pi/agent")).unwrap();
    let paths = Paths::from_home(&root);
    (root, paths)
}

fn model_draft(id: &str) -> ModelDraft {
    ModelDraft {
        id: id.into(),
        name: Some(id.into()),
        api: None,
        reasoning: false,
        input: vec!["text".into()],
        context_window: 128_000,
        max_tokens: 16_384,
    }
}

fn catalog_model(id: &str, context_window: u64, max_tokens: u64, input_cost: f64) -> CatalogModel {
    CatalogModel {
        id: id.into(),
        config: json!({
            "id": id,
            "name": id,
            "reasoning": false,
            "input": ["text"],
            "cost": {"input": input_cost, "output": 2, "cacheRead": 0.1, "cacheWrite": 0},
            "contextWindow": context_window,
            "maxTokens": max_tokens
        }),
    }
}

fn models_dev_model(id: &str, context_window: u64, max_tokens: u64, input_cost: f64) -> Value {
    json!({
        "id": id,
        "name": id,
        "reasoning": false,
        "modalities": {"input": ["text"], "output": ["text"]},
        "cost": {"input": input_cost, "output": 2, "cache_read": 0.1},
        "limit": {"context": context_window, "output": max_tokens}
    })
}

fn write_opencode(paths: &Paths, value: Value) {
    fs::create_dir_all(paths.opencode.parent().unwrap()).unwrap();
    fs::write(&paths.opencode, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
}

#[test]
fn opencode_import_maps_and_merges_providers_and_models() {
    let (root, paths) = fixture();
    fs::write(
        &paths.models,
        r#"{"future":1,"providers":{"custom":{"futureProvider":true,"api":"openai-completions","models":[{"id":"existing","futureModel":true}]}}}"#,
    )
    .unwrap();
    write_opencode(
        &paths,
        json!({
            "provider": {
                "custom": {
                    "npm": "@ai-sdk/openai-compatible",
                    "options": {
                        "baseURL": "https://custom.test/v1",
                        "apiKey": "{env:CUSTOM_KEY}",
                        "headers": {"x-team-key": "{env:TEAM_KEY}"}
                    },
                    "models": {
                        "existing": {"name": "Existing model"},
                        "vision": {
                            "name": "Vision model",
                            "reasoning": true,
                            "limit": {"context": 200000, "output": 32000},
                            "modalities": {"input": ["text", "image"]}
                        }
                    }
                },
                "anthropic": {
                    "npm": "@ai-sdk/anthropic",
                    "models": {"claude": {"name": "Claude"}}
                }
            }
        }),
    );

    assert_eq!(
        import_opencode_with_catalog(&paths, &ModelCatalog::default()).unwrap(),
        ImportSummary {
            providers: 2,
            models: 3,
            metadata: 0,
            defaults: 0,
            unresolved: 3,
            changed: true
        }
    );
    let imported: Value = serde_json::from_slice(&fs::read(&paths.models).unwrap()).unwrap();
    let custom = &imported["providers"]["custom"];
    assert_eq!(imported["future"], 1);
    assert_eq!(custom["futureProvider"], true);
    assert_eq!(custom["baseUrl"], "https://custom.test/v1");
    assert_eq!(custom["apiKey"], "${CUSTOM_KEY}");
    assert_eq!(custom["headers"]["x-team-key"], "${TEAM_KEY}");
    assert_eq!(custom["models"][0]["futureModel"], true);
    assert_eq!(custom["models"][0]["name"], "Existing model");
    assert_eq!(custom["models"][1]["id"], "vision");
    assert_eq!(custom["models"][1]["reasoning"], true);
    assert_eq!(custom["models"][1]["input"], json!(["text", "image"]));
    assert_eq!(custom["models"][1]["contextWindow"], 200_000);
    assert_eq!(custom["models"][1]["maxTokens"], 32_000);
    assert_eq!(
        imported["providers"]["anthropic"]["api"],
        "anthropic-messages"
    );
    assert!(!list_backups(&paths).unwrap().is_empty());

    assert!(
        !import_opencode_with_catalog(&paths, &ModelCatalog::default())
            .unwrap()
            .changed
    );
    let imported: Value = serde_json::from_slice(&fs::read(&paths.models).unwrap()).unwrap();
    assert_eq!(
        imported["providers"]["custom"]["models"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn invalid_opencode_import_does_not_overwrite_pi_models() {
    let (root, paths) = fixture();
    fs::write(&paths.models, r#"{"providers":{},"keep":true}"#).unwrap();
    let before = fs::read(&paths.models).unwrap();

    write_opencode(
        &paths,
        json!({"provider":{"bad":{"npm":"unsupported","models":{}}}}),
    );
    assert!(import_opencode_with_catalog(&paths, &ModelCatalog::default()).is_err());
    assert_eq!(fs::read(&paths.models).unwrap(), before);

    write_opencode(
        &paths,
        json!({
            "provider": {
                "bad": {
                    "npm": "@ai-sdk/openai-compatible",
                    "options": {"apiKey": "{file:~/.secret}"},
                    "models": {}
                }
            }
        }),
    );
    assert!(import_opencode_with_catalog(&paths, &ModelCatalog::default()).is_err());
    assert_eq!(fs::read(&paths.models).unwrap(), before);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn provider_updates_preserve_unknown_data_and_create_backup() {
    let (root, paths) = fixture();
    fs::write(
            &paths.models,
            r#"{"schemaVersion":9,"providers":{"old":{"baseUrl":"https://old.test/v1","api":"openai-completions","apiKey":"$KEY","future":true,"models":[{"id":"keep","futureModel":7}]}}}"#,
        )
        .unwrap();
    fs::write(
        &paths.settings,
        r#"{"defaultProvider":"old","defaultModel":"keep","theme":"x"}"#,
    )
    .unwrap();

    save_provider(
        &paths,
        Some("old"),
        &ProviderDraft {
            id: "new".into(),
            base_url: "https://new.test/v1".into(),
            api: Some("openai-responses".into()),
            api_key: "$KEY".into(),
            auth_header: true,
            headers: Some(json!({
                "User-Agent": "claude-cli/2.1.161",
                "x-team-key": "$TEAM_KEY"
            })),
            compat: Some(json!({"supportsDeveloperRole":false})),
        },
    )
    .unwrap();
    assert_eq!(
        import_models(
            &paths,
            "new",
            &[catalog_model("added", 200_000, 32_000, 1.5)],
            true,
        )
        .unwrap(),
        ModelImportSummary {
            added: 1,
            updated: 0
        }
    );

    let models: Value = serde_json::from_slice(&fs::read(&paths.models).unwrap()).unwrap();
    assert_eq!(models["schemaVersion"], 9);
    assert_eq!(models["providers"]["new"]["future"], true);
    assert_eq!(
        models["providers"]["new"]["headers"]["x-team-key"],
        "$TEAM_KEY"
    );
    assert_eq!(
        models["providers"]["new"]["headers"]["User-Agent"],
        "claude-cli/2.1.161"
    );
    assert_eq!(
        models["providers"]["new"]["compat"]["supportsDeveloperRole"],
        false
    );
    assert_eq!(models["providers"]["new"]["models"][0]["futureModel"], 7);
    assert!(models["providers"].get("old").is_none());
    let settings: Value = serde_json::from_slice(&fs::read(&paths.settings).unwrap()).unwrap();
    assert_eq!(settings["defaultProvider"], "new");
    assert_eq!(settings["theme"], "x");
    assert_eq!(list_backups(&paths).unwrap().len(), 2);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn full_backups_are_coalesced_capped_and_restored() {
    let (root, paths) = fixture();
    let original_models = json!({"providers":{"old":{"models":[]}},"future":true});
    let original_settings = json!({"defaultProvider":"old","defaultModel":"keep","theme":"dark"});
    fs::write(&paths.models, serde_json::to_vec(&original_models).unwrap()).unwrap();
    fs::write(
        &paths.settings,
        serde_json::to_vec(&original_settings).unwrap(),
    )
    .unwrap();

    let draft = ProviderDraft {
        id: "new".into(),
        base_url: "https://new.test/v1".into(),
        api: Some("openai-completions".into()),
        api_key: String::new(),
        auth_header: true,
        headers: None,
        compat: None,
    };
    save_provider(&paths, Some("old"), &draft).unwrap();
    let backups = list_backups(&paths).unwrap();
    assert_eq!(backups.len(), 1);
    assert!(backups[0].name.starts_with("backup-"));
    let first_backup: Value = serde_json::from_slice(&fs::read(&backups[0].path).unwrap()).unwrap();
    assert_eq!(first_backup["models"], original_models);
    assert_eq!(first_backup["settings"], original_settings);

    save_provider(&paths, Some("new"), &draft).unwrap();
    assert_eq!(list_backups(&paths).unwrap().len(), 1);
    set_language(&paths, "zh-CN").unwrap();
    assert_eq!(list_backups(&paths).unwrap().len(), 2);
    set_language(&paths, "zh-CN").unwrap();
    assert_eq!(list_backups(&paths).unwrap().len(), 2);
    let before_restore_models: Value =
        serde_json::from_slice(&fs::read(&paths.models).unwrap()).unwrap();
    let before_restore_settings: Value =
        serde_json::from_slice(&fs::read(&paths.settings).unwrap()).unwrap();

    restore_backup(&paths, &backups[0]).unwrap();
    let restored_models: Value = serde_json::from_slice(&fs::read(&paths.models).unwrap()).unwrap();
    let restored_settings: Value =
        serde_json::from_slice(&fs::read(&paths.settings).unwrap()).unwrap();
    assert_eq!(restored_models, original_models);
    assert_eq!(restored_settings, original_settings);
    assert!(list_backups(&paths).unwrap().iter().any(|backup| {
        let value: Value = serde_json::from_slice(&fs::read(&backup.path).unwrap()).unwrap();
        value["models"] == before_restore_models && value["settings"] == before_restore_settings
    }));

    for index in 0..12 {
        set_language(&paths, if index % 2 == 0 { "en" } else { "zh-CN" }).unwrap();
    }
    let backups = list_backups(&paths).unwrap();
    assert_eq!(backups.len(), 10);
    assert!(backups
        .iter()
        .all(|backup| backup.name.starts_with("backup-")));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn malformed_json_stops_without_overwriting() {
    let (root, paths) = fixture();
    fs::write(&paths.models, "{broken").unwrap();
    let before = fs::read(&paths.models).unwrap();
    let result = save_provider(
        &paths,
        None,
        &ProviderDraft {
            id: "x".into(),
            base_url: "https://example.test/v1".into(),
            api: Some("openai-completions".into()),
            api_key: String::new(),
            auth_header: true,
            headers: None,
            compat: None,
        },
    );
    assert!(result.is_err());
    assert_eq!(fs::read(&paths.models).unwrap(), before);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn language_setting_is_persisted_without_losing_pi_settings() {
    let (root, paths) = fixture();
    fs::write(&paths.settings, r#"{"theme":"dark","future":true}"#).unwrap();

    set_language(&paths, "zh-CN").unwrap();
    let settings: Value = serde_json::from_slice(&fs::read(&paths.settings).unwrap()).unwrap();
    assert_eq!(settings["piSwitch"]["language"], "zh-CN");
    assert_eq!(settings["theme"], "dark");
    assert_eq!(settings["future"], true);
    assert_eq!(load_snapshot(&paths).unwrap().language, "zh-CN");
    set_fetch_model_metadata(&paths, false).unwrap();
    let defaults = ModelDefaults {
        context_window: Some(256_000),
        input_cost: Some(0.5),
        ..Default::default()
    };
    set_model_defaults(&paths, &defaults).unwrap();
    let snapshot = load_snapshot(&paths).unwrap();
    assert!(!snapshot.fetch_model_metadata);
    assert_eq!(snapshot.model_defaults, defaults);
    assert!(set_language(&paths, "invalid").is_err());
    fs::write(&paths.settings, r#"{"piSwitch":{"language":"invalid"}}"#).unwrap();
    assert!(load_snapshot(&paths).is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn models_dev_catalog_parser_maps_supported_fields_and_skips_unusable_models() {
    assert_eq!(
        parse_provider_catalog(
            "openai-completions",
            &json!({"data":[{"id":"b"},{"id":"a"}]})
        )
        .unwrap(),
        vec!["a", "b"]
    );
    assert_eq!(
        parse_provider_catalog(
            "google-generative-ai",
            &json!({"models":[{"name":"models/gemini"}]})
        )
        .unwrap(),
        vec!["gemini"]
    );
    assert!(parse_provider_catalog("openai-completions", &json!({"models":[]})).is_err());

    let mut shared_a = models_dev_model("shared", 100_000, 10_000, 1.0);
    shared_a["reasoning"] = json!(true);
    shared_a["modalities"]["input"] = json!(["text", "image", "pdf"]);
    let shared_b = models_dev_model("shared", 200_000, 20_000, 2.0);
    let mut unique = models_dev_model("unique", 300_000, 30_000, 3.0);
    unique.as_object_mut().unwrap().remove("cost");
    let catalog = parse_models_dev_catalog(&json!({
        "one": {
            "id": "one",
            "name": "One",
            "env": [],
            "models": {
                "shared": shared_a,
                "unique": unique,
                "no-output": {
                    "id": "no-output",
                    "name": "No output limit",
                    "reasoning": false,
                    "modalities": {"input": ["text"], "output": ["text"]},
                    "limit": {"context": 128_000}
                },
                "audio-only": {
                    "id": "audio-only",
                    "name": "Audio only",
                    "reasoning": false,
                    "modalities": {"input": ["audio"], "output": ["text"]},
                    "limit": {"context": 128_000, "output": 16_384}
                }
            }
        },
        "two": {
            "id": "two",
            "name": "Two",
            "env": [],
            "models": {"shared": shared_b}
        }
    }))
    .unwrap();
    let shared = &catalog.resolve("one", "shared").unwrap().config;
    assert_eq!(shared["contextWindow"], 100_000);
    assert_eq!(shared["maxTokens"], 10_000);
    assert_eq!(shared["input"], json!(["text", "image"]));
    assert_eq!(shared["reasoning"], true);
    assert_eq!(shared["cost"]["cacheRead"], 0.1);
    assert_eq!(shared["cost"]["cacheWrite"], 0.0);
    assert!(catalog.resolve("custom", "shared").is_none());
    let candidates = catalog.ambiguous_candidates("custom", "shared");
    assert_eq!(
        candidates
            .iter()
            .map(|candidate| candidate.provider_id.as_str())
            .collect::<Vec<_>>(),
        ["one", "two"]
    );
    assert_eq!(candidates[0].model.config["cost"]["input"], 1.0);
    assert_eq!(candidates[1].model.config["cost"]["input"], 2.0);
    let unique = catalog.resolve("custom", "unique").unwrap();
    assert_eq!(unique.id, "unique");
    assert_eq!(
        unique.config["cost"],
        json!({
            "input": 0.0,
            "output": 0.0,
            "cacheRead": 0.0,
            "cacheWrite": 0.0
        })
    );
    assert!(catalog.resolve("one", "no-output").is_none());
    assert!(catalog.resolve("one", "audio-only").is_none());
}

#[test]
fn opencode_import_uses_live_catalog_metadata_when_unambiguous() {
    let (root, paths) = fixture();
    write_opencode(
        &paths,
        json!({
            "provider": {
                "custom": {
                    "npm": "@ai-sdk/openai-compatible",
                    "options": {"baseURL": "https://custom.test/v1"},
                    "models": {"unique-live": {"name": "Local display name"}}
                }
            }
        }),
    );
    let source = models_dev_model("unique-live", 262_144, 65_536, 0.6);
    let catalog = parse_models_dev_catalog(&json!({
        "upstream": {
            "id": "upstream",
            "name": "Upstream",
            "env": [],
            "models": {"unique-live": source}
        }
    }))
    .unwrap();

    let summary = import_opencode_with_catalog(&paths, &catalog).unwrap();
    assert_eq!(summary.metadata, 1);
    assert_eq!(summary.unresolved, 0);
    let models: Value = serde_json::from_slice(&fs::read(&paths.models).unwrap()).unwrap();
    let model = &models["providers"]["custom"]["models"][0];
    assert_eq!(model["name"], "Local display name");
    assert_eq!(model["contextWindow"], 262_144);
    assert_eq!(model["maxTokens"], 65_536);
    assert_eq!(model["cost"]["input"], 0.6);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn opencode_import_can_select_providers_and_use_custom_defaults() {
    let (root, paths) = fixture();
    write_opencode(
        &paths,
        json!({
            "provider": {
                "one": {
                    "npm": "@ai-sdk/openai-compatible",
                    "options": {"baseURL": "https://one.test/v1"},
                    "models": {"model-one": {}}
                },
                "two": {
                    "npm": "@ai-sdk/openai-compatible",
                    "options": {"baseURL": "https://two.test/v1"},
                    "models": {"model-two": {}}
                }
            }
        }),
    );
    assert_eq!(list_opencode_providers(&paths).unwrap(), ["one", "two"]);
    let plan = prepare_opencode_import(
        &paths,
        &["two".into()],
        ImportOptions {
            fetch_metadata: false,
            defaults: ModelDefaults {
                context_window: Some(256_000),
                output_cost: Some(3.5),
                ..Default::default()
            },
        },
    )
    .unwrap();
    assert!(plan.ambiguous.is_empty());
    let summary = apply_opencode_import(&paths, plan, &[]).unwrap();
    assert_eq!(summary.providers, 1);
    assert_eq!(summary.defaults, 1);
    assert_eq!(summary.metadata, 0);
    assert_eq!(summary.unresolved, 0);
    let models: Value = serde_json::from_slice(&fs::read(&paths.models).unwrap()).unwrap();
    assert!(models["providers"].get("one").is_none());
    let model = &models["providers"]["two"]["models"][0];
    assert_eq!(model["contextWindow"], 256_000);
    assert_eq!(model["maxTokens"], PI_DEFAULT_MAX_TOKENS);
    assert_eq!(model["cost"]["output"], 3.5);
    assert_eq!(model["cost"]["input"], 0.0);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn opencode_import_applies_the_selected_provider_price() {
    let (root, paths) = fixture();
    write_opencode(
        &paths,
        json!({
            "provider": {
                "custom": {
                    "npm": "@ai-sdk/openai-compatible",
                    "options": {"baseURL": "https://custom.test/v1"},
                    "models": {"shared": {"name": "Shared model"}}
                }
            }
        }),
    );
    let catalog = parse_models_dev_catalog(&json!({
        "one": {
            "id": "one",
            "name": "One",
            "env": [],
            "models": {"shared": models_dev_model("shared", 100_000, 10_000, 1.0)}
        },
        "two": {
            "id": "two",
            "name": "Two",
            "env": [],
            "models": {"shared": models_dev_model("shared", 200_000, 20_000, 2.0)}
        }
    }))
    .unwrap();
    let plan = prepare_opencode_with_catalog(&paths, catalog, &["custom".into()]).unwrap();
    assert_eq!(plan.ambiguous.len(), 1);
    assert_eq!(plan.ambiguous[0].model_id, "shared");
    assert_eq!(
        plan.ambiguous[0]
            .candidates
            .iter()
            .map(|candidate| candidate.provider_id.as_str())
            .collect::<Vec<_>>(),
        ["one", "two"]
    );

    let summary = apply_opencode_import(&paths, plan, &[1]).unwrap();
    assert_eq!(summary.metadata, 1);
    assert_eq!(summary.unresolved, 0);
    let models: Value = serde_json::from_slice(&fs::read(&paths.models).unwrap()).unwrap();
    let model = &models["providers"]["custom"]["models"][0];
    assert_eq!(model["cost"]["input"], 2.0);
    assert_eq!(model["contextWindow"], 200_000);
    assert_eq!(model["maxTokens"], 20_000);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn secret_resolution_matches_pi_interpolation_and_escape_rules() {
    const NAME: &str = "PI_SWITCH_TEST_INTERPOLATION_VALUE";
    env::set_var(NAME, "secret");
    assert_eq!(
        resolve_secret(&format!("prefix-${{{NAME}}}-suffix")).unwrap(),
        Some("prefix-secret-suffix".into())
    );
    assert_eq!(
        resolve_secret(&format!("${NAME}/tail")).unwrap(),
        Some("secret/tail".into())
    );
    assert_eq!(
        resolve_secret("$$cash-$!bang").unwrap(),
        Some("$cash-!bang".into())
    );
    assert!(resolve_secret("!read-secret").is_err());
    env::remove_var(NAME);
}

#[test]
fn fetch_models_uses_the_explicit_catalog_endpoint() {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 2048];
            let size = stream.read(&mut request).unwrap();
            let request = String::from_utf8_lossy(&request[..size]).to_ascii_lowercase();
            let body = if request.starts_with("get /v1/models ") {
                assert!(request.contains("authorization: bearer secret"));
                r#"{"data":[{"id":"model-z"},{"id":"shared"}]}"#
            } else {
                assert!(request.starts_with("get /api.json "));
                r#"{"local":{"id":"local","name":"Local","env":[],"models":{"model-z":{"id":"model-z","name":"Model Z","reasoning":false,"modalities":{"input":["text"],"output":["text"]},"cost":{"input":1.25,"output":5,"cache_read":0.1},"limit":{"context":200000,"output":32000}}}},"one":{"id":"one","name":"One","env":[],"models":{"shared":{"id":"shared","name":"Shared","reasoning":false,"modalities":{"input":["text"],"output":["text"]},"cost":{"input":1,"output":2},"limit":{"context":100000,"output":10000}}}},"two":{"id":"two","name":"Two","env":[],"models":{"shared":{"id":"shared","name":"Shared","reasoning":false,"modalities":{"input":["text"],"output":["text"]},"cost":{"input":2,"output":4},"limit":{"context":200000,"output":20000}}}}}"#
            };
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        }
    });
    let provider = ProviderView {
        id: "local".into(),
        base_url: format!("http://{address}/v1"),
        api: "openai-completions".into(),
        api_key: "secret".into(),
        auth_header: true,
        models: Vec::new(),
        raw: json!({}),
    };
    let fetched = fetch_models_for_test(
        provider,
        ImportOptions {
            fetch_metadata: true,
            defaults: ModelDefaults::default(),
        },
        &format!("http://{address}/api.json"),
    )
    .unwrap();
    assert_eq!(fetched.models.len(), 1);
    assert_eq!(fetched.models[0].id, "model-z");
    assert_eq!(fetched.models[0].config["contextWindow"], 200_000);
    assert_eq!(fetched.models[0].config["maxTokens"], 32_000);
    assert_eq!(fetched.models[0].config["cost"]["input"], 1.25);
    assert!(fetched.models[0].config.get("provider").is_none());
    assert_eq!(fetched.ambiguous.len(), 1);
    assert_eq!(fetched.ambiguous[0].model_id, "shared");
    assert_eq!(fetched.ambiguous[0].candidates.len(), 2);
    assert_eq!(fetched.unavailable, 0);
    server.join().unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0_u8; 2048];
        let size = stream.read(&mut request).unwrap();
        let request = String::from_utf8_lossy(&request[..size]).to_ascii_lowercase();
        assert!(request.starts_with("get /v1/models "));
        let body = r#"{"data":[{"id":"offline-model"}]}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .unwrap();
    });
    let provider = ProviderView {
        id: "local".into(),
        base_url: format!("http://{address}/v1"),
        api: "openai-completions".into(),
        api_key: String::new(),
        auth_header: false,
        models: Vec::new(),
        raw: json!({}),
    };
    let fetched = fetch_models_for_test(
        provider,
        ImportOptions {
            fetch_metadata: false,
            defaults: ModelDefaults {
                context_window: Some(64_000),
                input_cost: Some(0.25),
                ..Default::default()
            },
        },
        "http://127.0.0.1:1/must-not-be-requested",
    )
    .unwrap();
    assert_eq!(fetched.models[0].config["contextWindow"], 64_000);
    assert_eq!(fetched.models[0].config["maxTokens"], PI_DEFAULT_MAX_TOKENS);
    assert_eq!(fetched.models[0].config["cost"]["input"], 0.25);
    server.join().unwrap();
}

#[test]
fn invalid_shapes_and_stale_edits_fail_explicitly() {
    let (root, paths) = fixture();
    fs::write(
            &paths.models,
            r#"{"providers":{"bad":{"baseUrl":"https://example.test","api":"openai-completions","models":[{"id":7}]}}}"#,
        )
        .unwrap();
    assert!(load_snapshot(&paths)
        .unwrap_err()
        .to_string()
        .contains("string ID"));

    fs::write(
        &paths.models,
        r#"{"providers":{"bad":{"headers":{"User-Agent":7}}}}"#,
    )
    .unwrap();
    assert!(load_snapshot(&paths)
        .unwrap_err()
        .to_string()
        .contains("header 'User-Agent' must be a string"));

    fs::write(&paths.models, r#"{"providers":{}}"#).unwrap();
    let result = save_provider(
        &paths,
        Some("deleted-elsewhere"),
        &ProviderDraft {
            id: "deleted-elsewhere".into(),
            base_url: "https://example.test/v1".into(),
            api: Some("openai-completions".into()),
            api_key: String::new(),
            auth_header: true,
            headers: None,
            compat: None,
        },
    );
    assert!(result.unwrap_err().to_string().contains("no longer exists"));
    assert_eq!(
        read_document(&paths.models, json!({})).unwrap(),
        json!({"providers": {}})
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn model_crud_and_provider_copy_preserve_metadata_and_defaults() {
    let (root, paths) = fixture();
    fs::write(
            &paths.models,
            r#"{"rootFuture":4,"providers":{"p":{"baseUrl":"https://example.test/v1","api":"openai-completions","apiKey":"$KEY","providerFuture":true,"models":[{"id":"alpha","name":"alpha","modelFuture":7,"cost":{"input":1},"compat":{"thinkingFormat":"deepseek"}}]}}}"#,
        )
        .unwrap();
    fs::write(
        &paths.settings,
        r#"{"defaultProvider":"p","defaultModel":"alpha","theme":"keep"}"#,
    )
    .unwrap();

    let mut beta = model_draft("beta");
    beta.name = Some("Beta Display".into());
    beta.api = Some("openai-responses".into());
    beta.reasoning = true;
    beta.input.push("image".into());
    beta.context_window = 200_000;
    beta.max_tokens = 32_000;
    save_model(&paths, "p", Some("alpha"), &beta).unwrap();
    save_model(&paths, "p", None, &model_draft("gamma")).unwrap();
    assert_eq!(
        import_models(
            &paths,
            "p",
            &[catalog_model("gamma", 300_000, 40_000, 0.5)],
            false,
        )
        .unwrap(),
        ModelImportSummary {
            added: 0,
            updated: 0
        }
    );
    assert_eq!(
        load_snapshot(&paths).unwrap().providers[0].models[1].context_window,
        Some(128_000)
    );
    assert_eq!(
        import_models(
            &paths,
            "p",
            &[
                catalog_model("gamma", 300_000, 40_000, 0.5),
                catalog_model("delta", 400_000, 50_000, 0.75),
            ],
            true,
        )
        .unwrap(),
        ModelImportSummary {
            added: 1,
            updated: 1
        }
    );
    assert_eq!(duplicate_provider(&paths, "p").unwrap(), "p-copy");

    let models: Value = serde_json::from_slice(&fs::read(&paths.models).unwrap()).unwrap();
    assert_eq!(models["rootFuture"], 4);
    assert_eq!(models["providers"]["p"]["providerFuture"], true);
    assert_eq!(models["providers"]["p"]["models"][0]["id"], "beta");
    assert_eq!(
        models["providers"]["p"]["models"][0]["name"],
        "Beta Display"
    );
    assert_eq!(
        models["providers"]["p"]["models"][0]["api"],
        "openai-responses"
    );
    assert_eq!(models["providers"]["p"]["models"][0]["reasoning"], true);
    assert_eq!(
        models["providers"]["p"]["models"][0]["input"],
        json!(["text", "image"])
    );
    assert_eq!(
        models["providers"]["p"]["models"][0]["contextWindow"],
        200_000
    );
    assert_eq!(models["providers"]["p"]["models"][0]["maxTokens"], 32_000);
    assert_eq!(models["providers"]["p"]["models"][0]["modelFuture"], 7);
    assert_eq!(models["providers"]["p"]["models"][0]["cost"]["input"], 1);
    assert_eq!(
        models["providers"]["p"]["models"][0]["compat"]["thinkingFormat"],
        "deepseek"
    );
    assert_eq!(models["providers"]["p-copy"]["models"][0]["id"], "beta");
    let settings: Value = serde_json::from_slice(&fs::read(&paths.settings).unwrap()).unwrap();
    assert_eq!(settings["defaultModel"], "beta");

    remove_model(&paths, "p", "beta").unwrap();
    let settings: Value = serde_json::from_slice(&fs::read(&paths.settings).unwrap()).unwrap();
    assert!(settings.get("defaultProvider").is_none());
    assert!(settings.get("defaultModel").is_none());
    assert_eq!(settings["theme"], "keep");
    fs::remove_dir_all(root).unwrap();
}
