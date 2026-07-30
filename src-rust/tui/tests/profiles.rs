    #[test]
    fn provider_session_affinity_compat_is_first_class_and_preserves_other_keys() {
        let mut form = FormState::add();
        form.compat_json = r#"{"supportsDeveloperRole":false}"#.into();

        let compat = form.draft().unwrap().compat.unwrap();
        assert_eq!(compat["sendSessionAffinityHeaders"], true);
        assert_eq!(compat["supportsDeveloperRole"], false);

        form.send_session_affinity_headers = true;
        let compat = form.draft().unwrap().compat.unwrap();
        assert_eq!(compat["sendSessionAffinityHeaders"], true);
        assert_eq!(compat["supportsDeveloperRole"], false);

        form.send_session_affinity_headers = false;
        let compat = form.draft().unwrap().compat.unwrap();
        assert_eq!(compat["sendSessionAffinityHeaders"], false);
        assert_eq!(compat["supportsDeveloperRole"], false);

        form.compat_json =
            r#"{"sendSessionAffinityHeaders":true,"supportsDeveloperRole":false}"#.into();
        assert!(form
            .draft()
            .unwrap_err()
            .to_string()
            .contains("managed by Session affinity"));
        form.compat_json = r#"{"supportsDeveloperRole":false}"#.into();

        let mut provider = ProviderView {
            id: "custom".into(),
            in_pi: true,
            base_url: "https://example.test/v1".into(),
            api: "openai-completions".into(),
            api_key: "$KEY".into(),
            auth_header: true,
            models: Vec::new(),
            raw: json!({
                "compat": {
                    "sendSessionAffinityHeaders": true,
                    "supportsDeveloperRole": false
                }
            }),
        };
        let edited = FormState::edit(&provider);
        assert!(edited.send_session_affinity_headers);
        assert_eq!(edited.compat_json, r#"{"supportsDeveloperRole":false}"#);

        provider.raw = json!({"compat":{"supportsDeveloperRole":false}});
        let edited = FormState::edit(&provider);
        assert!(edited.send_session_affinity_headers);
        assert_eq!(edited.compat_json, r#"{"supportsDeveloperRole":false}"#);

        let (root, mut app) = app();
        let mut form = FormState::add();
        form.field = 6;
        assert!(form.send_session_affinity_headers);
        app.on_form_key(&mut form, KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        assert!(!form.send_session_affinity_headers);
        app.on_form_key(&mut form, KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        assert!(form.send_session_affinity_headers);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn contextual_actions_open_provider_and_model_crud() {
        let (root, mut app) = app();
        app.page = Page::Profiles;
        app.focus = Focus::Providers;
        app.on_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        assert!(matches!(app.overlay, Some(Overlay::Form(_))));

        app.overlay = None;
        app.focus = Focus::Models;
        app.on_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        assert!(matches!(
            app.overlay,
            Some(Overlay::ModelForm(ref form))
                if form.context_window.is_empty() && form.max_tokens.is_empty()
        ));

        app.overlay = None;
        app.on_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
        match app.overlay.as_ref() {
            Some(Overlay::ModelForm(form)) => {
                assert_eq!(form.id, "model-a");
                assert_eq!(form.name, "model-a display");
                assert_eq!(form.context_window, "128000");
            }
            _ => panic!("model edit did not open"),
        }

        app.overlay = None;
        app.on_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));
        assert!(matches!(
            app.overlay,
            Some(Overlay::ConfirmDeleteModel { ref model_id, .. }) if model_id == "model-a"
        ));

        app.overlay = None;
        app.on_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE));
        assert!(matches!(
            app.overlay,
            Some(Overlay::ModelForm(ref form)) if form.previous_id.is_none() && form.id == "model-a-copy"
        ));

        let mut provider_form = FormState::add();
        provider_form.user_agent = "  claude-cli/2.1.161  ".into();
        provider_form.headers_json = r#"{"user-agent":"old","x-api-key":"$KEY"}"#.into();
        provider_form.compat_json = r#"{"supportsDeveloperRole":false}"#.into();
        let draft = provider_form.draft().unwrap();
        let headers = draft.headers.unwrap();
        assert_eq!(headers["User-Agent"], "claude-cli/2.1.161");
        assert!(headers.get("user-agent").is_none());
        assert_eq!(headers["x-api-key"], "$KEY");
        assert_eq!(draft.compat.unwrap()["supportsDeveloperRole"], false);
        provider_form.user_agent.clear();
        let headers = provider_form.draft().unwrap().headers.unwrap();
        assert!(headers.get("User-Agent").is_none());
        provider_form.headers_json = "[]".into();
        assert!(provider_form.draft().is_err());

        let mut provider = app.snapshot.providers[0].clone();
        provider.raw = json!({
            "headers": {
                "user-agent": "legacy-agent",
                "x-team": "team"
            }
        });
        let edited = FormState::edit(&provider);
        assert_eq!(edited.user_agent, "legacy-agent");
        assert_eq!(edited.headers_json, r#"{"x-team":"team"}"#);

        let mut configured_form = FormState::add();
        configured_form.user_agent = "claude-cli/2.1.161".into();
        configured_form.headers_json = r#"{"x-api-key":"$KEY"}"#.into();
        app.overlay = Some(Overlay::Form(configured_form));
        let mut terminal = Terminal::new(TestBackend::new(64, 20)).unwrap();
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let provider_content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(provider_content.contains("Headers"));
        assert!(!provider_content.contains("(all models)"));
        assert!(provider_content.contains("User-Agent"));
        assert!(provider_content.contains("x-api-key"));

        let mut invalid_form = FormState::add();
        invalid_form.headers_json = "[".into();
        app.overlay = Some(Overlay::Form(invalid_form));
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let invalid_content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(invalid_content.contains("invalid JSON"));

        if let Some(Overlay::Form(form)) = app.overlay.as_mut() {
            *form = FormState::add();
            form.user_agent = "claude-cli/2.1.161".into();
            form.headers_json = r#"{"x-api-key":"$KEY"}"#.into();
            form.field = 5;
        }
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(matches!(
            app.overlay,
            Some(Overlay::Form(ref form)) if form.editing_headers
        ));
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(matches!(
            app.overlay,
            Some(Overlay::Form(ref form)) if form.editing_headers && form.user_agent == "claude-cli/2.1.161"
        ));
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let headers_content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(headers_content.contains("Provider headers"));
        assert!(!headers_content.contains("all models"));
        assert!(headers_content.contains("User-Agent"));
        assert!(headers_content.contains("Other headers JSON"));
        assert!(headers_content.contains("Ctrl+S"));
        assert!(headers_content.contains("Tab"));
        assert!(headers_content.contains("Esc"));
        assert!(!headers_content.contains("User-Agent is separate"));
        app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let json_content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(json_content.contains("Other headers JSON"));
        assert!(json_content.contains("$KEY"));
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        app.on_key(KeyEvent::new(KeyCode::Char('{'), KeyModifiers::NONE));
        app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(matches!(
            app.overlay,
            Some(Overlay::Form(ref form))
                if !form.editing_headers
                    && form.headers_json.contains("x-api-key")
                    && form.headers_json.ends_with("\n{")
                    && form.headers_field == 1
        ));

        let model = app.snapshot.providers[0].models[0].clone();
        app.overlay = Some(Overlay::ModelForm(ModelFormState::edit(
            "示例-provider",
            &model,
        )));
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let model_content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(model_content.contains("Context & limits"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn provider_form_question_mark_help_and_api_key_masking() {
        let (root, mut app) = app();
        app.language = super::i18n::Language::Chinese;
        let mut form = FormState::add();
        form.api_key = "sk-1234567890abcdef".into();
        form.field = 4;
        app.overlay = Some(Overlay::Form(form));
        app.on_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
        if let Some(Overlay::Form(ref form)) = app.overlay {
            assert!(form.show_help, "show_help should be true after '?'");
        }
        let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let help_content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        let help_content = help_content.replace(' ', "");
        assert!(help_content.contains("仅自定义"));
        app.on_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
        if let Some(Overlay::Form(ref form)) = app.overlay {
            assert!(!form.show_help);
        }
        if let Some(Overlay::Form(ref mut form)) = app.overlay.as_mut() {
            form.field = 3;
        }
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let masked = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(masked.contains("1234567890abcdef"));
        assert!(!masked.contains("sk-1...cdef"));
        if let Some(Overlay::Form(ref mut form)) = app.overlay.as_mut() {
            form.field = 0;
        }
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let plain = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(plain.contains("sk-1...cdef"));
        assert!(!plain.contains("1234567890abcdef"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn model_limits_are_compact_in_lists_but_raw_in_the_editor() {
        let (root, mut app) = app();
        app.snapshot.providers[0].models[0].context_window = Some(128_000);
        app.snapshot.providers[0].models[0].max_tokens = Some(1_048_576);
        app.page = Page::Profiles;
        app.focus = Focus::Providers;
        let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let profiles = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(profiles.contains("ctx 128k  max 1M"));

        let model = app.snapshot.providers[0].models[0].clone();
        app.overlay = Some(Overlay::ModelForm(ModelFormState::edit(
            "示例-provider",
            &model,
        )));
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let editor = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(editor.contains("128000"));
        assert!(editor.contains("1048576"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ambiguous_models_auto_resolve_into_the_fetched_list() {
        // Ambiguities used to gate the model list behind a metadata-source
        // picker; they now auto-resolve to the first candidate so the user
        // sees every gateway model up front in the selection list.
        let (root, mut app) = app();
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        app.overlay = Some(Overlay::Fetched {
            provider_id: "custom".into(),
            models: vec![
                catalog_model("resolved", 128_000, 16_384, 1.0),
                catalog_model("shared", 1_048_576, 128_000, 2.0),
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
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("resolved"));
        assert!(content.contains("shared"));
        assert!(content.contains("ratio_config"));
        // The auto-resolved model is selectable like any other.
        app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        app.on_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        if let Some(Overlay::Fetched { ref selected, .. }) = app.overlay {
            assert_eq!(selected.len(), 1);
        }
        let _ = fs::remove_dir_all(root);
    }

