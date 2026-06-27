use super::*;

#[test]
fn hunk_focus_uses_rendered_rows_when_annotations_hide_model_rows() {
    use crate::annotation::AnnotationKey;
    use crate::render::viewport_plan::{ViewportSlotKind, plan_diff_viewport_rows};

    let repo = PathBuf::from("/repo");
    let changeset = changeset_with_hunks_at(repo.clone(), &[10, 20, 30]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_width(40);
    app.set_viewport_rows(5);

    let annotated_row = app
        .document
        .model
        .rows
        .iter()
        .position(|row| {
            matches!(
                row,
                UiRow::UnifiedLine {
                    file: 0,
                    hunk: 0,
                    ..
                }
            )
        })
        .expect("first hunk should have a rendered line");
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
        .insert(key, "one\ntwo\nthree".to_owned());
    app.set_scroll(annotated_row.saturating_sub(1));

    let rendered_hunks: Vec<_> = plan_diff_viewport_rows(&app, app.viewport.viewport_rows)
        .into_iter()
        .filter_map(|slot| match slot.kind {
            ViewportSlotKind::DiffVisual { model_row, .. } => app
                .document
                .model
                .row(model_row)
                .and_then(|row| row.hunk_key()),
            _ => None,
        })
        .collect();
    assert!(rendered_hunks.contains(&(0, 0)));
    assert!(!rendered_hunks.contains(&(0, 1)));
    assert_eq!(
        app.focused_hunk_for_viewport(app.viewport.viewport_rows),
        Some((0, 0))
    );
    assert_eq!(
        app.focused_hunk_editor_target(),
        Some(EditorTarget {
            path: repo.join("file.rs"),
            line: 10,
        })
    );
}

#[test]
fn hunk_navigation_scrolls_past_annotation_blocks_to_show_target_hunk() {
    use crate::annotation::AnnotationKey;
    use crate::render::viewport_plan::{ViewportSlotKind, plan_diff_viewport_rows};

    let changeset = changeset_with_hunks_at(PathBuf::from("/repo"), &[10, 20, 30]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_width(40);
    app.set_viewport_rows(5);

    let annotated_row = app
        .document
        .model
        .rows
        .iter()
        .position(|row| {
            matches!(
                row,
                UiRow::UnifiedLine {
                    file: 0,
                    hunk: 0,
                    ..
                }
            )
        })
        .expect("first hunk should have a rendered line");
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
        .insert(key, "one\ntwo\nthree".to_owned());
    app.set_scroll(annotated_row.saturating_sub(1));

    assert_eq!(
        app.focused_hunk_for_viewport(app.viewport.viewport_rows),
        Some((0, 0))
    );

    app.next_hunk();

    let target_hunk_row = app
        .document
        .model
        .hunk_start_row(0, 1)
        .expect("target hunk should have a header row");
    let rendered_rows: Vec<_> = plan_diff_viewport_rows(&app, app.viewport.viewport_rows)
        .into_iter()
        .filter_map(|slot| match slot.kind {
            ViewportSlotKind::DiffVisual { model_row, .. } => Some(model_row),
            _ => None,
        })
        .collect();
    assert!(rendered_rows.contains(&target_hunk_row));
    assert_eq!(app.viewport.manual_hunk_focus, Some((0, 1)));
    assert_eq!(
        app.focused_hunk_for_viewport(app.viewport.viewport_rows),
        Some((0, 1))
    );

    app.next_hunk();
    assert_eq!(
        app.focused_hunk_for_viewport(app.viewport.viewport_rows),
        Some((0, 2))
    );
}

#[test]
fn max_scroll_reaches_rows_hidden_by_saved_annotation() {
    use crate::annotation::AnnotationKey;
    use crate::render::viewport_plan::{ViewportSlotKind, plan_diff_viewport_rows};

    let lines: Vec<&str> = (0..5).map(|_| "line").collect();
    let changeset = changeset_with_line_texts(&lines);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_width(40);
    app.set_viewport_rows(app.document.model.len());

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

    assert!(app.max_scroll() > annotated_row);
    app.set_scroll(app.max_scroll());
    let rendered_rows: Vec<_> = plan_diff_viewport_rows(&app, app.viewport.viewport_rows)
        .into_iter()
        .filter_map(|slot| match slot.kind {
            ViewportSlotKind::DiffVisual { model_row, .. } => Some(model_row),
            _ => None,
        })
        .collect();
    assert!(rendered_rows.contains(&(app.document.model.len() - 1)));
}

#[test]
fn max_scroll_for_short_annotation_at_end_avoids_blank_viewport() {
    use crate::annotation::AnnotationKey;
    use crate::render::viewport_plan::{ViewportSlotKind, plan_diff_viewport_rows};

    let lines: Vec<&str> = (0..100).map(|_| "line").collect();
    let changeset = changeset_with_line_texts(&lines);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_width(40);
    app.set_viewport_rows(app.document.model.len());

    let annotated_row = app
        .document
        .model
        .rows
        .iter()
        .rposition(|row| matches!(row, UiRow::UnifiedLine { .. }))
        .expect("last unified line");
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

    assert!(app.max_scroll() < annotated_row);
    app.set_scroll(usize::MAX);
    let plans = plan_diff_viewport_rows(&app, app.viewport.viewport_rows);

    assert_eq!(plans.len(), app.viewport.viewport_rows);
    assert!(plans.iter().any(|slot| matches!(
        slot.kind,
        ViewportSlotKind::DiffVisual { model_row, .. } if model_row == annotated_row
    )));
}

#[test]
fn annotation_navigation_centers_and_advances_from_centered_annotation() {
    use crate::annotation::AnnotationKey;

    let lines = vec!["line"; 100];
    let changeset = changeset_with_line_texts(&lines);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(11);

    let unified_rows = app
        .document
        .model
        .rows
        .iter()
        .enumerate()
        .filter_map(|(index, row)| matches!(row, UiRow::UnifiedLine { .. }).then_some(index))
        .collect::<Vec<_>>();
    let first_row = unified_rows[19];
    let second_row = unified_rows[59];
    for row in [first_row, second_row] {
        let key = AnnotationKey::from_ui_row(
            &app.document.changeset,
            app.document.model.row(row).expect("annotated row"),
        )
        .expect("annotation key");
        app.annotations_state
            .annotations
            .insert(key, "note".to_owned());
    }

    let first_anchor = app.annotation_anchor_visual_scroll(first_row);
    let second_anchor = app.annotation_anchor_visual_scroll(second_row);
    let center = viewport_center_offset(app.viewport.viewport_rows);

    app.handle_key(KeyEvent::new(KeyCode::Char('}'), KeyModifiers::NONE))
        .expect("next annotation should be handled");
    assert_eq!(app.viewport.scroll + center, first_anchor);

    app.handle_key(KeyEvent::new(KeyCode::Char('}'), KeyModifiers::NONE))
        .expect("next annotation should advance from centered annotation");
    assert_eq!(app.viewport.scroll + center, second_anchor);

    app.handle_key(KeyEvent::new(KeyCode::Char('{'), KeyModifiers::NONE))
        .expect("previous annotation should advance from centered annotation");
    assert_eq!(app.viewport.scroll + center, first_anchor);
}

#[test]
fn annotation_navigation_renders_target_after_previous_saved_block() {
    use crate::annotation::AnnotationKey;
    use crate::render::viewport_plan::{ViewportSlotKind, plan_diff_viewport_rows};

    let lines = vec!["line"; 40];
    let changeset = changeset_with_line_texts(&lines);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_width(40);
    app.set_viewport_rows(5);

    let unified_rows = app
        .document
        .model
        .rows
        .iter()
        .enumerate()
        .filter_map(|(index, row)| matches!(row, UiRow::UnifiedLine { .. }).then_some(index))
        .collect::<Vec<_>>();
    let first_row = unified_rows[8];
    let second_row = unified_rows[10];
    for (row, note) in [(first_row, "one\ntwo\nthree"), (second_row, "target")] {
        let key = AnnotationKey::from_ui_row(
            &app.document.changeset,
            app.document.model.row(row).expect("annotated row"),
        )
        .expect("annotation key");
        app.annotations_state
            .annotations
            .insert(key, note.to_owned());
    }

    app.move_annotation(1);
    app.move_annotation(1);

    fn rendered_rows(app: &DiffApp) -> Vec<usize> {
        plan_diff_viewport_rows(app, app.viewport.viewport_rows)
            .into_iter()
            .filter_map(|slot| match slot.kind {
                ViewportSlotKind::DiffVisual { model_row, .. } => Some(model_row),
                _ => None,
            })
            .collect::<Vec<_>>()
    }

    assert!(rendered_rows(&app).contains(&second_row));

    app.move_annotation(-1);
    assert!(rendered_rows(&app).contains(&first_row));
}

#[test]
fn queue_editor_scoped_reload_marks_dirty_for_terminal_repaint() {
    let changeset = changeset_with_hunk_at(PathBuf::from("/repo"), 20);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.runtime.dirty = false;

    app.queue_editor_scoped_reload(EditorReloadRequest {
        path: PathBuf::from("src/file.rs"),
        pathspecs: vec![PathBuf::from("src/file.rs")],
    });

    assert!(app.runtime.dirty);
    assert!(app.jobs.pending_editor_reload.is_some());
}

#[test]
fn live_reload_started_state_marks_pending_until_loaded() {
    let changeset = changeset_with_files(&["src/lib.rs"]);
    let mut app = DiffApp::new(
        DiffOptions::default(),
        changeset.clone(),
        DiffLayoutMode::Unified,
    );
    let (reload_tx, mut reload_rx) = mpsc::channel(2);

    reload_tx
        .try_send(LiveDiffReload::Started)
        .expect("started reload should send");
    drain_live_reloads(&mut app, Some(&mut reload_rx));

    assert!(app.jobs.live_reload_invalidated);
    assert!(app.jobs.live_reload_pending);
    app.runtime.dirty = false;

    reload_tx
        .try_send(LiveDiffReload::Loaded(Ok(changeset)))
        .expect("loaded reload should send");
    drain_live_reloads(&mut app, Some(&mut reload_rx));

    assert!(!app.jobs.live_reload_invalidated);
    assert!(!app.jobs.live_reload_pending);
    assert!(app.runtime.dirty);
}

#[test]
fn grep_jump_scrolls_past_annotation_blocks_to_show_match() {
    use crate::annotation::AnnotationKey;
    use crate::render::viewport_plan::{ViewportSlotKind, plan_diff_viewport_rows};

    let changeset = changeset_with_line_texts(&["annotated", "other", "needle", "tail"]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_width(40);
    app.set_viewport_rows(5);

    let annotated_row = app
        .document
        .model
        .rows
        .iter()
        .position(|row| {
            matches!(
                row,
                UiRow::UnifiedLine {
                    file: 0,
                    hunk: 0,
                    line: 0
                }
            )
        })
        .expect("annotated row should exist");
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

    app.filters.grep_filter = "needle".to_owned();
    app.apply_filters(true);

    let match_row = app
        .current_grep_match_row()
        .expect("grep match should be selected");
    let rendered_rows: Vec<_> = plan_diff_viewport_rows(&app, app.viewport.viewport_rows)
        .into_iter()
        .filter_map(|slot| match slot.kind {
            ViewportSlotKind::DiffVisual { model_row, .. } => Some(model_row),
            _ => None,
        })
        .collect();
    assert!(rendered_rows.contains(&match_row));
}

#[test]
fn question_mark_key_opens_help_menu_and_filters_when_open() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);

    assert!(!app.overlays.help_menu_open);

    let should_quit = app
        .handle_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::SHIFT))
        .expect("? should be handled");
    assert!(!should_quit);
    assert!(app.overlays.help_menu_open);

    app.handle_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::SHIFT))
        .expect("? should filter help");
    assert!(app.overlays.help_menu_open);
    assert_eq!(app.overlays.help_menu_input, "?");

    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
        .expect("Esc should close help");
    assert!(!app.overlays.help_menu_open);
}

#[test]
fn question_mark_key_filters_branch_menu() {
    let options = DiffOptions {
        source: DiffSource::Base("main".to_owned()),
        ..DiffOptions::default()
    };
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(options, changeset, DiffLayoutMode::Unified);
    app.refs.branch_menu_open = Some(BranchMenu::Head);
    app.refs.comparison_branches = vec!["main".to_owned(), "feature/header".to_owned()];

    let should_quit = app
        .handle_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::SHIFT))
        .expect("? should be handled by branch filter");

    assert!(!should_quit);
    assert!(!app.overlays.help_menu_open);
    assert_eq!(app.refs.branch_menu.input, "?");
}

#[test]
fn copy_marks_writes_structured_json_to_clipboard_sequence() {
    use crate::annotation::AnnotationKey;

    let mut changeset = changeset_with_line_text("hello");
    changeset.files[0].hunks[0].lines[0].kind = DiffLineKind::Addition;
    changeset.files[0].hunks[0].lines[0].old_line = None;
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    let code_row = app
        .document
        .model
        .rows
        .iter()
        .position(|row| matches!(row, UiRow::UnifiedLine { .. }))
        .expect("unified line");
    let row = app.document.model.row(code_row).expect("row");
    let key = AnnotationKey::from_ui_row(&app.document.changeset, row).expect("key");
    app.annotations_state
        .annotations
        .insert(key, "Needs \"escaping\"\nand context".to_owned());
    let expected = concat!(
        "{\n",
        "  \"version\": 1,\n",
        "  \"marks\": [\n",
        "    {\n",
        "      \"path\": \"file.rs\",\n",
        "      \"new_line\": 1,\n",
        "      \"body\": \"Needs \\\"escaping\\\"\\nand context\"\n",
        "    }\n",
        "  ]\n",
        "}"
    );
    let mut output = Vec::new();

    app.copy_marks_to_writer(&mut output);

    assert_eq!(app.marks_clipboard_json().as_deref(), Some(expected));
    assert_eq!(
        String::from_utf8(output).expect("OSC 52 sequence should be UTF-8"),
        osc52_clipboard_sequence(expected)
    );
    assert_eq!(app.notifications.toasts.latest_text(), Some("marks copied"));
}

#[test]
fn copy_marks_omits_annotations_without_current_diff_line() {
    use crate::annotation::{AnnotationKey, AnnotationSide};

    let changeset = changeset_with_replacement_pair();
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    let new_row = app
        .document
        .model
        .rows
        .iter()
        .position(|row| matches!(row, UiRow::UnifiedLine { line: 1, .. }))
        .expect("addition line");
    let old_key = AnnotationKey {
        path: "file.rs".to_owned(),
        side: AnnotationSide::Old,
        line: 1,
    };
    let new_key = AnnotationKey::from_ui_row(
        &app.document.changeset,
        app.document.model.row(new_row).expect("new diff row"),
    )
    .expect("new-side key");
    assert_eq!(old_key.side, AnnotationSide::Old);
    assert_eq!(new_key.side, AnnotationSide::New);
    app.annotations_state
        .annotations
        .insert(old_key.clone(), "old note".to_owned());
    app.annotations_state
        .annotations
        .insert(new_key, "new note".to_owned());

    let mut replacement = changeset_with_line_text("new");
    replacement.files[0].old_path = None;
    replacement.files[0].status = FileStatus::Added;
    replacement.files[0].additions = 1;
    replacement.files[0].hunks[0].old_count = 0;
    replacement.files[0].hunks[0].lines[0].kind = DiffLineKind::Addition;
    replacement.files[0].hunks[0].lines[0].old_line = None;
    app.replace_loaded_diff(DiffOptions::default(), replacement);

    let expected = concat!(
        "{\n",
        "  \"version\": 1,\n",
        "  \"marks\": [\n",
        "    {\n",
        "      \"path\": \"file.rs\",\n",
        "      \"new_line\": 1,\n",
        "      \"body\": \"new note\"\n",
        "    }\n",
        "  ]\n",
        "}"
    );

    assert_eq!(app.marks_clipboard_json().as_deref(), Some(expected));
    assert_eq!(
        app.annotations_state
            .annotations
            .get(&old_key)
            .map(String::as_str),
        Some("old note")
    );
}

#[test]
fn copy_marks_includes_marks_on_collapsed_context_lines() {
    use crate::annotation::AnnotationKey;

    let repo = temp_test_dir("copy-collapsed-context-mark");
    fs::create_dir_all(&repo).expect("repo directory should be created");
    let text = (1..=80)
        .map(|line| format!("line {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(repo.join("file.rs"), text).expect("context file should be written");
    let changeset = changeset_with_hunk_at(repo, 50);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);

    assert!(app.expand_context_at_row(1));
    let context_row = app
        .document
        .model
        .rows
        .iter()
        .copied()
        .find(|row| matches!(row, UiRow::ContextLine { .. }))
        .expect("expanded context line");
    let key =
        AnnotationKey::from_ui_row(&app.document.changeset, context_row).expect("context key");
    app.annotations_state
        .annotations
        .insert(key.clone(), "context note".to_owned());

    assert!(app.hide_context(0, 0));
    assert!(
        !app.document.model.rows.iter().any(|row| matches!(
            row,
            UiRow::ContextLine { new_line, .. } if *new_line == key.line
        )),
        "marked context line should be collapsed"
    );

    let expected = format!(
        concat!(
            "{{\n",
            "  \"version\": 1,\n",
            "  \"marks\": [\n",
            "    {{\n",
            "      \"path\": \"file.rs\",\n",
            "      \"new_line\": {},\n",
            "      \"body\": \"context note\"\n",
            "    }}\n",
            "  ]\n",
            "}}"
        ),
        key.line
    );

    assert_eq!(
        app.marks_clipboard_json().as_deref(),
        Some(expected.as_str())
    );
}

#[test]
fn copy_marks_includes_marks_on_collapsed_trailing_context_lines() {
    use crate::annotation::AnnotationKey;

    let repo = temp_test_dir("copy-collapsed-trailing-context-mark");
    fs::create_dir_all(&repo).expect("repo directory should be created");
    let text = (1..=80)
        .map(|line| format!("line {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(repo.join("file.rs"), text).expect("context file should be written");
    let changeset = changeset_with_hunk_at(repo, 50);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);

    assert!(app.expand_trailing_context_for_key(0, 1));
    let context_row = app
        .document
        .model
        .rows
        .iter()
        .copied()
        .find(|row| matches!(row, UiRow::ContextLine { new_line: 51, .. }))
        .expect("expanded trailing context line");
    let key =
        AnnotationKey::from_ui_row(&app.document.changeset, context_row).expect("context key");
    app.annotations_state
        .annotations
        .insert(key.clone(), "trailing context note".to_owned());

    assert!(app.hide_context(0, 1));
    assert!(
        !app.document.model.rows.iter().any(|row| matches!(
            row,
            UiRow::ContextLine { new_line, .. } if *new_line == key.line
        )),
        "marked trailing context line should be collapsed"
    );
    app.annotations_state.annotations.insert(
        AnnotationKey {
            path: key.path.clone(),
            side: key.side,
            line: 81,
        },
        "stale trailing note".to_owned(),
    );

    let expected = format!(
        concat!(
            "{{\n",
            "  \"version\": 1,\n",
            "  \"marks\": [\n",
            "    {{\n",
            "      \"path\": \"file.rs\",\n",
            "      \"new_line\": {},\n",
            "      \"body\": \"trailing context note\"\n",
            "    }}\n",
            "  ]\n",
            "}}"
        ),
        key.line
    );

    assert_eq!(
        app.marks_clipboard_json().as_deref(),
        Some(expected.as_str())
    );
}

#[test]
fn copy_marks_without_marks_shows_notice_without_writing() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    let mut output = Vec::new();

    app.copy_marks_to_writer(&mut output);

    assert!(output.is_empty());
    assert_eq!(
        app.notifications.toasts.latest_text(),
        Some("no marks to copy")
    );
}

#[test]
fn osc52_clipboard_sequence_base64_encodes_text() {
    assert_eq!(osc52_clipboard_sequence("abc"), "\x1b]52;c;YWJj\x07");
    assert_eq!(osc52_clipboard_sequence("mark"), "\x1b]52;c;bWFyaw==\x07");
}

#[test]
fn notices_mark_dirty_when_configured_max_visible_is_zero() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.notifications.toasts = Toasts::new(NotificationSettings {
        mode: NotificationMode::Default,
        corner: ToastCorner::TopRight,
        timeout_ms: 1_500,
        max_visible: 0,
    });
    app.runtime.dirty = false;
    app.runtime.terminal_clear_requested = true;

    app.set_notice("editor closed");

    assert!(app.runtime.dirty);
    assert!(app.runtime.terminal_clear_requested);
    assert_eq!(
        app.notifications.toasts.latest_text(),
        Some("editor closed")
    );
}

#[test]
fn viewport_width_change_clamps_scroll_after_saved_annotation_rewraps() {
    use crate::annotation::AnnotationKey;

    let changeset = changeset_with_line_text("hello");
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(6);
    app.set_viewport_width(8);

    let code_row = app
        .document
        .model
        .rows
        .iter()
        .position(|row| matches!(row, UiRow::UnifiedLine { .. }))
        .expect("unified line");
    let key = AnnotationKey::from_ui_row(
        &app.document.changeset,
        app.document.model.row(code_row).expect("annotated row"),
    )
    .expect("annotation key");
    app.annotations_state.annotations.insert(
        key,
        "one two three four five six seven eight nine ten".to_owned(),
    );

    app.set_scroll(app.max_scroll());
    let narrow_scroll = app.viewport.scroll;
    assert!(narrow_scroll > 0);

    app.set_viewport_width(80);

    assert!(narrow_scroll > app.max_scroll());
    assert_eq!(app.viewport.scroll, app.max_scroll());
    let lines = build_diff_viewport_lines(&mut app, 80, 6);
    assert!(lines.iter().any(|line| line_text(line).contains("hello")));
}

#[test]
fn old_side_annotation_renders_and_edits_on_deletion_only_split_row() {
    use crate::annotation::{AnnotationKey, AnnotationSide};

    let mut changeset = changeset_with_replacement_pair();
    changeset.files[0].additions = 0;
    changeset.files[0].hunks[0].new_count = 0;
    changeset.files[0].hunks[0].lines.truncate(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Split);
    app.set_rendered_diff_area(Rect {
        x: 0,
        y: 1,
        width: 60,
        height: 8,
    });
    app.set_viewport_width(60);
    app.set_viewport_rows(8);
    let split_row = app
        .document
        .model
        .rows
        .iter()
        .position(|row| matches!(row, UiRow::SplitLine { .. }))
        .expect("split line");
    app.viewport.scroll = split_row;
    let key = AnnotationKey::from_ui_row(
        &app.document.changeset,
        app.document.model.row(split_row).expect("row"),
    )
    .expect("key");
    assert_eq!(key.side, AnnotationSide::Old);
    app.annotations_state
        .annotations
        .insert(key.clone(), "old note".to_owned());

    let rendered = crate::render::diff::build_diff_viewport_lines(&mut app, 60, 8);
    assert!(
        rendered
            .iter()
            .any(|line| line_text(line).contains("old note"))
    );
    let footer_row = rendered
        .iter()
        .position(|line| line_text(line).ends_with("[↻]"))
        .expect("edit footer") as u16;

    assert!(app.handle_diff_click(58, 1 + footer_row));
    let draft = app
        .annotations_state
        .annotation_draft
        .as_ref()
        .expect("draft");
    assert_eq!(draft.key, key);
    assert_eq!(draft.input, "old note");
}

#[test]
fn renamed_file_annotations_use_side_specific_paths() {
    use crate::annotation::{AnnotationKey, AnnotationSide};

    let mut changeset = changeset_with_replacement_pair();
    changeset.files[0].old_path = Some("old.rs".to_owned());
    changeset.files[0].new_path = Some("new.rs".to_owned());
    changeset.files[0].status = FileStatus::Renamed;

    let app = DiffApp::new(
        DiffOptions::default(),
        changeset.clone(),
        DiffLayoutMode::Unified,
    );
    let deletion_row = app
        .document
        .model
        .rows
        .iter()
        .position(|row| matches!(row, UiRow::UnifiedLine { line: 0, .. }))
        .expect("deletion line");
    let old_key = AnnotationKey {
        path: "old.rs".to_owned(),
        side: AnnotationSide::Old,
        line: 1,
    };

    let addition_row = app
        .document
        .model
        .rows
        .iter()
        .position(|row| matches!(row, UiRow::UnifiedLine { line: 1, .. }))
        .expect("addition line");
    assert_eq!(
        AnnotationKey::from_ui_row(
            &app.document.changeset,
            app.document.model.row(deletion_row).expect("deletion row"),
        ),
        None
    );
    let new_key = AnnotationKey::from_ui_row(
        &app.document.changeset,
        app.document.model.row(addition_row).expect("addition row"),
    )
    .expect("new-side key");
    assert_eq!(new_key.path, "new.rs");
    assert_eq!(new_key.side, AnnotationSide::New);

    let split_app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Split);
    let split_row = split_app
        .document
        .model
        .rows
        .iter()
        .find(|row| matches!(row, UiRow::SplitLine { .. }))
        .copied()
        .expect("split row");
    let split_keys =
        AnnotationKey::candidates_from_ui_row(&split_app.document.changeset, split_row);
    assert!(!split_keys.contains(&old_key));
    assert!(split_keys.contains(&new_key));

    let mut export_app = split_app;
    export_app
        .annotations_state
        .annotations
        .insert(new_key, "new note".to_owned());
    let json = export_app.marks_clipboard_json().expect("marks JSON");
    assert!(json.contains("\"path\": \"new.rs\""));
    assert!(json.contains("\"old_line\": 1"));
    assert!(json.contains("\"new_line\": 1"));
    assert!(!json.contains("\"path\": \"old.rs\""));
}

#[test]
fn meta_rows_do_not_render_annotation_add_button() {
    let mut changeset = changeset_with_line_text("placeholder");
    let line = &mut changeset.files[0].hunks[0].lines[0];
    line.kind = DiffLineKind::Meta;
    line.old_line = None;
    line.new_line = None;
    line.text = "\\ No newline at end of file".to_owned();
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Split);
    app.set_rendered_diff_area(Rect {
        x: 0,
        y: 1,
        width: 40,
        height: 5,
    });
    app.set_viewport_width(40);
    app.set_viewport_rows(5);
    let meta_row = app
        .document
        .model
        .rows
        .iter()
        .position(|row| matches!(row, UiRow::MetaLine { .. }))
        .expect("meta line");
    app.viewport.scroll = meta_row;
    app.update_diff_mouse_hover(38, 1);

    let rendered = crate::render::diff::build_diff_viewport_lines(&mut app, 40, 5);
    assert!(!line_text(&rendered[0]).contains("[+]"));
    assert!(!app.handle_diff_click(38, 1));
    assert!(app.annotations_state.annotation_draft.is_none());
}

#[test]
fn annotations_are_keyed_by_path_and_line_across_model_changes() {
    use crate::annotation::AnnotationKey;

    let changeset = changeset_with_line_text("hello");
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_width(40);
    app.set_viewport_rows(6);
    let code_row = app
        .document
        .model
        .rows
        .iter()
        .position(|row| matches!(row, UiRow::UnifiedLine { .. }))
        .expect("unified line");
    let row = app.document.model.row(code_row).expect("row");
    let key = AnnotationKey::from_ui_row(&app.document.changeset, row).expect("key");
    app.annotations_state
        .annotations
        .insert(key, "note".to_owned());

    app.set_layout(DiffLayoutMode::Split);
    let rendered = crate::render::diff::build_diff_viewport_lines(&mut app, 40, 6);
    assert!(rendered.iter().any(|line| line_text(line).contains("note")));

    let mut replacement = changeset_with_line_text("hello");
    replacement.files[0].old_path = Some("other.rs".to_owned());
    replacement.files[0].new_path = Some("other.rs".to_owned());
    app.replace_loaded_diff(DiffOptions::default(), replacement);

    let rendered = crate::render::diff::build_diff_viewport_lines(&mut app, 40, 6);
    assert!(!rendered.iter().any(|line| line_text(line).contains("note")));
    assert_eq!(app.marks_clipboard_json(), None);
}

#[test]
fn annotation_input_wraps_words_and_ctrl_s_saves() {
    use crate::annotation::AnnotationKey;

    let changeset = changeset_with_line_text("hello");
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_rendered_diff_area(Rect {
        x: 0,
        y: 1,
        width: 20,
        height: 8,
    });
    app.set_viewport_width(20);
    app.set_viewport_rows(8);
    let code_row = app
        .document
        .model
        .rows
        .iter()
        .position(|row| matches!(row, UiRow::UnifiedLine { .. }))
        .expect("unified line");
    app.viewport.scroll = code_row;
    app.update_diff_mouse_hover(18, 1);
    assert!(app.handle_diff_click(18, 1));

    for character in "alpha beta gamma delta".chars() {
        app.handle_annotation_input_key(KeyEvent::new(
            KeyCode::Char(character),
            KeyModifiers::NONE,
        ));
    }

    let lines = crate::render::diff::build_diff_viewport_lines(&mut app, 20, 8);
    let rendered: Vec<String> = lines.iter().map(line_text).collect();
    assert!(
        rendered
            .iter()
            .any(|line| line.contains("alpha beta gamma"))
    );
    assert!(rendered.iter().any(|line| line.contains("delta")));
    assert!(rendered.iter().all(|line| line.width() <= 20));

    app.handle_annotation_input_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    for character in "next".chars() {
        app.handle_annotation_input_key(KeyEvent::new(
            KeyCode::Char(character),
            KeyModifiers::NONE,
        ));
    }
    assert!(app.annotations_state.annotation_draft.is_some());

    app.handle_annotation_input_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));

    assert!(app.annotations_state.annotation_draft.is_none());
    let row = app.document.model.row(code_row).expect("row");
    let key = AnnotationKey::from_ui_row(&app.document.changeset, row).expect("key");
    assert_eq!(
        app.annotations_state
            .annotations
            .get(&key)
            .map(String::as_str),
        Some("alpha beta gamma delta\nnext")
    );
}

#[test]
fn annotation_rendering_preserves_whitespace_while_wrapping() {
    use crate::annotation::AnnotationKey;

    let changeset = changeset_with_line_text("hello");
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_width(12);
    app.set_viewport_rows(8);
    let code_row = app
        .document
        .model
        .rows
        .iter()
        .position(|row| matches!(row, UiRow::UnifiedLine { .. }))
        .expect("unified line");
    let key = AnnotationKey::from_ui_row(
        &app.document.changeset,
        app.document.model.row(code_row).expect("annotated row"),
    )
    .expect("annotation key");
    app.annotations_state
        .annotations
        .insert(key, "  indented  code\na\t\tb".to_owned());
    app.viewport.scroll = code_row;

    let rendered: Vec<String> = crate::render::diff::build_diff_viewport_lines(&mut app, 12, 8)
        .iter()
        .map(line_text)
        .collect();

    assert!(rendered.iter().any(|line| line.starts_with("  indented  ")));
    assert!(rendered.iter().any(|line| line.starts_with("code")));
    assert!(rendered.iter().any(|line| line.contains("a\t\tb")));
}

#[test]
fn annotation_input_supports_native_cursor_shortcuts() {
    use crate::annotation::AnnotationKey;

    let changeset = changeset_with_line_text("hello");
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_rendered_diff_area(Rect {
        x: 0,
        y: 1,
        width: 40,
        height: 8,
    });
    app.set_viewport_width(40);
    app.set_viewport_rows(8);
    let code_row = app
        .document
        .model
        .rows
        .iter()
        .position(|row| matches!(row, UiRow::UnifiedLine { .. }))
        .expect("unified line");
    app.viewport.scroll = code_row;
    app.update_diff_mouse_hover(38, 1);
    assert!(app.handle_diff_click(38, 1));

    for character in "hello world".chars() {
        app.handle_annotation_input_key(KeyEvent::new(
            KeyCode::Char(character),
            KeyModifiers::NONE,
        ));
    }
    app.handle_annotation_input_key(KeyEvent::new(KeyCode::Left, KeyModifiers::SUPER));
    app.handle_annotation_input_key(KeyEvent::new(KeyCode::Char('>'), KeyModifiers::NONE));
    app.handle_annotation_input_key(KeyEvent::new(KeyCode::Right, KeyModifiers::SUPER));
    app.handle_annotation_input_key(KeyEvent::new(KeyCode::Char('!'), KeyModifiers::NONE));
    app.handle_annotation_input_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    app.handle_annotation_input_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
    app.handle_annotation_input_key(KeyEvent::new(KeyCode::Delete, KeyModifiers::SUPER));
    app.handle_annotation_input_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));

    let row = app.document.model.row(code_row).expect("row");
    let key = AnnotationKey::from_ui_row(&app.document.changeset, row).expect("key");
    assert_eq!(
        app.annotations_state
            .annotations
            .get(&key)
            .map(String::as_str),
        Some(">hello world!\n")
    );
}

#[test]
fn annotation_save_and_cancel_use_configured_keybindings() {
    use crate::annotation::AnnotationKey;

    let changeset = changeset_with_line_text("hello");
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.config.keymap = Keymap::parse(
        r#"
        [keymap.global]
        save_mark = "ctrl-enter"
        cancel_mark = "ctrl-x"
        "#,
    )
    .expect("keymap should parse");
    app.set_rendered_diff_area(Rect {
        x: 0,
        y: 1,
        width: 40,
        height: 8,
    });
    app.set_viewport_width(40);
    app.set_viewport_rows(8);
    let code_row = app
        .document
        .model
        .rows
        .iter()
        .position(|row| matches!(row, UiRow::UnifiedLine { .. }))
        .expect("unified line");
    app.viewport.scroll = code_row;
    app.update_diff_mouse_hover(38, 1);
    assert!(app.handle_diff_click(38, 1));
    app.handle_annotation_input_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
    app.handle_annotation_input_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
    assert!(app.annotations_state.annotation_draft.is_some());
    app.handle_annotation_input_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL));

    let row = app.document.model.row(code_row).expect("row");
    let key = AnnotationKey::from_ui_row(&app.document.changeset, row).expect("key");
    assert_eq!(
        app.annotations_state
            .annotations
            .get(&key)
            .map(String::as_str),
        Some("n")
    );

    let rendered = build_diff_viewport_lines(&mut app, 40, 8);
    let edit_row = rendered
        .iter()
        .position(|line| line_text(line).ends_with("[↻]"))
        .expect("saved annotation edit footer") as u16
        + app.viewport.rendered_diff_area.expect("diff area").y;
    assert!(app.handle_diff_click(38, edit_row));
    assert!(app.annotations_state.annotation_draft.is_some());
    app.handle_annotation_input_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL));
    assert!(app.annotations_state.annotation_draft.is_none());
    assert_eq!(
        app.annotations_state
            .annotations
            .get(&key)
            .map(String::as_str),
        Some("n")
    );
}

#[test]
fn annotation_cancel_binding_preempts_overlapping_edit_hunk_shortcut() {
    let changeset = changeset_with_line_text("hello");
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.config.keymap = Keymap::parse(
        r#"
        [keymap.global]
        edit_hunk = "ctrl-x"
        cancel_mark = "ctrl-x"
        "#,
    )
    .expect("keymap should parse");
    app.set_rendered_diff_area(Rect {
        x: 0,
        y: 1,
        width: 40,
        height: 8,
    });
    app.set_viewport_width(40);
    app.set_viewport_rows(8);
    let code_row = app
        .document
        .model
        .rows
        .iter()
        .position(|row| matches!(row, UiRow::UnifiedLine { .. }))
        .expect("unified line");
    app.viewport.scroll = code_row;
    app.update_diff_mouse_hover(38, 1);
    assert!(app.handle_diff_click(38, 1));
    assert!(app.annotations_state.annotation_draft.is_some());

    assert!(!handle_test_key_event(
        &mut app,
        KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL)
    ));

    assert!(app.annotations_state.annotation_draft.is_none());
    assert!(app.annotations_state.annotations.is_empty());
}

#[test]
fn annotation_typing_e_does_not_open_editor_shortcut() {
    use crate::annotation::{AnnotationDraft, AnnotationKey};

    let changeset = changeset_with_line_text("hello");
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    let code_row = app
        .document
        .model
        .rows
        .iter()
        .position(|row| matches!(row, UiRow::UnifiedLine { .. }))
        .expect("unified line");
    let key = AnnotationKey::from_ui_row(
        &app.document.changeset,
        app.document.model.row(code_row).expect("annotated row"),
    )
    .expect("annotation key");
    app.annotations_state.annotation_draft = Some(AnnotationDraft {
        key,
        model_row_index: code_row,
        input: String::new(),
        cursor: 0,
    });

    assert!(!handle_test_key_event(
        &mut app,
        KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE)
    ));

    let draft = app
        .annotations_state
        .annotation_draft
        .as_ref()
        .expect("draft should remain open");
    assert_eq!(draft.input, "e");
    assert_eq!(draft.cursor, 1);
}

#[test]
fn annotation_save_binding_preempts_overlapping_edit_hunk_shortcut() {
    use crate::annotation::{AnnotationDraft, AnnotationKey};

    let changeset = changeset_with_line_text("hello");
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.config.keymap = Keymap::parse(
        r#"
        [keymap.global]
        edit_hunk = "ctrl-g"
        save_mark = "ctrl-g"
        "#,
    )
    .expect("keymap should parse");
    let code_row = app
        .document
        .model
        .rows
        .iter()
        .position(|row| matches!(row, UiRow::UnifiedLine { .. }))
        .expect("unified line");
    let key = AnnotationKey::from_ui_row(
        &app.document.changeset,
        app.document.model.row(code_row).expect("annotated row"),
    )
    .expect("annotation key");
    app.annotations_state.annotation_draft = Some(AnnotationDraft {
        key: key.clone(),
        model_row_index: code_row,
        input: "note".to_owned(),
        cursor: "note".len(),
    });

    assert!(!handle_test_key_event(
        &mut app,
        KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL)
    ));

    assert!(app.annotations_state.annotation_draft.is_none());
    assert_eq!(
        app.annotations_state
            .annotations
            .get(&key)
            .map(String::as_str),
        Some("note")
    );
}

#[test]
fn annotation_draft_bindings_preempt_hard_quit_key() {
    use crate::annotation::{AnnotationDraft, AnnotationKey};

    let changeset = changeset_with_line_text("hello");
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.config.keymap = Keymap::parse(
        r#"
        [keymap.global]
        save_mark = "ctrl-c"
        "#,
    )
    .expect("keymap should parse");
    let code_row = app
        .document
        .model
        .rows
        .iter()
        .position(|row| matches!(row, UiRow::UnifiedLine { .. }))
        .expect("unified line");
    let key = AnnotationKey::from_ui_row(
        &app.document.changeset,
        app.document.model.row(code_row).expect("annotated row"),
    )
    .expect("annotation key");
    app.annotations_state.annotation_draft = Some(AnnotationDraft {
        key: key.clone(),
        model_row_index: code_row,
        input: "note".to_owned(),
        cursor: "note".len(),
    });

    assert!(!handle_test_key_event(
        &mut app,
        KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)
    ));

    assert!(app.annotations_state.annotation_draft.is_none());
    assert_eq!(
        app.annotations_state
            .annotations
            .get(&key)
            .map(String::as_str),
        Some("note")
    );

    app.config.keymap = Keymap::parse(
        r#"
        [keymap.global]
        cancel_mark = "ctrl-c"
        "#,
    )
    .expect("keymap should parse");
    app.annotations_state.annotation_draft = Some(AnnotationDraft {
        key: key.clone(),
        model_row_index: code_row,
        input: "discard".to_owned(),
        cursor: "discard".len(),
    });

    assert!(!handle_test_key_event(
        &mut app,
        KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)
    ));

    assert!(app.annotations_state.annotation_draft.is_none());
    assert_eq!(
        app.annotations_state
            .annotations
            .get(&key)
            .map(String::as_str),
        Some("note")
    );
}

#[test]
fn annotation_compose_scrolls_with_annotated_line() {
    let lines: Vec<&str> = (0..12).map(|_| "line").collect();
    let changeset = changeset_with_line_texts(&lines);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_width(40);
    app.set_viewport_rows(6);
    let code_row = app
        .document
        .model
        .rows
        .iter()
        .position(|row| matches!(row, UiRow::UnifiedLine { .. }))
        .expect("unified line");
    app.viewport.scroll = code_row;
    app.set_rendered_diff_area(Rect {
        x: 0,
        y: 1,
        width: 40,
        height: 6,
    });
    let compose_viewport_row = (code_row - app.viewport.scroll) as u16;
    app.update_diff_mouse_hover(38, compose_viewport_row.saturating_add(1));
    assert!(app.handle_diff_click(38, compose_viewport_row.saturating_add(1)));
    assert!(app.annotations_state.annotation_draft.is_some());

    let before = crate::render::diff::build_diff_viewport_lines(&mut app, 40, 6);
    assert!(
        before
            .iter()
            .any(|line| line_text(line).contains(INPUT_CURSOR)),
        "compose visible when annotated line is in view"
    );

    app.set_scroll(code_row.saturating_add(6));
    let after = crate::render::diff::build_diff_viewport_lines(&mut app, 40, 6);
    assert!(
        !after
            .iter()
            .any(|line| line_text(line).contains(INPUT_CURSOR)),
        "compose should scroll with the line, not stick on screen"
    );

    app.set_scroll(code_row);
    let back = crate::render::diff::build_diff_viewport_lines(&mut app, 40, 6);
    assert!(
        back.iter()
            .any(|line| line_text(line).contains(INPUT_CURSOR)),
        "compose returns when line scrolls back into view"
    );
}

#[test]
fn inline_emphasis_marks_changed_tokens_in_paired_lines() {
    let lines = vec![
        DiffLine {
            kind: DiffLineKind::Deletion,
            old_line: Some(1),
            new_line: None,
            text: "let count = 1;".to_owned(),
        },
        DiffLine {
            kind: DiffLineKind::Addition,
            old_line: None,
            new_line: Some(1),
            text: "let total = 2;".to_owned(),
        },
    ];

    let emphasis = compute_hunk_inline_emphasis(&lines);

    assert_eq!(
        range_texts(&lines[0].text, &emphasis[0].ranges),
        ["count", "1"]
    );
    assert_eq!(
        range_texts(&lines[1].text, &emphasis[1].ranges),
        ["total", "2"]
    );
}

#[test]
fn closed_queue_marks_full_file_source_skipped() {
    let queue = SyntaxWorkerQueue::new(8, 0);
    queue.close();
    let mut syntax = syntax_runtime_with_queue(queue);
    let source_id = SyntaxSourceId {
        generation: 0,
        file: 0,
        side: DiffSide::New,
        kind: SyntaxSourceKind::FullFile,
    };
    let key = SyntaxKey {
        source: source_id,
        language_hash: 1,
        theme_id: SYNTAX_THEME_ID,
    };

    assert!(!syntax.queue_job(
        key,
        "rust".to_owned(),
        full_file_syntax_job_source(),
        SyntaxPriority::Visible,
        None,
    ));

    assert!(syntax.skipped_sources.contains(&source_id));
    assert_eq!(syntax.stats.jobs_skipped, 1);
    assert_eq!(syntax.stats.jobs_rejected, 0);
}

#[test]
fn hunk_source_keeps_single_line_without_trailing_newline_marker() {
    let lines = vec![DiffLine {
        kind: DiffLineKind::Addition,
        old_line: None,
        new_line: Some(1),
        text: "let value = 1;".to_owned(),
    }];

    let source = build_hunk_source(&lines, DiffSide::New, SyntaxLimits::default()).unwrap();

    assert_eq!(source.text, "let value = 1;");
    assert_eq!(source.line_map, vec![Some(0)]);
    assert_eq!(source.source_lines, 1);
}
