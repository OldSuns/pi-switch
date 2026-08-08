    #[test]
    fn sessions_page_lists_filters_and_confirms_delete() {
        let _env_lock = SESSION_ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let (_root, mut app) = app();
        let sessions_root = _root.join(".pi/agent/sessions/--proj--");
        fs::create_dir_all(&sessions_root).unwrap();
        let session_path = sessions_root.join("demo.jsonl");
        fs::write(
            &session_path,
            r#"{"type":"session","version":3,"id":"demo-1","timestamp":"2026-01-02T00:00:00.000Z","cwd":"/tmp/demo"}
{"type":"session_info","id":"i1","parentId":null,"timestamp":"2026-01-02T00:00:01.000Z","name":"Demo Session"}
{"type":"message","id":"u1","parentId":"i1","timestamp":"2026-01-02T00:01:00.000Z","message":{"role":"user","content":"hello from demo","timestamp":1}}
{"type":"message","id":"a1","parentId":"u1","timestamp":"2026-01-02T00:02:00.000Z","message":{"role":"assistant","content":[{"type":"text","text":"assistant reply"}],"timestamp":2}}
"#,
        )
        .unwrap();

        // Point session listing at the fixture tree via env override.
        env::set_var(
            "PI_CODING_AGENT_SESSION_DIR",
            _root.join(".pi/agent/sessions"),
        );
        app.page = Page::Sessions;
        app.focus = Focus::Content;
        app.reload_sessions(None);
        assert_eq!(app.sessions.len(), 1);
        assert_eq!(app.sessions[0].name.as_deref(), Some("Demo Session"));
        assert!(app
            .preview
            .as_ref()
            .is_some_and(|preview| preview.messages.iter().any(|m| m.role == "user")));

        app.on_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE));
        assert!(app.user_only_preview);
        assert!(app
            .preview
            .as_ref()
            .is_some_and(|preview| preview.messages.iter().all(|m| m.role == "user")));

        app.on_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        assert!(app.named_only);
        app.on_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        assert!(app.session_filtering);
        app.on_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));
        app.on_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
        app.on_key(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE));
        app.on_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(!app.visible_sessions().is_empty());

        let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let content = buffer_string(&terminal);
        assert!(content.contains("Demo Session") || content.contains("demo"));
        assert!(content.contains("Preview") || content.contains("预览"));

        app.on_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));
        assert!(matches!(
            app.overlay,
            Some(Overlay::ConfirmDeleteSession { .. })
        ));

        env::remove_var("PI_CODING_AGENT_SESSION_DIR");
    }

    #[test]
    fn session_preview_focus_navigates_messages_and_copies() {
        let _env_lock = SESSION_ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let (_root, mut app) = app();
        let sessions_root = _root.join(".pi/agent/sessions/--proj--");
        fs::create_dir_all(&sessions_root).unwrap();
        let session_path = sessions_root.join("demo.jsonl");
        fs::write(
            &session_path,
            r#"{"type":"session","version":3,"id":"demo-1","timestamp":"2026-01-02T00:00:00.000Z","cwd":"/tmp/demo"}
{"type":"message","id":"u1","parentId":null,"timestamp":"2026-01-02T00:01:00.000Z","message":{"role":"user","content":"first user message with enough text to wrap across several preview lines for mouse wheel scrolling","timestamp":1}}
{"type":"message","id":"a1","parentId":"u1","timestamp":"2026-01-02T00:02:00.000Z","message":{"role":"assistant","content":[{"type":"text","text":"first assistant reply"}],"timestamp":2}}
{"type":"message","id":"u2","parentId":"a1","timestamp":"2026-01-02T00:03:00.000Z","message":{"role":"user","content":"second user message","timestamp":3}}
"#,
        )
        .unwrap();

        env::set_var(
            "PI_CODING_AGENT_SESSION_DIR",
            _root.join(".pi/agent/sessions"),
        );
        app.page = Page::Sessions;
        app.focus = Focus::Content;
        app.reload_sessions(None);
        assert_eq!(app.preview_message_count(), 3);

        // Only Right enters preview message selection.
        app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        assert!(app.focus == Focus::SessionPreview);
        assert_eq!(app.preview_message_cursor, 2);

        // Down stays on the active leaf at the end of the tree.
        app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(app.preview_message_cursor, 2);

        // Up moves to the previous branch node.
        app.on_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(app.preview_message_cursor, 1);

        // PageDown scrolls by viewport lines instead of jumping messages.
        app.preview_wrap_width = 12;
        app.preview_viewport_height = 4;
        app.preview_scroll = 0;
        app.on_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE));
        assert_eq!(app.preview_scroll, 3);
        assert_eq!(app.preview_message_cursor, 0);

        // Mouse-wheel style scrolling moves within a long message before changing selection.
        app.preview_scroll = 0;
        app.scroll_preview_lines(4);
        assert_eq!(app.preview_scroll, 4);
        assert_eq!(app.preview_message_cursor, 0);

        // Ctrl+C in preview copies instead of quitting.
        app.on_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(!app.quit);
        assert!(app.notice.is_some());

        // Pane switching only accepts the physical Left/Right arrows.
        app.on_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        assert!(app.focus == Focus::Content);
        for code in [KeyCode::Tab, KeyCode::Enter, KeyCode::Char('l')] {
            app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
            assert!(app.focus == Focus::Content);
        }
        app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        assert!(app.focus == Focus::SessionPreview);
        for code in [KeyCode::Tab, KeyCode::Esc, KeyCode::Char('h')] {
            app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
            assert!(app.focus == Focus::SessionPreview);
        }
        app.on_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        assert!(app.focus == Focus::Content);

        // Esc from list returns to menu.
        app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(app.focus == Focus::Menu);

        env::remove_var("PI_CODING_AGENT_SESSION_DIR");
    }

    #[test]
    fn session_preview_clears_old_background_and_resizes() {
        fn find_ascii(
            terminal: &Terminal<TestBackend>,
            width: u16,
            height: u16,
            needle: &str,
        ) -> (u16, u16) {
            let chars = needle.chars().map(|ch| ch.to_string()).collect::<Vec<_>>();
            let cells = terminal.backend().buffer().content();
            for y in 0..height {
                for x in 0..width.saturating_sub(chars.len() as u16) {
                    if chars.iter().enumerate().all(|(offset, expected)| {
                        cells[(y * width + x + offset as u16) as usize].symbol() == expected
                    }) {
                        return (x, y);
                    }
                }
            }
            panic!("{needle:?} was not rendered");
        }

        let (_root, mut app) = app();
        app.page = Page::Sessions;
        app.focus = Focus::SessionPreview;
        app.sessions_loaded = true;
        app.sessions = vec![crate::documents::SessionSummary {
            path: _root.join("demo.jsonl"),
            id: "demo-1".into(),
            cwd: "/work/demo".into(),
            name: Some("Demo Session".into()),
            created: SystemTime::now(),
            modified: SystemTime::now(),
            message_count: 2,
            first_message: "alpha highlighted body".into(),
            search_text: "alpha highlighted body short".into(),
        }];
        app.preview = Some(crate::documents::SessionPreview::from_messages(vec![
            crate::documents::PreviewMessage::new(
                "u1",
                None,
                "user",
                "**alpha** highlighted body with [link](https://example.test)",
            ),
            crate::documents::PreviewMessage::new("a1", Some("u1".into()), "assistant", "short"),
        ]));
        app.preview_visible = vec![0, 1];
        app.preview_path = Some(_root.join("demo.jsonl").display().to_string());
        app.preview_message_cursor = 0;

        let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let (alpha_x, alpha_y) = find_ascii(&terminal, 120, 30, "alpha");
        let sample_x = alpha_x + 30;
        let selected_background = terminal.backend().buffer().content()
            [(alpha_y * 120 + sample_x) as usize]
            .bg;
        assert!(!buffer_string(&terminal).contains("**alpha**"));

        app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let old_background = terminal.backend().buffer().content()
            [(alpha_y * 120 + sample_x) as usize]
            .bg;
        assert_ne!(old_background, selected_background);

        let (short_x, short_y) = find_ascii(&terminal, 120, 30, "short");
        let cells = terminal.backend().buffer().content();
        assert!((short_x + 5..short_x + 20)
            .all(|x| cells[(short_y * 120 + x) as usize].bg == selected_background));

        for (width, height) in [(120, 30), (80, 24), (64, 20)] {
            app.preview_scroll = u16::MAX;
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal.draw(|frame| draw(frame, &mut app)).unwrap();
            let content = buffer_string(&terminal);
            assert!(content.contains("Demo Session"));
            assert!(content.contains("Preview"));
            assert_ne!(app.preview_scroll, u16::MAX);
            assert_eq!(app.preview_layout.as_ref().unwrap().width, app.preview_wrap_width);
        }
    }

    #[test]
    fn session_preview_renders_tree_and_folds_branch() {
        let _env_lock = SESSION_ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let (_root, mut app) = app();
        let sessions_root = _root.join(".pi/agent/sessions/--proj--");
        fs::create_dir_all(&sessions_root).unwrap();
        fs::write(
            sessions_root.join("branch.jsonl"),
            r#"{"type":"session","version":3,"id":"branch-1","timestamp":"2026-01-01T00:00:00Z","cwd":"/work/tree"}
{"type":"message","id":"u1","parentId":null,"timestamp":"2026-01-01T00:00:01Z","message":{"role":"user","content":"root"}}
{"type":"message","id":"a1","parentId":"u1","timestamp":"2026-01-01T00:00:02Z","message":{"role":"assistant","content":[{"type":"text","text":"reply"}]}}
{"type":"message","id":"u-old","parentId":"a1","timestamp":"2026-01-01T00:00:03Z","message":{"role":"user","content":"old branch"}}
{"type":"message","id":"a-old","parentId":"u-old","timestamp":"2026-01-01T00:00:04Z","message":{"role":"assistant","content":[{"type":"text","text":"old reply"}]}}
{"type":"message","id":"u-new","parentId":"a1","timestamp":"2026-01-01T00:00:05Z","message":{"role":"user","content":"active branch"}}
{"type":"message","id":"a-new","parentId":"u-new","timestamp":"2026-01-01T00:00:06Z","message":{"role":"assistant","content":[{"type":"text","text":"active reply"}]}}
"#,
        )
        .unwrap();
        env::set_var(
            "PI_CODING_AGENT_SESSION_DIR",
            _root.join(".pi/agent/sessions"),
        );
        app.page = Page::Sessions;
        app.focus = Focus::Content;
        app.reload_sessions(None);
        assert_eq!(app.preview_message_count(), 6);
        assert_eq!(app.preview.as_ref().unwrap().branch_points, 1);
        app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));

        let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let content = buffer_string(&terminal);
        assert!(!content.contains("└─●"));
        assert!(content.contains("├─●"));
        assert!(content.contains("└─○"));
        assert!(content.contains("[1/2]"));
        assert!(content.contains("[2/2]"));
        assert!(content.contains('◆'));
        assert!(content.contains('▾'));
        assert!(content.contains("◆ Current") || content.contains("◆ 当前"));
        assert!(content.contains("Branch 1/2") || content.contains("分支 1/2"));

        // The default active leaf switches at the nearest branch point.
        assert_eq!(app.selected_preview_message().unwrap().id, "a-new");
        app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::ALT));
        assert_eq!(app.selected_preview_message().unwrap().id, "u-old");

        // Parent/child navigation must retain the branch we came from.
        app.on_key(KeyEvent::new(KeyCode::Left, KeyModifiers::CONTROL));
        assert_eq!(app.selected_preview_message().unwrap().id, "a1");
        app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::CONTROL));
        assert_eq!(app.selected_preview_message().unwrap().id, "u-old");

        // Horizontal navigation follows visual branch levels, not serial messages.
        app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(app.selected_preview_message().unwrap().id, "a-old");
        app.on_key(KeyEvent::new(
            KeyCode::Left,
            KeyModifiers::CONTROL | KeyModifiers::ALT,
        ));
        assert_eq!(app.selected_preview_message().unwrap().id, "a1");
        app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::CONTROL));
        assert_eq!(app.selected_preview_message().unwrap().id, "u-old");
        app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::CONTROL));
        assert_eq!(app.selected_preview_message().unwrap().id, "u-old");
        app.on_key(KeyEvent::new(KeyCode::Left, KeyModifiers::ALT));
        assert_eq!(app.selected_preview_message().unwrap().id, "u-new");
        app.on_key(KeyEvent::new(KeyCode::Left, KeyModifiers::CONTROL));
        assert_eq!(app.selected_preview_message().unwrap().id, "a1");

        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.preview_message_count(), 6);
        app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(app.preview_message_count(), 2);
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let collapsed = buffer_string(&terminal);
        assert!(collapsed.contains('▸'));
        assert!(collapsed.contains("hidden 4") || collapsed.contains("已隐藏 4"));

        app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::CONTROL));
        assert_eq!(app.preview_message_count(), 6);
        assert_eq!(app.selected_preview_message().unwrap().id, "u-new");
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        assert!(buffer_string(&terminal).contains('▾'));
        env::remove_var("PI_CODING_AGENT_SESSION_DIR");
    }

    #[test]
    fn sessions_grouped_by_cwd_with_headers_and_navigation() {
        let _env_lock = SESSION_ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let (_root, mut app) = app();
        let sessions_root = _root.join(".pi/agent/sessions/--proj--");
        fs::create_dir_all(&sessions_root).unwrap();

        // Beta group — most recently active (message timestamp 3000).
        fs::write(
            sessions_root.join("b1.jsonl"),
            r#"{"type":"session","version":3,"id":"beta-1","timestamp":"2026-01-01T00:00:00.000Z","cwd":"/work/beta"}
{"type":"message","id":"u1","parentId":null,"timestamp":1,"message":{"role":"user","content":"beta message","timestamp":3000}}
"#,
        )
        .unwrap();

        // Alpha group — two sessions (timestamps 2000 and 1000).
        fs::write(
            sessions_root.join("a1.jsonl"),
            r#"{"type":"session","version":3,"id":"alpha-1","timestamp":"2026-01-01T00:00:00.000Z","cwd":"/work/alpha"}
{"type":"message","id":"u1","parentId":null,"timestamp":1,"message":{"role":"user","content":"alpha one","timestamp":2000}}
"#,
        )
        .unwrap();
        fs::write(
            sessions_root.join("a2.jsonl"),
            r#"{"type":"session","version":3,"id":"alpha-2","timestamp":"2026-01-01T00:00:00.000Z","cwd":"/work/alpha"}
{"type":"message","id":"u1","parentId":null,"timestamp":1,"message":{"role":"user","content":"alpha two","timestamp":1000}}
"#,
        )
        .unwrap();

        env::set_var(
            "PI_CODING_AGENT_SESSION_DIR",
            _root.join(".pi/agent/sessions"),
        );
        app.page = Page::Sessions;
        app.focus = Focus::Content;
        app.reload_sessions(None);

        assert_eq!(app.sessions.len(), 3);

        // self.sessions is sorted by modified desc: beta-1(3000), alpha-1(2000), alpha-2(1000)
        let groups = app.session_groups();
        assert_eq!(groups.len(), 2);
        // Beta group first (max modified = 3000).
        assert_eq!(groups[0].cwd, "/work/beta");
        assert_eq!(groups[0].sessions.len(), 1);
        // Alpha group second (max modified = 2000).
        assert_eq!(groups[1].cwd, "/work/alpha");
        assert_eq!(groups[1].sessions.len(), 2);

        let visible = app.visible_sessions();
        assert_eq!(visible, vec![0, 1, 2]);

        // Render and verify group headers appear in the buffer.
        let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let content = buffer_string(&terminal);
        assert!(content.contains("/work/beta"));
        assert!(content.contains("/work/alpha"));

        // Navigation skips header rows — cursor always lands on a session.
        assert_eq!(app.session_cursor, 0);
        assert_eq!(app.selected_session().unwrap().id, "beta-1");

        app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(app.session_cursor, 1);
        assert_eq!(app.selected_session().unwrap().id, "alpha-1");

        app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(app.session_cursor, 2);
        assert_eq!(app.selected_session().unwrap().id, "alpha-2");

        app.on_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(app.session_cursor, 1);
        assert_eq!(app.selected_session().unwrap().id, "alpha-1");

        env::remove_var("PI_CODING_AGENT_SESSION_DIR");
    }

