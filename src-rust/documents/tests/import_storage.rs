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

