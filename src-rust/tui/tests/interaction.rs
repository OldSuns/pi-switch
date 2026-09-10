    #[test]
    fn menu_routes_pages_and_scopes_profile_commands() {
        let (_root, mut app) = app();
        app.on_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        assert!(app.overlay.is_none());

        app.narrow_detail = true;
        app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert!(app.page == Page::Profiles);
        assert!(!app.narrow_detail);
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(app.focus == Focus::Providers);
        app.width = 47;
        app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        assert!(app.focus == Focus::Models);
        assert!(app.narrow_detail);
        app.on_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        assert!(app.focus == Focus::Providers);
        assert!(!app.narrow_detail);
        app.on_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        assert!(matches!(app.overlay, Some(Overlay::Form(_))));

        app.overlay = None;
        app.on_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        assert!(app.focus == Focus::Menu);
        app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert!(app.page == Page::Sessions);
        app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert!(app.page == Page::Settings);
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(app.focus == Focus::Content);
        app.on_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
        assert!(matches!(app.overlay, Some(Overlay::Doctor(_))));

        app.overlay = None;
        app.settings_cursor = 0;
        app.on_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        assert_eq!(app.language, super::i18n::Language::Chinese);

        app.settings_cursor = 1;
        app.on_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        assert!(!app.snapshot.fetch_model_metadata);
        app.settings_cursor = 2;
        app.on_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        assert!(matches!(app.overlay, Some(Overlay::ModelDefaultsForm(_))));

        app.overlay = None;
        fs::create_dir_all(app.paths.opencode.parent().unwrap()).unwrap();
        fs::write(
            &app.paths.opencode,
            r#"{"provider":{"first":{"npm":"@ai-sdk/openai-compatible"},"second":{"npm":"@ai-sdk/openai-compatible"}}}"#,
        )
        .unwrap();
        app.settings_cursor = 6;
        app.on_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        assert!(matches!(
            app.overlay,
            Some(Overlay::OpenCodeProviders {
                ref providers,
                ref selected,
                ..
            }) if providers.len() == 2 && selected.is_empty()
        ));
        app.notice = None;
        app.on_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(matches!(
            app.overlay,
            Some(Overlay::Loading {
                cancelable: true,
                ..
            })
        ));
        let mut imported = false;
        for _ in 0..100 {
            app.tick();
            imported |= app.notice.is_some();
            if imported && app.overlay.is_none() {
                break;
            }
            thread::sleep(Duration::from_millis(1));
        }
        assert!(imported, "OpenCode import did not finish");
        assert!(
            app.overlay.is_none(),
            "loading overlay remained after import"
        );
    }

    #[test]
    fn corrupt_provider_library_opens_a_blocking_warning_once() {
        let (_root, app) = app();
        fs::create_dir_all(app.paths.providers.parent().unwrap()).unwrap();
        fs::create_dir_all(app.paths.pi_models.parent().unwrap()).unwrap();
        fs::write(&app.paths.providers, "{broken").unwrap();
        fs::write(&app.paths.pi_models, r#"{"providers":{}}"#).unwrap();

        let mut rebuilt = App::new(app.paths.clone());
        assert!(matches!(rebuilt.overlay, Some(Overlay::Warning(_))));
        rebuilt.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(rebuilt.overlay.is_none());
        let reloaded = App::new(rebuilt.paths.clone());
        assert!(reloaded.overlay.is_none());
    }

    #[test]
    fn text_editing_keeps_unicode_cursor_boundaries() {
        let mut value = "中a".to_owned();
        let mut cursor = 2;

        edit_text_key(
            &mut value,
            &mut cursor,
            KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
        );
        edit_text_key(
            &mut value,
            &mut cursor,
            KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE),
        );
        edit_text_key(
            &mut value,
            &mut cursor,
            KeyEvent::new(KeyCode::Home, KeyModifiers::NONE),
        );
        edit_text_key(
            &mut value,
            &mut cursor,
            KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE),
        );

        assert_eq!((value.as_str(), cursor), ("b", 0));
    }

    #[test]
    fn ctrl_u_clears_text_field_and_resets_cursor() {
        let mut value = "https://api.openai.com/v1".to_owned();
        let mut cursor = value.chars().count();

        edit_text_key(
            &mut value,
            &mut cursor,
            KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL),
        );

        assert_eq!((value.as_str(), cursor), ("", 0));
    }

    #[test]
    fn shortcuts_use_the_same_intuitive_commands_shown_in_the_ui() {
        assert!(matches!(
            command_for(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE)),
            Some(Command::New)
        ));
        assert!(matches!(
            command_for(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE)),
            Some(Command::SetDefault)
        ));
        assert!(matches!(
            command_for(KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE)),
            Some(Command::Delete)
        ));
        assert!(command_for(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)).is_none());
        assert!(command_for(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE)).is_none());
    }

    #[test]
    fn provider_and_model_space_actions_are_scoped_by_focus() {
        let (_root, mut app) = app();
        fs::create_dir_all(app.paths.providers.parent().unwrap()).unwrap();
        fs::create_dir_all(app.paths.pi_models.parent().unwrap()).unwrap();
        let provider = json!({
            "baseUrl": "https://example.test/v1",
            "api": "openai-completions",
            "models": [{"id":"model-a","name":"model-a display"}]
        });
        fs::write(
            &app.paths.providers,
            serde_json::to_vec_pretty(&json!({
                "version": 1,
                "providers": {"示例-provider": provider.clone()}
            }))
            .unwrap(),
        )
        .unwrap();
        fs::write(
            &app.paths.pi_models,
            serde_json::to_vec_pretty(&json!({"providers":{"示例-provider":provider}})).unwrap(),
        )
        .unwrap();
        fs::write(
            &app.paths.pi_settings,
            r#"{"defaultProvider":"示例-provider","defaultModel":"model-a"}"#,
        )
        .unwrap();
        app.reload(None);
        app.page = Page::Profiles;
        app.focus = Focus::Providers;

        app.on_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        assert!(matches!(
            app.overlay,
            Some(Overlay::ConfirmRemoveProviderFromPi(ref id)) if id == "示例-provider"
        ));
        app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(app.snapshot.providers[0].in_pi);

        app.focus = Focus::Models;
        app.on_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        let settings: serde_json::Value =
            serde_json::from_slice(&fs::read(&app.paths.pi_settings).unwrap()).unwrap();
        assert_eq!(settings["defaultProvider"], "示例-provider");
        assert_eq!(settings["defaultModel"], "model-a");

        app.focus = Focus::Providers;
        app.on_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        assert!(matches!(
            app.overlay,
            Some(Overlay::ConfirmRemoveProviderFromPi(_))
        ));
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(!app.snapshot.providers[0].in_pi);
        let models: serde_json::Value =
            serde_json::from_slice(&fs::read(&app.paths.pi_models).unwrap()).unwrap();
        assert!(models["providers"].get("示例-provider").is_none());

        app.focus = Focus::Models;
        app.notice = None;
        app.on_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        assert!(app.notice.is_some());
    }

    #[test]
    fn profile_footer_shows_only_the_current_left_action() {
        let (_root, mut app) = app();
        app.page = Page::Profiles;
        app.focus = Focus::Models;
        let mut terminal = Terminal::new(TestBackend::new(120, 24)).unwrap();

        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let models_footer = buffer_string(&terminal);
        assert!(models_footer.contains("Left providers"));
        assert!(!models_footer.contains("Left menu"));

        app.focus = Focus::Providers;
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let providers_footer = buffer_string(&terminal);
        assert!(providers_footer.contains("Left menu"));
        assert!(providers_footer.contains("Enter models"));
        assert!(providers_footer.contains("Space add/remove"));
        assert!(!providers_footer.contains("Left providers"));
    }

    #[test]
    fn interactive_overlays_show_their_key_hints() {
        let (_root, mut app) = app();
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();

        app.overlay = Some(Overlay::Fetched {
            provider_id: "custom".into(),
            models: vec![catalog_model("model-a", 128_000, 16_384, 1.0)],
            unavailable: 0,
            selected: [0].into_iter().collect(),
            cursor: 0,
            ratio_config_used: false,
            overwrite: false,
            existing: std::collections::BTreeSet::new(),
            filter: String::new(),
            filtering: false,
        });
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let fetched = buffer_string(&terminal);
        assert!(fetched.contains("Space toggle"));
        assert!(fetched.contains("a all"));
        assert!(fetched.contains("n none"));
        assert!(fetched.contains("i invert"));
        assert!(fetched.contains("o overwrite"));
        assert!(fetched.contains("/ filter"));
        assert!(fetched.contains("Enter/s import"));
        assert!(fetched.contains("Esc cancel"));

        app.overlay = Some(Overlay::OpenCodeProviders {
            providers: vec!["one".into(), "two".into()],
            selected: [0, 1].into_iter().collect(),
            cursor: 0,
        });
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let opencode = buffer_string(&terminal);
        assert!(opencode.contains("Space toggle"));
        assert!(opencode.contains("a all"));
        assert!(opencode.contains("n none"));
        assert!(opencode.contains("i invert"));
        assert!(opencode.contains("Enter import"));
        assert!(opencode.contains("Esc cancel"));

        app.overlay = Some(Overlay::Doctor(Vec::new()));
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let doctor = buffer_string(&terminal);
        assert!(doctor.contains("Esc/Enter close"));
    }

    #[test]
    fn fetched_catalog_filters_selects_all_and_requires_a_choice() {
        let (_root, mut app) = app();
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        app.overlay = Some(Overlay::Fetched {
            provider_id: "custom".into(),
            models: vec![
                catalog_model("gpt-4", 128_000, 16_384, 1.0),
                catalog_model("claude-3", 200_000, 8_192, 2.0),
                catalog_model("gemini-1.5", 1_000_000, 32_768, 3.0),
            ],
            unavailable: 0,
            selected: std::collections::BTreeSet::new(),
            cursor: 0,
            ratio_config_used: true,
            overwrite: false,
            existing: std::collections::BTreeSet::new(),
            filter: String::new(),
            filtering: false,
        });

        // Importing with nothing selected warns and keeps the overlay open.
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(matches!(app.overlay, Some(Overlay::Fetched { .. })));
        assert!(app.notice.is_some());
        app.notice = None;

        // 'a' selects every model; 'n' clears them all.
        app.on_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        if let Some(Overlay::Fetched { ref selected, .. }) = app.overlay {
            assert_eq!(selected.len(), 3);
        }
        app.on_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        if let Some(Overlay::Fetched { ref selected, .. }) = app.overlay {
            assert!(selected.is_empty());
        }

        // 'i' inverts the selection: nothing → all, all → nothing.
        app.on_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE));
        if let Some(Overlay::Fetched { ref selected, .. }) = app.overlay {
            assert_eq!(selected.len(), 3);
        }
        app.on_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE));
        if let Some(Overlay::Fetched { ref selected, .. }) = app.overlay {
            assert!(selected.is_empty());
        }

        // '/' enters filtering; typing narrows the visible list.
        app.on_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        app.on_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE));
        app.on_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE));
        app.on_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE));
        if let Some(Overlay::Fetched {
            ref filter,
            ref filtering,
            ..
        }) = app.overlay
        {
            assert_eq!(filter, "gpt");
            assert!(*filtering);
        }

        // Enter exits filter editing but retains the text.
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        if let Some(Overlay::Fetched {
            ref filter,
            ref filtering,
            ..
        }) = app.overlay
        {
            assert_eq!(filter, "gpt");
            assert!(!*filtering);
        }

        // 'a' now selects only the visible (filtered) model.
        app.on_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        if let Some(Overlay::Fetched { ref selected, .. }) = app.overlay {
            assert_eq!(selected.len(), 1);
        }

        // Render: only gpt-4 is visible; the price source label shows ratio_config.
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let content = buffer_string(&terminal);
        assert!(content.contains("gpt-4"));
        assert!(!content.contains("claude-3"));
        assert!(!content.contains("gemini-1.5"));
        assert!(content.contains("ratio_config"));

        // Esc cancels and closes the overlay.
        app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(app.overlay.is_none());
    }

    #[test]
    fn fetched_existing_models_show_tag_and_overwrite_toggles() {
        let (_root, mut app) = app();
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        app.overlay = Some(Overlay::Fetched {
            provider_id: "示例-provider".into(),
            models: vec![
                catalog_model("model-a", 128_000, 16_384, 1.0),
                catalog_model("model-c", 200_000, 8_192, 2.0),
            ],
            unavailable: 0,
            selected: [0].into_iter().collect(),
            cursor: 0,
            ratio_config_used: false,
            overwrite: false,
            existing: ["model-a".into()].into_iter().collect(),
            filter: String::new(),
            filtering: false,
        });
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let content = buffer_string(&terminal);
        // model-a is existing → tagged "exists"; model-c is not.
        assert!(content.contains("exists"));
        // Title shows the overwrite state (off by default).
        assert!(content.contains("overwrite: off"));
        // 'o' toggles overwrite on.
        app.on_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));
        if let Some(Overlay::Fetched { overwrite, .. }) = app.overlay {
            assert!(overwrite);
        }
    }

    #[test]
    fn opencode_provider_selector_all_none_and_invert() {
        let (_root, mut app) = app();
        app.overlay = Some(Overlay::OpenCodeProviders {
            providers: vec!["one".into(), "two".into(), "three".into()],
            selected: std::collections::BTreeSet::new(),
            cursor: 0,
        });

        // 'a' selects all.
        app.on_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        if let Some(Overlay::OpenCodeProviders { ref selected, .. }) = app.overlay {
            assert_eq!(selected.len(), 3);
        }

        // 'n' clears all.
        app.on_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        if let Some(Overlay::OpenCodeProviders { ref selected, .. }) = app.overlay {
            assert!(selected.is_empty());
        }

        // 'i' inverts empty → all, then all → empty.
        app.on_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE));
        if let Some(Overlay::OpenCodeProviders { ref selected, .. }) = app.overlay {
            assert_eq!(selected.len(), 3);
        }
        app.on_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE));
        if let Some(Overlay::OpenCodeProviders { ref selected, .. }) = app.overlay {
            assert!(selected.is_empty());
        }

        // 'i' with a partial selection toggles only the missing/extra items.
        app.on_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        app.on_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        if let Some(Overlay::OpenCodeProviders { ref selected, .. }) = app.overlay {
            assert_eq!(selected.len(), 2);
        }
        app.on_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE));
        if let Some(Overlay::OpenCodeProviders { ref selected, .. }) = app.overlay {
            assert_eq!(selected.len(), 1);
        }
    }

    #[test]
    fn fetched_existing_models_skipped_on_import_when_overwrite_off() {
        let (_root, mut app) = app();
        app.overlay = Some(Overlay::Fetched {
            provider_id: "示例-provider".into(),
            models: vec![
                catalog_model("model-a", 128_000, 16_384, 1.0),
                catalog_model("model-c", 200_000, 8_192, 2.0),
            ],
            unavailable: 0,
            selected: [0].into_iter().collect(),
            cursor: 0,
            ratio_config_used: false,
            overwrite: false,
            existing: ["model-a".into()].into_iter().collect(),
            filter: String::new(),
            filtering: false,
        });
        // Only model-a is selected and it already exists; overwrite is off.
        // Pressing Enter should NOT start a metadata resolve (overlay stays
        // Fetched, no Loading) and should show a warning notice instead.
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(!matches!(
            app.overlay,
            Some(Overlay::MetadataLoading { .. })
        ));
        assert!(app.notice.is_some(), "a notice should be shown");
        // The Fetched overlay should still be open so the user can adjust.
        assert!(matches!(app.overlay, Some(Overlay::Fetched { .. })));

        // Now toggle overwrite on and press Enter again — the flow should
        // proceed (overlay switches to MetadataLoading as metadata resolve starts).
        app.notice = None;
        app.on_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(matches!(
            app.overlay,
            Some(Overlay::MetadataLoading { .. })
        ));
    }

    #[test]
    fn readonly_loading_escape_cancels_and_discards_results() {
        let (_root, mut app) = app();
        let (sender, receiver) = std::sync::mpsc::channel();
        sender
            .send(Ok(BackgroundResult::ModelIds {
                provider_id: "queued-provider".into(),
                ids: vec!["queued-model".into()],
            }))
            .unwrap();
        app.task = Some(receiver);
        app.overlay = Some(Overlay::Loading {
            message: "Reading".into(),
            cancelable: true,
        });
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        assert!(buffer_string(&terminal).contains("Esc cancel"));

        app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

        assert!(app.task.is_none());
        assert!(app.overlay.is_none());
        assert!(app
            .notice
            .as_ref()
            .is_some_and(|notice| notice.message.contains("cancelled")));
        assert!(sender
            .send(Ok(BackgroundResult::ModelIds {
                provider_id: "late-provider".into(),
                ids: vec!["late-model".into()],
            }))
            .is_err());
        app.tick();
        assert!(app.overlay.is_none());
    }

    #[test]
    fn metadata_loading_escape_imports_defaults_and_discards_queued_result() {
        let (_root, mut app) = app();
        write_empty_provider(&mut app);
        app.snapshot.model_defaults.context_window = Some(64_000);

        let (sender, receiver) = std::sync::mpsc::channel();
        let result = |context_window| {
            Ok(BackgroundResult::Catalog {
                provider_id: "示例-provider".into(),
                fetched: CatalogFetch {
                    models: vec![catalog_model("late-model", context_window, 32_000, 1.0)],
                    ambiguous: Vec::new(),
                    unavailable: 0,
                    ratio_prices: std::collections::BTreeMap::new(),
                    ratio_config_used: false,
                    catalog_unreachable: false,
                },
                overwrite: true,
            })
        };
        sender.send(result(200_000)).unwrap();
        app.task = Some(receiver);
        app.overlay = Some(Overlay::MetadataLoading {
            provider_id: "示例-provider".into(),
            ids: vec!["late-model".into()],
            overwrite: true,
        });

        app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(app.task.is_none());
        assert!(app.overlay.is_none());
        assert!(app
            .notice
            .as_ref()
            .is_some_and(|notice| notice.message.contains("skipped online metadata")));
        assert!(sender.send(result(300_000)).is_err());

        app.tick();
        let providers: serde_json::Value =
            serde_json::from_slice(&fs::read(&app.paths.providers).unwrap()).unwrap();
        let models = providers["providers"]["示例-provider"]["models"]
            .as_array()
            .unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0]["id"], "late-model");
        assert_eq!(models[0]["contextWindow"], 64_000);
    }

    #[test]
    fn metadata_loading_closes_after_successful_import() {
        let (_root, mut app) = app();
        write_empty_provider(&mut app);
        let (sender, receiver) = std::sync::mpsc::channel();
        sender
            .send(Ok(BackgroundResult::Catalog {
                provider_id: "示例-provider".into(),
                fetched: CatalogFetch {
                    models: vec![catalog_model("resolved-model", 200_000, 32_000, 1.0)],
                    ambiguous: Vec::new(),
                    unavailable: 0,
                    ratio_prices: std::collections::BTreeMap::new(),
                    ratio_config_used: false,
                    catalog_unreachable: false,
                },
                overwrite: true,
            }))
            .unwrap();
        app.task = Some(receiver);
        app.overlay = Some(Overlay::MetadataLoading {
            provider_id: "示例-provider".into(),
            ids: vec!["resolved-model".into()],
            overwrite: true,
        });

        app.tick();

        assert!(app.task.is_none());
        assert!(app.overlay.is_none(), "metadata loading overlay remained");
        assert!(app
            .notice
            .as_ref()
            .is_some_and(|notice| notice.message.contains("Added 1")));
        let providers: serde_json::Value =
            serde_json::from_slice(&fs::read(&app.paths.providers).unwrap()).unwrap();
        let model = &providers["providers"]["示例-provider"]["models"][0];
        assert_eq!(model["id"], "resolved-model");
        assert_eq!(model["contextWindow"], 200_000);
    }

    #[test]
    fn catalog_ambiguity_uses_requested_id_for_ratio_price() {
        let (_root, mut app) = app();
        write_empty_provider(&mut app);
        let (sender, receiver) = std::sync::mpsc::channel();
        sender
            .send(Ok(BackgroundResult::Catalog {
                provider_id: "示例-provider".into(),
                fetched: CatalogFetch {
                    models: Vec::new(),
                    ambiguous: vec![CatalogAmbiguity {
                        provider_id: "示例-provider".into(),
                        model_id: "gateway-model-0731".into(),
                        candidates: vec![CatalogCandidate {
                            provider_id: "catalog-provider".into(),
                            model: catalog_model("catalog-model-0731", 200_000, 32_000, 1.0),
                        }],
                    }],
                    unavailable: 0,
                    ratio_prices: [(
                        "gateway-model-0731".into(),
                        RatioCost {
                            input: 9.0,
                            output: 18.0,
                            cache_read: 1.0,
                            cache_write: 2.0,
                        },
                    )]
                    .into_iter()
                    .collect(),
                    ratio_config_used: true,
                    catalog_unreachable: false,
                },
                overwrite: false,
            }))
            .unwrap();
        app.task = Some(receiver);

        app.tick();
        let Some(Overlay::CatalogMatches { ambiguities, .. }) = &app.overlay else {
            panic!("catalog matches overlay");
        };
        assert_eq!(ambiguities[0].candidates[0].model.config["cost"]["input"], 9.0);

        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let providers: serde_json::Value =
            serde_json::from_slice(&fs::read(&app.paths.providers).unwrap()).unwrap();
        let model = &providers["providers"]["示例-provider"]["models"][0];
        assert_eq!(model["id"], "gateway-model-0731");
        assert_eq!(model["cost"]["input"], 9.0);
    }

    #[test]
    fn unmatched_fallback_notice_survives_catalog_selection() {
        let (_root, mut app) = app();
        write_empty_provider(&mut app);
        app.overlay = Some(Overlay::CatalogMatches {
            ambiguities: vec![CatalogAmbiguity {
                provider_id: "示例-provider".into(),
                model_id: "ambiguous-model".into(),
                candidates: vec![CatalogCandidate {
                    provider_id: "catalog-provider".into(),
                    model: catalog_model("catalog-source-id", 200_000, 32_000, 1.0),
                }],
            }],
            index: 0,
            cursor: 0,
            continuation: Some(CatalogContinuation::ProviderImport {
                provider_id: "示例-provider".into(),
                resolved_models: vec![catalog_model(
                    "default-model",
                    PI_DEFAULT_CONTEXT_WINDOW,
                    PI_DEFAULT_MAX_TOKENS,
                    0.0,
                )],
                candidate_indices: Vec::new(),
                overwrite: false,
                fallback: Some(MetadataFallback::Unmatched(1)),
            }),
        });

        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert!(app.overlay.is_none());
        let notice = app.notice.as_ref().expect("import notice");
        assert!(notice.message.contains("Added 2"));
        assert!(notice
            .message
            .contains("no models.dev match for 1 model(s)"));
        let providers: serde_json::Value =
            serde_json::from_slice(&fs::read(&app.paths.providers).unwrap()).unwrap();
        assert_eq!(
            providers["providers"]["示例-provider"]["models"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            providers["providers"]["示例-provider"]["models"][1]["id"],
            "ambiguous-model"
        );
    }

    #[test]
    fn replacing_a_pi_credential_asks_for_confirmation() {
        let (_root, mut app) = app();
        let auth = app.paths.pi_auth.clone();
        fs::create_dir_all(auth.parent().unwrap()).unwrap();
        fs::write(
            &auth,
            r#"{"pi-login":{"type":"oauth","access":"a","refresh":"r","expires":9}}"#,
        )
        .unwrap();
        let kind = || {
            let value: serde_json::Value =
                serde_json::from_slice(&fs::read(&auth).unwrap()).unwrap();
            value["pi-login"]["type"].as_str().unwrap().to_owned()
        };

        let mut form = FormState::add();
        form.id = "pi-login".into();
        form.api_key = "sk-new".into();
        app.overlay = Some(Overlay::Form(form));
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(matches!(
            app.overlay,
            Some(Overlay::ConfirmOverwriteCredential { .. })
        ));
        assert_eq!(kind(), "oauth");

        // Cancelling returns to the form without writing anything.
        app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(matches!(app.overlay, Some(Overlay::Form(_))));
        assert_eq!(kind(), "oauth");

        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        app.on_key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE));
        assert_eq!(kind(), "api_key");
    }

    #[test]
    fn opencode_import_asks_before_replacing_a_pi_credential() {
        let (_root, mut app) = app();
        let auth = app.paths.pi_auth.clone();
        fs::create_dir_all(auth.parent().unwrap()).unwrap();
        fs::write(
            &auth,
            r#"{"gw":{"type":"oauth","access":"a","refresh":"r","expires":9}}"#,
        )
        .unwrap();
        fs::create_dir_all(app.paths.opencode.parent().unwrap()).unwrap();
        fs::write(
            &app.paths.opencode,
            r#"{"provider":{"gw":{"npm":"@ai-sdk/openai-compatible","options":{"apiKey":"imported"}}}}"#,
        )
        .unwrap();
        let plan = crate::documents::prepare_opencode_import(
            &app.paths,
            &["gw".into()],
            ImportOptions {
                fetch_metadata: false,
                defaults: Default::default(),
            },
        )
        .unwrap();
        let kind = || {
            let value: serde_json::Value =
                serde_json::from_slice(&fs::read(&auth).unwrap()).unwrap();
            value["gw"]["type"].as_str().unwrap().to_owned()
        };

        // Cancelling writes nothing; confirming replaces the Pi login.
        app.request_opencode_apply(plan.clone(), Vec::new());
        assert!(matches!(
            app.overlay,
            Some(Overlay::ConfirmImportCredentials { .. })
        ));
        app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(app.overlay.is_none());
        assert_eq!(kind(), "oauth");

        app.request_opencode_apply(plan, Vec::new());
        app.on_key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE));
        for _ in 0..400 {
            if app.task.is_none() {
                break;
            }
            app.tick();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(app.task.is_none(), "import did not finish");
        assert_eq!(kind(), "api_key");
        assert!(app.overlay.is_none());
    }
