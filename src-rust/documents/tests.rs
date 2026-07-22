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
            headers: Some(json!({"x-team-key":"$TEAM_KEY"})),
            compat: Some(json!({"supportsDeveloperRole":false})),
        },
    )
    .unwrap();
    assert_eq!(import_models(&paths, "new", &["added".into()]).unwrap(), 1);

    let models: Value = serde_json::from_slice(&fs::read(&paths.models).unwrap()).unwrap();
    assert_eq!(models["schemaVersion"], 9);
    assert_eq!(models["providers"]["new"]["future"], true);
    assert_eq!(
        models["providers"]["new"]["headers"]["x-team-key"],
        "$TEAM_KEY"
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
    assert!(list_backups(&paths).unwrap().len() >= 2);
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
fn catalog_parser_is_protocol_specific() {
    assert_eq!(
        parse_catalog(
            "openai-completions",
            &json!({"data":[{"id":"b"},{"id":"a"}]})
        )
        .unwrap(),
        vec!["a", "b"]
    );
    assert_eq!(
        parse_catalog(
            "google-generative-ai",
            &json!({"models":[{"name":"models/gemini"}]})
        )
        .unwrap(),
        vec!["gemini"]
    );
    assert!(parse_catalog("openai-completions", &json!({"models":[]})).is_err());
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
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0_u8; 2048];
        let size = stream.read(&mut request).unwrap();
        let request = String::from_utf8_lossy(&request[..size]).to_ascii_lowercase();
        assert!(request.starts_with("get /v1/models "));
        assert!(request.contains("authorization: bearer secret"));
        let body = r#"{"data":[{"id":"model-z"}]}"#;
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
        api_key: "secret".into(),
        auth_header: true,
        models: Vec::new(),
        raw: json!({}),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    assert_eq!(
        runtime.block_on(fetch_models(provider)).unwrap(),
        vec!["model-z"]
    );
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
        import_models(&paths, "p", &["gamma".into(), "delta".into()]).unwrap(),
        1
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
