use std::collections::BTreeSet;

use serde_json::{json, Value};

use crate::documents::{
    self, CatalogFetch, ImportOptions, ModelCatalog, OpenCodeImportPlan, ProviderView, Result,
    Snapshot,
};

use super::{input::Input, invalid, output, WebCore};

pub(super) struct FetchedModels {
    pub(super) provider: Value,
    pub(super) ids: BTreeSet<String>,
    pub(super) prepared: Option<PreparedModelImport>,
}

#[derive(Clone)]
pub(super) struct PreparedModelImport {
    pub(super) selection_id: u64,
    pub(super) ids: Vec<String>,
    pub(super) update_existing: bool,
    pub(super) fetched: CatalogFetch,
}

pub(super) struct PreparedOpenCode {
    id: u64,
    plan: OpenCodeImportPlan,
}

/// Borrow the key from Pi auth.json when the provider itself carries none, so
/// model fetching still works with auth.json-based key storage.
fn with_auth_key(paths: &crate::documents::Paths, provider: &ProviderView) -> Result<ProviderView> {
    if !provider.api_key.is_empty() {
        return Ok(provider.clone());
    }
    let mut provider = provider.clone();
    if let Some(key) = documents::credential_key(paths, &provider.id)? {
        provider.api_key = key;
    }
    Ok(provider)
}

impl WebCore {
    pub(super) fn fetch_models(&mut self, provider_id: &str) -> Result<Value> {
        let snapshot = documents::load_snapshot(&self.paths)?;
        let provider = provider(&snapshot, provider_id)?;
        let provider = with_auth_key(&self.paths, provider)?;
        let ids = documents::fetch_model_ids(&provider)?;
        let response = json!({
            "providerId": provider_id,
            "total": ids.len(),
            "models": ids.iter().map(|id| json!({
                "id": id,
                "existing": provider.models.iter().any(|model| &model.id == id),
            })).collect::<Vec<_>>(),
        });
        self.fetched_models.insert(
            provider_id.into(),
            FetchedModels {
                provider: provider.raw.clone(),
                ids: ids.into_iter().collect(),
                prepared: None,
            },
        );
        Ok(response)
    }

    pub(super) fn import_models(&mut self, request: &Input<'_>) -> Result<Value> {
        let provider_id = request.string("providerId")?;
        let ids = request.strings("ids")?;
        require_selection(&ids)?;
        let update_existing = request.boolean("updateExisting")?;
        let selection_id = request.optional_integer("selectionId")?;
        let snapshot = documents::load_snapshot(&self.paths)?;
        let provider = provider(&snapshot, provider_id)?;
        let provider = with_auth_key(&self.paths, provider)?;
        let cached = self
            .fetched_models
            .get(provider_id)
            .ok_or_else(|| invalid("model candidates have expired; fetch models again"))?;
        if cached.provider != provider.raw {
            return Err(invalid(
                "provider configuration changed; fetch models again",
            ));
        }
        if ids.iter().any(|id| !cached.ids.contains(id)) {
            return Err(invalid(
                "selection contains an ID not returned by the provider; fetch models again",
            ));
        }

        if let Some(selection_id) = selection_id {
            let prepared = cached
                .prepared
                .as_ref()
                .filter(|prepared| prepared.selection_id == selection_id)
                .ok_or_else(|| {
                    invalid("catalog selection has expired; prepare the import again")
                })?;
            if prepared.ids != ids || prepared.update_existing != update_existing {
                return Err(invalid("model selection changed; prepare the import again"));
            }
            return self.finish_model_import(
                provider_id,
                prepared.fetched.clone(),
                &request.indices("candidateIndices")?,
                update_existing,
            );
        }
        if request.has("candidateIndices") {
            return Err(invalid("candidateIndices requires a current selectionId"));
        }

        let mut fetched = if snapshot.fetch_model_metadata {
            documents::resolve_metadata(provider.clone(), ids.clone(), import_options(&snapshot))?
        } else {
            CatalogFetch {
                models: ids
                    .iter()
                    .map(|id| snapshot.model_defaults.model(id))
                    .collect(),
                ambiguous: Vec::new(),
                unavailable: 0,
                ratio_prices: Default::default(),
                ratio_config_used: false,
                catalog_unreachable: false,
            }
        };
        fetched.apply_ratio_prices();
        if fetched.ambiguous.is_empty() {
            return self.finish_model_import(provider_id, fetched, &[], update_existing);
        }

        let selection_id = self.selection_id();
        let response = json!({
            "requiresSelection": true,
            "selectionId": selection_id,
            "ambiguities": output::ambiguities(&fetched.ambiguous),
            "warning": metadata_warning(&fetched),
        });
        self.fetched_models
            .get_mut(provider_id)
            .expect("provider cache was checked above")
            .prepared = Some(PreparedModelImport {
            selection_id,
            ids,
            update_existing,
            fetched,
        });
        Ok(response)
    }

    fn finish_model_import(
        &mut self,
        provider_id: &str,
        fetched: CatalogFetch,
        candidate_indices: &[usize],
        update_existing: bool,
    ) -> Result<Value> {
        if candidate_indices.len() != fetched.ambiguous.len() {
            return Err(invalid(
                "every ambiguous model requires a catalog selection",
            ));
        }
        let warning = metadata_warning(&fetched);
        let mut models = fetched.models;
        for (ambiguity, index) in fetched.ambiguous.iter().zip(candidate_indices) {
            let candidate = ambiguity.candidates.get(*index).ok_or_else(|| {
                invalid(format!(
                    "catalog selection for '{}' is out of range",
                    ambiguity.model_id
                ))
            })?;
            models.push(ModelCatalog::selected_candidate(
                candidate,
                &ambiguity.model_id,
            ));
        }
        let summary = documents::import_models(&self.paths, provider_id, &models, update_existing)?;
        self.fetched_models.remove(provider_id);
        Ok(json!({
            "snapshot": self.snapshot()?,
            "summary": { "added": summary.added, "updated": summary.updated },
            "warning": warning,
        }))
    }

    pub(super) fn prepare_opencode(&mut self, provider_ids: Vec<String>) -> Result<Value> {
        require_selection(&provider_ids)?;
        let snapshot = documents::load_snapshot(&self.paths)?;
        let plan = documents::prepare_opencode_import(
            &self.paths,
            &provider_ids,
            import_options(&snapshot),
        )?;
        let id = self.selection_id();
        let response = json!({
            "planId": id,
            "providerIds": provider_ids,
            "ambiguities": output::ambiguities(&plan.ambiguous),
        });
        self.opencode_plan = Some(PreparedOpenCode { id, plan });
        Ok(response)
    }

    pub(super) fn import_opencode(&mut self, request: &Input<'_>) -> Result<Value> {
        let plan_id = request.integer("planId")?;
        let indices = request.indices("candidateIndices")?;
        let prepared = self
            .opencode_plan
            .as_ref()
            .filter(|prepared| prepared.id == plan_id)
            .ok_or_else(|| invalid("OpenCode import plan has expired; prepare the import again"))?;
        let summary =
            documents::apply_opencode_import(&self.paths, prepared.plan.clone(), &indices)?;
        self.opencode_plan = None;
        self.fetched_models.clear();
        Ok(json!({
            "snapshot": self.snapshot()?,
            "summary": {
                "providers": summary.providers,
                "models": summary.models,
                "metadata": summary.metadata,
                "defaults": summary.defaults,
                "unresolved": summary.unresolved,
                "changed": summary.changed,
            },
        }))
    }
}

fn provider<'a>(snapshot: &'a Snapshot, id: &str) -> Result<&'a ProviderView> {
    snapshot
        .providers
        .iter()
        .find(|provider| provider.id == id)
        .ok_or_else(|| invalid(format!("provider '{id}' no longer exists")))
}

fn import_options(snapshot: &Snapshot) -> ImportOptions {
    ImportOptions {
        fetch_metadata: snapshot.fetch_model_metadata,
        defaults: snapshot.model_defaults.clone(),
    }
}

fn require_selection(ids: &[String]) -> Result<()> {
    if ids.is_empty() || ids.iter().collect::<BTreeSet<_>>().len() != ids.len() {
        return Err(invalid("select at least one ID, without duplicates"));
    }
    Ok(())
}

fn metadata_warning(fetched: &CatalogFetch) -> Option<String> {
    if fetched.catalog_unreachable {
        return Some(
            "models.dev could not be reached; unmatched models use configured defaults.".into(),
        );
    }
    (fetched.unavailable > 0).then(|| {
        format!(
            "{} model(s) had no matching metadata and use configured defaults.",
            fetched.unavailable,
        )
    })
}
