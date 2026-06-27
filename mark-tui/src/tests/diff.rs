use super::*;

#[test]
fn default_layout_uses_split_only_when_terminal_is_wide_enough() {
    assert_eq!(
        default_layout_for_width(MIN_SPLIT_WIDTH - 1),
        DiffLayoutMode::Unified
    );
    assert_eq!(
        default_layout_for_width(MIN_SPLIT_WIDTH),
        DiffLayoutMode::Split
    );
}

#[test]
fn hunk_focus_uses_sliding_viewport_anchor() {
    let changeset = changeset_with_hunks_at(PathBuf::from("/repo"), &[1, 2, 3]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(5);

    assert_eq!(app.focused_hunk_for_viewport(5), Some((0, 0)));

    app.set_scroll(1);
    assert_eq!(app.focused_hunk_for_viewport(5), Some((0, 1)));

    app.set_scroll(usize::MAX);
    assert_eq!(app.focused_hunk_for_viewport(5), Some((0, 2)));
    assert_eq!(app.viewport.scroll, app.max_scroll());
}

#[test]
fn bracket_hunk_navigation_places_oversized_hunk_at_top() {
    let changeset =
        changeset_with_hunk_line_counts(PathBuf::from("/repo"), &[(1, 2), (20, 20), (60, 2)]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(9);
    let range = app
        .document
        .model
        .hunk_row_range(0, 1)
        .expect("target hunk should have rows");

    app.next_hunk();

    assert_eq!(app.viewport.scroll, range.start - 1);
    assert!(matches!(
        app.document.model.row(app.viewport.scroll),
        Some(UiRow::Collapsed { .. })
    ));
    assert_eq!(
        app.document.model.row(app.viewport.scroll + 1),
        Some(UiRow::HunkHeader { file: 0, hunk: 1 })
    );
    assert_eq!(app.focused_hunk_for_viewport(9), Some((0, 1)));
}

#[test]
fn hunk_navigation_centers_with_surrounding_collapsed_context() {
    let changeset =
        changeset_with_hunk_line_counts(PathBuf::from("/repo"), &[(1, 2), (20, 2), (40, 2)]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(9);

    let range = app
        .document
        .model
        .hunk_row_range(0, 1)
        .expect("target hunk should have rows");
    let range_start = range.start - 1;
    let range_end = range.end + 1;
    assert!(matches!(
        app.document.model.row(range_start),
        Some(UiRow::Collapsed { .. })
    ));
    assert!(matches!(
        app.document.model.row(range.end),
        Some(UiRow::Collapsed { .. })
    ));

    app.next_hunk();

    let center = range_start.saturating_add(range_end.saturating_sub(range_start + 1) / 2);
    assert_eq!(
        app.viewport.scroll + viewport_center_offset(app.viewport.viewport_rows),
        center
    );
    assert_eq!(app.focused_hunk_for_viewport(9), Some((0, 1)));
}

#[test]
fn hunk_navigation_keeps_target_visible_after_expanded_pre_hunk_context() {
    let changeset = changeset_with_hunks_at(PathBuf::from("/repo"), &[50, 100]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(9);
    app.document.context_expansions.insert(
        ContextKey { file: 0, hunk: 1 },
        default_context_expand_step(),
    );
    app.document.model = UiModel::new(
        &app.document.changeset,
        app.viewport.layout,
        &app.document.context_expansions,
    );
    let hunk_row = app
        .document
        .model
        .hunk_start_row(0, 1)
        .expect("target hunk should have a header row");
    assert!(matches!(
        app.document.model.row(hunk_row - 1),
        Some(UiRow::Collapsed { .. })
    ));
    assert!(matches!(
        app.document.model.row(hunk_row - 2),
        Some(UiRow::ContextLine { .. })
    ));

    app.next_hunk();

    assert!(hunk_row >= app.viewport.scroll);
    assert!(hunk_row < app.viewport.scroll + app.viewport.viewport_rows);
    assert_eq!(app.focused_hunk_for_viewport(9), Some((0, 1)));
}

#[test]
fn hunk_navigation_includes_expanded_context_before_hunk() {
    let changeset = changeset_with_hunks_at(PathBuf::from("/repo"), &[50, 100, 150]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(8);
    app.document
        .context_expansions
        .insert(ContextKey { file: 0, hunk: 1 }, 3);
    app.document.model = UiModel::new(
        &app.document.changeset,
        app.viewport.layout,
        &app.document.context_expansions,
    );
    let context_start = app
        .document
        .model
        .rows
        .iter()
        .position(|row| {
            matches!(
                row,
                UiRow::ContextHide {
                    file: 0,
                    hunk: 1,
                    ..
                }
            )
        })
        .expect("expanded context should have a hide control");
    let hunk_row = app
        .document
        .model
        .hunk_start_row(0, 1)
        .expect("target hunk should have a header row");

    app.next_hunk();

    assert_eq!(app.viewport.scroll, context_start);
    assert!(hunk_row >= app.viewport.scroll);
    assert!(hunk_row < app.viewport.scroll + app.viewport.viewport_rows);
    assert_eq!(
        app.document.model.row(app.viewport.scroll),
        Some(UiRow::ContextHide {
            file: 0,
            hunk: 1,
            lines: 3
        })
    );
    assert_eq!(app.focused_hunk_for_viewport(8), Some((0, 1)));
}

#[test]
fn selecting_file_centers_and_focuses_its_first_hunk() {
    let changeset = changeset_with_files(&["a.rs", "b.rs", "c.rs"]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(7);

    app.select_file(1);

    let range = app
        .document
        .model
        .hunk_row_range(1, 0)
        .expect("selected file hunk should have rows");
    let hunk_center = range
        .start
        .saturating_add(range.end.saturating_sub(range.start).saturating_sub(1) / 2);
    assert_eq!(
        app.viewport.scroll + viewport_center_offset(app.viewport.viewport_rows),
        hunk_center
    );
    assert_eq!(app.sidebar.selected_file, 1);
    assert_eq!(app.focused_hunk_for_viewport(7), Some((1, 0)));
}

#[test]
fn paren_file_navigation_focuses_first_hunk_and_updates_selected_file() {
    let changeset = changeset_with_files(&["a.rs", "b.rs", "c.rs"]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(7);

    app.handle_key(KeyEvent::new(KeyCode::Char(')'), KeyModifiers::NONE))
        .expect(") should be handled");

    assert_eq!(app.sidebar.selected_file, 1);
    assert_eq!(app.focused_hunk_for_viewport(7), Some((1, 0)));

    app.handle_key(KeyEvent::new(KeyCode::Char(')'), KeyModifiers::NONE))
        .expect(") should be handled");
    assert_eq!(app.sidebar.selected_file, 2);
    assert_eq!(app.focused_hunk_for_viewport(7), Some((2, 0)));

    app.handle_key(KeyEvent::new(KeyCode::Char('('), KeyModifiers::NONE))
        .expect("( should be handled");
    assert_eq!(app.sidebar.selected_file, 1);
    assert_eq!(app.focused_hunk_for_viewport(7), Some((1, 0)));
}

#[test]
fn selecting_current_file_preserves_focused_hunk() {
    let changeset = changeset_with_hunks_at(PathBuf::from("/repo"), &[1, 2, 3]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(20);

    app.next_hunk();
    assert_eq!(app.viewport.manual_hunk_focus, Some((0, 1)));

    app.select_file(0);

    assert_eq!(app.sidebar.selected_file, 0);
    assert_eq!(app.viewport.manual_hunk_focus, Some((0, 1)));
    assert_eq!(app.focused_hunk_for_viewport(20), Some((0, 1)));
}

#[test]
fn replace_loaded_diff_clears_manual_hunk_focus() {
    let repo = PathBuf::from("/repo");
    let changeset = changeset_with_hunks_at(repo.clone(), &[10, 20, 30]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(20);

    app.next_hunk();
    app.next_hunk();
    assert_eq!(app.viewport.manual_hunk_focus, Some((0, 2)));
    assert_eq!(
        app.focused_hunk_editor_target(),
        Some(EditorTarget {
            path: repo.join("file.rs"),
            line: 30,
        })
    );

    app.replace_loaded_diff(
        DiffOptions::default(),
        changeset_with_hunks_at(repo.clone(), &[100, 200, 300]),
    );

    assert_eq!(app.viewport.manual_hunk_focus, None);
    assert_eq!(app.focused_hunk_for_viewport(20), Some((0, 0)));
    assert_eq!(
        app.focused_hunk_editor_target(),
        Some(EditorTarget {
            path: repo.join("file.rs"),
            line: 100,
        })
    );
}

#[test]
fn focused_hunk_editor_target_falls_back_to_hunk_start() {
    let repo = PathBuf::from("/repo");
    let changeset = changeset_with_hunks_at(repo.clone(), &[20, 40]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(5);
    app.set_scroll(1);

    assert_eq!(
        app.focused_hunk_editor_target(),
        Some(EditorTarget {
            path: repo.join("file.rs"),
            line: 40,
        })
    );
}

#[test]
fn focused_hunk_editor_target_skips_deleted_files() {
    let mut changeset = changeset_with_hunk_at(PathBuf::from("/repo"), 20);
    changeset.files[0].status = FileStatus::Deleted;
    changeset.files[0].new_path = None;
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(5);

    assert_eq!(app.focused_hunk_editor_target(), None);
}

#[test]
fn focused_hunk_editor_target_skips_show_sources() {
    let changeset = changeset_with_hunk_at(PathBuf::from("/repo"), 20);
    let mut app = DiffApp::new(
        DiffOptions {
            source: DiffSource::Show("HEAD~1".to_owned()),
            ..DiffOptions::default()
        },
        changeset,
        DiffLayoutMode::Unified,
    );
    app.set_viewport_rows(5);

    assert_eq!(app.focused_hunk_editor_target(), None);
    assert_eq!(app.focused_hunk_editor_reload_request(), None);
}

#[test]
fn editor_reload_behavior_supports_worktree_backed_diffs() {
    let changeset = changeset_with_hunk_at(PathBuf::from("/repo"), 20);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);

    assert_eq!(
        app.editor_reload_behavior(false, Some(Path::new("src/file.rs"))),
        EditorReloadBehavior::None
    );
    assert_eq!(
        app.editor_reload_behavior(true, Some(Path::new("src/file.rs"))),
        EditorReloadBehavior::ScopedAsync
    );
    assert_eq!(
        app.editor_reload_behavior(true, None),
        EditorReloadBehavior::Sync
    );

    app.document.options.source = DiffSource::Base("main".to_owned());
    assert_eq!(
        app.editor_reload_behavior(true, Some(Path::new("src/file.rs"))),
        EditorReloadBehavior::ScopedAsync
    );

    app.document.options.source = DiffSource::Branch {
        base: "main".to_owned(),
        head: "feature".to_owned(),
    };
    assert_eq!(
        app.editor_reload_behavior(true, Some(Path::new("src/file.rs"))),
        EditorReloadBehavior::None
    );
}

#[test]
fn focused_editor_reload_request_preserves_rename_pair() {
    let mut changeset = changeset_with_hunk_at(PathBuf::from("/repo"), 20);
    changeset.files[0].old_path = Some("old.rs".to_owned());
    changeset.files[0].new_path = Some("new.rs".to_owned());
    changeset.files[0].status = FileStatus::Renamed;
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(5);

    let request = app.focused_hunk_editor_reload_request().unwrap();

    assert_eq!(request.path, PathBuf::from("new.rs"));
    assert_eq!(
        request.pathspecs,
        vec![PathBuf::from("old.rs"), PathBuf::from("new.rs")]
    );
}

#[test]
fn explicit_layout_ignores_saved_layout_preference() {
    let settings = SyntaxSettings {
        layout: Some(LayoutSetting::Unified),
        ..SyntaxSettings::default()
    };

    assert_eq!(
        layout_override_from_settings(&settings, true),
        Some(DiffLayoutMode::Unified)
    );
    assert_eq!(layout_override_from_settings(&settings, false), None);

    let settings = SyntaxSettings {
        layout: Some(LayoutSetting::Dynamic),
        ..SyntaxSettings::default()
    };
    assert_eq!(layout_override_from_settings(&settings, true), None);
}

#[test]
fn file_changed_since_compares_target_fingerprint() {
    let dir = temp_test_dir("file-changed-since");
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("file.txt");
    fs::write(&path, "a").unwrap();

    let before = FileFingerprint::read(&path);
    assert!(!file_changed_since(&path, before));

    fs::write(&path, "abcd").unwrap();
    assert!(file_changed_since(&path, before));

    let missing = dir.join("missing.txt");
    assert!(!file_changed_since(&missing, None));
    assert!(file_changed_since(&missing, before));
}

#[test]
fn path_changeset_replaces_only_edited_file() {
    let changeset = changeset_with_files(&["a.rs", "b.rs", "c.rs"]);
    let replacement = changeset_with_files(&["b.rs"]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);

    app.replace_path_changeset(Path::new("b.rs"), replacement);

    assert_eq!(visible_paths(&app), vec!["a.rs", "b.rs", "c.rs"]);
    assert_eq!(
        app.document.changeset.files[0].hunks[0].lines[0].text,
        "line 0"
    );
    assert_eq!(
        app.document.changeset.files[1].hunks[0].lines[0].text,
        "line 0"
    );
    assert_eq!(
        app.document.changeset.files[2].hunks[0].lines[0].text,
        "line 2"
    );
}

#[test]
fn path_changeset_removes_file_when_diff_disappears() {
    let changeset = changeset_with_files(&["a.rs", "b.rs", "c.rs"]);
    let mut replacement = changeset_with_files(&[]);
    replacement.repo = PathBuf::from("/repo");
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);

    app.replace_path_changeset(Path::new("b.rs"), replacement);

    assert_eq!(visible_paths(&app), vec!["a.rs", "c.rs"]);
}

#[test]
fn ui_model_inserts_file_separator_between_files() {
    let changeset = changeset_with_files(&["a.rs", "b.rs"]);
    let model = UiModel::new(&changeset, DiffLayoutMode::Unified, &HashMap::new());

    assert_eq!(model.file_start_row(0), Some(0));
    assert_eq!(model.file_start_row(1), Some(4));
    assert_eq!(model.row(3), Some(UiRow::FileSeparator));
    assert_eq!(model.row(4), Some(UiRow::FileHeader(1)));
    assert_eq!(model.file_at_row(3), Some(0));
    assert_eq!(model.file_at_row(4), Some(1));
}

#[test]
fn ui_model_expands_context_after_previous_hunk_downward() {
    let step = default_context_expand_step();
    let changeset = changeset_with_hunks_at(PathBuf::from("/repo"), &[50, 100]);
    let mut expansions = HashMap::new();

    let model = UiModel::new(&changeset, DiffLayoutMode::Unified, &expansions);
    assert_eq!(
        model.row(4),
        Some(UiRow::Collapsed {
            file: 0,
            hunk: 1,
            old_start: 51,
            new_start: 51,
            lines: 49,
            expanded: 0,
        })
    );

    expansions.insert(ContextKey { file: 0, hunk: 1 }, step);
    let model = UiModel::new(&changeset, DiffLayoutMode::Unified, &expansions);

    assert_eq!(
        model.row(4),
        Some(UiRow::ContextHide {
            file: 0,
            hunk: 1,
            lines: step,
        })
    );
    assert_eq!(
        model.row(5),
        Some(UiRow::ContextLine {
            file: 0,
            old_line: 51,
            new_line: 51,
        })
    );
    assert_eq!(
        model.row(25),
        Some(UiRow::Collapsed {
            file: 0,
            hunk: 1,
            old_start: 71,
            new_start: 71,
            lines: 29,
            expanded: step,
        })
    );
    assert_eq!(model.row(26), Some(UiRow::HunkHeader { file: 0, hunk: 1 }));
}

#[test]
fn context_source_side_uses_loaded_fallback_side() {
    let repo = temp_test_dir("context-source-side-fallback");
    fs::create_dir_all(&repo).expect("repo directory should be created");
    git(&repo, &["init", "-q"]);
    git(&repo, &["config", "user.email", "test@example.com"]);
    git(&repo, &["config", "user.name", "Test"]);

    let text = (1..=10)
        .map(|line| format!("line {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(repo.join("file.rs"), text).expect("old file should be written");
    git(&repo, &["add", "file.rs"]);
    git(&repo, &["commit", "-q", "-m", "init"]);
    fs::remove_file(repo.join("file.rs")).expect("worktree file should be removed");

    let changeset = changeset_with_hunk_at(repo.clone(), 5);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);

    let (side, lines) = app
        .ensure_context_lines(0)
        .expect("old context should load after new side fails");
    assert_eq!(side, DiffSide::Old);
    assert_eq!(lines.first().map(String::as_str), Some("line 1"));
    assert_eq!(app.context_source_side(0), Some(DiffSide::Old));

    fs::remove_dir_all(repo).expect("repo directory should be removed");
}

#[test]
fn collapsed_context_control_is_minimal_and_readable() {
    let theme = DiffTheme::default();
    let line = context_show_line(20, false, "▴", 72, theme);
    let text = line_text(&line);

    assert_eq!(text.width(), 72);
    assert!(text.starts_with(DIFF_INDICATOR));
    assert!(text.contains("▴ show 20 unchanged lines"));
    assert_eq!(line.spans[1].style.fg, Some(theme.muted));
    assert_eq!(
        line.spans[0].style.bg,
        Some(line_gutter_bg(DiffLineKind::Meta, theme))
    );

    let hide = context_hide_line(20, "▾", 24, theme);
    let hide_text = line_text(&hide);
    assert!(hide_text.contains("▾ hide 20 unchanged"));
    assert_eq!(hide.spans[1].style.fg, Some(theme.muted));

    assert_eq!(context_expand_marker(0), "▴");
    assert_eq!(context_expand_marker(1), "▾");
    assert_eq!(context_hide_marker(0), "▾");
    assert_eq!(context_hide_marker(1), "▴");

    let more = context_show_line(20, true, "▾", 72, theme);
    assert!(line_text(&more).contains("▾ show 20 more unchanged lines"));
}

#[test]
fn responsive_layout_preserves_manual_unified_choice_on_wide_resize() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Split);

    app.toggle_layout();
    assert_eq!(app.viewport.layout, DiffLayoutMode::Unified);

    app.apply_responsive_layout(MIN_SPLIT_WIDTH + 40);

    assert_eq!(app.viewport.layout, DiffLayoutMode::Unified);
}

#[test]
fn responsive_layout_preserves_manual_split_choice_on_narrow_resize() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);

    app.toggle_layout();
    assert_eq!(app.viewport.layout, DiffLayoutMode::Split);
    assert_eq!(app.viewport.layout_override, Some(DiffLayoutMode::Split));

    app.apply_responsive_layout(MIN_SPLIT_WIDTH - 1);
    assert_eq!(app.viewport.layout, DiffLayoutMode::Split);
    assert_eq!(app.viewport.layout_override, Some(DiffLayoutMode::Split));

    app.apply_responsive_layout(MIN_SPLIT_WIDTH + 40);
    assert_eq!(app.viewport.layout, DiffLayoutMode::Split);
}

#[test]
fn explicit_layout_preserves_split_choice_on_narrow_resize() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new_with_explicit_layout(
        DiffOptions::default(),
        changeset,
        DiffLayoutMode::Split,
        SyntaxStartupMode::Disabled,
    );

    assert_eq!(app.viewport.layout, DiffLayoutMode::Split);
    assert_eq!(app.viewport.layout_override, Some(DiffLayoutMode::Split));
    assert_eq!(app.overlays.options_menu_draft.layout, LayoutSetting::Split);

    app.apply_responsive_layout(MIN_SPLIT_WIDTH - 1);

    assert_eq!(app.viewport.layout, DiffLayoutMode::Split);
    assert_eq!(app.viewport.layout_override, Some(DiffLayoutMode::Split));
}

#[test]
fn live_reload_invalidation_clears_cache_without_visible_pending_state() {
    let changeset = changeset_with_files(&["src/lib.rs"]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    let options = DiffOptions {
        scope: DiffScope::Unstaged,
        ..DiffOptions::default()
    };

    app.cache_loaded_diff(options, changeset_with_files(&["cached.rs"]));
    assert!(!app.jobs.diff_cache.is_empty());

    app.mark_live_reload_invalidated();

    assert!(app.jobs.live_reload_invalidated);
    assert!(!app.jobs.live_reload_pending);
    assert!(app.jobs.diff_cache.is_empty());

    let line = statusline_header_line(&app, 80);
    assert!(!line_text(&line).contains("refreshing diff"));
}

#[test]
fn explicit_diff_load_returns_before_replacing_changeset() {
    let changeset = changeset_with_files(&["src/lib.rs"]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    let patch = Arc::<[u8]>::from(
        b"diff --git a/other.rs b/other.rs\n--- a/other.rs\n+++ b/other.rs\n@@ -1 +1 @@\n-old\n+new\n"
            .as_slice(),
    );
    let options = DiffOptions {
        source: DiffSource::Patch(PatchSource::Text {
            label: "test patch".to_owned(),
            patch,
        }),
        include_untracked: false,
        ..DiffOptions::default()
    };

    app.start_diff_load(options, "diff unavailable");

    assert!(app.jobs.pending_diff_load.is_some());
    assert_eq!(app.document.changeset.files[0].display_path(), "src/lib.rs");

    for _ in 0..100 {
        app.drain_pending_diff_load();
        if app.jobs.pending_diff_load.is_none() {
            break;
        }
        thread::sleep(Duration::from_millis(1));
    }

    assert!(app.jobs.pending_diff_load.is_none());
    assert_eq!(app.document.changeset.files[0].display_path(), "other.rs");
    assert_eq!(app.document.options.source, DiffSource::Patch(PatchSource::Text {
        label: "test patch".to_owned(),
        patch: Arc::<[u8]>::from(
            b"diff --git a/other.rs b/other.rs\n--- a/other.rs\n+++ b/other.rs\n@@ -1 +1 @@\n-old\n+new\n"
                .as_slice(),
        ),
    }));
}

#[test]
fn file_filter_edit_with_active_grep_preserves_current_grep_match() {
    let changeset = changeset_with_files(&["a.rs", "b.rs", "c.rs"]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(5);
    app.filters.grep_filter = "line".to_owned();
    app.apply_filters(true);
    app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE))
        .expect("n should move to next grep match");
    let scroll_before_file_filter = app.viewport.scroll;

    app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE))
        .expect("f should open file filter");
    for character in "rs".chars() {
        app.handle_key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE))
            .expect("file filter input should be handled");
    }

    assert_eq!(visible_paths(&app), vec!["a.rs", "b.rs", "c.rs"]);
    assert_eq!(app.current_grep_match_row(), Some(6));
    assert_eq!(app.viewport.scroll, scroll_before_file_filter);
}

#[test]
fn configured_leader_diff_type_bindings_cycle_choices() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.config.keymap = Keymap::parse(
        r#"
        [keymap.global]
        next_diff_type = "space n"
        previous_diff_type = "space p"
        "#,
    )
    .expect("keymap should parse");

    app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE))
        .expect("leader should be handled");
    app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE))
        .expect("leader n should cycle diff type");
    let load = app
        .jobs
        .pending_diff_load
        .as_ref()
        .expect("leader n should queue diff load");
    assert_eq!(load.options.source, DiffSource::Show("HEAD".to_owned()));
    assert_eq!(load.options.scope, DiffScope::All);
    assert!(app.input.key_prefix_pending.is_none());

    app.jobs.pending_diff_load = None;
    app.document.options = DiffOptions {
        source: DiffSource::Show("HEAD".to_owned()),
        ..DiffOptions::default()
    };
    app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE))
        .expect("leader should be handled");
    app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE))
        .expect("leader p should cycle diff type");
    let load = app
        .jobs
        .pending_diff_load
        .as_ref()
        .expect("leader p should queue diff load");
    assert_eq!(load.options.source, DiffSource::Worktree);
    assert_eq!(load.options.scope, DiffScope::All);
    assert!(app.input.key_prefix_pending.is_none());
}

#[test]
fn range_diff_has_no_diff_type_choices() {
    let options = DiffOptions {
        source: DiffSource::Range {
            left: "main".to_owned(),
            right: "feature".to_owned(),
        },
        ..DiffOptions::default()
    };
    let mut app = DiffApp::new(
        options,
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.refs.branch_base = Some("main".to_owned());
    app.refs.branch_head = Some("feature".to_owned());
    app.refs.current_head = Some("main".to_owned());

    assert!(app.diff_menu_choices().is_empty());
}

#[test]
fn cached_current_diff_rebuilds_model_while_filter_apply_is_pending() {
    let mut app = DiffApp::new(
        DiffOptions::default(),
        changeset_with_files(&["all.rs", "filtered.rs"]),
        DiffLayoutMode::Unified,
    );
    let unstaged = DiffOptions {
        scope: DiffScope::Unstaged,
        ..DiffOptions::default()
    };
    app.cache_loaded_diff(unstaged.clone(), changeset_with_files(&["unstaged.rs"]));

    app.filters.file_filter = "filtered".to_owned();
    app.apply_filters(false);
    assert_eq!(visible_paths(&app), vec!["filtered.rs"]);

    app.filters.file_filter.clear();
    app.filters.file_filter_input.clear();
    app.jobs.filter_searching = true;

    app.select_diff_choice(DiffChoice::Unstaged);
    assert_eq!(app.document.options, unstaged);
    assert_eq!(visible_paths(&app), vec!["unstaged.rs"]);

    app.select_diff_choice(DiffChoice::All);
    assert_eq!(app.document.options, DiffOptions::default());
    assert_eq!(visible_paths(&app), vec!["all.rs", "filtered.rs"]);
}

#[test]
fn cached_diff_choice_is_not_reused_without_live_invalidator() {
    let mut app = DiffApp::new(
        DiffOptions::default(),
        changeset_with_files(&["all.rs"]),
        DiffLayoutMode::Unified,
    );
    let unstaged = DiffOptions {
        scope: DiffScope::Unstaged,
        ..DiffOptions::default()
    };
    app.cache_loaded_diff(unstaged.clone(), changeset_with_files(&["stale.rs"]));
    app.jobs.live_updates_allowed = false;
    app.jobs.live_updates_enabled = false;

    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
        .expect("tab should cycle to show");
    app.jobs.pending_diff_load = None;
    app.document.options = DiffOptions {
        source: DiffSource::Show("HEAD".to_owned()),
        ..DiffOptions::default()
    };

    let should_quit = app
        .handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
        .expect("tab should cycle diff type");

    assert!(!should_quit);
    let load = app
        .jobs
        .pending_diff_load
        .as_ref()
        .expect("tab should queue a fresh diff load");
    assert_eq!(load.options, unstaged);
    assert_eq!(
        app.document.options.source,
        DiffSource::Show("HEAD".to_owned())
    );
    assert_eq!(visible_paths(&app), vec!["all.rs"]);
    assert!(app.jobs.diff_cache.is_empty());
}

#[test]
fn cached_diff_choice_is_not_reused_during_pending_live_reload() {
    let mut app = DiffApp::new(
        DiffOptions::default(),
        changeset_with_files(&["all.rs"]),
        DiffLayoutMode::Unified,
    );
    let unstaged = DiffOptions {
        scope: DiffScope::Unstaged,
        ..DiffOptions::default()
    };
    app.cache_loaded_diff(unstaged.clone(), changeset_with_files(&["stale.rs"]));
    app.mark_live_reload_pending();

    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
        .expect("tab should cycle to show");
    app.jobs.pending_diff_load = None;
    app.document.options = DiffOptions {
        source: DiffSource::Show("HEAD".to_owned()),
        ..DiffOptions::default()
    };

    let should_quit = app
        .handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
        .expect("tab should cycle diff type");

    assert!(!should_quit);
    let load = app
        .jobs
        .pending_diff_load
        .as_ref()
        .expect("tab should queue a fresh diff load");
    assert_eq!(load.options, unstaged);
    assert_eq!(
        app.document.options.source,
        DiffSource::Show("HEAD".to_owned())
    );
    assert_eq!(visible_paths(&app), vec!["all.rs"]);
    assert!(app.jobs.diff_cache.is_empty());
    assert!(app.jobs.live_reload_pending);
}

#[test]
fn diff_prefetch_skips_when_live_reload_is_disabled() {
    let mut app = DiffApp::new(
        DiffOptions::default(),
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.jobs.live_updates_allowed = false;
    app.jobs.live_updates_enabled = false;

    app.start_diff_prefetches();

    assert!(app.jobs.pending_diff_prefetch.is_none());
    assert!(app.jobs.diff_prefetch_queue.is_empty());
    assert!(!app.jobs.diff_prefetch_started);
}

#[test]
fn diff_prefetch_skips_for_sources_without_live_reload() {
    let options = DiffOptions {
        source: DiffSource::Range {
            left: "main".to_owned(),
            right: "HEAD".to_owned(),
        },
        ..DiffOptions::default()
    };
    let mut app = DiffApp::new(
        options,
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );

    app.start_diff_prefetches();

    assert!(app.jobs.pending_diff_prefetch.is_none());
    assert!(app.jobs.diff_prefetch_queue.is_empty());
    assert!(!app.jobs.diff_prefetch_started);
}

#[test]
fn cycling_back_to_current_diff_clears_pending_load() {
    let mut app = DiffApp::new(
        DiffOptions::default(),
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );

    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
        .expect("tab should queue next diff type");
    assert!(app.jobs.pending_diff_load.is_some());

    app.handle_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT))
        .expect("shift-tab should return to current diff type");

    assert_eq!(app.document.options, DiffOptions::default());
    assert!(app.jobs.pending_diff_load.is_none());
}

#[test]
fn reload_invalidates_cached_diff_choices() {
    let mut app = DiffApp::new(
        DiffOptions::default(),
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    let unstaged = DiffOptions {
        scope: DiffScope::Unstaged,
        ..DiffOptions::default()
    };
    app.cache_loaded_diff(unstaged, changeset_with_files(&["unstaged.rs"]));

    app.reload().expect("reload should start");

    assert!(app.jobs.diff_cache.is_empty());
    assert!(app.jobs.pending_diff_prefetch.is_none());
    assert!(app.jobs.diff_prefetch_queue.is_empty());
    assert!(!app.jobs.diff_prefetch_started);
}

#[test]
fn cache_invalidation_preserves_pending_diff_load() {
    let mut app = DiffApp::new(
        DiffOptions::default(),
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    let pending_options = DiffOptions {
        scope: DiffScope::Unstaged,
        ..DiffOptions::default()
    };
    app.jobs.pending_diff_load = Some(pending_diff_load(pending_options.clone()));

    app.invalidate_diff_cache();

    assert_eq!(
        app.jobs
            .pending_diff_load
            .as_ref()
            .map(|load| &load.options),
        Some(&pending_options)
    );
}

#[test]
fn live_diff_filter_ignores_non_state_git_paths() {
    let repo = std::env::temp_dir().join("mark-tui-live-filter-repo");
    let other = std::env::temp_dir().join("mark-tui-live-filter-other");
    let filter = LiveDiffFilter {
        repo: repo.clone(),
        git_state_paths: vec![
            repo.join(".git/index"),
            repo.join(".git/index.lock"),
            repo.join(".git/refs"),
        ],
        exact_paths: Vec::new(),
    };

    assert!(filter.is_relevant_path(Path::new("src/lib.rs")));
    assert!(filter.is_relevant_path(&repo.join("src/lib.rs")));
    assert!(filter.is_relevant_path(&repo.join(".git/index")));
    assert!(filter.is_relevant_path(&repo.join(".git/index.lock")));
    assert!(filter.is_relevant_path(&repo.join(".git/refs/heads/main")));
    assert!(!filter.is_relevant_path(&repo.join(".git/logs/HEAD")));
    assert!(filter.is_relevant_path(&repo.join("vendor/plugin/.git")));
    assert!(!filter.is_relevant_path(&repo.join("vendor/plugin/.git/index.lock")));
    assert!(!filter.is_relevant_path(&repo.join("vendor/plugin/.git/objects/tmp")));
    assert!(!filter.is_relevant_path(&repo.join("vendor/plugin/.git/logs/HEAD")));
    assert!(filter.is_relevant_path(&repo.join("vendor/plugin/.git/HEAD")));
    assert!(filter.is_relevant_path(&repo.join("vendor/plugin/.git/index")));
    assert!(filter.is_relevant_path(&repo.join("vendor/plugin/.git/refs/heads/main")));
    assert!(filter.is_relevant_path(&repo.join("vendor/plugin/.git/info")));
    assert!(filter.is_relevant_path(&repo.join("vendor/plugin/.git/info/exclude")));
    assert!(!filter.is_relevant_path(&repo.join("vendor/plugin/.git/info/attributes")));
    assert!(filter.is_relevant_path(&repo.join("vendor/plugin/src/lib.rs")));
    assert!(!filter.is_relevant_path(&other.join("file.rs")));
}

#[test]
fn live_diff_watch_paths_upgrade_to_recursive() {
    let mut spec = LiveDiffWatchSpec::new(Path::new("repo"));

    spec.add_watch_path(PathBuf::from("repo/.git"), false);
    spec.add_watch_path(PathBuf::from("repo/.git"), true);

    let watch_path = spec
        .watch_paths
        .iter()
        .find(|watch_path| watch_path.path == Path::new("repo/.git"))
        .unwrap();
    assert!(watch_path.recursive);
}

#[test]
fn empty_diff_fill_draws_shifted_diagonal_pattern() {
    assert_eq!(empty_diff_fill_from(8, 0, 0), "╱  ╱  ╱ ");
    assert_eq!(empty_diff_fill_from(8, 1, 0), "  ╱  ╱  ");
    assert_eq!(empty_diff_fill_from(8, 2, 0), " ╱  ╱  ╱");
}

#[test]
fn focused_hunk_highlights_diff_indicators() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    let theme = app.config.theme;

    let header = render_row_with_focus(
        &mut app,
        1,
        UiRow::HunkHeader { file: 0, hunk: 0 },
        24,
        Some((0, 0)),
    );
    assert_eq!(header.spans[0].style.fg, Some(theme.hunk));
    assert!(header.spans[0].style.add_modifier.contains(Modifier::BOLD));

    let row = app
        .document
        .model
        .row(2)
        .expect("diff line should be visible");
    let focused = render_row_with_focus(&mut app, 2, row, 24, Some((0, 0)));
    let unfocused = render_row_with_focus(&mut app, 2, row, 24, Some((0, 1)));

    assert_eq!(focused.spans[0].style.fg, Some(theme.hunk));
    assert!(focused.spans[0].style.add_modifier.contains(Modifier::BOLD));
    assert_eq!(unfocused.spans[0].style.fg, Some(theme.muted));
    assert!(
        !unfocused.spans[0]
            .style
            .add_modifier
            .contains(Modifier::BOLD)
    );
}

#[test]
fn highlight_cache_evicts_least_recently_used_entry() {
    let mut cache = LruCache::new(2);
    let first = syntax_key(0);
    let second = syntax_key(1);
    let third = syntax_key(2);

    cache.insert(first, 1);
    cache.insert(second, 2);
    assert_eq!(cache.get(&first), Some(&1));

    cache.insert(third, 3);

    assert_eq!(cache.get(&second), None);
    assert_eq!(cache.get(&first), Some(&1));
    assert_eq!(cache.get(&third), Some(&3));
}

#[test]
fn oversized_hunks_fall_back_to_plain_diff_text() {
    let limits = SyntaxLimits::default();
    let text = "x".repeat(limits.max_line_bytes);
    let line_count = (limits.max_source_bytes / limits.max_line_bytes) + 2;
    let lines = (0..line_count)
        .map(|index| DiffLine {
            kind: DiffLineKind::Context,
            old_line: Some(index + 1),
            new_line: Some(index + 1),
            text: text.clone(),
        })
        .collect::<Vec<_>>();

    assert_eq!(
        build_hunk_source(&lines, DiffSide::New, limits).unwrap_err(),
        SyntaxSkipReason::TooLarge
    );
}

#[test]
fn full_file_sources_cover_diff_modes_and_statuses() {
    let repo = std::env::temp_dir();
    let file = mark_diff::DiffFile {
        old_path: Some("old.rs".to_owned()),
        new_path: Some("new.rs".to_owned()),
        status: mark_diff::FileStatus::Renamed,
        hunks: Vec::new(),
        additions: 0,
        deletions: 0,
        is_binary: false,
    };

    assert_eq!(
        full_file_source(&repo, &DiffOptions::default(), &file, DiffSide::Old)
            .unwrap()
            .kind,
        FullFileSourceKind::GitRevision {
            rev: "HEAD".to_owned(),
            path: "old.rs".to_owned(),
        }
    );
    assert_eq!(
        full_file_source(&repo, &DiffOptions::default(), &file, DiffSide::New)
            .unwrap()
            .kind,
        FullFileSourceKind::Worktree {
            path: "new.rs".to_owned(),
        }
    );

    let staged = DiffOptions {
        scope: DiffScope::Staged,
        ..DiffOptions::default()
    };
    assert_eq!(
        full_file_source(&repo, &staged, &file, DiffSide::New)
            .unwrap()
            .kind,
        FullFileSourceKind::GitIndex {
            path: "new.rs".to_owned(),
        }
    );

    let unstaged = DiffOptions {
        scope: DiffScope::Unstaged,
        ..DiffOptions::default()
    };
    assert_eq!(
        full_file_source(&repo, &unstaged, &file, DiffSide::Old)
            .unwrap()
            .kind,
        FullFileSourceKind::GitIndex {
            path: "old.rs".to_owned(),
        }
    );

    let base = DiffOptions {
        source: DiffSource::Base("main".to_owned()),
        ..DiffOptions::default()
    };
    assert_eq!(
        full_file_source(&repo, &base, &file, DiffSide::Old)
            .unwrap()
            .kind,
        FullFileSourceKind::GitMergeBase {
            base: "main".to_owned(),
            head: "HEAD".to_owned(),
            path: "old.rs".to_owned(),
        }
    );
    assert_eq!(
        full_file_source(&repo, &base, &file, DiffSide::New)
            .unwrap()
            .kind,
        FullFileSourceKind::Worktree {
            path: "new.rs".to_owned(),
        }
    );

    let range = DiffOptions {
        source: DiffSource::Range {
            left: "left".to_owned(),
            right: "right".to_owned(),
        },
        ..DiffOptions::default()
    };
    assert_eq!(
        full_file_source(&repo, &range, &file, DiffSide::New)
            .unwrap()
            .kind,
        FullFileSourceKind::GitRevision {
            rev: "right".to_owned(),
            path: "new.rs".to_owned(),
        }
    );

    let show = DiffOptions {
        source: DiffSource::Show("HEAD".to_owned()),
        ..DiffOptions::default()
    };
    assert!(full_file_source(&repo, &show, &file, DiffSide::Old).is_none());
    assert!(full_file_source(&repo, &show, &file, DiffSide::New).is_none());

    let patch = DiffOptions {
        source: DiffSource::Patch(mark_diff::PatchSource::Stdin(Arc::from(&b""[..]))),
        ..DiffOptions::default()
    };
    assert!(full_file_source(&repo, &patch, &file, DiffSide::New).is_none());

    let deleted = mark_diff::DiffFile {
        new_path: None,
        status: mark_diff::FileStatus::Deleted,
        ..file.clone()
    };
    assert!(full_file_source(&repo, &DiffOptions::default(), &deleted, DiffSide::New).is_none());
}

#[test]
fn full_file_source_loads_worktree_index_and_revision_contents() {
    let repo = temp_test_dir("full-file-source");
    fs::create_dir_all(&repo).expect("repo directory should be created");
    git(&repo, &["init", "-q"]);
    git(&repo, &["config", "user.email", "test@example.com"]);
    git(&repo, &["config", "user.name", "Test"]);

    fs::write(repo.join("file.rs"), "fn old() {}\n").expect("old file should be written");
    git(&repo, &["add", "file.rs"]);
    git(&repo, &["commit", "-q", "-m", "init"]);

    fs::write(repo.join("file.rs"), "fn new() {}\n").expect("new file should be written");
    assert_eq!(
        load_full_file_source(&FullFileSource {
            repo: repo.clone(),
            kind: FullFileSourceKind::GitRevision {
                rev: "HEAD".to_owned(),
                path: "file.rs".to_owned(),
            },
        })
        .unwrap(),
        "fn old() {}\n"
    );
    assert_eq!(
        load_full_file_source(&FullFileSource {
            repo: repo.clone(),
            kind: FullFileSourceKind::Worktree {
                path: "file.rs".to_owned(),
            },
        })
        .unwrap(),
        "fn new() {}\n"
    );

    git(&repo, &["add", "file.rs"]);
    assert_eq!(
        load_full_file_source(&FullFileSource {
            repo: repo.clone(),
            kind: FullFileSourceKind::GitIndex {
                path: "file.rs".to_owned(),
            },
        })
        .unwrap(),
        "fn new() {}\n"
    );

    fs::remove_dir_all(repo).expect("repo directory should be removed");
}
