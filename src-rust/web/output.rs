use std::path::Path;

use serde_json::{json, Value};

use crate::documents::{
    CatalogAmbiguity, ModelDefaults, ModelView, Paths, ProviderView, SessionPreview, Snapshot,
};

pub(super) fn snapshot(snapshot: &Snapshot, paths: &Paths, sessions_root: &Path) -> Value {
    // A corrupt auth.json must not break the snapshot; the delete dialog just
    // skips the credential question then.
    let auth_ids: std::collections::BTreeSet<String> = crate::documents::credential_ids(paths)
        .unwrap_or_default()
        .into_iter()
        .collect();
    json!({
        "version": env!("CARGO_PKG_VERSION"),
        "apiTypes": crate::documents::API_TYPES,
        "providers": snapshot.providers.iter().map(|view| provider(view, auth_ids.contains(&view.id), snapshot.key_storage)).collect::<Vec<_>>(),
        "ordering": snapshot.ordering.to_json(),
        "defaultProvider": snapshot.default_provider,
        "defaultModel": snapshot.default_model,
        "language": snapshot.language,
        "fetchModelMetadata": snapshot.fetch_model_metadata,
        "keyStorage": snapshot.key_storage.as_str(),
        "checkUpdates": snapshot.check_updates,
        "modelDefaults": defaults(&snapshot.model_defaults),
        "paths": {
            "providers": snapshot.providers_path,
            "piModels": snapshot.pi_models_path,
            "piSettings": snapshot.pi_settings_path,
            "appSettings": snapshot.app_settings_path,
            "sessions": sessions_root.display().to_string(),
            "backups": paths.backups.display().to_string(),
            "opencode": paths.opencode.display().to_string(),
        },
        "warning": snapshot.warning,
    })
}

fn provider(
    provider: &ProviderView,
    has_auth: bool,
    key_storage: crate::documents::KeyStorage,
) -> Value {
    let key_source = if provider.api_key.is_empty() {
        None
    } else if provider.in_pi && has_auth {
        Some("auth.json")
    } else if provider.in_pi
        && provider
            .raw
            .get("apiKey")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.is_empty())
    {
        Some(match key_storage {
            crate::documents::KeyStorage::AuthJson => "pi-switch",
            crate::documents::KeyStorage::ModelsJson => "models.json",
        })
    } else {
        None
    };
    json!({
        "id": provider.id,
        "inPi": provider.in_pi,
        "baseUrl": provider.base_url,
        "api": (!provider.api.is_empty()).then_some(&provider.api),
        "apiKey": provider.api_key,
        "apiKeySource": key_source,
        "hasAuth": has_auth,
        "authHeader": provider.auth_header,
        "headers": provider.raw.get("headers"),
        "compat": provider.raw.get("compat"),
        "models": provider.models.iter().map(model).collect::<Vec<_>>(),
    })
}

fn model(model: &ModelView) -> Value {
    json!({
        "id": model.id,
        "name": model.name,
        "api": model.api,
        "reasoning": model.reasoning,
        "input": model.input,
        "contextWindow": model.context_window,
        "maxTokens": model.max_tokens,
        "inputCost": model.input_cost,
        "outputCost": model.output_cost,
        "cacheReadCost": model.cache_read_cost,
        "cacheWriteCost": model.cache_write_cost,
        "thinkingLevelMap": model.thinking_level_map,
    })
}

fn defaults(defaults: &ModelDefaults) -> Value {
    json!({
        "contextWindow": defaults.context_window,
        "maxTokens": defaults.max_tokens,
        "inputCost": defaults.input_cost,
        "outputCost": defaults.output_cost,
        "cacheReadCost": defaults.cache_read_cost,
        "cacheWriteCost": defaults.cache_write_cost,
    })
}

pub(super) fn preview(id: &str, preview: &SessionPreview) -> Value {
    json!({
        "id": id,
        "activeLeafId": preview.active_leaf_id,
        "activeMessageId": preview.active_message_id,
        "branchPoints": preview.branch_points,
        "messages": preview.messages.iter().map(|message| json!({
            "id": message.id,
            "role": message.role,
            "text": message.text,
            "html": super::markdown::render(&message.text),
            "label": message.label,
            "tree": {
                "parentId": message.tree.parent_id,
                "level": message.tree.level,
                "indent": message.tree.indent,
                "showConnector": message.tree.show_connector,
                "isLast": message.tree.is_last,
                "activePath": message.tree.active_path,
                "hasChildren": message.tree.has_children,
                "gutters": message.tree.gutters.iter().map(|gutter| json!({
                    "position": gutter.position, "show": gutter.show,
                })).collect::<Vec<_>>(),
            },
        })).collect::<Vec<_>>(),
    })
}

pub(super) fn ambiguities(ambiguities: &[CatalogAmbiguity]) -> Vec<Value> {
    ambiguities
        .iter()
        .map(|ambiguity| {
            json!({
                "providerId": ambiguity.provider_id,
                "modelId": ambiguity.model_id,
                "candidates": ambiguity.candidates.iter().map(|candidate| json!({
                    "providerId": candidate.provider_id,
                    "id": candidate.model.id,
                    "config": candidate.model.config,
                })).collect::<Vec<_>>(),
            })
        })
        .collect()
}
