use super::*;
use std::{
    env, fs,
    path::PathBuf,
    process,
    sync::atomic::{AtomicUsize, Ordering},
};

static FIXTURE_ID: AtomicUsize = AtomicUsize::new(0);

struct TempDir(PathBuf);
impl std::ops::Deref for TempDir {
    type Target = PathBuf;
    fn deref(&self) -> &PathBuf {
        &self.0
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn fixture() -> (TempDir, Paths) {
    let root = env::temp_dir().join(format!(
        "pi-switch-test-{}-{}-{}",
        process::id(),
        now_millis(),
        FIXTURE_ID.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(root.join(".pi/agent")).unwrap();
    let paths = Paths::from_home(&root);
    (TempDir(root), paths)
}

fn model_draft(id: &str) -> ModelDraft {
    ModelDraft {
        id: id.into(),
        name: Some(id.into()),
        api: None,
        reasoning: false,
        input: vec!["text".into()],
        context_window: Some(128_000),
        max_tokens: Some(16_384),
        input_cost: None,
        output_cost: None,
        cache_read_cost: None,
        cache_write_cost: None,
        thinking_level_map: None,
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

fn read_json(path: impl AsRef<std::path::Path>) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn provider_view(address: &std::net::SocketAddr, api_key: &str, auth_header: bool) -> ProviderView {
    ProviderView {
        id: "local".into(),
        in_pi: true,
        base_url: format!("http://{address}/v1"),
        api: "openai-completions".into(),
        api_key: api_key.into(),
        auth_header,
        models: Vec::new(),
        raw: json!({}),
    }
}

fn write_opencode(paths: &Paths, value: Value) {
    fs::create_dir_all(paths.opencode.parent().unwrap()).unwrap();
    fs::write(&paths.opencode, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
}

include!("tests/import_storage.rs");
include!("tests/catalog_import.rs");
include!("tests/network_models.rs");
include!("tests/update_check.rs");
