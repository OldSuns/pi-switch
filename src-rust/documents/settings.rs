use super::*;

pub(super) struct SettingsDocuments {
    pub app: Value,
    pub pi: Value,
    pub warning: Option<String>,
}

pub(super) fn load_settings(paths: &Paths) -> Result<SettingsDocuments> {
    let app = read_document(&paths.app_settings, json!({}))?;
    let pi = read_document(&paths.pi_settings, json!({}))?;
    let needs_migration = pi.get("piSwitch").is_some();
    let (pi, app) = split_legacy_settings(pi, app)?;
    if !needs_migration {
        return Ok(SettingsDocuments {
            app,
            pi,
            warning: None,
        });
    }

    let lock = WriteLock::acquire(paths)?;
    let app = read_document(&paths.app_settings, json!({}))?;
    let pi = read_document(&paths.pi_settings, json!({}))?;
    let needs_migration = pi.get("piSwitch").is_some();
    let (pi, app) = split_legacy_settings(pi, app)?;
    if !needs_migration {
        return Ok(SettingsDocuments {
            app,
            pi,
            warning: None,
        });
    }

    write_document(paths, &lock, &paths.app_settings, &app)?;
    let warning = write_document(paths, &lock, &paths.pi_settings, &pi)
        .err()
        .map(|error| {
            format!(
                "pi-switch settings were migrated to {}, but the legacy piSwitch field could not be removed from {}: {error}. Reload to retry.",
                paths.app_settings.display(),
                paths.pi_settings.display()
            )
        });
    Ok(SettingsDocuments { app, pi, warning })
}

pub fn set_language(paths: &Paths, language: &str) -> Result<()> {
    if !matches!(language, "en" | "zh-CN") {
        return Err(AppError::Invalid(format!(
            "unsupported pi-switch language '{language}'"
        )));
    }
    update_app_settings(paths, |settings| {
        settings.insert("language".into(), Value::String(language.into()));
    })
}

pub fn set_fetch_model_metadata(paths: &Paths, enabled: bool) -> Result<()> {
    update_app_settings(paths, |settings| {
        settings.insert("fetchModelMetadata".into(), Value::Bool(enabled));
    })
}

/// Where a provider's API key is stored when saving a provider.
/// Defaults to `auth.json` so `models.json` stays free of secrets.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KeyStorage {
    AuthJson,
    ModelsJson,
}

impl KeyStorage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AuthJson => "auth.json",
            Self::ModelsJson => "models.json",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "auth.json" => Some(Self::AuthJson),
            "models.json" => Some(Self::ModelsJson),
            _ => None,
        }
    }
}

pub(super) fn key_storage_field(settings: &Value) -> Result<KeyStorage> {
    match settings.get("keyStorage") {
        None => Ok(KeyStorage::AuthJson),
        Some(Value::String(value)) => KeyStorage::parse(value).ok_or_else(|| {
            AppError::Invalid(
                "pi-switch settings keyStorage must be 'auth.json' or 'models.json'".into(),
            )
        }),
        Some(_) => Err(AppError::Invalid(
            "pi-switch settings keyStorage must be 'auth.json' or 'models.json'".into(),
        )),
    }
}

pub fn set_key_storage(paths: &Paths, storage: KeyStorage) -> Result<()> {
    update_app_settings(paths, |settings| {
        settings.insert("keyStorage".into(), Value::String(storage.as_str().into()));
    })
}

/// Current choice (reads the app settings document directly, no migration).
pub(super) fn key_storage_setting(paths: &Paths) -> Result<KeyStorage> {
    key_storage_field(&read_document(&paths.app_settings, json!({}))?)
}

pub(super) fn check_updates_field(settings: &Value) -> Result<bool> {
    match settings.get("checkForUpdates") {
        None => Ok(true),
        Some(Value::Bool(value)) => Ok(*value),
        Some(_) => Err(AppError::Invalid(
            "pi-switch settings checkForUpdates must be a boolean".into(),
        )),
    }
}

pub fn set_check_updates(paths: &Paths, enabled: bool) -> Result<()> {
    update_app_settings(paths, |settings| {
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
    update_app_settings(paths, |settings| {
        if value.is_empty() {
            settings.remove("modelDefaults");
        } else {
            settings.insert("modelDefaults".into(), Value::Object(value));
        }
    })
}

fn update_app_settings(paths: &Paths, update: impl FnOnce(&mut Map<String, Value>)) -> Result<()> {
    let lock = WriteLock::acquire(paths)?;
    let mut settings = read_document(&paths.app_settings, json!({}))?;
    update(root_object_mut(&mut settings, &paths.app_settings)?);
    validate_app_settings(&settings)?;
    write_document(paths, &lock, &paths.app_settings, &settings).map(|_| ())
}

pub(super) fn validate_app_settings(settings: &Value) -> Result<()> {
    if !settings.is_object() {
        return Err(AppError::Invalid(
            "pi-switch settings must contain a JSON object".into(),
        ));
    }
    language_field(settings)?;
    fetch_model_metadata_field(settings)?;
    key_storage_field(settings)?;
    check_updates_field(settings)?;
    model_defaults_field(settings)?;
    Ok(())
}

pub(super) fn split_legacy_settings(mut pi: Value, app: Value) -> Result<(Value, Value)> {
    validate_app_settings(&app)?;
    let Some(legacy) = legacy_settings(&pi)?.cloned() else {
        return Ok((pi, app));
    };
    validate_app_settings(&Value::Object(legacy.clone()))?;
    let mut merged = legacy;
    merged.extend(app.as_object().cloned().ok_or_else(|| {
        AppError::Invalid("pi-switch settings must contain a JSON object".into())
    })?);
    let app = Value::Object(merged);
    validate_app_settings(&app)?;
    pi.as_object_mut()
        .ok_or_else(|| AppError::Invalid("Pi settings must contain a JSON object".into()))?
        .remove("piSwitch");
    Ok((pi, app))
}

pub(super) fn language_field(settings: &Value) -> Result<String> {
    match settings.get("language") {
        None => Ok("en".into()),
        Some(Value::String(value)) if matches!(value.as_str(), "en" | "zh-CN") => Ok(value.clone()),
        Some(_) => Err(AppError::Invalid(
            "pi-switch settings language must be 'en' or 'zh-CN'".into(),
        )),
    }
}

pub(super) fn fetch_model_metadata_field(settings: &Value) -> Result<bool> {
    match settings.get("fetchModelMetadata") {
        None => Ok(true),
        Some(Value::Bool(value)) => Ok(*value),
        Some(_) => Err(AppError::Invalid(
            "pi-switch settings fetchModelMetadata must be a boolean".into(),
        )),
    }
}

pub(super) fn model_defaults_field(settings: &Value) -> Result<ModelDefaults> {
    let Some(value) = settings.get("modelDefaults") else {
        return Ok(ModelDefaults::default());
    };
    let object = value.as_object().ok_or_else(|| {
        AppError::Invalid("pi-switch settings modelDefaults must be an object".into())
    })?;
    Ok(ModelDefaults {
        context_window: optional_positive_u64(object, "contextWindow")?,
        max_tokens: optional_positive_u64(object, "maxTokens")?,
        input_cost: optional_nonnegative_f64(object, "inputCost")?,
        output_cost: optional_nonnegative_f64(object, "outputCost")?,
        cache_read_cost: optional_nonnegative_f64(object, "cacheReadCost")?,
        cache_write_cost: optional_nonnegative_f64(object, "cacheWriteCost")?,
    })
}

fn legacy_settings(settings: &Value) -> Result<Option<&Map<String, Value>>> {
    settings
        .get("piSwitch")
        .map(|value| {
            value
                .as_object()
                .ok_or_else(|| AppError::Invalid("settings piSwitch must be an object".into()))
        })
        .transpose()
}

fn optional_positive_u64(object: &Map<String, Value>, field: &str) -> Result<Option<u64>> {
    match object.get(field) {
        None => Ok(None),
        Some(value) => value
            .as_u64()
            .filter(|value| *value > 0)
            .map(Some)
            .ok_or_else(|| {
                AppError::Invalid(format!(
                    "pi-switch settings modelDefaults.{field} must be a positive integer"
                ))
            }),
    }
}

fn optional_nonnegative_f64(object: &Map<String, Value>, field: &str) -> Result<Option<f64>> {
    match object.get(field) {
        None => Ok(None),
        Some(value) => value
            .as_f64()
            .filter(|value| *value >= 0.0)
            .map(Some)
            .ok_or_else(|| {
                AppError::Invalid(format!(
                    "pi-switch settings modelDefaults.{field} must be a non-negative number"
                ))
            }),
    }
}
