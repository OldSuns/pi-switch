//! Pi `auth.json` credentials.
//!
//! Follows Pi's own storage contract (pi-mono `auth-storage.ts`): the file is
//! `<agent dir>/auth.json`, entries map provider IDs to
//! `{"type": "api_key", "key": "..."}` (Pi also resolves `$ENV` references in
//! `key`), and writers hold a proper-lockfile-compatible lock. proper-lockfile
//! locks by creating a `<file>.lock` directory; we mirror that with
//! `create_dir` and back off briefly while locked. We do not steal existing
//! lock directories because proper-lockfile exposes no portable ownership
//! token that would make takeover safe against a concurrent replacement.

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    thread::sleep,
    time::{Duration, Instant, SystemTime},
};

use serde_json::{json, Map, Value};

use super::{
    storage::{io_error, now_millis, read_document, replace_file},
    AppError, Paths, Result,
};

const RETRY: Duration = Duration::from_millis(20);
const TIMEOUT: Duration = Duration::from_secs(5);

pub(super) fn read_credentials(paths: &Paths) -> Result<Map<String, Value>> {
    let value = read_document(&paths.pi_auth, json!({}))?;
    value
        .as_object()
        .cloned()
        .ok_or_else(|| AppError::Invalid("auth.json must contain a JSON object".into()))
}

/// Unresolved `key` of a provider's `api_key` credential, if present.
pub fn credential_key(paths: &Paths, provider_id: &str) -> Result<Option<String>> {
    let credentials = read_credentials(paths)?;
    Ok(match credentials.get(provider_id) {
        Some(Value::Object(credential))
            if credential.get("type").and_then(Value::as_str) == Some("api_key") =>
        {
            credential
                .get("key")
                .and_then(Value::as_str)
                .map(str::to_owned)
        }
        _ => None,
    })
}

/// Only `api_key` credentials count: OAuth and other credential types are
/// managed by Pi itself and never surface as deletable API keys here.
pub fn has_credential(paths: &Paths, provider_id: &str) -> Result<bool> {
    Ok(credential_key(paths, provider_id)?.is_some())
}

pub fn credential_ids(paths: &Paths) -> Result<Vec<String>> {
    Ok(credential_keys(paths)?.into_keys().collect())
}

/// Provider id -> unresolved `key`, for all `api_key` credentials.
pub(super) fn credential_keys(paths: &Paths) -> Result<std::collections::BTreeMap<String, String>> {
    let mut keys = std::collections::BTreeMap::new();
    for (id, credential) in read_credentials(paths)? {
        let Some(object) = credential.as_object() else {
            continue;
        };
        if object.get("type").and_then(Value::as_str) != Some("api_key") {
            continue;
        }
        if let Some(key) = object.get("key").and_then(Value::as_str) {
            keys.insert(id, key.to_owned());
        }
    }
    Ok(keys)
}

/// `api_key` entry for a provider: only `type`/`key` are rewritten, so an
/// existing entry keeps its provider-scoped `env` and extension fields.
pub(super) fn api_key_entry(existing: Option<&Value>, key: &str) -> Value {
    let mut entry = existing
        .filter(|entry| entry["type"] == "api_key")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    entry.insert("type".into(), Value::String("api_key".into()));
    entry.insert("key".into(), Value::String(key.into()));
    Value::Object(entry)
}

/// Whether an entry is an `api_key` credential — the only kind this tool
/// rewrites. OAuth and other types belong to Pi.
pub(super) fn is_api_key(entry: &Value) -> bool {
    entry["type"] == "api_key"
}

/// Whether an entry is an `api_key` credential with nothing but `type`/`key`
/// to lose, so retiring it takes no provider-scoped `env` values or extensions
/// with it.
pub(super) fn is_plain_api_key(entry: &Value) -> bool {
    is_api_key(entry) && !has_provider_config(entry)
}

/// Whether an entry holds more than `type`/`key` — provider-scoped `env` values
/// and extensions, which a `models.json` key cannot carry.
pub(super) fn has_provider_config(entry: &Value) -> bool {
    entry
        .as_object()
        .is_some_and(|entry| entry.keys().any(|field| field != "type" && field != "key"))
}

/// Read-modify-write `auth.json` under a proper-lockfile-compatible lock.
/// The file is only rewritten when the edit actually changed something, and
/// the returned flag says whether it did.
pub(super) fn edit(
    paths: &Paths,
    update: impl FnOnce(&mut Map<String, Value>) -> Result<()>,
) -> Result<bool> {
    let lock = AuthLock::acquire(&paths.pi_auth)?;
    let mut credentials = read_credentials(paths)?;
    let before = credentials.clone();
    update(&mut credentials)?;
    let changed = credentials != before;
    if changed {
        write_private_json(&paths.pi_auth, &Value::Object(credentials))?;
    }
    drop(lock);
    Ok(changed)
}

fn lock_dir(auth_path: &Path) -> PathBuf {
    let mut name = auth_path
        .file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_else(|| "auth.json".into());
    name.push(".lock");
    auth_path.with_file_name(name)
}

struct AuthLock {
    dir: PathBuf,
    /// mtime of the directory this lock created: a stale takeover replaces that
    /// directory, so this is what tells our lock apart from a replacement.
    created: SystemTime,
}

impl AuthLock {
    fn acquire(auth_path: &Path) -> Result<Self> {
        let dir = lock_dir(auth_path);
        let parent = dir.parent().ok_or_else(|| {
            AppError::Invalid(format!("{} has no parent directory", dir.display()))
        })?;
        fs::create_dir_all(parent).map_err(|source| io_error(parent, source))?;
        let deadline = Instant::now() + TIMEOUT;
        loop {
            match fs::create_dir(&dir) {
                Ok(()) => {
                    return match fs::metadata(&dir).and_then(|metadata| metadata.modified()) {
                        Ok(created) => Ok(Self { dir, created }),
                        Err(source) => {
                            let _ = fs::remove_dir(&dir);
                            Err(io_error(&dir, source))
                        }
                    };
                }
                Err(source) if source.kind() == std::io::ErrorKind::AlreadyExists => {
                    if Instant::now() >= deadline {
                        return Err(AppError::Busy(dir));
                    }
                    sleep(RETRY);
                }
                Err(source) => return Err(io_error(&dir, source)),
            }
        }
    }
}

impl Drop for AuthLock {
    fn drop(&mut self) {
        // A stale takeover replaces the lock directory while its previous owner
        // is paused: only remove the directory that is still the one we made.
        if mtime_of(&self.dir) == Some(self.created) {
            let _ = fs::remove_dir(&self.dir);
        }
    }
}

fn mtime_of(dir: &Path) -> Option<SystemTime> {
    fs::metadata(dir)
        .and_then(|metadata| metadata.modified())
        .ok()
}

/// Atomic write that marks a newly created file private (0600 on Unix), the
/// same mode Pi uses for `auth.json`. Existing files keep their permissions.
fn write_private_json(target: &Path, value: &Value) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|source| AppError::Json {
        path: target.into(),
        source,
    })?;
    bytes.push(b'\n');
    let parent = target.parent().ok_or_else(|| {
        AppError::Invalid(format!("{} has no parent directory", target.display()))
    })?;
    fs::create_dir_all(parent).map_err(|source| io_error(parent, source))?;
    let name = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("auth.json");
    let temporary = parent.join(format!(
        ".{name}.{}.{}.tmp",
        std::process::id(),
        now_millis()
    ));
    let result = (|| {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        // Only applies at creation; administrator-managed modes stay intact.
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temporary)
            .map_err(|source| io_error(&temporary, source))?;
        file.write_all(&bytes)
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

#[cfg(test)]
mod lock_tests {
    use super::*;

    #[test]
    fn drop_only_removes_the_lock_directory_it_created() {
        let root = std::env::temp_dir().join(format!("pi-switch-lock-{}", now_millis()));
        fs::create_dir_all(&root).unwrap();
        let auth_path = root.join("auth.json");
        let dir = lock_dir(&auth_path);

        // Our own lock goes away again.
        let lock = AuthLock::acquire(&auth_path).unwrap();
        assert_eq!(mtime_of(&dir), Some(lock.created));
        drop(lock);
        assert!(!dir.exists());

        // A directory a stale takeover replaced stays, even when the old owner
        // (which recorded the previous mtime) resumes and drops its lock.
        let stale = AuthLock {
            dir: dir.clone(),
            created: SystemTime::UNIX_EPOCH,
        };
        fs::create_dir(&dir).unwrap();
        drop(stale);
        assert!(dir.exists());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn acquire_creates_the_auth_parent_directory() {
        let root = std::env::temp_dir().join(format!("pi-switch-lock-parent-{}", now_millis()));
        let auth_path = root.join(".pi/agent/auth.json");

        let lock = AuthLock::acquire(&auth_path).unwrap();
        assert!(lock_dir(&auth_path).is_dir());
        drop(lock);
        assert!(!lock_dir(&auth_path).exists());

        let _ = fs::remove_dir_all(&root);
    }
}
