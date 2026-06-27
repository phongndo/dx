use super::*;

#[test]
fn viewport_focus_offset_slides_between_edges_and_center() {
    assert_eq!(viewport_focus_offset(0, 100, 11), 0);
    assert_eq!(viewport_focus_offset(3, 100, 11), 3);
    assert_eq!(viewport_focus_offset(20, 100, 11), 5);
    assert_eq!(viewport_focus_offset(87, 100, 11), 8);
    assert_eq!(viewport_focus_offset(89, 100, 11), 10);
    assert_eq!(viewport_focus_offset(0, 5, 11), 2);
}

#[test]
fn n_and_p_do_not_navigate_without_grep_filter() {
    let changeset = changeset_with_files(&["a.rs", "b.rs", "c.rs"]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(7);

    app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE))
        .expect("n should be ignored without grep");
    assert_eq!(app.sidebar.selected_file, 0);
    assert_eq!(app.focused_hunk_for_viewport(7), Some((0, 0)));

    app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE))
        .expect("p should be ignored without grep");
    assert_eq!(app.sidebar.selected_file, 0);
    assert_eq!(app.focused_hunk_for_viewport(7), Some((0, 0)));
}

#[test]
fn draw_reserves_filter_bar_only_while_filter_visible() {
    let changeset = changeset_with_files(&["a.rs", "b.rs", "c.rs"]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 10))
        .expect("test terminal should be created");

    terminal
        .draw(|frame| crate::render::draw(frame, &mut app))
        .expect("initial draw should succeed");
    assert_eq!(app.viewport.viewport_rows, 9);

    app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE))
        .expect("f should open file filter");
    terminal
        .draw(|frame| crate::render::draw(frame, &mut app))
        .expect("file filter draw should succeed");
    assert_eq!(app.viewport.viewport_rows, 8);

    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
        .expect("escape should close file filter");
    terminal
        .draw(|frame| crate::render::draw(frame, &mut app))
        .expect("draw after closing filter should succeed");
    assert_eq!(app.viewport.viewport_rows, 9);

    app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE))
        .expect("slash should open grep filter");
    terminal
        .draw(|frame| crate::render::draw(frame, &mut app))
        .expect("grep filter draw should succeed");
    assert_eq!(app.viewport.viewport_rows, 8);
}

#[test]
fn n_and_p_navigate_grep_matches_when_grep_filter_is_active() {
    let changeset = changeset_with_files(&["a.rs", "b.rs", "c.rs"]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(5);
    app.filters.grep_filter = "line".to_owned();
    app.apply_filters(true);

    assert_eq!(app.filters.grep_matches.len(), 3);
    assert_eq!(app.current_grep_match_row(), Some(2));
    let row = app.document.model.row(2).unwrap();
    let rendered = render_row(&mut app, 2, row, 40);
    assert!(
        rendered
            .spans
            .iter()
            .any(|span| span.content.as_ref() == "line"
                && span.style.bg == Some(app.config.theme.search_match_bg)),
        "grep text should be highlighted"
    );
    assert!(
        rendered
            .spans
            .iter()
            .any(|span| span.content.as_ref() == "line"
                && span.style.fg == Some(app.config.theme.search_match_fg))
    );
    assert_ne!(
        rendered.spans[0].style.bg,
        Some(app.config.theme.search_match_bg)
    );

    app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE))
        .expect("n should move to next grep match");
    assert_eq!(app.current_grep_match_row(), Some(6));
    assert_eq!(
        app.viewport.scroll + viewport_center_offset(app.viewport.viewport_rows),
        6
    );

    app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE))
        .expect("p should move to previous grep match");
    assert_eq!(app.current_grep_match_row(), Some(2));
    assert_eq!(
        app.viewport.scroll + viewport_center_offset(app.viewport.viewport_rows),
        2
    );
}

#[test]
fn grep_match_stays_centered_after_viewport_rows_are_known() {
    let changeset = changeset_with_line_texts(&[
        "other 0", "other 1", "other 2", "other 3", "other 4", "needle", "other 6", "other 7",
    ]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.filters.grep_filter = "needle".to_owned();
    app.apply_filters(true);

    assert_eq!(app.viewport.viewport_rows, 1);
    assert_eq!(app.current_grep_match_row(), Some(7));
    assert_eq!(app.viewport.scroll, 7);

    app.set_viewport_rows(5);

    assert_eq!(app.current_grep_match_row(), Some(7));
    assert_eq!(
        app.viewport.scroll + viewport_center_offset(app.viewport.viewport_rows),
        7
    );
}

#[test]
fn queue_close_wakes_blocked_pop() {
    let queue = SyntaxWorkerQueue::new(8, 0);
    let worker_queue = queue.clone();
    let worker = thread::spawn(move || worker_queue.pop());

    queue.close();

    assert!(worker.join().unwrap().is_none());
}
