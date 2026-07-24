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
    shared_a["reasoning_options"] =
        json!([{"type":"effort","values":["low","medium","high","xhigh","max"]}]);
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
    assert_eq!(
        shared["thinkingLevelMap"],
        json!({
            "minimal": null, "low": "low", "medium": "medium",
            "high": "high", "xhigh": "xhigh", "max": "max"
        })
    );
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
    assert!(unique.config.get("thinkingLevelMap").is_none());
    assert!(catalog.resolve("one", "no-output").is_none());
    assert!(catalog.resolve("one", "audio-only").is_none());
}

#[test]
fn thinking_level_map_maps_effort_values_and_omits_unsupported() {
    // reasoning:false (helper default) → no map
    let no_reasoning = parse_models_dev_catalog(&json!({
        "p": {"id":"p","name":"P","env":[],"models":{
            "m": models_dev_model("m", 128_000, 16_384, 0.0)
        }}
    }))
    .unwrap();
    assert!(no_reasoning
        .resolve("p", "m")
        .unwrap()
        .config
        .get("thinkingLevelMap")
        .is_none());

    // effort values with "none" → off:"none", unsupported graded levels → null
    let with_none = parse_models_dev_catalog(&json!({
        "p": {"id":"p","name":"P","env":[],"models":{
            "m": {
                "id": "m", "name": "m", "reasoning": true,
                "modalities": {"input": ["text"], "output": ["text"]},
                "limit": {"context": 128_000, "output": 16_384},
                "reasoning_options": [{"type":"effort","values":["none","low","medium","high"]}]
            }
        }}
    }))
    .unwrap();
    assert_eq!(
        with_none.resolve("p", "m").unwrap().config["thinkingLevelMap"],
        json!({"off":"none","minimal":null,"low":"low","medium":"medium","high":"high","xhigh":null,"max":null})
    );

    // anthropic-style: no "none" → off omitted (pi closes thinking by omitting the param)
    let anthropic_style = parse_models_dev_catalog(&json!({
        "p": {"id":"p","name":"P","env":[],"models":{
            "m": {
                "id": "m", "name": "m", "reasoning": true,
                "modalities": {"input": ["text"], "output": ["text"]},
                "limit": {"context": 128_000, "output": 16_384},
                "reasoning_options": [{"type":"effort","values":["low","medium","high","xhigh","max"]}]
            }
        }}
    }))
    .unwrap();
    assert!(
        anthropic_style.resolve("p", "m").unwrap().config["thinkingLevelMap"]
            .as_object()
            .unwrap()
            .get("off")
            .is_none()
    );

    // toggle-only → no map
    let toggle_only = parse_models_dev_catalog(&json!({
        "p": {"id":"p","name":"P","env":[],"models":{
            "m": {
                "id": "m", "name": "m", "reasoning": true,
                "modalities": {"input": ["text"], "output": ["text"]},
                "limit": {"context": 128_000, "output": 16_384},
                "reasoning_options": [{"type":"toggle"}]
            }
        }}
    }))
    .unwrap();
    assert!(toggle_only
        .resolve("p", "m")
        .unwrap()
        .config
        .get("thinkingLevelMap")
        .is_none());

    // effort with only "none" (no graded level) → no map
    let only_none = parse_models_dev_catalog(&json!({
        "p": {"id":"p","name":"P","env":[],"models":{
            "m": {
                "id": "m", "name": "m", "reasoning": true,
                "modalities": {"input": ["text"], "output": ["text"]},
                "limit": {"context": 128_000, "output": 16_384},
                "reasoning_options": [{"type":"effort","values":["none"]}]
            }
        }}
    }))
    .unwrap();
    assert!(only_none
        .resolve("p", "m")
        .unwrap()
        .config
        .get("thinkingLevelMap")
        .is_none());

    // missing reasoning_options on a reasoning model → no map
    let missing = parse_models_dev_catalog(&json!({
        "p": {"id":"p","name":"P","env":[],"models":{
            "m": {
                "id": "m", "name": "m", "reasoning": true,
                "modalities": {"input": ["text"], "output": ["text"]},
                "limit": {"context": 128_000, "output": 16_384}
            }
        }}
    }))
    .unwrap();
    assert!(missing
        .resolve("p", "m")
        .unwrap()
        .config
        .get("thinkingLevelMap")
        .is_none());
}

#[test]
fn reasoning_models_use_most_detailed_thinking_level_map() {
    // Same model under two providers: one lists 4 effort levels, one lists 2.
    // Both listings unify to the more detailed (4-level) map.
    let catalog = parse_models_dev_catalog(&json!({
        "rich": {"id":"rich","name":"Rich","env":[],"models":{
            "glm": {
                "id": "glm", "name": "glm", "reasoning": true,
                "modalities": {"input": ["text"], "output": ["text"]},
                "limit": {"context": 128_000, "output": 16_384},
                "reasoning_options": [{"type":"effort","values":["none","low","medium","high"]}]
            }
        }},
        "thin": {"id":"thin","name":"Thin","env":[],"models":{
            "glm": {
                "id": "glm", "name": "glm", "reasoning": true,
                "modalities": {"input": ["text"], "output": ["text"]},
                "limit": {"context": 128_000, "output": 16_384},
                "reasoning_options": [{"type":"effort","values":["high","max"]}]
            }
        }}
    }))
    .unwrap();
    let rich = catalog.resolve("rich", "glm").unwrap();
    let thin = catalog.resolve("thin", "glm").unwrap();
    // Both carry the richer map: off→none, low/medium/high set, max null.
    assert_eq!(rich.config["thinkingLevelMap"]["off"], "none");
    assert_eq!(rich.config["thinkingLevelMap"]["low"], "low");
    assert_eq!(thin.config["thinkingLevelMap"]["off"], "none");
    assert_eq!(thin.config["thinkingLevelMap"]["low"], "low");
    assert_eq!(thin.config["thinkingLevelMap"]["high"], "high");
    // thin's own ["max"] is dropped — the unified map reflects the richer source.
    assert_eq!(thin.config["thinkingLevelMap"]["max"], Value::Null);

    // A reasoning model with no sibling carrying effort stays unmapped.
    let lonely = parse_models_dev_catalog(&json!({
        "only": {"id":"only","name":"Only","env":[],"models":{
            "solo": {
                "id": "solo", "name": "solo", "reasoning": true,
                "modalities": {"input": ["text"], "output": ["text"]},
                "limit": {"context": 128_000, "output": 16_384}
            }
        }}
    }))
    .unwrap();
    assert!(lonely
        .resolve("only", "solo")
        .unwrap()
        .config
        .get("thinkingLevelMap")
        .is_none());
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
    let mut source = models_dev_model("unique-live", 262_144, 65_536, 0.6);
    source["reasoning"] = json!(true);
    source["reasoning_options"] =
        json!([{"type":"effort","values":["none","low","medium","high","xhigh","max"]}]);
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
    assert_eq!(model["reasoning"], true);
    assert_eq!(model["contextWindow"], 262_144);
    assert_eq!(model["maxTokens"], 65_536);
    assert_eq!(model["cost"]["input"], 0.6);
    assert_eq!(model["thinkingLevelMap"]["off"], "none");
    assert_eq!(model["thinkingLevelMap"]["max"], "max");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn opencode_import_ignores_empty_model_name() {
    let (root, paths) = fixture();
    write_opencode(
        &paths,
        json!({
            "provider": {
                "caroline": {
                    "npm": "@ai-sdk/openai-compatible",
                    "models": {"deepseek-v4-pro": {"name": ""}}
                }
            }
        }),
    );
    let mut catalog = ModelCatalog::default();
    catalog.insert(
        "caroline".into(),
        vec![catalog_model("deepseek-v4-pro", 128_000, 16_384, 0.0)],
    );

    import_opencode_with_catalog(&paths, &catalog).unwrap();

    let models: Value = serde_json::from_slice(&fs::read(&paths.models).unwrap()).unwrap();
    assert_eq!(
        models["providers"]["caroline"]["models"][0]["name"],
        "deepseek-v4-pro"
    );
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

