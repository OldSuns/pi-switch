    #[test]
    fn responsive_layout_renders_wide_and_narrow_with_cjk() {
        let (root, mut app) = app();
        for (width, height) in [(120, 36), (80, 24), (64, 20)] {
            let backend = TestBackend::new(width, height);
            let mut terminal = Terminal::new(backend).unwrap();

            app.page = Page::Home;
            app.focus = Focus::Menu;
            terminal.draw(|frame| draw(frame, &mut app)).unwrap();
            let menu = terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>();
            assert!(menu.contains("pi-switch"));
            assert!(menu.contains("Home"));
            assert!(menu.contains("Profiles"));
            assert!(menu.contains("Sessions"));
            assert!(menu.contains("Settings"));
            if width == 120 {
                assert!(menu.contains("models.json"));
                assert!(menu.contains("settings.json"));
            }

            app.page = Page::Profiles;
            app.focus = Focus::Providers;
            terminal.draw(|frame| draw(frame, &mut app)).unwrap();
            let profiles = terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>();
            assert!(profiles.contains("Providers"));
            assert!(profiles.contains("openai-completions"));
            assert!(profiles.contains("new"));

            app.page = Page::Settings;
            app.focus = Focus::Content;
            terminal.draw(|frame| draw(frame, &mut app)).unwrap();
            let settings = terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>();
            assert!(settings.contains("Configuration"));
            assert!(settings.contains("Actions"));
            assert!(settings.contains("Enter/Space run"));
            if height >= 24 {
                assert!(settings.contains("Fetch model metadata from models.dev"));
                assert!(!settings.contains("Default model parameters"));
                assert!(settings.contains("Import from OpenCode"));
            }
        }
        assert_eq!(truncate_width("示例-provider", 8), "示例-...");
        assert_eq!(mask_secret("sk-1234567890abcdef"), "sk-1...cdef");
        assert_eq!(UnicodeWidthStr::width("示例-provider"), 13);
        let wrapped = wrap_width("示例-provider", 6);
        assert_eq!(wrapped.concat(), "示例-provider");
        assert!(wrapped
            .iter()
            .all(|line| UnicodeWidthStr::width(line.as_str()) <= 6));

        app.overlay = Some(Overlay::Loading {
            message: "loading".into(),
        });
        app.on_overlay_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(matches!(app.overlay, Some(Overlay::Loading { .. })));

        app.overlay = None;
        app.language = super::i18n::Language::Chinese;
        app.page = Page::Settings;
        app.focus = Focus::Content;
        let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let chinese = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        let chinese = chinese.replace(' ', "");
        assert!(chinese.contains("设置"));
        assert!(chinese.contains("语言:中文"));
        assert!(chinese.contains("从models.dev获取模型信息"));
        assert!(!chinese.contains("默认模型参数"));
        assert!(chinese.contains("从OpenCode导入"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn settings_show_model_defaults_only_when_metadata_fetch_is_off() {
        let (root, mut app) = app();
        app.page = Page::Settings;
        app.focus = Focus::Content;
        let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();

        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let enabled = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(!enabled.contains("Default model parameters"));
        assert!(enabled.contains("●  Fetch model metadata from models.dev"));

        app.snapshot.fetch_model_metadata = false;
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let disabled = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(disabled.contains("Default model parameters"));
        assert!(disabled.contains("○  Fetch model metadata from models.dev"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn model_defaults_dialog_keeps_its_left_border() {
        let (root, mut app) = app();
        app.language = super::i18n::Language::Chinese;
        app.page = Page::Settings;
        app.focus = Focus::Content;
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        app.overlay = Some(Overlay::ModelDefaultsForm(ModelDefaultsFormState::new(
            &app.snapshot.model_defaults,
        )));

        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let buffer = terminal.backend().buffer();
        let symbols = buffer.content();
        let symbol_at = |x: u16, y: u16| symbols[(y * 80 + x) as usize].symbol();
        assert_eq!(symbol_at(19, 4), "┌");
        for y in 5..18 {
            assert_eq!(symbol_at(19, y), "│", "missing left border at row {y}");
        }
        assert_eq!(symbol_at(19, 18), "└");
        let content = symbols.iter().map(|cell| cell.symbol()).collect::<String>();
        assert!(content.contains("128000"));
        assert!(content.contains("16384"));
        if let Some(Overlay::ModelDefaultsForm(form)) = app.overlay.as_mut() {
            form.context_window = "256000".into();
            form.field = 1;
        }
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("256000"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn backups_open_restore_confirmation_with_space() {
        let (root, mut app) = app();
        app.overlay = Some(Overlay::Backups {
            items: vec![Backup {
                path: root
                    .join("backup-2026-07-23_14-36-08-527.json")
                    .display()
                    .to_string(),
                name: "backup-2026-07-23_14-36-08-527.json".into(),
            }],
            selected: 0,
        });
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("backup-2026-07-23_14-36-08-527.json"));
        assert!(content.contains("Enter/Space"));
        app.on_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        assert!(matches!(app.overlay, Some(Overlay::ConfirmRestore(_))));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn chinese_provider_form_values_share_one_column() {
        let (root, mut app) = app();
        app.language = super::i18n::Language::Chinese;
        let mut form = FormState::add();
        form.id = "#".into();
        form.base_url = "@".into();
        form.headers_json = "H".into();
        form.compat_json = "%".into();
        form.field = 2;
        app.overlay = Some(Overlay::Form(form));
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let symbols = terminal.backend().buffer().content();
        let value_x = |row: u16, value: &str| {
            (0..80)
                .find(|x| symbols[(row * 80 + x) as usize].symbol() == value)
                .unwrap()
        };
        let columns = [
            value_x(2, "#"),
            value_x(4, "@"),
            value_x(12, "<"),
            value_x(14, "<"),
            value_x(16, "%"),
        ];
        assert!(
            columns.iter().all(|column| *column == columns[0]),
            "{columns:?}"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn profiles_use_the_actual_layout_width_for_responsive_breakpoints() {
        let (root, mut app) = app();
        app.page = Page::Profiles;
        app.focus = Focus::Providers;
        for (width, menu_visible, detail_visible) in [
            (94, true, true),
            (80, false, true),
            (48, false, true),
            (47, false, false),
        ] {
            let mut terminal = Terminal::new(TestBackend::new(width, 24)).unwrap();
            terminal.draw(|frame| draw(frame, &mut app)).unwrap();
            let content = terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>();

            assert_eq!(content.contains("Home"), menu_visible);
            assert_eq!(content.contains("Base URL"), detail_visible);
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn compact_profiles_wrap_detail_values_and_model_metadata() {
        let (root, mut app) = app();
        app.page = Page::Profiles;
        app.focus = Focus::Providers;
        let mut terminal = Terminal::new(TestBackend::new(48, 24)).unwrap();
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let rows = terminal
            .backend()
            .buffer()
            .content()
            .chunks(48)
            .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
            .collect::<Vec<_>>();
        let base_url_row = rows
            .iter()
            .position(|row| row.contains("Base URL"))
            .unwrap();

        assert!(rows.iter().any(|row| row.contains("https://example.tes")));
        assert!(rows.iter().any(|row| row.contains("t/v1")));
        assert_eq!(rows[base_url_row + 1].chars().nth(29), Some('t'));
        assert!(rows.iter().any(|row| row.contains("model-a display")));
        assert!(rows.iter().any(|row| row.contains("ctx 128k")));
        assert!(!rows
            .iter()
            .any(|row| row.contains("https://example.test/v1...")));
        let _ = fs::remove_dir_all(root);
    }

