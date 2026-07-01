use super::*;

#[test]
fn max_scroll_stops_at_last_full_viewport() {
    assert_eq!(max_scroll_for_viewport(10, 1), 9);
    assert_eq!(max_scroll_for_viewport(10, 4), 6);
    assert_eq!(max_scroll_for_viewport(3, 10), 0);
    assert_eq!(max_scroll_for_viewport(10, 10), 0);
    assert_eq!(max_scroll_for_viewport(10, 0), 9);
}

#[test]
fn app_clamps_scroll_to_last_full_viewport() {
    let changeset = changeset_with_context_lines(10);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);

    app.set_viewport_rows(5);
    app.set_scroll(usize::MAX);

    assert_eq!(app.viewport.scroll, app.document.model.len() - 5);
    assert_eq!(app.viewport_focus_row(), app.document.model.len() - 1);

    app.set_viewport_rows(usize::MAX);

    assert_eq!(app.viewport.scroll, 0);
}

#[test]
fn app_clamps_horizontal_scroll_to_diff_content() {
    let changeset = changeset_with_line_text("abcdefghijkl");
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);

    app.set_viewport_width(18);
    assert_eq!(
        diff_content_width(app.viewport.layout, app.viewport.viewport_width),
        4
    );

    app.scroll_horizontally_by(HORIZONTAL_SCROLL_STEP as isize);
    assert_eq!(app.viewport.horizontal_scroll, 8);

    app.scroll_horizontally_by(HORIZONTAL_SCROLL_STEP as isize);
    assert_eq!(app.viewport.horizontal_scroll, 8);

    app.scroll_horizontally_by(-(HORIZONTAL_SCROLL_STEP as isize));
    assert_eq!(app.viewport.horizontal_scroll, 0);

    app.set_horizontal_scroll(8);
    app.set_viewport_width(80);
    assert_eq!(app.viewport.horizontal_scroll, 0);
}

#[test]
fn max_scroll_stays_zero_when_annotated_diff_fits_viewport() {
    use crate::annotation::AnnotationKey;

    let lines: Vec<&str> = (0..3).map(|_| "line").collect();
    let changeset = changeset_with_line_texts(&lines);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_width(40);
    app.set_viewport_rows(20);

    let annotated_row = app
        .document
        .model
        .rows
        .iter()
        .position(|row| matches!(row, UiRow::UnifiedLine { .. }))
        .expect("unified line");
    let key = AnnotationKey::from_ui_row(
        &app.document.changeset,
        app.document
            .model
            .row(annotated_row)
            .expect("annotated row"),
    )
    .expect("annotation key");
    app.annotations_state
        .annotations
        .insert(key, "note".to_owned());

    assert_eq!(app.max_scroll(), 0);
    app.set_scroll(usize::MAX);
    assert_eq!(app.viewport.scroll, 0);
}

#[test]
fn scroll_change_clears_manual_hunk_focus() {
    let changeset = changeset_with_hunks_at(PathBuf::from("/repo"), &[1, 2, 3]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(5);

    app.next_hunk();
    assert_eq!(app.viewport.manual_hunk_focus, Some((FILE_0, HUNK_1)));
    assert_eq!(app.focused_hunk_for_viewport(5), Some((FILE_0, HUNK_1)));

    app.set_scroll(0);

    assert_eq!(app.viewport.scroll, 0);
    assert_eq!(app.viewport.manual_hunk_focus, None);
    assert_eq!(app.focused_hunk_for_viewport(5), Some((FILE_0, HUNK_0)));
}

#[test]
fn model_rebuild_clears_manual_hunk_focus_when_scroll_is_unchanged() {
    let changeset = changeset_with_files(&["a.rs", "b.rs", "c.rs"]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(20);

    app.next_hunk();
    assert_eq!(app.viewport.scroll, 0);
    assert_eq!(app.viewport.manual_hunk_focus, Some((FILE_1, HUNK_0)));

    app.filters.file_filter = "a.rs".to_owned();
    app.apply_filters(PostFilterNavigation::Preserve);

    assert_eq!(app.viewport.scroll, 0);
    assert_eq!(app.sidebar.selected_file, FILE_0);
    assert_eq!(app.viewport.manual_hunk_focus, None);
    assert_eq!(app.focused_hunk_for_viewport(20), Some((FILE_0, HUNK_0)));
}

#[test]
fn model_rebuild_preserves_valid_manual_hunk_focus_when_scroll_is_unchanged() {
    let changeset = changeset_with_hunks_at(PathBuf::from("/repo"), &[1, 2, 3]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(20);

    app.next_hunk();
    app.next_hunk();
    assert_eq!(app.viewport.scroll, 0);
    assert_eq!(app.viewport.manual_hunk_focus, Some((FILE_0, HUNK_2)));

    app.filters.file_filter = "file.rs".to_owned();
    app.apply_filters(PostFilterNavigation::Preserve);

    assert_eq!(app.viewport.scroll, 0);
    assert_eq!(app.viewport.manual_hunk_focus, Some((FILE_0, HUNK_2)));
    assert_eq!(app.focused_hunk_for_viewport(20), Some((FILE_0, HUNK_2)));
}

#[test]
fn model_rebuild_preserves_valid_manual_hunk_focus_when_scroll_changes() {
    let changeset = changeset_with_files(&["a.rs", "b.rs", "c.rs"]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(3);

    app.select_file(2);
    assert!(app.viewport.scroll > 0);
    assert_eq!(app.viewport.manual_hunk_focus, Some((FILE_2, HUNK_0)));

    app.filters.file_filter = "c.rs".to_owned();
    app.apply_filters(PostFilterNavigation::Preserve);

    assert_eq!(app.viewport.scroll, 0);
    assert_eq!(app.sidebar.selected_file, FILE_2);
    assert_eq!(app.viewport.manual_hunk_focus, Some((FILE_2, HUNK_0)));
    assert_eq!(app.focused_hunk_for_viewport(3), Some((FILE_2, HUNK_0)));
}

#[test]
fn arrow_keys_move_hunk_focus_when_diff_fits_viewport() {
    assert_key_pair_moves_hunk_focus_when_diff_fits_viewport(KeyCode::Down, KeyCode::Up);
}

#[test]
fn page_keys_move_hunk_focus_when_diff_fits_viewport() {
    assert_key_pair_moves_hunk_focus_when_diff_fits_viewport(KeyCode::PageDown, KeyCode::PageUp);
}

#[test]
fn arrow_keys_scroll_then_move_hunk_focus_at_edges_in_scrollable_diff() {
    assert_key_pair_scrolls_then_moves_hunk_focus_at_edges(KeyCode::Down, KeyCode::Up, 1);
}

#[test]
fn page_keys_scroll_then_move_hunk_focus_at_edges_in_scrollable_diff() {
    assert_key_pair_scrolls_then_moves_hunk_focus_at_edges(KeyCode::PageDown, KeyCode::PageUp, 20);
}

#[test]
fn d_key_pages_down() {
    let changeset = changeset_with_files(&[
        "a.rs", "b.rs", "c.rs", "d.rs", "e.rs", "f.rs", "g.rs", "h.rs",
    ]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(6);

    app.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE))
        .expect("d should page down");

    assert_eq!(app.viewport.scroll, 20);
    assert!(app.input.key_prefix_pending.is_none());
}

#[test]
fn j_and_k_scroll_then_move_hunk_focus_at_edges_in_scrollable_diff() {
    let changeset = changeset_with_files(&[
        "a.rs", "b.rs", "c.rs", "d.rs", "e.rs", "f.rs", "g.rs", "h.rs",
    ]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(6);

    assert!(app.max_scroll() > 0);
    app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE))
        .expect("j should be handled");
    assert_eq!(app.viewport.scroll, 1);
    assert_eq!(app.viewport.manual_hunk_focus, None);

    app.set_scroll(0);
    let top_hunks = visible_hunk_keys(&app);
    assert!(top_hunks.len() >= 2);
    app.viewport.manual_hunk_focus = Some(typed_hunk_key(top_hunks[1]));
    app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE))
        .expect("k should be handled");
    assert_eq!(app.viewport.scroll, 0);
    assert_eq!(app.sidebar.selected_file, FileIndex::new(top_hunks[0].0));
    assert_eq!(
        app.focused_hunk_for_viewport(app.viewport.viewport_rows),
        Some(typed_hunk_key(top_hunks[0]))
    );

    while app.viewport.scroll < app.max_scroll() {
        app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE))
            .expect("j should be handled");
    }
    let bottom_scroll = app.viewport.scroll;
    let bottom_hunks = visible_hunk_keys(&app);
    assert!(bottom_hunks.len() >= 2);
    let previous = bottom_hunks[bottom_hunks.len() - 2];
    let next = bottom_hunks[bottom_hunks.len() - 1];
    app.viewport.manual_hunk_focus = Some(typed_hunk_key(previous));

    app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE))
        .expect("j should be handled");

    assert_eq!(app.viewport.scroll, bottom_scroll);
    assert_eq!(app.sidebar.selected_file, FileIndex::new(next.0));
    assert_eq!(
        app.focused_hunk_for_viewport(app.viewport.viewport_rows),
        Some(typed_hunk_key(next))
    );
}

#[test]
fn mouse_wheel_moves_hunk_focus_when_diff_fits_viewport() {
    let changeset = changeset_with_hunks_at(PathBuf::from("/repo"), &[1, 2, 3]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(20);

    assert_eq!(app.max_scroll(), 0);
    assert_eq!(app.focused_hunk_for_viewport(20), Some((FILE_0, HUNK_0)));

    for _ in 0..2 {
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        })
        .expect("mouse wheel should be handled");
        assert_eq!(app.focused_hunk_for_viewport(20), Some((FILE_0, HUNK_0)));
    }
    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: 0,
        row: 0,
        modifiers: KeyModifiers::NONE,
    })
    .expect("mouse wheel should be handled");
    assert_eq!(app.viewport.scroll, 0);
    assert_eq!(app.focused_hunk_for_viewport(20), Some((FILE_0, HUNK_1)));

    for _ in 0..2 {
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        })
        .expect("mouse wheel should be handled");
        assert_eq!(app.focused_hunk_for_viewport(20), Some((FILE_0, HUNK_1)));
    }
    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::ScrollUp,
        column: 0,
        row: 0,
        modifiers: KeyModifiers::NONE,
    })
    .expect("mouse wheel should be handled");
    assert_eq!(app.viewport.scroll, 0);
    assert_eq!(app.focused_hunk_for_viewport(20), Some((FILE_0, HUNK_0)));
}

#[test]
fn mouse_wheel_scrolls_then_accumulates_hunk_focus_at_edge_in_scrollable_diff() {
    let changeset = changeset_with_files(&[
        "a.rs", "b.rs", "c.rs", "d.rs", "e.rs", "f.rs", "g.rs", "h.rs",
    ]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(6);

    assert!(app.max_scroll() > 0);
    for _ in 0..100 {
        if app.viewport.scroll == app.max_scroll() {
            break;
        }
        mouse_scroll(&mut app, MouseEventKind::ScrollDown);
    }
    assert_eq!(app.viewport.scroll, app.max_scroll());

    let bottom_hunks = visible_hunk_keys(&app);
    assert!(bottom_hunks.len() >= 2);
    let previous = bottom_hunks[bottom_hunks.len() - 2];
    let next = bottom_hunks[bottom_hunks.len() - 1];
    app.viewport.manual_hunk_focus = Some(typed_hunk_key(previous));

    for _ in 0..2 {
        mouse_scroll(&mut app, MouseEventKind::ScrollDown);
        assert_eq!(app.viewport.scroll, app.max_scroll());
        assert_eq!(
            app.focused_hunk_for_viewport(app.viewport.viewport_rows),
            Some(typed_hunk_key(previous))
        );
    }
    mouse_scroll(&mut app, MouseEventKind::ScrollDown);

    assert_eq!(app.viewport.scroll, app.max_scroll());
    assert_eq!(
        app.focused_hunk_for_viewport(app.viewport.viewport_rows),
        Some(typed_hunk_key(next))
    );
}

#[test]
fn bracket_hunk_navigation_uses_focused_hunk_in_scrollable_diff() {
    let changeset = changeset_with_hunks_at(PathBuf::from("/repo"), &[1, 2, 3]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(5);

    assert!(app.max_scroll() > 0);
    assert_eq!(app.focused_hunk_for_viewport(5), Some((FILE_0, HUNK_0)));

    app.next_hunk();
    assert_eq!(app.focused_hunk_for_viewport(5), Some((FILE_0, HUNK_1)));

    app.next_hunk();
    assert_eq!(app.focused_hunk_for_viewport(5), Some((FILE_0, HUNK_2)));

    app.previous_hunk();
    assert_eq!(app.focused_hunk_for_viewport(5), Some((FILE_0, HUNK_1)));
}

#[test]
fn bracket_key_hunk_navigation_uses_focused_hunk_in_scrollable_diff() {
    let changeset = changeset_with_hunks_at(PathBuf::from("/repo"), &[1, 2, 3]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(5);

    app.handle_key(KeyEvent::new(KeyCode::Char(']'), KeyModifiers::NONE))
        .expect("] should move to next hunk");
    assert_eq!(app.focused_hunk_for_viewport(5), Some((FILE_0, HUNK_1)));

    app.handle_key(KeyEvent::new(KeyCode::Char('['), KeyModifiers::NONE))
        .expect("[ should move to previous hunk");
    assert_eq!(app.focused_hunk_for_viewport(5), Some((FILE_0, HUNK_0)));
}

#[test]
fn bracket_hunk_navigation_focuses_visible_hunk_when_scroll_is_clamped() {
    let changeset = changeset_with_files(&[
        "a.rs", "b.rs", "c.rs", "d.rs", "e.rs", "f.rs", "g.rs", "h.rs", "i.rs", "j.rs",
    ]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(20);

    assert!(app.max_scroll() > 0);
    assert_eq!(app.viewport.scroll, 0);
    assert_eq!(app.focused_hunk_for_viewport(20), Some((FILE_0, HUNK_0)));

    app.next_hunk();
    assert_eq!(app.viewport.scroll, 0);
    assert_eq!(app.focused_hunk_for_viewport(20), Some((FILE_1, HUNK_0)));

    app.next_hunk();
    assert_eq!(app.viewport.scroll, 0);
    assert_eq!(app.focused_hunk_for_viewport(20), Some((FILE_2, HUNK_0)));

    app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE))
        .expect("k should be handled");
    assert_eq!(app.viewport.scroll, 0);
    assert_eq!(app.focused_hunk_for_viewport(20), Some((FILE_1, HUNK_0)));
}

#[test]
fn bracket_hunk_navigation_can_return_to_first_hunk_in_short_scrollable_diff() {
    let changeset = changeset_with_files(&["a.rs", "b.rs", "c.rs", "d.rs", "e.rs", "f.rs"]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(20);

    assert!(app.max_scroll() > 0);
    assert_eq!(app.focused_hunk_for_viewport(20), Some((FILE_0, HUNK_0)));

    for _ in 0..10 {
        app.previous_hunk();
    }
    assert_eq!(app.viewport.scroll, 0);
    assert_eq!(app.focused_hunk_for_viewport(20), Some((FILE_0, HUNK_0)));

    app.next_hunk();
    assert_eq!(app.focused_hunk_for_viewport(20), Some((FILE_1, HUNK_0)));

    app.previous_hunk();
    assert_eq!(app.focused_hunk_for_viewport(20), Some((FILE_0, HUNK_0)));
}

#[test]
fn edit_key_without_editable_target_does_not_scroll_to_top() {
    let mut changeset = changeset_with_hunk_at(PathBuf::from("/repo"), 20);
    set_test_file_deleted(&mut changeset.files[0]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(1);
    app.set_scroll(1);

    let should_quit = app
        .handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL))
        .expect("edit key should be handled");

    assert!(!should_quit);
    assert_eq!(app.viewport.scroll, 1);
    assert_eq!(
        app.notifications.toasts.latest_text(),
        Some("no editable focused hunk")
    );
}

#[test]
fn edit_key_without_editor_launch_preserves_queued_events() {
    let mut changeset = changeset_with_hunk_at(PathBuf::from("/repo"), 20);
    set_test_file_deleted(&mut changeset.files[0]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    let queued_quit = Event::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
    let (tx, rx) = mpsc::channel(1);
    tx.try_send(Ok(queued_quit.clone())).unwrap();
    let mut events = crate::event_reader::TerminalEventReader::from_receiver(rx);
    let mut live_diff = None;

    let should_quit = handle_event(
        &mut app,
        Event::Key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL)),
        &mut live_diff,
        &mut events,
    )
    .expect("edit key should be handled");

    assert!(!should_quit);
    assert_eq!(events.try_read().unwrap(), Some(queued_quit));
}

#[test]
fn editable_hunk_without_configured_editor_sets_notice() {
    let changeset = changeset_with_hunk_at(PathBuf::from("/repo"), 20);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(5);

    assert!(!app.prepare_focused_hunk_editor_for_test(None));
    assert_eq!(
        app.notifications.toasts.latest_text(),
        Some("set $VISUAL, $GIT_EDITOR, or $EDITOR to edit focused hunk")
    );
    assert!(app.notifications.error_log.is_none());
    assert!(app.runtime.dirty);
}

#[test]
fn post_editor_quit_key_guard_ignores_only_transient_quit_keys() {
    let changeset = changeset_with_hunk_at(PathBuf::from("/repo"), 20);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    let now = Instant::now();
    app.jobs.post_editor_quit_key_ignore_until = Some(now + Duration::from_millis(250));

    assert!(app.ignore_post_editor_quit_key(
        KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        now
    ));
    assert!(
        app.ignore_post_editor_quit_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE), now)
    );
    assert!(!app.ignore_post_editor_quit_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), now));
    assert!(
        !app.ignore_post_editor_quit_key(
            KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE),
            now
        )
    );
    assert!(!app.ignore_post_editor_quit_key(
        KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
        now + Duration::from_millis(251)
    ));
}

#[test]
fn post_editor_quit_key_guard_swallows_configured_single_quit_key_event() {
    let changeset = changeset_with_hunk_at(PathBuf::from("/repo"), 20);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.config.keymap = Keymap::parse(
        r#"
        [keymap.global]
        quit = "q"
        "#,
    )
    .expect("keymap should parse");
    app.jobs.post_editor_quit_key_ignore_until = Some(Instant::now() + Duration::from_millis(250));

    let should_quit = handle_test_key_event(
        &mut app,
        KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
    );

    assert!(!should_quit);
}

#[test]
fn bounded_context_expansion_config_does_not_limit_mouse_expansion() {
    let repo = temp_test_dir("bounded-context-expansion");
    fs::create_dir_all(&repo).expect("repo directory should be created");
    let text = (1..=80)
        .map(|line| format!("line {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(repo.join("file.rs"), text).expect("context file should be written");
    let changeset = changeset_with_hunk_at(repo, 50);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.config.theme.diff.context_expansion = DiffContextExpansion::Lines(20);

    assert!(app.expand_context_at_row(1));
    assert_eq!(
        app.document.context_expansions.get(&ContextKey {
            file: FILE_0,
            hunk: HUNK_0
        }),
        Some(&49)
    );
    assert_eq!(
        app.document.model.row(1),
        Some(UiRow::ContextLine {
            file: FILE_0,
            old_line: 1,
            new_line: 1,
        })
    );
    assert_eq!(
        app.document.model.row(49),
        Some(UiRow::ContextLine {
            file: FILE_0,
            old_line: 49,
            new_line: 49,
        })
    );
    assert_eq!(
        app.document.model.row(50),
        Some(UiRow::ContextHide {
            file: FILE_0,
            hunk: HUNK_0,
            lines: 49,
        })
    );
}

#[test]
fn clicking_collapsed_context_expands_full_gap_and_hide_collapses() {
    let repo = temp_test_dir("expand-context");
    fs::create_dir_all(&repo).expect("repo directory should be created");
    let text = (1..=80)
        .map(|line| format!("line {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(repo.join("file.rs"), text).expect("context file should be written");
    let changeset = changeset_with_hunk_at(repo, 50);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);

    let collapsed = app
        .document
        .model
        .row(1)
        .expect("collapsed context row should exist");
    let rendered = render_row(&mut app, 1, collapsed, 80);
    assert!(line_text(&rendered).contains("▴ show 49 unchanged lines"));

    assert!(app.expand_context_at_row(1));
    assert_eq!(
        app.document.context_expansions.get(&ContextKey {
            file: FILE_0,
            hunk: HUNK_0
        }),
        Some(&49)
    );
    assert_eq!(
        app.document.model.row(1),
        Some(UiRow::ContextLine {
            file: FILE_0,
            old_line: 1,
            new_line: 1,
        })
    );
    let hide = app
        .document
        .model
        .row(50)
        .expect("hide context row should exist");
    let rendered = render_row(&mut app, 50, hide, 80);
    assert!(line_text(&rendered).contains("▾ hide 49 unchanged lines"));
    let row = app
        .document
        .model
        .row(49)
        .expect("expanded context row should exist");
    assert_eq!(
        row,
        UiRow::ContextLine {
            file: FILE_0,
            old_line: 49,
            new_line: 49,
        }
    );
    let rendered = render_row(&mut app, 49, row, 80);
    assert!(line_text(&rendered).contains("line 49"));

    assert!(app.handle_context_at_row(50));
    assert!(!app.document.context_expansions.contains_key(&ContextKey {
        file: FILE_0,
        hunk: HUNK_0
    }));
    assert_eq!(
        app.document.model.row(1),
        Some(UiRow::Collapsed {
            file: FILE_0,
            hunk: HUNK_0,
            old_start: 1,
            new_start: 1,
            lines: 49,
            expanded: 0,
        })
    );
}

#[test]
fn clicking_collapsed_context_between_hunks_expands_full_gap() {
    let repo = temp_test_dir("expand-context-downward");
    fs::create_dir_all(&repo).expect("repo directory should be created");
    let text = (1..=120)
        .map(|line| format!("line {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(repo.join("file.rs"), text).expect("context file should be written");
    let changeset = changeset_with_hunks_at(repo, &[50, 100]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);

    let collapsed = app
        .document
        .model
        .row(4)
        .expect("collapsed context row should exist");
    let rendered = render_row(&mut app, 4, collapsed, 80);
    assert!(line_text(&rendered).contains("▾ show 49 unchanged lines"));

    assert!(app.expand_context_at_row(4));
    assert_eq!(
        app.document.context_expansions.get(&ContextKey {
            file: FILE_0,
            hunk: HUNK_1
        }),
        Some(&49)
    );
    assert_eq!(
        app.document.model.row(4),
        Some(UiRow::ContextHide {
            file: FILE_0,
            hunk: HUNK_1,
            lines: 49,
        })
    );
    let hide = app
        .document
        .model
        .row(4)
        .expect("hide context row should exist");
    let rendered = render_row(&mut app, 4, hide, 80);
    assert!(line_text(&rendered).contains("▴ hide 49 unchanged lines"));
    let row = app
        .document
        .model
        .row(5)
        .expect("expanded context row should exist");
    assert_eq!(
        row,
        UiRow::ContextLine {
            file: FILE_0,
            old_line: 51,
            new_line: 51,
        }
    );
    let rendered = render_row(&mut app, 5, row, 80);
    assert!(line_text(&rendered).contains("line 51"));
    assert_eq!(
        app.document.model.row(53),
        Some(UiRow::ContextLine {
            file: FILE_0,
            old_line: 99,
            new_line: 99,
        })
    );
    assert_eq!(
        app.document.model.row(54),
        Some(UiRow::HunkHeader {
            file: FILE_0,
            hunk: HUNK_1
        })
    );

    assert!(app.handle_context_at_row(4));
    assert!(!app.document.context_expansions.contains_key(&ContextKey {
        file: FILE_0,
        hunk: HUNK_1
    }));
    assert_eq!(
        app.document.model.row(4),
        Some(UiRow::Collapsed {
            file: FILE_0,
            hunk: HUNK_1,
            old_start: 51,
            new_start: 51,
            lines: 49,
            expanded: 0,
        })
    );
}

#[test]
fn context_keyboard_shortcuts_expand_and_collapse_by_default() {
    let repo = temp_test_dir("context-keyboard");
    fs::create_dir_all(&repo).expect("repo directory should be created");
    let text = (1..=120)
        .map(|line| format!("line {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(repo.join("file.rs"), text).expect("context file should be written");
    let changeset = changeset_with_hunks_at(repo, &[50, 100]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(8);

    assert!(app.document.context_expansions.is_empty());

    app.handle_key(KeyEvent::new(KeyCode::Char(','), KeyModifiers::NONE))
        .expect(", should expand context above focused hunk");
    assert_eq!(
        app.document.context_expansions.get(&ContextKey {
            file: FILE_0,
            hunk: HUNK_0
        }),
        Some(&49)
    );

    app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE))
        .expect("c should collapse expanded context");
    assert!(app.document.context_expansions.is_empty());

    app.handle_key(KeyEvent::new(KeyCode::Char('.'), KeyModifiers::NONE))
        .expect(". should expand context below focused hunk");
    assert_eq!(
        app.document.context_expansions.get(&ContextKey {
            file: FILE_0,
            hunk: HUNK_1
        }),
        Some(&49)
    );
}

#[test]
fn context_keyboard_expand_down_reveals_context_after_final_hunk() {
    let repo = temp_test_dir("context-keyboard-trailing");
    fs::create_dir_all(&repo).expect("repo directory should be created");
    let text = (1..=80)
        .map(|line| format!("line {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(repo.join("file.rs"), text).expect("context file should be written");
    let changeset = changeset_with_hunk_at(repo, 50);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(8);

    app.handle_key(KeyEvent::new(KeyCode::Char('.'), KeyModifiers::NONE))
        .expect(". should expand context below the final hunk");

    assert_eq!(
        app.document.context_expansions.get(&ContextKey {
            file: FILE_0,
            hunk: HUNK_1
        }),
        Some(&30)
    );
    assert_eq!(
        app.document.model.row(4),
        Some(UiRow::ContextHide {
            file: FILE_0,
            hunk: HUNK_1,
            lines: 30,
        })
    );
    assert_eq!(
        app.document.model.row(5),
        Some(UiRow::ContextLine {
            file: FILE_0,
            old_line: 51,
            new_line: 51,
        })
    );
    assert_eq!(
        app.document.model.row(34),
        Some(UiRow::ContextLine {
            file: FILE_0,
            old_line: 80,
            new_line: 80,
        })
    );
}

#[test]
fn responsive_layout_preserves_valid_horizontal_scroll() {
    let long_line = "a".repeat(120);
    let changeset = changeset_with_line_text(&long_line);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_width(80);
    app.set_horizontal_scroll(40);

    app.apply_responsive_layout(MIN_SPLIT_WIDTH);

    assert_eq!(app.viewport.layout, DiffLayoutMode::Split);
    assert_eq!(app.viewport.horizontal_scroll, 40);
}

#[test]
fn responsive_layout_clamps_horizontal_scroll_without_layout_change() {
    let long_line = "a".repeat(100);
    let changeset = changeset_with_line_text(&long_line);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Split);

    app.apply_responsive_layout(MIN_SPLIT_WIDTH);
    assert_eq!(app.viewport.layout, DiffLayoutMode::Split);
    app.set_horizontal_scroll(usize::MAX);
    let previous_scroll = app.viewport.horizontal_scroll;

    app.apply_responsive_layout(MIN_SPLIT_WIDTH + 40);

    assert_eq!(app.viewport.layout, DiffLayoutMode::Split);
    assert!(app.max_horizontal_scroll() < previous_scroll);
    assert_eq!(app.viewport.horizontal_scroll, app.max_horizontal_scroll());
}

#[test]
fn b_key_toggles_file_sidebar() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);

    assert!(!app.sidebar.file_sidebar_open);

    let should_quit = app
        .handle_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE))
        .expect("b should be handled");
    assert!(!should_quit);
    assert!(app.sidebar.file_sidebar_open);

    app.handle_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE))
        .expect("b should be handled");
    assert!(!app.sidebar.file_sidebar_open);
}

#[test]
fn f_key_filters_files_and_escape_clears_filter() {
    let changeset = changeset_with_files(&["src/lib.rs", "README.md", "mark-tui/src/lib.rs"]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);

    app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE))
        .expect("f should open file filter");
    assert!(line_text(&filter_bar_line(&app, 40)).starts_with(&format!("@{INPUT_CURSOR}")));
    let generation_before_input = app.document.generation;
    for character in "tui".chars() {
        app.handle_key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE))
            .expect("file filter input should be handled");
    }

    assert_eq!(app.filters.file_filter, "tui");
    assert_eq!(app.filters.file_filter_input, "tui");
    assert_eq!(visible_paths(&app), vec!["mark-tui/src/lib.rs"]);
    assert_eq!(app.document.generation, generation_before_input);
    assert!(line_text(&filter_bar_line(&app, 40)).starts_with("@tui"));

    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .expect("enter should keep file filter");

    assert_eq!(app.document.generation, generation_before_input);
    assert_eq!(app.filters.file_filter, "tui");
    assert_eq!(visible_paths(&app), vec!["mark-tui/src/lib.rs"]);
    assert_eq!(statusline_file_count_label(&app), "1/3 files");
    assert!(line_text(&filter_bar_line(&app, 40)).starts_with("@tui"));
    assert!(!line_text(&statusline_header_line(&app, 120)).contains("f:tui"));
    assert!(app.filters.filter_input.is_none());
    assert!(filter_bar_visible(&app));
    assert!(line_text(&filter_bar_line(&app, 40)).starts_with("@tui"));

    app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE))
        .expect("f should reopen file filter");
    assert_eq!(app.filters.file_filter, "");
    assert_eq!(app.filters.file_filter_input, "");
    assert_eq!(
        visible_paths(&app),
        vec!["src/lib.rs", "README.md", "mark-tui/src/lib.rs"]
    );

    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
        .expect("escape should clear file filter");

    assert_eq!(app.filters.file_filter, "");
    assert_eq!(
        visible_paths(&app),
        vec!["src/lib.rs", "README.md", "mark-tui/src/lib.rs"]
    );
    assert!(app.filters.filter_input.is_none());
}

#[test]
fn slash_filters_files_by_diff_content_and_escape_clears_filter() {
    let changeset = changeset_with_files(&["a.rs", "b.rs", "c.rs"]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);

    app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE))
        .expect("slash should open grep filter");
    assert!(line_text(&filter_bar_line(&app, 40)).starts_with(&format!("/{INPUT_CURSOR}")));
    for character in "line 1".chars() {
        app.handle_key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE))
            .expect("grep filter input should be handled");
    }

    assert_eq!(app.filters.grep_filter, "line 1");
    assert_eq!(app.filters.grep_filter_input, "line 1");
    assert_eq!(visible_paths(&app), vec!["b.rs"]);
    assert!(line_text(&filter_bar_line(&app, 40)).starts_with("/line 1"));

    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .expect("enter should keep grep filter");

    assert_eq!(app.filters.grep_filter, "line 1");
    assert_eq!(visible_paths(&app), vec!["b.rs"]);
    assert_eq!(app.filters.grep_matches.len(), 1);
    assert_eq!(app.current_grep_match_row(), Some(2));
    assert!(line_text(&filter_bar_line(&app, 40)).starts_with("/line 1"));
    assert!(!line_text(&statusline_header_line(&app, 120)).contains("/:line 1"));
    assert!(app.filters.filter_input.is_none());
    assert!(filter_bar_visible(&app));
    assert!(line_text(&filter_bar_line(&app, 40)).starts_with("/line 1"));

    app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE))
        .expect("slash should reopen grep filter");
    assert_eq!(app.filters.grep_filter, "");
    assert_eq!(app.filters.grep_filter_input, "");
    assert_eq!(visible_paths(&app), vec!["a.rs", "b.rs", "c.rs"]);

    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
        .expect("escape should clear grep filter");

    assert_eq!(app.filters.grep_filter, "");
    assert!(app.filters.grep_matches.is_empty());
    assert_eq!(app.current_grep_match_row(), None);
    assert_eq!(visible_paths(&app), vec!["a.rs", "b.rs", "c.rs"]);
    assert!(app.filters.filter_input.is_none());

    let selected_file = app.sidebar.selected_file;
    let scroll = app.viewport.scroll;
    app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE))
        .expect("n should not navigate after grep is cleared");
    app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE))
        .expect("p should not navigate after grep is cleared");
    assert_eq!(app.sidebar.selected_file, selected_file);
    assert_eq!(app.viewport.scroll, scroll);
}

#[test]
fn slash_does_not_match_file_paths() {
    let changeset = changeset_with_files(&["unique_name.rs", "other.rs"]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);

    app.filters.grep_filter = "unique_name".to_owned();
    app.apply_filters(PostFilterNavigation::JumpToGrep);

    assert!(visible_paths(&app).is_empty());
    assert!(app.filters.grep_matches.is_empty());
}

#[test]
fn q_key_quits_without_leader() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);

    let should_quit = app
        .handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE))
        .expect("q should be handled");

    assert!(should_quit);
}

#[test]
fn space_is_unmapped_by_default() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);

    let should_quit = app
        .handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE))
        .expect("space should be handled");
    assert!(!should_quit);
    assert!(app.input.key_prefix_pending.is_none());
}

#[test]
fn configured_leader_escape_cancels() {
    let changeset = changeset_with_context_lines(1);
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
    assert!(app.input.key_prefix_pending.is_some());
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
        .expect("escape should cancel leader");

    assert!(app.input.key_prefix_pending.is_none());
}

#[test]
fn flat_action_keys_are_unmapped_under_leader() {
    let changeset = changeset_with_context_lines(1);
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
    app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE))
        .expect("leader f should be handled");
    assert!(app.filters.filter_input.is_none());

    app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE))
        .expect("leader should be handled");
    app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE))
        .expect("leader slash should be handled");
    assert!(app.filters.filter_input.is_none());

    app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE))
        .expect("leader should be handled");
    app.handle_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::SHIFT))
        .expect("leader question mark should be handled");
    assert!(!app.overlays.help_menu_is_open());
}

#[test]
fn default_mode_prefix_keys_open_diff_pickers() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.refs.comparison_branches = branch_names(&["main", "feature"]);
    app.refs.comparison_commits = vec![GitCommit {
        sha: "abcdef0".into(),
        subject: "commit".to_owned(),
    }];

    app.handle_key(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE))
        .expect("m prefix should be handled");
    assert!(app.input.key_prefix_pending.is_some());
    app.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE))
        .expect("m h should open head picker");
    assert_eq!(app.refs.branch_menu_open(), Some(BranchMenu::Head));

    app.close_branch_menu();
    app.handle_key(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE))
        .expect("m prefix should be handled");
    app.handle_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE))
        .expect("m b should open base picker");
    assert_eq!(app.refs.branch_menu_open(), Some(BranchMenu::Base));

    app.close_branch_menu();
    app.handle_key(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE))
        .expect("m prefix should be handled");
    app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE))
        .expect("m c should open commit picker");
    assert!(app.refs.commit_menu_is_open());
}

#[test]
fn ctrl_u_clears_active_filters() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.filters.file_filter = "src".to_owned();
    app.filters.file_filter_input = "src".to_owned();
    app.filters.grep_filter = "needle".to_owned();
    app.filters.grep_filter_input = "needle".to_owned();

    app.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL))
        .expect("ctrl-u should clear filters");

    assert!(app.filters.file_filter.is_empty());
    assert!(app.filters.file_filter_input.is_empty());
    assert!(app.filters.grep_filter.is_empty());
    assert!(app.filters.grep_filter_input.is_empty());
}

#[test]
fn configured_keymap_changes_leader_actions_and_flat_keys() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.config.keymap = Keymap::parse(
        r#"
        [keymap.global]
        leader = ","
        diff_menu = ", d"
        options_menu = ", o"
        file_filter = "ctrl-f"
        expand_context_up = []
        "#,
    )
    .expect("keymap should parse");

    app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE))
        .expect("unmapped f should be handled");
    assert!(app.filters.filter_input.is_none());

    app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL))
        .expect("configured file filter should be handled");
    assert_eq!(app.filters.filter_input, Some(DiffFilterKind::File));

    app.filters.filter_input = None;
    app.handle_key(KeyEvent::new(KeyCode::Char(','), KeyModifiers::NONE))
        .expect("configured leader should be handled");
    app.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE))
        .expect("configured diff menu should be handled");
    assert!(app.overlays.diff_menu_is_open());

    app.close_diff_menu();
    app.handle_key(KeyEvent::new(KeyCode::Char(','), KeyModifiers::NONE))
        .expect("configured leader should be handled");
    app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE))
        .expect("configured options menu should be handled");
    assert!(app.overlays.options_menu_is_open());
}

#[test]
fn default_edit_hunk_key_is_ctrl_g() {
    let mut changeset = changeset_with_hunk_at(PathBuf::from("/repo"), 20);
    set_test_file_deleted(&mut changeset.files[0]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(1);
    app.set_scroll(1);

    app.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE))
        .expect("unmapped e should be handled");
    assert_eq!(app.viewport.scroll, 1);
    assert!(app.notifications.error_log.is_none());

    app.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL))
        .expect("default edit key should be handled");
    assert!(app.notifications.error_log.is_none());
}

#[test]
fn ctrl_c_force_quit_wins_over_configured_edit_hunk_key() {
    let changeset = changeset_with_hunk_at(PathBuf::from("/repo"), 20);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.config.keymap = Keymap::parse(
        r#"
        [keymap.global]
        edit_hunk = "ctrl-c"
        "#,
    )
    .expect("keymap should parse");

    let should_quit = handle_test_key_event(
        &mut app,
        KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
    );

    assert!(should_quit);
    assert!(app.notifications.error_log.is_none());
}

#[test]
fn ctrl_shift_c_does_not_force_quit() {
    assert!(!is_quit_key(KeyEvent::new(
        KeyCode::Char('C'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT
    )));
    assert!(!is_quit_key(KeyEvent::new(
        KeyCode::Char('c'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT
    )));
}

#[test]
fn esc_without_overlays_or_filters_does_not_quit() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);

    let should_quit = app
        .handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
        .expect("Esc should be handled");

    assert!(!should_quit);
}

#[test]
fn esc_closes_error_log_without_quitting() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_error_log("reload failed:\nfatal: bad revision");

    assert!(app.notifications.error_log.is_some());
    assert_eq!(error_log_height(&app, 20), ERROR_LOG_DEFAULT_HEIGHT);

    let should_quit = app
        .handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
        .expect("Esc should close error log");

    assert!(!should_quit);
    assert!(app.notifications.error_log.is_none());
}

#[test]
fn esc_closes_error_log_and_clears_pending_leader() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.config.keymap = Keymap::parse(
        r#"
        [keymap.global]
        diff_menu = "space m"
        "#,
    )
    .expect("keymap should parse");
    app.set_error_log("reload failed");

    app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE))
        .expect("leader should be handled");
    assert!(app.input.key_prefix_pending.is_some());

    let should_quit = app
        .handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
        .expect("Esc should close error log");

    assert!(!should_quit);
    assert!(app.notifications.error_log.is_none());
    assert!(app.input.key_prefix_pending.is_none());

    let should_quit = app
        .handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE))
        .expect("q should be handled as a fresh quit key");

    assert!(should_quit);
}

#[test]
fn error_log_separator_drag_resizes_pane() {
    let changeset = changeset_with_context_lines(8);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_error_log("reload failed");
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 17))
        .expect("test terminal should be created");

    terminal
        .draw(|frame| crate::render::draw(frame, &mut app))
        .expect("error log draw should succeed");

    let separator_row = app
        .error_log_separator_row()
        .expect("error log should expose separator row");
    assert_eq!(separator_row, 11);

    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 0,
        row: separator_row,
        modifiers: KeyModifiers::NONE,
    })
    .expect("resize should start");

    assert!(app.notifications.error_log_resizing);

    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Drag(MouseButton::Left),
        column: 0,
        row: separator_row.saturating_sub(2),
        modifiers: KeyModifiers::NONE,
    })
    .expect("drag should resize");

    assert_eq!(
        app.notifications.error_log_height,
        ERROR_LOG_DEFAULT_HEIGHT + 2
    );

    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: 0,
        row: separator_row.saturating_sub(2),
        modifiers: KeyModifiers::NONE,
    })
    .expect("resize should stop");

    assert!(!app.notifications.error_log_resizing);
}

#[test]
fn copy_error_log_key_ignores_absent_error_log() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.config.keymap = Keymap::parse(
        r#"
        [keymap.global]
        copy_error_log = "z"
        "#,
    )
    .expect("keymap should parse");

    let should_quit = app
        .handle_key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE))
        .expect("copy key without error log should be handled");

    assert!(!should_quit);
    assert!(app.notifications.error_log.is_none());
    assert!(app.notifications.toasts.is_empty());
}

#[test]
fn copy_error_log_key_falls_through_without_error_log() {
    let changeset = changeset_with_files(&[
        "a.rs", "b.rs", "c.rs", "d.rs", "e.rs", "f.rs", "g.rs", "h.rs",
    ]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(6);
    app.config.keymap = Keymap::parse(
        r#"
        [keymap.global]
        copy_error_log = "d"
        "#,
    )
    .expect("keymap should parse");

    app.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE))
        .expect("copy key without error log should fall through");

    assert_eq!(app.viewport.scroll, 20);
    assert!(app.notifications.error_log.is_none());
    assert!(app.notifications.toasts.is_empty());
}

#[test]
fn copy_error_log_key_does_not_preempt_filter_input() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.config.keymap = Keymap::parse(
        r#"
        [keymap.global]
        copy_error_log = "z"
        "#,
    )
    .expect("keymap should parse");
    app.set_error_log("reload failed");
    app.open_filter_input(DiffFilterKind::File);

    let should_quit = app
        .handle_key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE))
        .expect("copy key should be handled as filter input");

    assert!(!should_quit);
    assert_eq!(app.filters.file_filter_input, "z");
    assert_eq!(app.filters.file_filter, "z");
    assert!(app.notifications.toasts.is_empty());
}

#[test]
fn diff_scroll_does_not_move_file_sidebar_scroll() {
    let changeset = changeset_with_files(&["a.rs", "b.rs", "c.rs", "d.rs", "e.rs"]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.sidebar.file_sidebar_open = true;
    app.set_viewport_rows(4);
    app.set_file_sidebar_scroll(1);

    app.set_scroll(0);

    assert_eq!(app.sidebar.selected_file, FILE_0);
    assert_eq!(app.sidebar.file_sidebar_scroll, 1);
}

#[test]
fn mouse_wheel_over_file_sidebar_scrolls_sidebar_only() {
    let changeset = changeset_with_files(&["a.rs", "b.rs", "c.rs", "d.rs", "e.rs"]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.sidebar.file_sidebar_open = true;
    app.sidebar.file_sidebar_render_width = 20;
    app.set_viewport_rows(4);

    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: 1,
        row: 1,
        modifiers: KeyModifiers::NONE,
    })
    .expect("sidebar scroll should be handled");

    assert_eq!(app.viewport.scroll, 0);
    assert_eq!(app.sidebar.file_sidebar_scroll, 1);
}

#[test]
fn horizontal_mouse_wheel_over_file_sidebar_is_ignored() {
    let changeset = changeset_with_line_text("abcdefghijkl");
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.sidebar.file_sidebar_open = true;
    app.sidebar.file_sidebar_render_width = 20;
    app.set_viewport_width(18);

    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::ScrollRight,
        column: 1,
        row: 1,
        modifiers: KeyModifiers::NONE,
    })
    .expect("sidebar horizontal scroll should be ignored");

    assert_eq!(app.viewport.horizontal_scroll, 0);

    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::ScrollRight,
        column: 21,
        row: 1,
        modifiers: KeyModifiers::NONE,
    })
    .expect("diff horizontal scroll should still work");

    assert_eq!(app.viewport.horizontal_scroll, HORIZONTAL_SCROLL_STEP);
}

#[test]
fn file_sidebar_separator_drag_resizes_sidebar() {
    let changeset = changeset_with_files(&["a.rs"]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.sidebar.file_sidebar_open = true;
    app.sidebar.file_sidebar_render_width = 30;
    app.viewport.viewport_width = 70;

    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 29,
        row: 1,
        modifiers: KeyModifiers::NONE,
    })
    .expect("resize should start");

    assert!(app.sidebar.file_sidebar_resizing);

    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Drag(MouseButton::Left),
        column: 49,
        row: 1,
        modifiers: KeyModifiers::NONE,
    })
    .expect("drag should resize");

    assert_eq!(app.sidebar.file_sidebar_width, Some(50));
    assert!(app.runtime.dirty);

    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: 49,
        row: 1,
        modifiers: KeyModifiers::NONE,
    })
    .expect("resize should end");

    assert!(!app.sidebar.file_sidebar_resizing);
}

#[test]
fn diff_header_labels_describe_selected_source() {
    let mut options = DiffOptions::default();

    assert_eq!(diff_selector_text(&options), " All changes ");
    assert_eq!(diff_comparison_label(&options), "HEAD → working tree");

    options.source = DiffSource::Base("origin/main".into());
    assert_eq!(diff_selector_text(&options), " Branch ");
    assert_eq!(diff_comparison_label(&options), "HEAD → origin/main");

    options.source = DiffSource::Branch {
        base: "origin/main".into(),
        head: "feature/ui".into(),
    };
    assert_eq!(diff_comparison_label(&options), "feature/ui → origin/main");
}

#[test]
fn line_wrapping_scrolls_through_continuation_rows() {
    let changeset = changeset_with_line_text("abcdefghijklmnopqrstuvwx");
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.viewport.line_wrapping = true;
    app.set_viewport_width(18);
    app.set_viewport_rows(4);

    assert_eq!(app.document.model.len(), 3);
    assert_eq!(app.max_scroll(), 4);

    app.set_scroll(app.max_scroll());
    let lines = build_diff_viewport_lines(&mut app, 18, 4);
    let rendered = lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>();

    assert_eq!(rendered.len(), 4);
    assert!(rendered[0].contains("ijkl"));
    assert!(rendered[3].contains("uvwx"));
}

#[test]
fn line_wrapping_recomputes_scroll_bounds_after_width_change() {
    let changeset = changeset_with_line_text("abcdefghijkl");
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.viewport.line_wrapping = true;
    app.set_viewport_rows(1);

    app.set_viewport_width(18);
    assert_eq!(app.max_scroll(), 4);

    app.set_viewport_width(22);
    assert_eq!(app.max_scroll(), 3);
}

#[test]
fn responsive_resize_clamps_wrapped_scroll_after_width_change() {
    let changeset = changeset_with_line_text("abcdefghijklmnopqrstuvwx");
    let mut app = DiffApp::new_with_explicit_layout(
        DiffOptions::default(),
        changeset,
        DiffLayoutMode::Unified,
        SyntaxStartupMode::Disabled,
    );
    app.viewport.line_wrapping = true;
    app.set_viewport_width(18);
    app.set_viewport_rows(4);
    app.set_scroll(app.max_scroll());

    let previous_scroll = app.viewport.scroll;
    assert!(previous_scroll > 0);

    app.apply_responsive_layout(80);

    assert!(previous_scroll > app.max_scroll());
    assert_eq!(app.viewport.scroll, app.max_scroll());
    let lines = build_diff_viewport_lines(&mut app, 80, 4);
    assert!(!lines.is_empty());
    assert!(
        lines
            .iter()
            .any(|line| line_text(line).contains("abcdefghijklmnopqrstuvwx"))
    );
}

#[test]
fn select_file_scrolls_to_visual_file_start_for_wrapped_no_hunk_file() {
    let mut changeset = changeset_with_wrapped_leading_file();
    changeset.files[1].hunks_mut().clear();
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.viewport.line_wrapping = true;
    app.set_viewport_width(18);

    app.select_file(1);

    assert_eq!(app.sidebar.selected_file, FILE_1);
    assert_eq!(app.viewport.scroll, wrapped_file_start_scroll(&app, 1));
}

#[test]
fn replace_loaded_diff_preserves_wrapped_file_relative_scroll() {
    let mut app = DiffApp::new(
        DiffOptions::default(),
        changeset_with_wrapped_leading_file(),
        DiffLayoutMode::Unified,
    );
    let relative_scroll = 1;
    set_wrapped_scroll_relative_to_file_start(&mut app, 1, relative_scroll);
    let mut replacement = changeset_with_wrapped_leading_file();
    *replacement.files[1].hunks_mut()[0].lines[0].text_mut() = "updated target".to_owned();

    app.replace_loaded_diff(DiffOptions::default(), replacement);

    assert_eq!(
        app.viewport.scroll,
        wrapped_file_start_scroll(&app, 1).saturating_add(relative_scroll)
    );
}

#[test]
fn replace_path_changeset_preserves_wrapped_file_relative_scroll() {
    let mut app = DiffApp::new(
        DiffOptions::default(),
        changeset_with_wrapped_leading_file(),
        DiffLayoutMode::Unified,
    );
    let relative_scroll = 1;
    set_wrapped_scroll_relative_to_file_start(&mut app, 1, relative_scroll);
    let replacement = changeset_with_files(&["target.rs"]);

    app.replace_path_changeset(Path::new("target.rs"), replacement);

    assert_eq!(
        app.viewport.scroll,
        wrapped_file_start_scroll(&app, 1).saturating_add(relative_scroll)
    );
}

#[test]
fn replace_cached_diff_preserves_wrapped_file_relative_scroll() {
    let mut app = DiffApp::new(
        DiffOptions::default(),
        changeset_with_wrapped_leading_file(),
        DiffLayoutMode::Unified,
    );
    let relative_scroll = 1;
    set_wrapped_scroll_relative_to_file_start(&mut app, 1, relative_scroll);
    let options = DiffOptions {
        source: DiffSource::Worktree,
        ..DiffOptions::default()
    };
    let mut replacement = changeset_with_wrapped_leading_file();
    *replacement.files[1].hunks_mut()[0].lines[0].text_mut() = "cached target".to_owned();

    app.replace_cached_diff(
        options.clone(),
        diff_cache_entry(options, replacement),
        BranchMetadataPolicy::Preserve,
    );

    assert_eq!(
        app.viewport.scroll,
        wrapped_file_start_scroll(&app, 1).saturating_add(relative_scroll)
    );
}

#[test]
fn number_keys_do_not_switch_diff_choice() {
    let mut app = DiffApp::new(
        DiffOptions::default(),
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.refs.branch_base = Some("origin/main".to_owned());
    app.refs.current_head = Some("feature".to_owned());

    let should_quit = app
        .handle_key(KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE))
        .expect("number key should be handled");

    assert!(!should_quit);
    assert!(app.jobs.pending_diff_load.is_none());
    assert_eq!(app.document.options.source, DiffSource::Worktree);
}

#[test]
fn tab_keys_cycle_diff_choice() {
    let mut app = DiffApp::new(
        DiffOptions::default(),
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );

    let should_quit = app
        .handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
        .expect("tab should cycle diff type");

    assert!(!should_quit);
    let load = app
        .jobs
        .pending_diff_load
        .as_ref()
        .expect("tab should queue diff load");
    assert_eq!(load.options.source, DiffSource::Show("HEAD".into()));

    app.jobs.pending_diff_load = None;
    app.document.options = DiffOptions {
        source: DiffSource::Show("HEAD".into()),
        ..DiffOptions::default()
    };
    app.handle_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT))
        .expect("shift-tab should cycle diff type backwards");

    let load = app
        .jobs
        .pending_diff_load
        .as_ref()
        .expect("shift-tab should queue diff load");
    assert_eq!(load.options.source, DiffSource::Worktree);
}

#[test]
fn cached_tab_key_switches_diff_choice_without_loading() {
    let mut app = DiffApp::new(
        DiffOptions::default(),
        changeset_with_files(&["all.rs"]),
        DiffLayoutMode::Unified,
    );
    let show = DiffOptions {
        source: DiffSource::Show("HEAD".into()),
        ..DiffOptions::default()
    };
    let cached_changeset = changeset_with_files(&["show.rs"]);
    app.cache_loaded_diff(show.clone(), cached_changeset.clone());

    app.select_diff_choice(DiffChoice::Show);

    assert!(app.jobs.pending_diff_load.is_none());
    assert_eq!(app.document.options, show);
    assert_eq!(app.document.base_changeset, cached_changeset);
    assert_eq!(visible_paths(&app), vec!["show.rs"]);
}

#[test]
fn repeated_tab_uses_pending_diff_choice_for_next_target() {
    let mut app = DiffApp::new(
        DiffOptions::default(),
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.refs.branch_base = Some("main".to_owned());
    app.refs.current_head = Some("feature".to_owned());

    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
        .expect("tab should queue branch");
    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
        .expect("tab should advance from pending branch to show");

    let load = app
        .jobs
        .pending_diff_load
        .as_ref()
        .expect("second tab should queue diff load");
    assert_eq!(load.options.source, DiffSource::Show("HEAD".into()));
}

#[test]
fn number_key_does_not_switch_show_source_diff_choice() {
    let options = DiffOptions {
        source: DiffSource::Show("HEAD".into()),
        ..DiffOptions::default()
    };
    let mut app = DiffApp::new(
        options.clone(),
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.refs.branch_base = Some("origin/main".to_owned());
    app.refs.current_head = Some("feature".to_owned());

    let should_quit = app
        .handle_key(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE))
        .expect("number key should be handled");

    assert!(!should_quit);
    assert!(app.jobs.pending_diff_load.is_none());
    assert_eq!(app.document.options, options);
}

#[test]
fn diff_mouse_hover_tracks_position_inside_diff_area() {
    let changeset = changeset_with_line_text("abcdef");
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_rendered_diff_area(Rect {
        x: 10,
        y: 3,
        width: 40,
        height: 8,
    });

    app.set_viewport_rows(8);
    app.update_diff_mouse_hover(22, 5);
    assert_eq!(app.viewport.mouse_hover, Some((12, 2)));
    assert_eq!(
        app.diff_mouse_highlight_visual_row(),
        crate::render::viewport_plan::visual_scroll_for_viewport_row(&app, 2)
    );

    app.update_diff_mouse_hover(22, 5);
    app.update_diff_mouse_hover(0, 5);
    assert_eq!(app.viewport.mouse_hover, None);
}

#[test]
fn diff_mouse_hover_clears_when_diff_area_changes() {
    let changeset = changeset_with_line_text("abcdef");
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    let area = Rect {
        x: 0,
        y: 1,
        width: 40,
        height: 5,
    };
    app.set_rendered_diff_area(area);

    app.update_diff_mouse_hover(38, 1);
    assert_eq!(app.viewport.mouse_hover, Some((38, 0)));

    app.set_rendered_diff_area(area);
    assert_eq!(app.viewport.mouse_hover, Some((38, 0)));

    app.set_rendered_diff_area(Rect {
        x: 10,
        y: area.y,
        width: area.width,
        height: area.height,
    });

    assert_eq!(app.viewport.mouse_hover, None);
}

#[test]
fn diff_mouse_hover_clears_on_non_mouse_scroll() {
    let lines: Vec<&str> = (0..12).map(|_| "line").collect();
    let changeset = changeset_with_line_texts(&lines);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_rendered_diff_area(Rect {
        x: 0,
        y: 1,
        width: 40,
        height: 5,
    });
    app.set_viewport_width(40);
    app.set_viewport_rows(5);
    let code_row = app
        .document
        .model
        .rows
        .iter()
        .position(|row| matches!(row, UiRow::UnifiedLine { .. }))
        .expect("unified line");
    app.viewport.scroll = code_row;
    app.update_diff_mouse_hover(38, 1);

    let before = crate::render::diff::build_diff_viewport_lines(&mut app, 40, 5);
    assert!(line_text(&before[0]).contains("[+]"));

    app.scroll_by(1);

    assert_eq!(app.viewport.mouse_hover, None);
    let after = crate::render::diff::build_diff_viewport_lines(&mut app, 40, 5);
    assert!(!after.iter().any(|line| line_text(line).contains("[+]")));
}

#[test]
fn diff_mouse_hover_stays_on_mouse_scroll_inside_diff_area() {
    let lines: Vec<&str> = (0..12).map(|_| "line").collect();
    let changeset = changeset_with_line_texts(&lines);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_rendered_diff_area(Rect {
        x: 0,
        y: 1,
        width: 40,
        height: 5,
    });
    app.set_viewport_width(40);
    app.set_viewport_rows(5);
    let code_row = app
        .document
        .model
        .rows
        .iter()
        .position(|row| matches!(row, UiRow::UnifiedLine { .. }))
        .expect("unified line");
    app.viewport.scroll = code_row;

    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: 38,
        row: 1,
        modifiers: KeyModifiers::NONE,
    })
    .expect("mouse scroll");

    assert_eq!(app.viewport.mouse_hover, Some((38, 0)));
    let after = crate::render::diff::build_diff_viewport_lines(&mut app, 40, 5);
    assert!(line_text(&after[0]).contains("[+]"));
}

#[test]
fn inline_ascii_tokenizer_treats_vertical_tab_as_whitespace() {
    let tokens = inline_tokens("a \x0B\tb");

    assert_eq!(tokens.len(), 3);
    assert_eq!(tokens[1].byte_start, 1);
    assert_eq!(tokens[1].byte_end, 4);
    assert_eq!(inline_ascii_class(0x0B), InlineCharClass::Whitespace);
}

#[test]
fn mouse_scroll_starts_precise_then_accelerates_sustained_bursts() {
    let start = Instant::now();
    let mut scroll = MouseScroll::default();

    assert_eq!(scroll.scroll_delta(MouseScrollDirection::Down, start), 1);

    let mut total = 1;
    for tick in 1..10 {
        total += scroll.scroll_delta(
            MouseScrollDirection::Down,
            start + Duration::from_millis(tick * 20),
        );
    }

    assert!(total > 10, "sustained wheel bursts should accelerate");
    assert!(
        total <= 30,
        "acceleration should stay capped at three rows per tick"
    );
}

#[test]
fn mouse_scroll_resets_after_pause_or_direction_change() {
    let start = Instant::now();
    let mut scroll = MouseScroll::default();

    assert_eq!(scroll.scroll_delta(MouseScrollDirection::Down, start), 1);
    assert!(
        scroll.scroll_delta(
            MouseScrollDirection::Down,
            start + Duration::from_millis(20)
        ) >= 1
    );
    assert_eq!(
        scroll.scroll_delta(
            MouseScrollDirection::Down,
            start + Duration::from_millis(400)
        ),
        1
    );
    assert_eq!(
        scroll.scroll_delta(MouseScrollDirection::Up, start + Duration::from_millis(420)),
        -1
    );
}
