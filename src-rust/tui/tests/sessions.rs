    #[test]
    fn sessions_page_lists_filters_and_confirms_delete() {
        let (root, mut app) = app();
        let sessions_root = root.join(".pi/agent/sessions/--proj--");
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
            root.join(".pi/agent/sessions"),
        );
        app.page = Page::Sessions;
        app.focus = Focus::Content;
        app.reload_sessions(None);
        assert_eq!(app.sessions.len(), 1);
        assert_eq!(app.sessions[0].name.as_deref(), Some("Demo Session"));
        assert!(app
            .preview
            .as_ref()
            .is_some_and(|messages| messages.iter().any(|m| m.role == "user")));

        app.on_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE));
        assert!(app.user_only_preview);
        assert!(app
            .preview
            .as_ref()
            .is_some_and(|messages| messages.iter().all(|m| m.role == "user")));

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
        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("Demo Session") || content.contains("demo"));
        assert!(content.contains("Preview") || content.contains("预览"));

        app.on_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));
        assert!(matches!(
            app.overlay,
            Some(Overlay::ConfirmDeleteSession { .. })
        ));

        env::remove_var("PI_CODING_AGENT_SESSION_DIR");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn preview_wrap_removes_invisible_trailing_whitespace() {
        assert_eq!(
            wrap_preview_text("visible   \t\nnext  \n", 20),
            vec!["visible", "next", ""]
        );
        assert_eq!(
            wrap_preview_text("123456   next", 6),
            vec!["123456", "next"]
        );
    }

    #[test]
    fn session_preview_focus_navigates_messages_and_copies() {
        let (root, mut app) = app();
        let sessions_root = root.join(".pi/agent/sessions/--proj--");
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
            root.join(".pi/agent/sessions"),
        );
        app.page = Page::Sessions;
        app.focus = Focus::Content;
        app.reload_sessions(None);
        assert_eq!(app.preview_message_count(), 3);

        // Right / Enter enters preview message selection.
        app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        assert!(app.focus == Focus::SessionPreview);
        assert_eq!(app.preview_message_cursor, 0);

        // Down moves to next message.
        app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(app.preview_message_cursor, 1);

        // Up moves back.
        app.on_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(app.preview_message_cursor, 0);

        // Mouse-wheel style scrolling moves within a long message before changing selection.
        app.preview_wrap_width = 12;
        app.preview_viewport_height = 4;
        app.preview_scroll = 0;
        app.scroll_preview_lines(4);
        assert_eq!(app.preview_scroll, 4);
        assert_eq!(app.preview_message_cursor, 0);

        // Ctrl+C in preview copies instead of quitting.
        app.on_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(!app.quit);
        assert!(app.notice.is_some());

        // Left / Esc returns to list.
        app.on_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        assert!(app.focus == Focus::Content);

        // Esc from list returns to menu.
        app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(app.focus == Focus::Menu);

        env::remove_var("PI_CODING_AGENT_SESSION_DIR");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn sessions_grouped_by_cwd_with_headers_and_navigation() {
        let (root, mut app) = app();
        let sessions_root = root.join(".pi/agent/sessions/--proj--");
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
            root.join(".pi/agent/sessions"),
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
        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
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
        let _ = fs::remove_dir_all(root);
    }

