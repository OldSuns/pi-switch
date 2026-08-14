use std::{
    cell::Cell,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process, thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use chrono::Local;
use serde_json::{json, Map, Value};

#[cfg(not(windows))]
use std::fs::File;

use super::{
    settings::{split_legacy_settings, validate_app_settings},
    snapshot::{validate_local_library, validate_pi_settings, validate_provider_document},
    AppError, Backup, Paths, Result,
};

const BACKUP_LIMIT: usize = 10;

pub fn list_backups(paths: &Paths) -> Result<Vec<Backup>> {
    if !paths.backups.exists() {
        return Ok(Vec::new());
    }
    let mut backups = fs::read_dir(&paths.backups)
        .map_err(|source| io_error(&paths.backups, source))?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.starts_with("backup-") || !name.ends_with(".json") {
                return None;
            }
            let value: Value = serde_json::from_slice(&fs::read(entry.path()).ok()?).ok()?;
            backup_documents(&value, &entry.path()).ok()?;
            Some(Backup {
                path: entry.path().display().to_string(),
                name,
            })
        })
        .collect::<Vec<_>>();
    backups.sort_by(|a, b| b.name.cmp(&a.name));
    Ok(backups)
}

pub fn restore_backup(paths: &Paths, backup: &Backup) -> Result<()> {
    let backup_path = PathBuf::from(&backup.path);
    if backup_path.parent() != Some(paths.backups.as_path()) {
        return Err(AppError::Invalid(
            "backup path is outside the backup directory".into(),
        ));
    }
    let snapshot = read_document(&backup_path, json!({}))?;
    let documents = backup_documents(&snapshot, &backup_path)?;
    let lock = WriteLock::acquire(paths)?;
    let models_changed = write_document(paths, &lock, &paths.pi_models, &documents.models)?;
    let pi_settings_changed =
        write_document(paths, &lock, &paths.pi_settings, &documents.pi_settings)
            .map_err(|error| partial_restore_error(models_changed, "Pi settings", error))?;
    let app_settings_changed =
        write_document(paths, &lock, &paths.app_settings, &documents.app_settings).map_err(
            |error| {
                partial_restore_error(
                    models_changed || pi_settings_changed,
                    "pi-switch settings",
                    error,
                )
            },
        )?;
    write_document(paths, &lock, &paths.providers, &documents.providers)
        .map(|_| ())
        .map_err(|error| {
            partial_restore_error(
                models_changed || pi_settings_changed || app_settings_changed,
                "providers.json",
                error,
            )
        })
}

struct BackupDocuments {
    providers: Value,
    models: Value,
    pi_settings: Value,
    app_settings: Value,
}

fn backup_documents(snapshot: &Value, path: &Path) -> Result<BackupDocuments> {
    let object = snapshot.as_object().ok_or_else(|| {
        AppError::Invalid(format!("{} must contain a backup object", path.display()))
    })?;
    let version = object.get("version").and_then(Value::as_u64);
    if !matches!(version, Some(2 | 3)) {
        return Err(AppError::Invalid(
            "legacy backups without the local provider library are not supported".into(),
        ));
    }
    let providers = required_backup_field(object, path, "providers")?;
    let models = required_backup_field(object, path, "models")?;
    let (pi_settings, app_settings) = match version {
        Some(2) => {
            split_legacy_settings(required_backup_field(object, path, "settings")?, json!({}))?
        }
        Some(3) => (
            required_backup_field(object, path, "piSettings")?,
            required_backup_field(object, path, "appSettings")?,
        ),
        _ => unreachable!(),
    };
    if object.get("version").and_then(Value::as_u64) == Some(3)
        && pi_settings.get("piSwitch").is_some()
    {
        return Err(AppError::Invalid(
            "version 3 backup Pi settings must not contain piSwitch".into(),
        ));
    }
    validate_local_library(&providers)?;
    validate_provider_document(&models)?;
    validate_pi_settings(&pi_settings, &models)?;
    validate_app_settings(&app_settings)?;
    Ok(BackupDocuments {
        providers,
        models,
        pi_settings,
        app_settings,
    })
}

fn required_backup_field(object: &Map<String, Value>, path: &Path, field: &str) -> Result<Value> {
    object
        .get(field)
        .filter(|value| value.is_object())
        .cloned()
        .ok_or_else(|| AppError::Invalid(format!("{} is missing {field}", path.display())))
}

fn partial_restore_error(changed: bool, target: &str, error: AppError) -> AppError {
    if changed {
        AppError::Partial(format!(
            "backup restore partially completed; {target} failed: {error}"
        ))
    } else {
        error
    }
}

pub(super) fn read_document(path: &Path, missing: Value) -> Result<Value> {
    if !path.exists() {
        return Ok(missing);
    }
    let bytes = fs::read(path).map_err(|source| io_error(path, source))?;
    let value = serde_json::from_slice(&bytes).map_err(|source| AppError::Json {
        path: path.into(),
        source,
    })?;
    validate_root(&value, path)?;
    Ok(value)
}

pub(super) fn validate_root(value: &Value, path: &Path) -> Result<()> {
    if value.is_object() {
        Ok(())
    } else {
        Err(AppError::Invalid(format!(
            "{} must contain a JSON object",
            path.display()
        )))
    }
}

pub(super) fn root_object_mut<'a>(
    value: &'a mut Value,
    path: &Path,
) -> Result<&'a mut Map<String, Value>> {
    value
        .as_object_mut()
        .ok_or_else(|| AppError::Invalid(format!("{} must contain a JSON object", path.display())))
}

pub(super) fn providers_object(value: &Value) -> Result<&Map<String, Value>> {
    match value.get("providers") {
        Some(Value::Object(providers)) => Ok(providers),
        Some(_) => Err(AppError::Invalid(
            "models.json 'providers' must be an object".into(),
        )),
        None => Ok(empty_object()),
    }
}

pub(super) fn empty_object() -> &'static Map<String, Value> {
    static EMPTY: std::sync::OnceLock<Map<String, Value>> = std::sync::OnceLock::new();
    EMPTY.get_or_init(Map::new)
}

pub(super) fn providers_object_mut(value: &mut Value) -> Result<&mut Map<String, Value>> {
    let root = value
        .as_object_mut()
        .ok_or_else(|| AppError::Invalid("models.json must contain a JSON object".into()))?;
    if !root.contains_key("providers") {
        root.insert("providers".into(), json!({}));
    }
    root.get_mut("providers")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| AppError::Invalid("models.json 'providers' must be an object".into()))
}

pub(super) fn string_field(value: &Value, field: &str) -> Result<Option<String>> {
    match value.get(field) {
        None => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(AppError::Invalid(format!(
            "settings.json '{field}' must be a string"
        ))),
    }
}

pub(super) fn provider_string(
    object: &Map<String, Value>,
    id: &str,
    field: &str,
) -> Result<String> {
    match object.get(field) {
        None => Ok(String::new()),
        Some(Value::String(value)) => Ok(value.clone()),
        Some(_) => Err(AppError::Invalid(format!(
            "provider '{id}' {field} must be a string"
        ))),
    }
}

pub(super) fn write_document(
    paths: &Paths,
    lock: &WriteLock,
    target: &Path,
    value: &Value,
) -> Result<bool> {
    if target.exists() && read_document(target, json!({}))? == *value {
        return Ok(false);
    }
    if !lock.backed_up.get() {
        create_backup(paths)?;
        lock.backed_up.set(true);
    }
    write_json(target, value)?;
    Ok(true)
}

pub(super) fn write_initial_document(target: &Path, value: &Value) -> Result<()> {
    if target.exists() {
        return Err(AppError::Invalid(format!(
            "{} already exists",
            target.display()
        )));
    }
    write_json(target, value)
}

fn write_json(target: &Path, value: &Value) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|source| AppError::Json {
        path: target.into(),
        source,
    })?;
    bytes.push(b'\n');
    atomic_write(target, &bytes)
}

pub(super) fn create_backup(paths: &Paths) -> Result<()> {
    if !paths.providers.exists()
        && !paths.pi_models.exists()
        && !paths.pi_settings.exists()
        && !paths.app_settings.exists()
    {
        return Ok(());
    }
    fs::create_dir_all(&paths.backups).map_err(|source| io_error(&paths.backups, source))?;
    let (timestamp, backup) = unique_backup_path(paths, "backup");
    let models = read_document(&paths.pi_models, json!({ "providers": {} }))?;
    let (pi_settings, app_settings) = split_legacy_settings(
        read_document(&paths.pi_settings, json!({}))?,
        read_document(&paths.app_settings, json!({}))?,
    )?;
    let snapshot = json!({
        "version": 3,
        "createdAt": timestamp.to_rfc3339(),
        "providers": read_document(&paths.providers, json!({ "version": 1, "providers": {} }))?,
        "models": models,
        "piSettings": pi_settings,
        "appSettings": app_settings,
    });
    write_json(&backup, &snapshot)?;
    prune_backups(paths)
}

pub(super) fn archive_corrupt_provider_store(paths: &Paths) -> Result<PathBuf> {
    fs::create_dir_all(&paths.backups).map_err(|source| io_error(&paths.backups, source))?;
    let (_, backup) = unique_backup_path(paths, "corrupt-providers");
    fs::rename(&paths.providers, &backup).map_err(|source| io_error(&paths.providers, source))?;
    prune_backups(paths)?;
    Ok(backup)
}

fn unique_backup_path(paths: &Paths, prefix: &str) -> (chrono::DateTime<Local>, PathBuf) {
    loop {
        let timestamp = Local::now();
        let name = format!(
            "{}-{}.json",
            prefix,
            timestamp.format("%Y-%m-%d_%H-%M-%S-%3f")
        );
        let backup = paths.backups.join(name);
        if !backup.exists() {
            return (timestamp, backup);
        }
        thread::sleep(Duration::from_millis(1));
    }
}

pub(super) fn prune_backups(paths: &Paths) -> Result<()> {
    if !paths.backups.exists() {
        return Ok(());
    }
    let mut files = fs::read_dir(&paths.backups)
        .map_err(|source| io_error(&paths.backups, source))?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_name().to_string_lossy().ends_with(".json"))
        .collect::<Vec<_>>();
    files.sort_by_key(|entry| std::cmp::Reverse(entry.file_name()));
    for old in files.into_iter().skip(BACKUP_LIMIT) {
        fs::remove_file(old.path()).map_err(|source| io_error(&old.path(), source))?;
    }
    Ok(())
}

pub(super) fn atomic_write(target: &Path, bytes: &[u8]) -> Result<()> {
    let parent = target.parent().ok_or_else(|| {
        AppError::Invalid(format!("{} has no parent directory", target.display()))
    })?;
    fs::create_dir_all(parent).map_err(|source| io_error(parent, source))?;
    let name = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("document");
    let temporary = parent.join(format!(".{name}.{}.{}.tmp", process::id(), now_millis()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|source| io_error(&temporary, source))?;
        file.write_all(bytes)
            .map_err(|source| io_error(&temporary, source))?;
        file.sync_all()
            .map_err(|source| io_error(&temporary, source))?;
        replace_file(&temporary, target)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(not(windows))]
pub(super) fn replace_file(source: &Path, target: &Path) -> Result<()> {
    fs::rename(source, target).map_err(|source| io_error(target, source))?;
    if let Some(parent) = target.parent() {
        File::open(parent)
            .and_then(|file| file.sync_all())
            .map_err(|source| io_error(parent, source))?;
    }
    Ok(())
}

#[cfg(windows)]
pub(super) fn replace_file(source: &Path, target: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;
    #[link(name = "kernel32")]
    extern "system" {
        fn MoveFileExW(existing: *const u16, replacement: *const u16, flags: u32) -> i32;
    }

    let source = source
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let target_wide = target
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let success = unsafe {
        MoveFileExW(
            source.as_ptr(),
            target_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if success == 0 {
        Err(io_error(target, std::io::Error::last_os_error()))
    } else {
        Ok(())
    }
}

pub(super) struct WriteLock {
    path: PathBuf,
    backed_up: Cell<bool>,
}

impl WriteLock {
    pub(super) fn acquire(paths: &Paths) -> Result<Self> {
        let parent = paths
            .lock
            .parent()
            .ok_or_else(|| AppError::Invalid("write lock has no parent directory".into()))?;
        fs::create_dir_all(parent).map_err(|source| io_error(parent, source))?;
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&paths.lock)
        {
            Ok(_file) => Ok(Self {
                path: paths.lock.clone(),
                backed_up: Cell::new(false),
            }),
            Err(source) if source.kind() == std::io::ErrorKind::AlreadyExists => {
                Err(AppError::Busy(paths.lock.clone()))
            }
            Err(source) => Err(io_error(&paths.lock, source)),
        }
    }
}

impl Drop for WriteLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub(super) fn io_error(path: &Path, source: std::io::Error) -> AppError {
    AppError::Io {
        path: path.into(),
        source,
    }
}

pub(super) fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
