use super::*;

#[test]
fn text_matcher_preserves_case_and_prefix_matching() {
    let lowercase = TextMatcher::new("line").expect("matcher should be created");
    assert!(lowercase.matches("LINE"));
    assert_eq!(lowercase.match_ranges("LINE").len(), 1);

    let uppercase = TextMatcher::new("Line").expect("matcher should be created");
    assert!(!uppercase.matches("line"));

    let unicode = TextMatcher::new("éclair").expect("matcher should be created");
    assert!(!unicode.case_sensitive);
    assert!(unicode.matches("éclair"));
    assert!(unicode.matches("éCLAIR"));
    assert!(!unicode.matches("Éclair"));

    let addition = DiffLine::addition(1, "changed".to_owned());
    let prefixed = TextMatcher::new("+changed").expect("matcher should be created");
    assert!(diff_line_grep_text_matches(&addition, &prefixed));
}

#[test]
fn configured_leader_e_is_unmapped() {
    let mut changeset = changeset_with_hunk_at(PathBuf::from("/repo"), 20);
    set_test_file_deleted(&mut changeset.files[0]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.config.keymap = Keymap::parse(
        r#"
        [keymap.global]
        diff_menu = "space m"
        "#,
    )
    .expect("keymap should parse");

    app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE))
        .expect("leader should be handled");
    let should_quit = app
        .handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE))
        .expect("leader e should be ignored");

    assert!(!should_quit);
    assert!(app.input.key_prefix_pending.is_none());
    assert!(app.notifications.error_log.is_none());
}

#[test]
fn error_log_can_be_resized_with_bounds() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_error_log("reload failed");

    assert_eq!(error_log_height(&app, 20), ERROR_LOG_DEFAULT_HEIGHT);

    app.handle_key(KeyEvent::new(KeyCode::Char('+'), KeyModifiers::NONE))
        .expect("plus should resize error log");
    assert_eq!(error_log_height(&app, 20), ERROR_LOG_DEFAULT_HEIGHT + 1);

    for _ in 0..32 {
        app.handle_key(KeyEvent::new(KeyCode::Char('-'), KeyModifiers::NONE))
            .expect("minus should resize error log");
    }
    assert_eq!(error_log_height(&app, 20), ERROR_LOG_MIN_HEIGHT);

    for _ in 0..64 {
        app.handle_key(KeyEvent::new(KeyCode::Char('+'), KeyModifiers::NONE))
            .expect("plus should resize error log");
    }
    assert_eq!(error_log_height(&app, 80), ERROR_LOG_MAX_HEIGHT);
    assert_eq!(error_log_height(&app, 4), 4);
}

#[test]
fn copy_error_log_writes_full_log_to_clipboard_sequence() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_error_log("reload failed:\nfatal: bad revision");
    let mut output = Vec::new();

    app.copy_error_log_to_writer(&mut output);

    assert_eq!(
        String::from_utf8(output).expect("OSC 52 sequence should be UTF-8"),
        osc52_clipboard_sequence("reload failed:\nfatal: bad revision")
    );
    assert_eq!(
        app.notifications.toasts.latest_text(),
        Some("error log copied")
    );
    assert_eq!(
        app.notifications.error_log.as_deref(),
        Some("reload failed:\nfatal: bad revision")
    );
}

#[test]
fn progress_label_is_bounded() {
    assert_eq!(progress_label(0, 0), "100%");
    assert_eq!(progress_label(0, 20), "0%");
    assert_eq!(progress_label(10, 20), "50%");
    assert_eq!(progress_label(100, 20), "100%");
}

#[test]
fn format_count_groups_thousands() {
    assert_eq!(format_count(0), "0");
    assert_eq!(format_count(42), "42");
    assert_eq!(format_count(999), "999");
    assert_eq!(format_count(1_000), "1,000");
    assert_eq!(format_count(1_009_257), "1,009,257");
}

#[test]
fn input_boxes_support_native_cursor_shortcuts() {
    let changeset = changeset_with_line_text("alpha beta");
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);

    app.open_filter_input(DiffFilterKind::File);
    for character in "alpha beta".chars() {
        app.handle_filter_input_key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE));
    }
    app.handle_filter_input_key(KeyEvent::new(KeyCode::Left, KeyModifiers::SUPER));
    app.handle_filter_input_key(KeyEvent::new(KeyCode::Char('>'), KeyModifiers::NONE));
    app.handle_filter_input_key(KeyEvent::new(KeyCode::Right, KeyModifiers::SUPER));
    app.handle_filter_input_key(KeyEvent::new(KeyCode::Char('!'), KeyModifiers::NONE));

    assert_eq!(app.filters.file_filter_input, ">alpha beta!");

    app.handle_filter_input_key(KeyEvent::new(KeyCode::Delete, KeyModifiers::SUPER));
    assert!(app.filters.file_filter_input.is_empty());

    for character in "done".chars() {
        app.handle_filter_input_key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE));
    }
    app.handle_filter_input_key(KeyEvent::new(KeyCode::Left, KeyModifiers::ALT));
    app.handle_filter_input_key(KeyEvent::new(KeyCode::Char('!'), KeyModifiers::NONE));
    assert_eq!(app.filters.file_filter_input, "!done");
}
