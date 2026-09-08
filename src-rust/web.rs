mod files;
mod imports;
mod input;
mod markdown;
mod output;

#[cfg(test)]
mod tests;

use std::{collections::BTreeMap, path::PathBuf};

use serde_json::{json, Value};

use crate::documents::{self, AppError, Paths, Result};
use imports::{FetchedModels, PreparedOpenCode};
use input::Input;

pub(crate) struct WebCore {
    paths: Paths,
    sessions_root: PathBuf,
    fetched_models: BTreeMap<String, FetchedModels>,
    opencode_plan: Option<PreparedOpenCode>,
    next_selection_id: u64,
}

impl WebCore {
    #[cfg(not(test))]
    pub(crate) fn discover() -> Result<Self> {
        Ok(Self::new(Paths::discover()?, documents::sessions_root()?))
    }

    fn new(paths: Paths, sessions_root: PathBuf) -> Self {
        Self {
            paths,
            sessions_root,
            fetched_models: BTreeMap::new(),
            opencode_plan: None,
            next_selection_id: 0,
        }
    }

    pub(crate) fn request(&mut self, request: &str) -> Result<String> {
        let value: Value = serde_json::from_str(request)
            .map_err(|error| invalid(format!("invalid Web request JSON: {error}")))?;
        Ok(self.dispatch(&value)?.to_string())
    }

    fn dispatch(&mut self, value: &Value) -> Result<Value> {
        let request = Input::new(value, "request")?;
        let action = request.string("action")?;
        request.check_fields(action_fields(action)?)?;
        match action {
            "snapshot" => self.snapshot(),
            "providers.sort" | "models.sort" => {
                let sort = documents::ProfileSort::parse(request.string("value")?)?;
                let list = if action == "providers.sort" {
                    documents::ProfileList::Providers
                } else {
                    documents::ProfileList::Models(request.string("providerId")?)
                };
                documents::set_profile_sort(&self.paths, list, sort)?;
                self.snapshot_result()
            }
            "providers.reorder" | "models.reorder" => {
                let ids = request.strings("ids")?;
                let list = if action == "providers.reorder" {
                    documents::ProfileList::Providers
                } else {
                    documents::ProfileList::Models(request.string("providerId")?)
                };
                documents::reorder_profiles(&self.paths, list, &ids)?;
                self.snapshot_result()
            }
            "provider.save" => {
                let previous_id = request.optional_string("previousId")?;
                let draft = input::provider_draft(&request.object("draft")?)?;
                documents::save_provider(&self.paths, previous_id.as_deref(), &draft)?;
                self.snapshot_result()
            }
            "provider.duplicate" => {
                let id = documents::duplicate_provider(&self.paths, request.string("providerId")?)?;
                Ok(json!({ "snapshot": self.snapshot()?, "providerId": id }))
            }
            "provider.remove" => {
                let id = request.string("providerId")?;
                documents::remove_provider(&self.paths, id)?;
                self.fetched_models.remove(id);
                self.snapshot_result()
            }
            "provider.sync" => {
                documents::set_provider_in_pi(
                    &self.paths,
                    request.string("providerId")?,
                    request.boolean("inPi")?,
                )?;
                self.snapshot_result()
            }
            "model.save" => {
                let previous_id = request.optional_string("previousId")?;
                let draft = input::model_draft(&request.object("draft")?)?;
                documents::save_model(
                    &self.paths,
                    request.string("providerId")?,
                    previous_id.as_deref(),
                    &draft,
                )?;
                self.snapshot_result()
            }
            "model.duplicate" => {
                let draft = input::model_draft(&request.object("draft")?)?;
                documents::duplicate_model(
                    &self.paths,
                    request.string("providerId")?,
                    request.string("sourceModelId")?,
                    &draft,
                )?;
                self.snapshot_result()
            }
            "model.remove" => {
                documents::remove_model(
                    &self.paths,
                    request.string("providerId")?,
                    request.string("modelId")?,
                )?;
                self.snapshot_result()
            }
            "model.default" => {
                documents::set_default(
                    &self.paths,
                    request.string("providerId")?,
                    request.string("modelId")?,
                )?;
                self.snapshot_result()
            }
            "settings.language" => {
                documents::set_language(&self.paths, request.string("value")?)?;
                self.snapshot_result()
            }
            "settings.metadata" => {
                documents::set_fetch_model_metadata(&self.paths, request.boolean("value")?)?;
                self.snapshot_result()
            }
            "settings.updates" => {
                documents::set_check_updates(&self.paths, request.boolean("value")?)?;
                self.snapshot_result()
            }
            "settings.defaults" => {
                let defaults = input::model_defaults(&request.object("value")?)?;
                documents::set_model_defaults(&self.paths, &defaults)?;
                self.snapshot_result()
            }
            "doctor" => Ok(json!({
                "checks": documents::doctor(&self.paths).iter().map(|check| json!({
                    "ok": check.ok, "label": check.label, "detail": check.detail
                })).collect::<Vec<_>>()
            })),
            "backups.list" => self.list_backups(),
            "backups.restore" => self.restore_backup(request.string("name")?),
            "sessions.list" => self.list_sessions(),
            "sessions.preview" => self.preview_session(
                request.string("id")?,
                request.optional_boolean("userOnly")?.unwrap_or(false),
            ),
            "sessions.delete" => self.delete_session(request.string("id")?),
            "models.fetch" => self.fetch_models(request.string("providerId")?),
            "models.import" => self.import_models(&request),
            "opencode.list" => Ok(json!({
                "providerIds": documents::list_opencode_providers(&self.paths)?,
                "path": self.paths.opencode.display().to_string(),
            })),
            "opencode.prepare" => self.prepare_opencode(request.strings("providerIds")?),
            "opencode.import" => self.import_opencode(&request),
            "updates.check" => {
                let latest = documents::check_npm_update_strict(&self.paths.update)?;
                Ok(json!({
                    "current": env!("CARGO_PKG_VERSION"),
                    "available": latest.is_some(),
                    "latest": latest,
                }))
            }
            "updates.install" => {
                documents::install_update()?;
                Ok(json!({ "installed": true, "restartRequired": true }))
            }
            _ => Err(invalid(format!("unknown Web action '{action}'"))),
        }
    }

    fn snapshot(&self) -> Result<Value> {
        let snapshot = documents::load_snapshot(&self.paths)?;
        Ok(output::snapshot(
            &snapshot,
            &self.paths,
            &self.sessions_root,
        ))
    }

    fn snapshot_result(&self) -> Result<Value> {
        Ok(json!({ "snapshot": self.snapshot()? }))
    }

    fn selection_id(&mut self) -> u64 {
        self.next_selection_id += 1;
        self.next_selection_id
    }
}

fn action_fields(action: &str) -> Result<&'static [&'static str]> {
    let fields: &[&str] = match action {
        "snapshot" | "doctor" | "backups.list" | "sessions.list" | "opencode.list"
        | "updates.check" | "updates.install" => &[],
        "provider.save" => &["previousId", "draft"],
        "providers.sort" => &["value"],
        "models.sort" => &["providerId", "value"],
        "providers.reorder" => &["ids"],
        "models.reorder" => &["providerId", "ids"],
        "provider.duplicate" | "provider.remove" | "models.fetch" => &["providerId"],
        "provider.sync" => &["providerId", "inPi"],
        "model.save" => &["providerId", "previousId", "draft"],
        "model.duplicate" => &["providerId", "sourceModelId", "draft"],
        "model.remove" | "model.default" => &["providerId", "modelId"],
        "settings.language" | "settings.metadata" | "settings.updates" | "settings.defaults" => {
            &["value"]
        }
        "backups.restore" => &["name"],
        "sessions.preview" => &["id", "userOnly"],
        "sessions.delete" => &["id"],
        "models.import" => &[
            "providerId",
            "ids",
            "updateExisting",
            "selectionId",
            "candidateIndices",
        ],
        "opencode.prepare" => &["providerIds"],
        "opencode.import" => &["planId", "candidateIndices"],
        _ => return Err(invalid(format!("unknown Web action '{action}'"))),
    };
    Ok(fields)
}

fn invalid(message: impl Into<String>) -> AppError {
    AppError::Invalid(message.into())
}
