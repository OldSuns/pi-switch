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
        for _ in 0..3 {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 2048];
            let size = stream.read(&mut request).unwrap();
            let request = String::from_utf8_lossy(&request[..size]).to_ascii_lowercase();
            let body = if request.starts_with("get /v1/models ") {
                assert!(request.contains("authorization: bearer secret"));
                r#"{"data":[{"id":"model-z"},{"id":"shared"}]}"#
            } else if request.starts_with("get /api/ratio_config ") {
                r#"{"success":true,"data":{"model_ratio":{"model-z":0.5},"completion_ratio":{"model-z":2.0}}}"#
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
    let provider = provider_view(&address, "secret", true);
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
    // ratio_config prices are computed at the network layer; the app overlays
    // them onto the catalog metadata when displaying the import list.
    assert!(fetched.ratio_config_used);
    let ratio = &fetched.ratio_prices["model-z"];
    assert_eq!(ratio.input, 1.0);
    assert_eq!(ratio.output, 2.0);
    assert!(!fetched.ratio_prices.contains_key("shared"));
    server.join().unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        for _ in 0..3 {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 2048];
            let size = stream.read(&mut request).unwrap();
            let request = String::from_utf8_lossy(&request[..size]).to_ascii_lowercase();
            if request.starts_with("get /api/ratio_config ")
                || request.starts_with("get /api/pricing ")
            {
                // No gateway pricing endpoints on this gateway.
                write!(
                    stream,
                    "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                )
                .unwrap();
            } else {
                assert!(request.starts_with("get /v1/models "));
                let body = r#"{"data":[{"id":"offline-model"}]}"#;
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                )
                .unwrap();
            }
        }
    });
    let provider = provider_view(&address, "", false);
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
    assert!(!fetched.ratio_config_used);
    assert!(fetched.ratio_prices.is_empty());
    server.join().unwrap();
}

#[test]
fn fetch_models_falls_back_to_defaults_when_a_model_has_no_models_dev_metadata() {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        for _ in 0..4 {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 2048];
            let size = stream.read(&mut request).unwrap();
            let request = String::from_utf8_lossy(&request[..size]).to_ascii_lowercase();
            let body = if request.starts_with("get /v1/models ") {
                r#"{"data":[{"id":"ghost-model"},{"id":"model-z"}]}"#
            } else if request.starts_with("get /api/ratio_config ")
                || request.starts_with("get /api/pricing ")
            {
                // No gateway pricing endpoint.
                write!(
                    stream,
                    "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                )
                .unwrap();
                continue;
            } else {
                assert!(request.starts_with("get /api.json "));
                // Only model-z has models.dev metadata; ghost-model does not.
                r#"{"local":{"id":"local","name":"Local","env":[],"models":{"model-z":{"id":"model-z","name":"Model Z","reasoning":false,"modalities":{"input":["text"],"output":["text"]},"cost":{"input":1.25,"output":5},"limit":{"context":200000,"output":32000}}}}}"#
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
    let provider = provider_view(&address, "", false);
    let fetched = fetch_models_for_test(
        provider,
        ImportOptions {
            fetch_metadata: true,
            defaults: ModelDefaults::default(),
        },
        &format!("http://{address}/api.json"),
    )
    .unwrap();
    // Both models are imported: model-z resolved from the catalog, ghost-model
    // falling back to defaults rather than being silently dropped.
    assert_eq!(fetched.models.len(), 2);
    assert_eq!(fetched.unavailable, 1);
    assert!(fetched.ambiguous.is_empty());
    let ghost = fetched
        .models
        .iter()
        .find(|model| model.id == "ghost-model")
        .expect("ghost-model kept via default fallback");
    assert_eq!(ghost.config["contextWindow"], PI_DEFAULT_CONTEXT_WINDOW);
    assert_eq!(ghost.config["cost"]["input"], 0.0);
    server.join().unwrap();
}

#[test]
fn fetch_models_uses_defaults_when_models_dev_is_unreachable() {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        for _ in 0..3 {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 2048];
            let size = stream.read(&mut request).unwrap();
            let request = String::from_utf8_lossy(&request[..size]).to_ascii_lowercase();
            if request.starts_with("get /v1/models ") {
                let body = r#"{"data":[{"id":"alpha"},{"id":"beta"}]}"#;
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                )
                .unwrap();
            } else if request.starts_with("get /api/ratio_config ")
                || request.starts_with("get /api/pricing ")
            {
                write!(
                    stream,
                    "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                )
                .unwrap();
            } else {
                // models.dev catalog is unreachable (503).
                assert!(request.starts_with("get /api.json "));
                write!(
                    stream,
                    "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                )
                .unwrap();
            }
        }
    });
    let provider = provider_view(&address, "", false);
    let fetched = fetch_models_for_test(
        provider,
        ImportOptions {
            fetch_metadata: true,
            defaults: ModelDefaults::default(),
        },
        &format!("http://{address}/api.json"),
    )
    .unwrap();
    // The flow does NOT abort: both models are imported with default metadata.
    assert_eq!(fetched.models.len(), 2);
    assert_eq!(fetched.unavailable, 2);
    assert!(fetched.ambiguous.is_empty());
    assert!(fetched.catalog_unreachable);
    for model in &fetched.models {
        assert_eq!(model.config["contextWindow"], PI_DEFAULT_CONTEXT_WINDOW);
        assert_eq!(model.config["cost"]["input"], 0.0);
    }
    server.join().unwrap();
}

#[test]
fn invalid_shapes_and_stale_edits_fail_explicitly() {
    let (_root, paths) = fixture();
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

    fs::write(
        &paths.models,
        r#"{"providers":{"bad":{"headers":{"User-Agent":"one","user-agent":"two"}}}}"#,
    )
    .unwrap();
    assert!(load_snapshot(&paths)
        .unwrap_err()
        .to_string()
        .contains("multiple User-Agent headers with different casing"));

    fs::write(&paths.models, r#"{"providers":{"bad":{"compat":[]}}}"#).unwrap();
    assert!(load_snapshot(&paths)
        .unwrap_err()
        .to_string()
        .contains("compat must be an object"));

    fs::write(
        &paths.models,
        r#"{"providers":{"bad":{"compat":{"sendSessionAffinityHeaders":"yes"}}}}"#,
    )
    .unwrap();
    assert!(load_snapshot(&paths)
        .unwrap_err()
        .to_string()
        .contains("compat.sendSessionAffinityHeaders must be a boolean"));

    fs::write(&paths.models, r#"{"providers":{}}"#).unwrap();
    let result = save_provider(
        &paths,
        Some("deleted-elsewhere"),
        &ProviderDraft {
            id: "deleted-elsewhere".into(),
            in_pi: true,
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
}

#[test]
fn model_crud_and_provider_copy_preserve_metadata_and_defaults() {
    let (_root, paths) = fixture();
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
    beta.context_window = Some(200_000);
    beta.max_tokens = Some(32_000);
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

    let models = read_json(&paths.models);
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
    let settings = read_json(&paths.settings);
    assert_eq!(settings["defaultModel"], "beta");

    remove_model(&paths, "p", "beta").unwrap();
    let settings = read_json(&paths.settings);
    assert!(settings.get("defaultProvider").is_none());
    assert!(settings.get("defaultModel").is_none());
    assert_eq!(settings["theme"], "keep");
}

#[test]
fn parse_ratio_config_handles_envelope_and_failure() {
    // wrapped in { success, data }
    let body = json!({
        "success": true,
        "data": {
            "model_ratio": {"gpt-4": 2.5},
            "completion_ratio": {"gpt-4": 1.5},
            "cache_ratio": {"gpt-4": 0.5},
            "create_cache_ratio": {"gpt-4": 0.25}
        }
    });
    let ratios = parse_ratio_config(&body);
    assert_eq!(ratios.model_ratio.get("gpt-4"), Some(&2.5));
    assert_eq!(ratios.completion_ratio.get("gpt-4"), Some(&1.5));
    assert_eq!(ratios.cache_ratio.get("gpt-4"), Some(&0.5));
    assert_eq!(ratios.create_cache_ratio.get("gpt-4"), Some(&0.25));

    // bare object (no envelope)
    let ratios = parse_ratio_config(&json!({"model_ratio": {"claude": 1.0}}));
    assert_eq!(ratios.model_ratio.get("claude"), Some(&1.0));
    assert!(ratios.completion_ratio.is_empty());

    // success:false → empty
    let ratios = parse_ratio_config(&json!({"success": false, "data": {"model_ratio": {"x": 1}}}));
    assert!(ratios.model_ratio.is_empty());

    // non-object → empty
    assert!(parse_ratio_config(&json!("nope")).model_ratio.is_empty());
    assert!(parse_ratio_config(&json!(null)).model_ratio.is_empty());

    // missing keys → empty maps, no panic
    assert!(parse_ratio_config(&json!({})).model_ratio.is_empty());

    // non-numeric values ignored
    let ratios = parse_ratio_config(&json!({"model_ratio": {"ok": 1.0, "bad": "x", "nan": null}}));
    assert_eq!(ratios.model_ratio.len(), 1);
    assert_eq!(ratios.model_ratio.get("ok"), Some(&1.0));
}

#[test]
fn parse_pricing_converts_array_format_to_ratios() {
    // /api/pricing returns an array of per-model objects.
    let body = json!({
        "success": true,
        "data": [
            {"model_name": "glm-5.2", "model_ratio": 0.4, "completion_ratio": 3.75, "cache_ratio": 0.25},
            {"model_name": "deepseek-v4-pro", "model_ratio": 0.175, "completion_ratio": 2.0, "cache_ratio": 0.571428571429}
        ]
    });
    let ratios = parse_pricing(&body);
    assert_eq!(ratios.model_ratio.get("glm-5.2"), Some(&0.4));
    assert_eq!(ratios.completion_ratio.get("glm-5.2"), Some(&3.75));
    assert_eq!(ratios.cache_ratio.get("glm-5.2"), Some(&0.25));
    // create_cache_ratio is absent in /api/pricing → not populated.
    assert!(ratios.create_cache_ratio.is_empty());
    assert_eq!(ratios.model_ratio.get("deepseek-v4-pro"), Some(&0.175));

    // success:false → empty
    let ratios = parse_pricing(
        &json!({"success": false, "data": [{"model_name": "x", "model_ratio": 1.0}]}),
    );
    assert!(ratios.model_ratio.is_empty());

    // non-object / non-array data → empty
    assert!(parse_pricing(&json!("nope")).model_ratio.is_empty());
    assert!(parse_pricing(&json!(null)).model_ratio.is_empty());
    assert!(parse_pricing(&json!({})).model_ratio.is_empty());

    // entries missing model_name or non-numeric ratios are skipped
    let ratios = parse_pricing(&json!({
        "data": [
            {"model_ratio": 1.0},
            {"model_name": "ok", "model_ratio": 2.0, "completion_ratio": "bad"}
        ]
    }));
    assert_eq!(ratios.model_ratio.len(), 1);
    assert_eq!(ratios.model_ratio.get("ok"), Some(&2.0));
    assert!(ratios.completion_ratio.is_empty());
}

#[test]
fn find_ratio_matches_exact_case_insensitive_and_prefix() {
    let mut map = std::collections::BTreeMap::new();
    map.insert("gpt-4".into(), 2.0);
    map.insert("claude-3".into(), 1.0);

    // exact
    assert_eq!(find_ratio("gpt-4", &map), Some(2.0));
    // case-insensitive
    assert_eq!(find_ratio("GPT-4", &map), Some(2.0));
    // prefix match (version tags)
    assert_eq!(find_ratio("gpt-4-turbo-2024", &map), Some(2.0));
    // no match
    assert_eq!(find_ratio("gemini", &map), None);
}

#[test]
fn round_price_eliminates_floating_point_artifacts() {
    // The classic 0.1 × 1.5 × 2.0 produces 0.30000000000000004 in f64.
    assert_eq!(round_price(0.1 * 1.5 * 2.0), 0.3);
    // Other artifact-prone products.
    assert_eq!(round_price(0.3 * 0.7 * 2.0), 0.42);
    assert_eq!(round_price(0.07 * 1.0 * 2.0), 0.14);
    assert_eq!(round_price(0.21 * 1.0 * 2.0), 0.42);
    // Already-clean values pass through unchanged.
    assert_eq!(round_price(0.0), 0.0);
    assert_eq!(round_price(1.5), 1.5);
    assert_eq!(round_price(0.25), 0.25);
    // High-precision ratios are preserved to 6 decimals.
    assert_eq!(round_price(0.571428571429 * 0.175 * 2.0), 0.2);
}

#[test]
fn compute_ratio_prices_rounds_floating_point_artifacts() {
    // model_ratio=0.1, completion_ratio=1.5: without rounding the output
    // would be 0.30000000000000004.
    let ratios = Ratios {
        model_ratio: [("m1".into(), 0.1_f64)].into_iter().collect(),
        completion_ratio: [("m1".into(), 1.5_f64)].into_iter().collect(),
        cache_ratio: [("m1".into(), 0.5_f64)].into_iter().collect(),
        create_cache_ratio: [("m1".into(), 0.25_f64)].into_iter().collect(),
    };
    let ids = vec!["m1".to_string()];
    let prices = compute_ratio_prices(&ids, &ratios);
    let cost = &prices["m1"];
    assert_eq!(cost.input, 0.2);
    assert_eq!(cost.output, 0.3);
    assert_eq!(cost.cache_read, 0.1);
    assert_eq!(cost.cache_write, 0.05);

    // to_cost_json must serialize clean values — no floating-point artifacts.
    let json = cost.to_cost_json();
    let serialized = serde_json::to_string(&json).unwrap();
    assert_eq!(serialized, r#"{"cacheRead":0.1,"cacheWrite":0.05,"input":0.2,"output":0.3}"#);
    assert!(!serialized.contains("00000000"));

    // A second model with artifact-prone ratios: 0.3 × 0.7 × 2.0 = 0.42.
    let ratios = Ratios {
        model_ratio: [
            ("m2".into(), 0.3_f64),
            ("m3".into(), 0.07_f64),
        ]
        .into_iter()
        .collect(),
        completion_ratio: [("m2".into(), 0.7_f64)].into_iter().collect(),
        ..Default::default()
    };
    let ids = vec!["m2".to_string(), "m3".to_string()];
    let prices = compute_ratio_prices(&ids, &ratios);
    assert_eq!(prices["m2"].output, 0.42);
    assert_eq!(prices["m3"].output, 0.14);
    // Serialized JSON for m2 must be artifact-free.
    let serialized = serde_json::to_string(&prices["m2"].to_cost_json()).unwrap();
    assert!(!serialized.contains("00000000"));
}

#[test]
fn model_cost_fields_round_trip_and_preserve_through_untouched_edits() {
    let (_root, paths) = fixture();
    fs::write(
        &paths.models,
        r#"{"providers":{"p":{"baseUrl":"https://e.test/v1","api":"openai-completions","apiKey":"$K","models":[{"id":"m","contextWindow":128000,"maxTokens":16384}]}}}"#,
    )
    .unwrap();

    // A form-style draft carrying explicit cost writes all four keys.
    let mut priced = model_draft("m");
    priced.input_cost = Some(1.0);
    priced.output_cost = Some(2.0);
    priced.cache_read_cost = Some(0.5);
    priced.cache_write_cost = Some(0.0);
    save_model(&paths, "p", Some("m"), &priced).unwrap();
    let models = read_json(&paths.models);
    assert_eq!(models["providers"]["p"]["models"][0]["cost"]["input"], 1.0);
    assert_eq!(models["providers"]["p"]["models"][0]["cost"]["output"], 2.0);
    assert_eq!(models["providers"]["p"]["models"][0]["cost"]["cacheRead"], 0.5);
    assert_eq!(models["providers"]["p"]["models"][0]["cost"]["cacheWrite"], 0.0);

    // A draft that leaves cost unset (None) must NOT strip the existing cost.
    let mut renamed = model_draft("m2");
    renamed.context_window = Some(128_000);
    renamed.max_tokens = Some(16_384);
    save_model(&paths, "p", Some("m"), &renamed).unwrap();
    let models = read_json(&paths.models);
    assert_eq!(models["providers"]["p"]["models"][0]["id"], "m2");
    assert_eq!(models["providers"]["p"]["models"][0]["cost"]["input"], 1.0);
}
