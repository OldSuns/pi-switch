use super::*;

pub fn doctor(paths: &Paths) -> Vec<DoctorCheck> {
    let mut checks = Vec::new();
    checks.push(check(
        true,
        "Pi agent directory",
        paths
            .pi_models
            .parent()
            .unwrap_or(paths.pi_models.as_path())
            .display()
            .to_string(),
    ));
    checks.push(check(
        paths.providers.exists(),
        "providers.json",
        paths.providers.display().to_string(),
    ));
    checks.push(check(
        paths.pi_models.exists(),
        "Pi models",
        if paths.pi_models.exists() {
            paths.pi_models.display().to_string()
        } else {
            format!(
                "not found at {}; Pi may not be initialized",
                paths.pi_models.display()
            )
        },
    ));
    checks.push(check(
        true,
        "Pi settings",
        if paths.pi_settings.exists() {
            paths.pi_settings.display().to_string()
        } else {
            format!(
                "not found at {}; Pi defaults apply",
                paths.pi_settings.display()
            )
        },
    ));
    checks.push(check(
        true,
        "pi-switch settings",
        if paths.app_settings.exists() {
            paths.app_settings.display().to_string()
        } else {
            format!(
                "not found at {}; pi-switch defaults apply",
                paths.app_settings.display()
            )
        },
    ));
    match load_snapshot(paths) {
        Ok(snapshot) => {
            if let Some(warning) = snapshot.warning.as_deref() {
                checks.push(check(false, "Configuration warning", warning));
            }
            let enabled = snapshot
                .providers
                .iter()
                .filter(|provider| provider.in_pi)
                .count();
            checks.push(check(
                true,
                "Provider library",
                format!(
                    "{} provider(s), {enabled} added to Pi; JSON and projection are valid",
                    snapshot.providers.len()
                ),
            ));
            let default_ok = match (&snapshot.default_provider, &snapshot.default_model) {
                (None, None) => true,
                (Some(provider), Some(model)) => snapshot.providers.iter().any(|item| {
                    item.in_pi
                        && item.id == *provider
                        && item.models.iter().any(|candidate| candidate.id == *model)
                }),
                _ => false,
            };
            checks.push(check(
                default_ok,
                "Default model",
                if default_ok {
                    snapshot
                        .default_provider
                        .zip(snapshot.default_model)
                        .map(|(provider, model)| format!("{provider}/{model}"))
                        .unwrap_or_else(|| "not explicitly configured".into())
                } else {
                    "defaultProvider/defaultModel is incomplete or references a provider not added to Pi"
                        .into()
                },
            ));
            for provider in snapshot.providers {
                let valid = validate_provider_view(&provider).is_ok();
                checks.push(check(
                    valid,
                    format!("Provider {}", provider.id),
                    validate_provider_view(&provider)
                        .map(|_| {
                            format!(
                                "{} model(s), {}, {}",
                                provider.models.len(),
                                provider.api,
                                if provider.in_pi {
                                    "synced to Pi"
                                } else {
                                    "not synced"
                                }
                            )
                        })
                        .unwrap_or_else(|error| error.to_string()),
                ));
            }
        }
        Err(error) => checks.push(check(false, "Configuration documents", error.to_string())),
    }
    let (legacy, corrupt) = backup_diagnostics(paths);
    checks.push(check(
        legacy == 0,
        "Legacy backups",
        format!("{legacy} unsupported legacy backup(s)"),
    ));
    checks.push(check(
        corrupt == 0,
        "Corrupt provider archives",
        format!("{corrupt} archived provider file(s)"),
    ));
    checks.push(check(
        !paths.lock.exists(),
        "Write lock",
        if paths.lock.exists() {
            format!(
                "lock exists at {}; remove it after confirming no writer is active",
                paths.lock.display()
            )
        } else {
            "available".into()
        },
    ));
    checks
}

fn backup_diagnostics(paths: &Paths) -> (usize, usize) {
    let Ok(entries) = std::fs::read_dir(&paths.backups) else {
        return (0, 0);
    };
    let mut legacy = 0;
    let mut corrupt = 0;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with("corrupt-providers-") && name.ends_with(".json") {
            corrupt += 1;
        } else if name.starts_with("backup-") && name.ends_with(".json") {
            let version = std::fs::read(entry.path())
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                .and_then(|value| value.get("version").and_then(Value::as_u64));
            if !matches!(version, Some(2 | 3)) {
                legacy += 1;
            }
        }
    }
    (legacy, corrupt)
}

fn check(ok: bool, label: impl Into<String>, detail: impl Into<String>) -> DoctorCheck {
    DoctorCheck {
        ok,
        label: label.into(),
        detail: detail.into(),
    }
}
