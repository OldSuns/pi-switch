use super::snapshot::pi_switch_object;
use super::*;

pub fn set_language(paths: &Paths, language: &str) -> Result<()> {
    if !matches!(language, "en" | "zh-CN") {
        return Err(AppError::Invalid(format!(
            "unsupported pi-switch language '{language}'"
        )));
    }
    update_pi_switch(paths, |settings| {
        settings.insert("language".into(), Value::String(language.into()));
    })
}

pub fn set_fetch_model_metadata(paths: &Paths, enabled: bool) -> Result<()> {
    update_pi_switch(paths, |settings| {
        settings.insert("fetchModelMetadata".into(), Value::Bool(enabled));
    })
}

/// Whether the background npm update check runs on launch. Defaults to `true`.
pub(super) fn check_updates_field(settings: &Value) -> Result<bool> {
    match pi_switch_object(settings)?.and_then(|value| value.get("checkForUpdates")) {
        None => Ok(true),
        Some(Value::Bool(value)) => Ok(*value),
        Some(_) => Err(AppError::Invalid(
            "settings piSwitch.checkForUpdates must be a boolean".into(),
        )),
    }
}

pub fn set_check_updates(paths: &Paths, enabled: bool) -> Result<()> {
    update_pi_switch(paths, |settings| {
        settings.insert("checkForUpdates".into(), Value::Bool(enabled));
    })
}

pub fn set_model_defaults(paths: &Paths, defaults: &ModelDefaults) -> Result<()> {
    if defaults.context_window == Some(0) || defaults.max_tokens == Some(0) {
        return Err(AppError::Invalid(
            "default context window and max tokens must be positive".into(),
        ));
    }
    for (field, value) in [
        ("input cost", defaults.input_cost),
        ("output cost", defaults.output_cost),
        ("cache read cost", defaults.cache_read_cost),
        ("cache write cost", defaults.cache_write_cost),
    ] {
        if value.is_some_and(|value| !value.is_finite() || value < 0.0) {
            return Err(AppError::Invalid(format!(
                "default {field} must be a non-negative number"
            )));
        }
    }
    let mut value = Map::new();
    for (field, item) in [
        ("contextWindow", defaults.context_window.map(Value::from)),
        ("maxTokens", defaults.max_tokens.map(Value::from)),
        ("inputCost", defaults.input_cost.map(Value::from)),
        ("outputCost", defaults.output_cost.map(Value::from)),
        ("cacheReadCost", defaults.cache_read_cost.map(Value::from)),
        ("cacheWriteCost", defaults.cache_write_cost.map(Value::from)),
    ] {
        if let Some(item) = item {
            value.insert(field.into(), item);
        }
    }
    update_pi_switch(paths, |settings| {
        if value.is_empty() {
            settings.remove("modelDefaults");
        } else {
            settings.insert("modelDefaults".into(), Value::Object(value));
        }
    })
}

fn update_pi_switch(paths: &Paths, update: impl FnOnce(&mut Map<String, Value>)) -> Result<()> {
    let _lock = WriteLock::acquire(paths)?;
    let mut settings = read_document(&paths.settings, json!({}))?;
    let root = root_object_mut(&mut settings, &paths.settings)?;
    let pi_switch = root
        .entry("piSwitch")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| AppError::Invalid("settings piSwitch must be an object".into()))?;
    update(pi_switch);
    write_document(paths, &_lock, &paths.settings, &settings).map(|_| ())
}
