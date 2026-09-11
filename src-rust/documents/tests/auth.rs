fn auth_draft(id: &str, key: &str) -> ProviderDraft {
    ProviderDraft {
        id: id.into(),
        in_pi: true,
        base_url: "https://example.test/v1".into(),
        api: Some("openai-completions".into()),
        api_key: key.into(),
        auth_header: true,
        headers: None,
        compat: None,
    }
}

#[test]
fn key_defaults_to_auth_json_and_keeps_a_local_copy() {
    let (_root, paths) = fixture();
    save_provider(&paths, None, &auth_draft("p", "sk-plain")).unwrap();

    assert!(read_json(&paths.pi_models)["providers"]["p"]
        .get("apiKey")
        .is_none());
    assert_eq!(
        read_json(&paths.providers)["providers"]["p"]["apiKey"],
        "sk-plain"
    );
    assert_eq!(
        read_json(&paths.pi_auth)["p"],
        json!({"type": "api_key", "key": "sk-plain"})
    );
    assert_eq!(credential_key(&paths, "p").unwrap(), Some("sk-plain".into()));
    assert!(has_credential(&paths, "p").unwrap());
    // Views surface the auth.json key so previews and edit forms are real.
    assert_eq!(
        load_snapshot(&paths).unwrap().providers[0].api_key,
        "sk-plain"
    );
    // Rotating this provider's own key needs no confirmation.
    save_provider(&paths, Some("p"), &auth_draft("p", "sk-rotated")).unwrap();
    assert_eq!(read_json(&paths.pi_auth)["p"]["key"], "sk-rotated");
}

#[test]
fn empty_key_preserves_existing_auth_credential() {
    let (_root, paths) = fixture();
    save_provider(&paths, None, &auth_draft("p", "sk-first")).unwrap();
    save_provider(&paths, Some("p"), &auth_draft("p", "")).unwrap();

    assert_eq!(
        credential_key(&paths, "p").unwrap(),
        Some("sk-first".into())
    );
}

#[test]
fn renaming_provider_moves_the_auth_credential() {
    let (_root, paths) = fixture();
    save_provider(&paths, None, &auth_draft("p", "sk-one")).unwrap();
    save_provider(&paths, Some("p"), &auth_draft("renamed", "")).unwrap();

    let auth = read_json(&paths.pi_auth);
    assert!(auth.get("p").is_none());
    assert_eq!(
        auth["renamed"],
        json!({"type": "api_key", "key": "sk-one"})
    );
    assert_eq!(
        read_json(&paths.providers)["providers"]["renamed"]["apiKey"],
        "sk-one"
    );
    assert!(read_json(&paths.pi_models)["providers"]["renamed"]
        .get("apiKey")
        .is_none());
}

#[test]
fn renames_still_account_for_the_source_credential() {
    let (_root, paths) = fixture();
    // A key that moves into models.json leaves no shadowing entry behind.
    save_provider(&paths, None, &auth_draft("p", "sk-a")).unwrap();
    set_key_storage(&paths, KeyStorage::ModelsJson).unwrap();
    save_provider(&paths, Some("p"), &auth_draft("renamed", "sk-b")).unwrap();
    assert!(read_json(&paths.pi_auth).get("p").is_none());
    assert_eq!(
        read_json(&paths.pi_models)["providers"]["renamed"]["apiKey"],
        "sk-b"
    );

    // A Pi login stays put when the rename waits for a typed key.
    set_key_storage(&paths, KeyStorage::AuthJson).unwrap();
    save_provider(&paths, None, &auth_draft("q", "")).unwrap();
    fs::write(
        &paths.pi_auth,
        r#"{"q":{"type":"oauth","access":"a","refresh":"r","expires":9}}"#,
    )
    .unwrap();
    save_provider(&paths, Some("q"), &auth_draft("r", "sk-new")).unwrap();
    let auth = read_json(&paths.pi_auth);
    assert_eq!(auth["q"]["type"], json!("oauth"));
    assert_eq!(auth["r"]["key"], "sk-new");
}

#[test]
fn models_json_storage_keeps_the_legacy_behavior() {
    let (_root, paths) = fixture();
    set_key_storage(&paths, KeyStorage::ModelsJson).unwrap();
    save_provider(&paths, None, &auth_draft("p", "$MY_ENV_KEY")).unwrap();

    assert_eq!(
        read_json(&paths.pi_models)["providers"]["p"]["apiKey"],
        "$MY_ENV_KEY"
    );
    assert!(!has_credential(&paths, "p").unwrap());
    assert_eq!(credential_key(&paths, "p").unwrap(), None);

    // Switching back applies to the next save; existing key references are
    // moved to auth.json untouched.
    set_key_storage(&paths, KeyStorage::AuthJson).unwrap();
    save_provider(&paths, Some("p"), &auth_draft("p", "$MY_ENV_KEY")).unwrap();
    // auth.json received the Pi projection; models.json stays clean.
    assert!(read_json(&paths.pi_models)["providers"]["p"]
        .get("apiKey")
        .is_none());
    assert_eq!(
        read_json(&paths.pi_auth)["p"]["key"],
        "$MY_ENV_KEY"
    );
}

#[test]
fn removing_provider_optionally_removes_auth_credential() {
    let (_root, paths) = fixture();
    fs::write(
        &paths.pi_auth,
        r#"{"other":{"type":"oauth","access":"a","refresh":"r","expires":9999999999}}"#,
    )
    .unwrap();
    save_provider(&paths, None, &auth_draft("p", "sk-a")).unwrap();

    remove_provider(&paths, "p", false).unwrap();
    assert!(has_credential(&paths, "p").unwrap());

    save_provider(&paths, None, &auth_draft("p", "sk-b")).unwrap();
    remove_provider(&paths, "p", true).unwrap();
    assert!(!has_credential(&paths, "p").unwrap());

    // Credentials of other providers (any type) are never touched, and
    // non-API-key credentials are not reported as removable API keys.
    assert_eq!(
        read_json(&paths.pi_auth)["other"]["type"],
        json!("oauth")
    );
    assert!(!has_credential(&paths, "other").unwrap());
}

#[test]
fn opencode_import_routes_keys_by_storage_setting() {
    let (_root, paths) = fixture();
    write_opencode(
        &paths,
        json!({
            "provider": {
                "gw": {
                    "npm": "@ai-sdk/openai-compatible",
                    "options": {"baseURL": "https://gw.test/v1", "apiKey": "{env:GW_KEY}"},
                    "models": {}
                }
            }
        }),
    );

    // Default auth.json mode: key goes to auth.json and the local provider copy.
    import_opencode_with_catalog(&paths, &ModelCatalog::default()).unwrap();
    assert!(read_json(&paths.pi_models)["providers"]["gw"]
        .get("apiKey")
        .is_none());
    assert_eq!(read_json(&paths.pi_auth)["gw"]["key"], "${GW_KEY}");
    assert_eq!(read_json(&paths.providers)["providers"]["gw"]["apiKey"], "${GW_KEY}");

    // models.json mode keeps the key inline; auth.json is not touched.
    set_key_storage(&paths, KeyStorage::ModelsJson).unwrap();
    write_opencode(
        &paths,
        json!({
            "provider": {
                "second": {
                    "npm": "@ai-sdk/openai-compatible",
                    "options": {"apiKey": "plain"},
                    "models": {}
                }
            }
        }),
    );
    import_opencode_with_catalog(&paths, &ModelCatalog::default()).unwrap();
    assert_eq!(
        read_json(&paths.pi_models)["providers"]["second"]["apiKey"],
        "plain"
    );
    assert!(!has_credential(&paths, "second").unwrap());
}

#[test]
fn saving_follows_the_key_pi_resolves() {
    let (_root, paths) = fixture();
    set_key_storage(&paths, KeyStorage::ModelsJson).unwrap();
    save_provider(&paths, None, &auth_draft("p", "inline")).unwrap();
    // Pi resolves auth.json first, so this is the key it actually uses.
    fs::write(&paths.pi_auth, r#"{"p":{"type":"api_key","key":"auth"}}"#).unwrap();
    let shown = load_snapshot(&paths).unwrap().providers[0].api_key.clone();
    assert_eq!(shown, "auth");

    // An unrelated edit keeps Pi's key instead of writing the stale one back.
    let mut draft = auth_draft("p", &shown);
    draft.base_url = "https://changed.test/v1".into();
    save_provider(&paths, Some("p"), &draft).unwrap();
    assert_eq!(
        read_json(&paths.pi_models)["providers"]["p"]["apiKey"],
        "auth"
    );

    // Auth.json mode leaves no inline copy behind either.
    set_key_storage(&paths, KeyStorage::AuthJson).unwrap();
    save_provider(&paths, Some("p"), &auth_draft("p", "auth")).unwrap();
    assert_eq!(read_json(&paths.pi_auth)["p"]["key"], "auth");
    assert!(read_json(&paths.pi_models)["providers"]["p"]
        .get("apiKey")
        .is_none());
}

#[test]
fn opencode_import_asks_before_replacing_a_pi_credential() {
    let (_root, paths) = fixture();
    fs::write(
        &paths.pi_auth,
        r#"{"gw":{"type":"oauth","access":"a","refresh":"r","expires":9}}"#,
    )
    .unwrap();
    write_opencode(
        &paths,
        json!({"provider": {"gw": {"npm": "@ai-sdk/openai-compatible",
            "options": {"apiKey": "imported"}, "models": {}}}}),
    );
    let plan =
        prepare_opencode_with_catalog(&paths, ModelCatalog::default(), &["gw".into()]).unwrap();
    assert_eq!(plan.credential_conflicts, vec!["gw".to_owned()]);
    assert!(matches!(
        apply_opencode_import(&paths, plan.clone(), &[]).unwrap_err(),
        AppError::CredentialOverwriteRequired(_)
    ));
    // Nothing was written before the user confirmed.
    assert_eq!(read_json(&paths.pi_auth)["gw"]["type"], json!("oauth"));
    assert!(read_optional_json(&paths.pi_models)["providers"]
        .get("gw")
        .is_none());

    apply_opencode_import_overwriting_credentials(&paths, plan, &[]).unwrap();
    assert_eq!(read_json(&paths.pi_auth)["gw"]["key"], "imported");
}

#[test]
fn opencode_import_keeps_keys_and_provider_documents_in_step() {
    let (_root, paths) = fixture();
    write_opencode(
        &paths,
        json!({"provider": {"gw": {"npm": "@ai-sdk/openai-compatible",
            "options": {"apiKey": "inline"}, "models": {}}}}),
    );
    // A plain entry of ours is retired, so it cannot shadow the inline key.
    fs::write(
        &paths.pi_auth,
        r#"{"gw":{"type":"api_key","key":"sk-old"}}"#,
    )
    .unwrap();
    set_key_storage(&paths, KeyStorage::ModelsJson).unwrap();
    import_opencode_with_catalog(&paths, &ModelCatalog::default()).unwrap();
    assert!(read_json(&paths.pi_auth).get("gw").is_none());
    assert_eq!(
        read_json(&paths.pi_models)["providers"]["gw"]["apiKey"],
        "inline"
    );

    // A Pi login asks first and then goes with the confirmation.
    fs::write(
        &paths.pi_auth,
        r#"{"gw":{"type":"oauth","access":"a","refresh":"r","expires":9}}"#,
    )
    .unwrap();
    assert!(matches!(
        import_opencode_with_catalog(&paths, &ModelCatalog::default()).unwrap_err(),
        AppError::CredentialOverwriteRequired(_)
    ));
    assert_eq!(read_json(&paths.pi_auth)["gw"]["type"], json!("oauth"));
    import_opencode_overwriting_credentials(&paths, &ModelCatalog::default()).unwrap();
    assert!(read_json(&paths.pi_auth).get("gw").is_none());

    // auth.json mode keeps the key in pi-switch's local copy and auth.json.
    set_key_storage(&paths, KeyStorage::AuthJson).unwrap();
    import_opencode_with_catalog(&paths, &ModelCatalog::default()).unwrap();
    assert_eq!(read_json(&paths.pi_auth)["gw"]["key"], "inline");
    assert!(read_json(&paths.pi_models)["providers"]["gw"]
        .get("apiKey")
        .is_none());
    assert_eq!(read_json(&paths.providers)["providers"]["gw"]["apiKey"], "inline");
}

#[test]
fn snapshot_survives_a_missing_or_invalid_auth_json() {
    let (_root, paths) = fixture();
    save_provider(&paths, None, &auth_draft("p", "sk-a")).unwrap();
    let snapshot = load_snapshot(&paths).unwrap();
    assert_eq!(snapshot.key_storage, KeyStorage::AuthJson);

    fs::write(&paths.pi_auth, "{broken").unwrap();
    // Snapshot loading itself does not depend on auth.json.
    load_snapshot(&paths).unwrap();
}

#[test]
fn invalid_key_storage_setting_is_rejected() {
    let (_root, paths) = fixture();
    set_key_storage(&paths, KeyStorage::ModelsJson).unwrap();
    assert_eq!(
        load_snapshot(&paths).unwrap().key_storage,
        KeyStorage::ModelsJson
    );
    fs::write(&paths.app_settings, r#"{"keyStorage":"elsewhere"}"#).unwrap();
    assert!(load_snapshot(&paths).is_err());
}

#[test]
fn renaming_onto_a_taken_id_leaves_credentials_untouched() {
    let (_root, paths) = fixture();
    save_provider(&paths, None, &auth_draft("p", "sk-p")).unwrap();
    save_provider(&paths, None, &auth_draft("q", "sk-q")).unwrap();

    // 'q' still exists as a provider: rejected before any credential moves.
    assert!(save_provider(&paths, Some("p"), &auth_draft("q", "")).is_err());

    // 'q' keeps only its credential: replacing it needs confirmation.
    remove_provider(&paths, "q", false).unwrap();
    assert!(matches!(
        save_provider(&paths, Some("p"), &auth_draft("q", "")).unwrap_err(),
        AppError::CredentialOverwriteRequired(_)
    ));
    let auth = read_json(&paths.pi_auth);
    assert_eq!(auth["p"]["key"], "sk-p");
    assert_eq!(auth["q"]["key"], "sk-q");
}

#[test]
fn removing_a_provider_leaves_non_api_key_credentials_alone() {
    let (_root, paths) = fixture();
    save_provider(&paths, None, &auth_draft("p", "sk-a")).unwrap();
    // Pi may have replaced the entry with an OAuth credential meanwhile.
    fs::write(
        &paths.pi_auth,
        r#"{"p":{"type":"oauth","access":"a","refresh":"r","expires":9}}"#,
    )
    .unwrap();

    remove_provider(&paths, "p", true).unwrap();
    assert_eq!(read_json(&paths.pi_auth)["p"]["type"], json!("oauth"));
}

#[test]
fn key_only_opencode_reimport_persists_the_credential() {
    let (_root, paths) = fixture();
    let opencode = |key: &str| {
        json!({"provider": {"gw": {"npm": "@ai-sdk/openai-compatible",
            "options": {"baseURL": "https://gw.test/v1", "apiKey": key}, "models": {}}}})
    };
    write_opencode(&paths, opencode("first"));
    import_opencode_with_catalog(&paths, &ModelCatalog::default()).unwrap();

    // Same provider documents, new key: the credential must still be written.
    write_opencode(&paths, opencode("second"));
    let summary = import_opencode_with_catalog(&paths, &ModelCatalog::default()).unwrap();
    assert!(summary.changed);
    assert_eq!(read_json(&paths.pi_auth)["gw"]["key"], "second");
}

#[test]
fn replacing_a_pi_credential_requires_confirmation() {
    let (_root, paths) = fixture();
    fs::write(
        &paths.pi_auth,
        r#"{"pi-login":{"type":"oauth","access":"a","refresh":"r","expires":9}}"#,
    )
    .unwrap();

    let draft = auth_draft("pi-login", "sk-new");
    assert!(matches!(
        save_provider(&paths, None, &draft).unwrap_err(),
        AppError::CredentialOverwriteRequired(_)
    ));
    // Nothing was written before the user confirmed.
    assert_eq!(read_json(&paths.pi_auth)["pi-login"]["type"], json!("oauth"));
    assert!(load_snapshot(&paths).unwrap().providers.is_empty());

    save_provider_overwriting_credential(&paths, None, &draft).unwrap();
    assert_eq!(
        read_json(&paths.pi_auth)["pi-login"],
        json!({"type": "api_key", "key": "sk-new"})
    );
}

#[test]
fn saving_a_key_keeps_the_rest_of_the_api_key_entry() {
    let (_root, paths) = fixture();
    save_provider(&paths, None, &auth_draft("cf", "sk-old")).unwrap();
    // Pi (or the user) adds provider-scoped env values to the same entry.
    let mut auth = read_json(&paths.pi_auth);
    auth["cf"]["env"] = json!({"CLOUDFLARE_ACCOUNT_ID": "account"});
    fs::write(&paths.pi_auth, serde_json::to_vec(&auth).unwrap()).unwrap();

    save_provider(&paths, Some("cf"), &auth_draft("cf", "sk-new")).unwrap();
    assert_eq!(
        read_json(&paths.pi_auth)["cf"],
        json!({"type": "api_key", "key": "sk-new", "env": {"CLOUDFLARE_ACCOUNT_ID": "account"}})
    );
}

#[test]
fn models_json_mode_retires_the_shadowing_auth_entry() {
    let (_root, paths) = fixture();
    save_provider(&paths, None, &auth_draft("plain", "sk-a")).unwrap();
    save_provider(&paths, None, &auth_draft("cf", "sk-a")).unwrap();
    let mut auth = read_json(&paths.pi_auth);
    auth["cf"]["env"] = json!({"CLOUDFLARE_ACCOUNT_ID": "account"});
    fs::write(&paths.pi_auth, serde_json::to_vec(&auth).unwrap()).unwrap();
    set_key_storage(&paths, KeyStorage::ModelsJson).unwrap();

    // A plain entry is retired; the key itself lands in the provider JSON.
    save_provider(&paths, Some("plain"), &auth_draft("plain", "sk-b")).unwrap();
    assert!(read_json(&paths.pi_auth).get("plain").is_none());
    assert_eq!(
        read_json(&paths.pi_models)["providers"]["plain"]["apiKey"],
        "sk-b"
    );

    // Provider config cannot move into models.json, so this one asks first.
    assert!(matches!(
        save_provider(&paths, Some("cf"), &auth_draft("cf", "sk-b")).unwrap_err(),
        AppError::CredentialOverwriteRequired(_)
    ));
    assert!(read_json(&paths.pi_auth).get("cf").is_some());

    save_provider_overwriting_credential(&paths, Some("cf"), &auth_draft("cf", "sk-b")).unwrap();
    assert!(read_json(&paths.pi_auth).get("cf").is_none());
    assert_eq!(
        read_json(&paths.pi_models)["providers"]["cf"]["apiKey"],
        "sk-b"
    );
}

