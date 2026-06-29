use super::*;

#[test]
fn editor_command_helpers_choose_line_arguments() {
    assert_eq!(
        split_editor_command("nvim -f").unwrap(),
        vec!["nvim".to_owned(), "-f".to_owned()]
    );

    let quoted_code = split_editor_command(
        r#""/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code" -g"#,
    )
    .unwrap();
    assert_eq!(
        quoted_code,
        vec![
            "/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code".to_owned(),
            "-g".to_owned(),
        ]
    );
    assert!(editor_uses_goto_arg(&quoted_code[0]));

    assert_eq!(
        split_editor_command(r#"/Applications/Some\ Editor/bin/editor -g"#).unwrap(),
        vec![
            "/Applications/Some Editor/bin/editor".to_owned(),
            "-g".to_owned(),
        ]
    );
    assert_eq!(
        split_editor_command(r#"editor "--flag with spaces""#).unwrap(),
        vec!["editor".to_owned(), "--flag with spaces".to_owned()]
    );
    assert_eq!(split_editor_command(r#""unterminated"#), None);

    assert!(editor_uses_goto_arg("/usr/local/bin/code"));
    assert!(!editor_uses_goto_arg("vim"));

    let target = EditorTarget {
        path: PathBuf::from("/repo/file.rs"),
        line: 12,
    };
    assert_eq!(
        editor_args(&["code".to_owned()], &target),
        vec![
            "--wait".to_owned(),
            "--goto".to_owned(),
            "/repo/file.rs:12".to_owned(),
        ]
    );
    assert_eq!(
        editor_args(&["code".to_owned(), "--wait".to_owned()], &target),
        vec![
            "--wait".to_owned(),
            "--goto".to_owned(),
            "/repo/file.rs:12".to_owned(),
        ]
    );
    assert_eq!(
        editor_args(&["vim".to_owned(), "-f".to_owned()], &target),
        vec![
            "-f".to_owned(),
            "+12".to_owned(),
            "/repo/file.rs".to_owned(),
        ]
    );
}

#[test]
fn responsive_layout_preserves_options_menu_unified_choice_on_wide_resize() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Split);

    app.open_options_menu();
    app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE))
        .expect("right should select split layout");
    app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE))
        .expect("right should select unified layout");
    assert_eq!(app.viewport.layout, DiffLayoutMode::Unified);
    assert_eq!(app.viewport.layout_override, Some(DiffLayoutMode::Unified));

    app.apply_responsive_layout(MIN_SPLIT_WIDTH + 40);

    assert_eq!(app.viewport.layout, DiffLayoutMode::Unified);
}

#[test]
fn options_menu_dynamic_layout_tracks_terminal_width() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Split);
    app.set_manual_layout(DiffLayoutMode::Unified);
    app.set_viewport_width(usize::from(MIN_SPLIT_WIDTH) + 40);

    app.open_options_menu();
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .expect("enter should select dynamic layout");

    assert_eq!(app.viewport.layout_override, None);
    assert_eq!(app.viewport.layout, DiffLayoutMode::Split);

    app.apply_responsive_layout(MIN_SPLIT_WIDTH - 1);
    assert_eq!(app.viewport.layout, DiffLayoutMode::Unified);

    app.apply_responsive_layout(MIN_SPLIT_WIDTH + 40);
    assert_eq!(app.viewport.layout, DiffLayoutMode::Split);
}

#[test]
fn configured_help_key_filters_help_menu_when_open() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.config.keymap = Keymap::parse(
        r#"
        [keymap.global]
        help = "h"
        "#,
    )
    .expect("keymap should parse");

    app.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE))
        .expect("configured help key should open help");
    assert!(app.overlays.help_menu_is_open());

    app.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE))
        .expect("configured help key should filter help");
    assert!(app.overlays.help_menu_is_open());
    assert_eq!(app.overlays.help_menu_input, "h");
}

#[test]
fn configured_leader_help_key_filters_help_menu_when_open() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.config.keymap = Keymap::parse(
        r#"
        [keymap.global]
        help = "space h"
        "#,
    )
    .expect("keymap should parse");

    app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE))
        .expect("leader should be handled");
    app.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE))
        .expect("leader help should open help");
    assert!(app.overlays.help_menu_is_open());

    app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE))
        .expect("space should filter while help is open");
    assert!(app.input.key_prefix_pending.is_none());
    app.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE))
        .expect("h should filter while help is open");
    assert!(app.overlays.help_menu_is_open());
    assert_eq!(app.overlays.help_menu_input, " h");
    assert!(app.input.key_prefix_pending.is_none());
}

#[test]
fn m_m_opens_diff_source_menu() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);

    app.handle_key(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE))
        .expect("m prefix should be handled");
    assert!(app.input.key_prefix_pending.is_some());
    app.handle_key(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE))
        .expect("m m should be handled");

    assert!(app.overlays.diff_menu_is_open());
    assert_eq!(app.highlighted_diff_choice(), Some(DiffChoice::Show));
}

#[test]
fn o_key_opens_options_menu() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);

    app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE))
        .expect("o should be handled");

    assert!(app.overlays.options_menu_is_open());
    assert_eq!(app.highlighted_option(), Some(OptionsMenuItem::Layout));
}

#[test]
fn configured_edit_hunk_key_does_not_bypass_open_menus() {
    let mut changeset = changeset_with_hunk_at(PathBuf::from("/repo"), 20);
    set_test_file_deleted(&mut changeset.files[0]);
    let mut app = DiffApp::new(
        DiffOptions::default(),
        changeset.clone(),
        DiffLayoutMode::Unified,
    );
    app.config.keymap = Keymap::parse(
        r#"
        [keymap.global]
        edit_hunk = "j"
        "#,
    )
    .expect("keymap should parse");
    app.open_diff_menu();

    let should_quit = handle_test_key_event(
        &mut app,
        KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE),
    );

    assert!(!should_quit);
    assert!(app.overlays.diff_menu_is_open());
    assert_eq!(app.overlays.diff_menu.input, "j");
    assert!(app.notifications.error_log.is_none());

    let mut app = DiffApp::new(
        DiffOptions::default(),
        changeset.clone(),
        DiffLayoutMode::Unified,
    );
    app.config.keymap = Keymap::parse(
        r#"
        [keymap.global]
        edit_hunk = "enter"
        "#,
    )
    .expect("keymap should parse");
    app.open_options_menu();

    let should_quit =
        handle_test_key_event(&mut app, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert!(!should_quit);
    assert!(app.overlays.options_menu_is_open());
    assert_eq!(app.viewport.layout, DiffLayoutMode::Split);
    assert!(app.notifications.error_log.is_none());

    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.config.keymap = Keymap::parse(
        r#"
        [keymap.global]
        edit_hunk = "j"
        "#,
    )
    .expect("keymap should parse");
    app.refs.open_branch_menu(BranchMenu::Head);

    let should_quit = handle_test_key_event(
        &mut app,
        KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE),
    );

    assert!(!should_quit);
    assert_eq!(app.refs.branch_menu_open(), Some(BranchMenu::Head));
    assert_eq!(app.refs.branch_menu.input, "j");
    assert!(app.notifications.error_log.is_none());
}

#[test]
fn help_menu_esc_closes_without_quitting() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.overlays.open_help_menu();
    app.runtime.dirty = false;

    let should_quit = app
        .handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
        .expect("Esc should close help");

    assert!(!should_quit);
    assert!(!app.overlays.help_menu_is_open());
    assert!(app.runtime.dirty);
}

#[test]
fn esc_closes_diff_menu_before_error_log() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.overlays.open_diff_menu();
    app.set_error_log("reload failed");

    let should_quit = app
        .handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
        .expect("Esc should close topmost menu");

    assert!(!should_quit);
    assert!(!app.overlays.diff_menu_is_open());
    assert!(app.notifications.error_log.is_some());
}

#[test]
fn copy_error_log_key_does_not_preempt_branch_menu_input() {
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
    app.refs.open_branch_menu(BranchMenu::Head);

    let should_quit = app
        .handle_key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE))
        .expect("copy key should be handled as branch input");

    assert!(!should_quit);
    assert_eq!(app.refs.branch_menu.input, "z");
    assert!(app.notifications.toasts.is_empty());
}

#[test]
fn help_menu_lines_list_keybindings() {
    let width = 80;
    let keymap = Keymap::default();
    let lines = help_menu_lines(
        width,
        help_menu_content_rows(width),
        DiffTheme::default(),
        &keymap,
    );
    let text: Vec<_> = lines.iter().map(line_text).collect();

    assert_eq!(lines.len(), help_menu_content_rows(width));
    assert!(text.iter().any(|line| line.contains("?")));
    assert!(
        text.iter()
            .any(|line| line.contains("  q") && line.contains("quit"))
    );
    assert!(text.iter().any(|line| line.contains("Tab/Shift-Tab")));
    assert!(text.iter().any(|line| line.contains("Ctrl-C")));
    assert!(text.iter().any(|line| line.contains("j/k")));
    assert!(text.iter().any(|line| line.contains("n/p")));
    assert!(text.iter().any(|line| line.contains("[/]")));
    assert!(text.iter().any(|line| line.contains("(/)")));
    assert!(text.iter().any(|line| line.contains(",/.")));
    assert!(text.iter().any(|line| line.contains(" c")));
    assert!(
        text.iter()
            .any(|line| line.contains("Ctrl-G") && line.contains("edit focused hunk"))
    );
    assert!(text.iter().any(|line| line.contains("m m")));
    assert!(text.iter().any(|line| line.contains("Ctrl-Shift-C")));
    assert_eq!(keymap.global_action_label(GlobalAction::FileBrowser), "b");
    assert!(text.iter().any(|line| line.contains("toggle file sidebar")));
    assert_eq!(keymap.global_action_label(GlobalAction::Layout), "s");
    assert!(
        text.iter()
            .any(|line| line.contains(" s") && line.contains("split / unified"))
    );
    assert!(!text.iter().any(|line| line.contains("leader")));
    assert!(!text.iter().any(|line| line.contains("b, Space b")));
    assert!(!text.iter().any(|line| line.contains("s, Space s")));
    assert!(text.iter().any(|line| line.contains("Backspace")));
    assert!(text.iter().any(|line| line.contains("Ctrl-U")));
}

#[test]
fn help_menu_lines_use_configured_keymap_labels() {
    let width = 80;
    let keymap = Keymap::parse(
        r#"
        [keymap.global]
        leader = ","
        help = "ctrl-h"
        quit = "q"
        file_browser = ", v"
        layout = ", l"
        expand_context_up = []
        "#,
    )
    .expect("keymap should parse");
    let lines = help_menu_lines(
        width,
        help_menu_content_rows(width),
        DiffTheme::default(),
        &keymap,
    );
    let text: Vec<_> = lines.iter().map(line_text).collect();

    assert!(text.iter().any(|line| line.contains("Ctrl-H")));
    assert!(
        text.iter()
            .any(|line| line.contains("  q") && line.contains("quit"))
    );
    assert!(
        text.iter()
            .any(|line| line.contains(", v") && line.contains("file sidebar"))
    );
    assert!(
        text.iter()
            .any(|line| line.contains(", l") && line.contains("split / unified"))
    );
    assert!(!text.iter().any(|line| line.contains("Space q")));
}

#[test]
fn help_menu_long_key_labels_do_not_cover_descriptions() {
    let width = 80;
    let keymap = Keymap::default();
    let lines = help_menu_lines(
        width,
        help_menu_content_rows(width),
        DiffTheme::default(),
        &keymap,
    );
    let text: Vec<_> = lines.iter().map(line_text).collect();

    assert!(
        text.iter()
            .any(|line| line.contains("Cmd-←/→, Ctrl-A/E  line start / end"))
    );
}

#[test]
fn help_menu_long_key_labels_leave_description_space() {
    let keymap = Keymap::default();
    let line = help_menu_row_line(
        HelpMenuRow::Binding(
            HelpMenuKey::Static("Very-Long-Key-Label-That-Would-Otherwise-Cover-Text"),
            "description remains visible",
        ),
        48,
        DiffTheme::default(),
        &keymap,
    );

    assert!(line_text(&line).contains("description remains"));
}

#[test]
fn help_menu_rendered_rows_keep_key_description_separator() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.toggle_help_menu();
    for character in "line start".chars() {
        app.push_help_menu_input(character);
    }

    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 20))
        .expect("test terminal should be created");
    terminal
        .draw(|frame| crate::render::draw(frame, &mut app))
        .expect("help menu draw should succeed");

    let rows = buffer_rows(terminal.backend().buffer());
    assert!(
        rows.iter()
            .any(|row| row.contains("Cmd-←/→, Ctrl-A/E  line start / end")),
        "rendered rows did not keep key/description separator:\n{}",
        rows.join("\n")
    );
}

#[test]
fn help_menu_rendered_rows_are_wide_enough_for_default_descriptions() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.toggle_help_menu();

    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 80))
        .expect("test terminal should be created");
    terminal
        .draw(|frame| crate::render::draw(frame, &mut app))
        .expect("help menu draw should succeed");

    let rows = buffer_rows(terminal.backend().buffer());
    assert!(
        rows.iter()
            .any(|row| row.contains("next / previous grep match")),
        "rendered rows cut off grep description:\n{}",
        rows.join("\n")
    );

    app.set_help_menu_scroll(usize::MAX);
    terminal
        .draw(|frame| crate::render::draw(frame, &mut app))
        .expect("help menu draw should succeed");
    let rows = buffer_rows(terminal.backend().buffer());
    assert!(
        rows.iter().any(|row| row.contains("line start / end")),
        "rendered rows cut off annotation description:\n{}",
        rows.join("\n")
    );
}

#[test]
fn help_menu_height_is_capped_in_tall_terminals() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.toggle_help_menu();

    let visible_rows = help_menu_list_visible_rows(
        &app,
        Rect {
            x: 0,
            y: 0,
            width: 120,
            height: 60,
        },
    )
    .expect("help menu layout should exist");

    assert_eq!(visible_rows, 32);
}

#[test]
fn help_menu_ctrl_n_scrolls_without_tab() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_terminal_area(Rect {
        x: 0,
        y: 0,
        width: 80,
        height: 40,
    });
    app.toggle_help_menu();
    assert!(app.overlays.help_menu_visible_rows > 1);

    app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL))
        .expect("ctrl-n should scroll help");
    assert_eq!(app.overlays.help_menu_scroll, 1);

    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
        .expect("tab should not scroll help");
    assert_eq!(app.overlays.help_menu_scroll, 1);
}

#[test]
fn help_menu_page_down_uses_layout_before_paint() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_terminal_area(Rect {
        x: 0,
        y: 0,
        width: 80,
        height: 40,
    });
    app.toggle_help_menu();
    let page = app.overlays.help_menu_visible_rows;
    assert!(page > 1);

    app.handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE))
        .expect("page down should scroll help");
    let max_scroll = app.filtered_help_menu_rows().len().saturating_sub(page);
    assert_eq!(app.overlays.help_menu_scroll, page.min(max_scroll));
}

#[test]
fn help_menu_filter_matches_section_headers() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.toggle_help_menu();
    for character in "branch".chars() {
        app.handle_key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE))
            .expect("typing should filter help");
    }

    let rows = app.filtered_help_menu_rows();
    assert!(rows.contains(&HelpMenuRow::Section("Branch filter")));
    assert!(rows.contains(&HelpMenuRow::Binding(
        HelpMenuKey::Static("type"),
        "filter branches"
    )));
    assert!(!rows.contains(&HelpMenuRow::Section("Global")));
}

#[test]
fn help_menu_uses_diff_theme_colors() {
    let default_theme = DiffTheme::default();
    let section_color = Color::Rgb(10, 11, 12);
    let key_color = Color::Rgb(13, 14, 15);
    let theme = DiffTheme {
        background: Color::Rgb(1, 2, 3),
        header: Color::Rgb(4, 5, 6),
        foreground: Color::Rgb(7, 8, 9),
        syntax: SyntaxPalette {
            keyword: Some(section_color),
            function: Some(key_color),
            ..default_theme.syntax
        },
        ..default_theme
    };

    assert_eq!(help_menu_bg(theme), theme.background);
    assert_eq!(help_menu_title_color(theme), key_color);

    let keymap = Keymap::default();
    let section = help_menu_row_spans(HelpMenuRow::Section("Section"), 20, theme, &keymap);
    assert_eq!(section[0].style.fg, Some(section_color));
    assert_eq!(section[0].style.bg, Some(theme.background));

    let binding = help_menu_row_spans(
        HelpMenuRow::Binding(HelpMenuKey::Static("?"), "help"),
        20,
        theme,
        &keymap,
    );
    assert_eq!(binding[0].style.fg, Some(key_color));
    assert_eq!(binding[0].style.bg, Some(theme.background));
    assert_eq!(binding[1].style.fg, Some(theme.foreground));
    assert_eq!(binding[1].style.bg, Some(theme.background));
}

#[test]
fn commit_match_score_matches_sha_and_subject() {
    let commit = GitCommit {
        sha: "abcdef0123456789".into(),
        subject: "fix tui menus".to_owned(),
    };
    assert!(commit_match_score("abcdef0", &commit).is_some());
    assert!(commit_match_score("tui", &commit).is_some());
    assert!(commit_match_score("menus", &commit).is_some());
    assert!(commit_match_score("zzzz", &commit).is_none());
}

#[test]
fn diff_menu_show_detail_uses_resolved_head_sha() {
    let mut app = DiffApp::new(
        DiffOptions::default(),
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.refs.current_head = Some("feature".to_owned());
    assert_eq!(app.show_rev_menu_detail(), "feature");
    app.refs.current_head = Some("a1b2c3d".to_owned());
    assert_eq!(app.show_rev_menu_detail(), "a1b2c3d");
    app.refs.show_rev = Some("HEAD~1".to_owned());
    assert_eq!(app.show_rev_menu_detail(), "HEAD~1");
}

#[test]
fn diff_menu_show_loads_current_commit_like_branch() {
    let mut app = DiffApp::new(
        DiffOptions::default(),
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.open_diff_menu();
    while app.highlighted_diff_choice() != Some(DiffChoice::Show) {
        app.move_diff_menu_selection(1);
    }
    app.select_highlighted_diff_choice();
    assert!(!app.overlays.diff_menu_is_open());
    assert!(!app.refs.commit_menu_is_open());
    let load = app
        .jobs
        .pending_diff_load
        .as_ref()
        .expect("show choice should queue diff load");
    assert_eq!(load.options.source, DiffSource::Show("HEAD".into()));
    assert_eq!(load.options.worktree_scope(), None);
}

#[test]
fn diff_menu_lists_all_changes_first() {
    let mut app = DiffApp::new(
        DiffOptions::default(),
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.refs.branch_base = Some("origin/main".to_owned());

    assert_eq!(
        app.diff_menu_choices(),
        vec![
            DiffChoice::All,
            DiffChoice::Branch,
            DiffChoice::Show,
            DiffChoice::Unstaged,
            DiffChoice::Staged,
            DiffChoice::Review,
        ]
    );
}

#[test]
fn tab_does_not_switch_range_diff_to_branch_or_worktree() {
    let options = DiffOptions {
        source: DiffSource::Range {
            left: "main".into(),
            right: "feature".into(),
        },
        ..DiffOptions::default()
    };
    let mut app = DiffApp::new(
        options.clone(),
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.refs.branch_base = Some("main".to_owned());
    app.refs.branch_head = Some("feature".to_owned());
    app.refs.current_head = Some("main".to_owned());

    let should_quit = app
        .handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
        .expect("tab should be handled");

    assert!(!should_quit);
    assert!(app.jobs.pending_diff_load.is_none());
    assert_eq!(app.document.options, options);
}

#[test]
fn diff_menu_keyboard_selects_diff_choice() {
    let mut app = DiffApp::new(
        DiffOptions::default(),
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );

    app.open_diff_menu();
    assert_eq!(app.highlighted_diff_choice(), Some(DiffChoice::Show));

    app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE))
        .expect("down should move to unstaged");
    assert_eq!(app.highlighted_diff_choice(), Some(DiffChoice::Unstaged));

    let should_quit = app
        .handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .expect("enter should apply menu selection");

    assert!(!should_quit);
    assert!(!app.overlays.diff_menu_is_open());
    let load = app
        .jobs
        .pending_diff_load
        .as_ref()
        .expect("menu selection should queue diff load");
    assert_eq!(
        load.options.source,
        DiffSource::Worktree {
            scope: DiffScope::Unstaged
        }
    );
    assert_eq!(load.options.worktree_scope(), Some(DiffScope::Unstaged));
}

#[test]
fn diff_menu_review_choice_opens_review_input() {
    let mut app = DiffApp::new(
        DiffOptions::default(),
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );

    app.open_diff_menu();
    while app.highlighted_diff_choice() != Some(DiffChoice::Review) {
        app.move_diff_menu_selection(1);
    }
    app.select_highlighted_diff_choice();

    assert!(!app.overlays.diff_menu_is_open());
    assert!(app.overlays.review_input_is_open());
    assert_eq!(app.overlays.review_input, "");
}

#[test]
fn review_input_url_queues_review_load() {
    let mut app = DiffApp::new(
        DiffOptions::default(),
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );

    app.open_review_input();
    app.overlays.review_input = "https://github.com/owner/repo/pull/1".to_owned();
    app.overlays.review_input_cursor = app.overlays.review_input.len();
    let mut submitted_target = None;
    app.submit_review_input_for_test(|app, target| {
        submitted_target = Some(target);
        app.jobs.pending_review_load = Some(pending_review_load());
    });

    assert!(!app.overlays.review_input_is_open());
    assert_eq!(
        submitted_target.as_deref(),
        Some("https://github.com/owner/repo/pull/1")
    );
    assert!(app.jobs.pending_review_load.is_some());
}

#[test]
fn review_input_number_queues_review_load() {
    let mut app = DiffApp::new(
        DiffOptions::default(),
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );

    app.open_review_input();
    app.overlays.review_input = " 123 ".to_owned();
    app.overlays.review_input_cursor = app.overlays.review_input.len();
    let mut submitted_target = None;
    app.submit_review_input_for_test(|app, target| {
        submitted_target = Some(target);
        app.jobs.pending_review_load = Some(pending_review_load());
    });

    assert!(!app.overlays.review_input_is_open());
    assert_eq!(submitted_target.as_deref(), Some("123"));
    assert!(app.jobs.pending_review_load.is_some());
}

#[test]
fn review_load_repo_preserves_current_repo_context() {
    assert_eq!(
        DiffApp::review_load_repo_for_target(Path::new("/repo"), "123"),
        Some(PathBuf::from("/repo"))
    );
    assert_eq!(
        DiffApp::review_load_repo_for_target(Path::new("/repo"), " 123 "),
        Some(PathBuf::from("/repo"))
    );
    assert_eq!(
        DiffApp::review_load_repo_for_target(
            Path::new("/repo"),
            "https://github.com/owner/repo/pull/123",
        ),
        Some(PathBuf::from("/repo"))
    );
    assert_eq!(
        DiffApp::review_load_repo_for_target(Path::new(""), "123"),
        None
    );
}

#[test]
fn review_patch_source_uses_review_selector_label() {
    let options = DiffOptions {
        source: DiffSource::Patch(PatchSource::Review {
            label: "review owner/repo#123".into(),
            patch: Arc::from(&b""[..]),
        }),
        ..DiffOptions::default()
    };
    let app = DiffApp::new(
        options.clone(),
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );

    assert_eq!(diff_selector_text(&options), " Review ");
    assert_eq!(app.current_diff_choice(), Some(DiffChoice::Review));
    assert_eq!(app.selected_diff_menu_choice(), None);
    assert!(app.diff_menu_choices().contains(&DiffChoice::Review));
    assert!(app.selectable_diff_choices().contains(&DiffChoice::Review));
}

#[test]
fn review_load_preserves_include_untracked_for_followup_local_diffs() {
    let options = DiffOptions {
        local_untracked: mark_diff::UntrackedMode::Include,
        ..DiffOptions::default()
    };
    let mut app = DiffApp::new(
        options,
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    let review_options = DiffOptions {
        source: DiffSource::Patch(PatchSource::Review {
            label: "review owner/repo#123".into(),
            patch: Arc::from(&b""[..]),
        }),
        local_untracked: mark_diff::UntrackedMode::Exclude,
        ..DiffOptions::default()
    };
    let (tx, rx) = oneshot::channel();
    let _ = tx.send(Ok((review_options, changeset_with_context_lines(1))));
    app.jobs.pending_review_load = Some(PendingReviewLoad {
        error_prefix: "review unavailable".to_owned(),
        job: AsyncJob::new(rx),
    });

    app.drain_pending_review_load();

    assert!(app.document.options.include_untracked());
    assert!(
        app.options_for_choice(DiffChoice::All)
            .expect("all choice should be available")
            .include_untracked()
    );
}

#[test]
fn diff_menu_uses_configured_menu_keymap() {
    let mut app = DiffApp::new(
        DiffOptions::default(),
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.refs.branch_base = Some("main".to_owned());
    app.refs.branch_head = Some("feature".to_owned());
    app.refs.current_head = Some("feature".to_owned());
    app.config.keymap = Keymap::parse(
        r#"
        [keymap.menu]
        down = "j"
        up = "k"
        confirm = "space"
        close = "q"
        "#,
    )
    .expect("keymap should parse");

    app.open_diff_menu();
    assert_eq!(app.highlighted_diff_choice(), Some(DiffChoice::Branch));

    app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE))
        .expect("configured down key should move menu selection");
    assert_eq!(app.overlays.diff_menu.input, "");
    assert_eq!(app.highlighted_diff_choice(), Some(DiffChoice::Show));

    app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE))
        .expect("configured up key should move menu selection");
    assert_eq!(app.overlays.diff_menu.input, "");
    assert_eq!(app.highlighted_diff_choice(), Some(DiffChoice::Branch));

    app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE))
        .expect("configured close key should close menu");
    assert!(!app.overlays.diff_menu_is_open());
    assert_eq!(app.overlays.diff_menu.input, "");

    app.open_diff_menu();
    app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE))
        .expect("configured confirm key should select menu item");

    assert!(!app.overlays.diff_menu_is_open());
    let load = app
        .jobs
        .pending_diff_load
        .as_ref()
        .expect("menu selection should queue diff load");
    assert_eq!(load.options.source, DiffSource::Base("main".into()));
    assert_eq!(load.options.worktree_scope(), None);
}

#[test]
fn branch_menu_uses_configured_menu_keymap() {
    let options = DiffOptions {
        source: DiffSource::Base("main".into()),
        ..DiffOptions::default()
    };
    let mut app = DiffApp::new(
        options,
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.refs.branch_base = Some("main".to_owned());
    app.refs.branch_head = Some("feature".to_owned());
    app.refs.current_head = Some("feature".to_owned());
    app.refs.comparison_branches = branch_names(&["main", "feature", "topic"]);
    app.config.keymap = Keymap::parse(
        r#"
        [keymap.menu]
        down = "j"
        up = "k"
        confirm = "space"
        close = "q"
        "#,
    )
    .expect("keymap should parse");

    app.toggle_branch_menu(BranchMenu::Head);
    assert_eq!(app.refs.branch_menu.selected, 0);

    app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE))
        .expect("configured down key should move branch selection");
    assert_eq!(app.refs.branch_menu.input, "");
    assert_eq!(app.refs.branch_menu.selected, 1);

    app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE))
        .expect("configured up key should move branch selection");
    assert_eq!(app.refs.branch_menu.input, "");
    assert_eq!(app.refs.branch_menu.selected, 0);

    app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE))
        .expect("configured close key should close branch menu");
    assert!(app.refs.branch_menu_open().is_none());
    assert_eq!(app.refs.branch_menu.input, "");

    app.toggle_branch_menu(BranchMenu::Head);
    app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE))
        .expect("configured down key should move branch selection");
    app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE))
        .expect("configured confirm key should select branch");

    assert!(app.refs.branch_menu_open().is_none());
    let load = app
        .jobs
        .pending_diff_load
        .as_ref()
        .expect("branch selection should queue diff load");
    assert_eq!(
        load.options.source,
        DiffSource::Branch {
            base: "main".into(),
            head: "topic".into()
        }
    );
    assert_eq!(load.options.worktree_scope(), None);
}

#[test]
fn diff_menu_ctrl_n_and_ctrl_p_move_selection() {
    let mut app = DiffApp::new(
        DiffOptions::default(),
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.refs.branch_base = Some("main".to_owned());

    app.open_diff_menu();
    assert_eq!(app.highlighted_diff_choice(), Some(DiffChoice::Branch));

    app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL))
        .expect("ctrl-n should move menu selection");
    assert_eq!(app.highlighted_diff_choice(), Some(DiffChoice::Show));

    app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL))
        .expect("ctrl-p should move menu selection");
    assert_eq!(app.highlighted_diff_choice(), Some(DiffChoice::Branch));
}

#[test]
fn diff_menu_plain_letters_filter_input() {
    let mut app = DiffApp::new(
        DiffOptions::default(),
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );

    app.open_diff_menu();
    app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE))
        .expect("plain j should filter menu input");

    assert_eq!(app.overlays.diff_menu.input, "j");
    assert!(app.overlays.diff_menu_is_open());
    assert_eq!(app.highlighted_diff_choice(), None);
}

#[test]
fn diff_menu_space_filters_without_entering_leader() {
    let mut app = DiffApp::new(
        DiffOptions::default(),
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );

    app.open_diff_menu();
    app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE))
        .expect("space should filter menu input");

    assert!(app.overlays.diff_menu_is_open());
    assert!(app.input.key_prefix_pending.is_none());
    assert_eq!(app.overlays.diff_menu.input, " ");
    assert!(app.jobs.pending_diff_load.is_none());
}

#[test]
fn diff_menu_q_filters_without_quitting() {
    let mut app = DiffApp::new(
        DiffOptions::default(),
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.open_diff_menu();

    let should_quit = app
        .handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE))
        .expect("q should filter menu input");

    assert!(!should_quit);
    assert!(app.overlays.diff_menu_is_open());
    assert_eq!(app.overlays.diff_menu.input, "q");
}

#[test]
fn diff_menu_branch_keys_do_not_open_branch_picker() {
    let mut app = DiffApp::new(
        DiffOptions::default(),
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.refs.branch_base = Some("main".to_owned());
    app.refs.branch_head = Some("feature".to_owned());
    app.refs.current_head = Some("feature".to_owned());
    app.refs.comparison_branches = branch_names(&["main", "feature"]);

    app.open_diff_menu();
    assert_eq!(app.highlighted_diff_choice(), Some(DiffChoice::Branch));
    app.handle_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE))
        .expect("b should filter diff menu");

    assert!(app.overlays.diff_menu_is_open());
    assert!(app.refs.branch_menu_open().is_none());
    assert_eq!(app.overlays.diff_menu.input, "b");

    app.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE))
        .expect("h should filter diff menu");

    assert!(app.overlays.diff_menu_is_open());
    assert!(app.refs.branch_menu_open().is_none());
    assert_eq!(app.overlays.diff_menu.input, "bh");
}

#[test]
fn diff_menu_number_keys_filter_input() {
    let mut app = DiffApp::new(
        DiffOptions::default(),
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.refs.branch_base = Some("main".to_owned());
    app.refs.branch_head = Some("feature".to_owned());
    app.refs.current_head = Some("feature".to_owned());

    app.open_diff_menu();
    app.handle_key(KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE))
        .expect("2 should filter diff choices");

    assert!(app.overlays.diff_menu_is_open());
    assert_eq!(app.overlays.diff_menu.input, "2");
    assert!(app.jobs.pending_diff_load.is_none());
}

#[test]
fn diff_menu_draws_centered_floating_menu() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.open_diff_menu();
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 20))
        .expect("test terminal should be created");

    terminal
        .draw(|frame| crate::render::draw(frame, &mut app))
        .expect("diff menu draw should succeed");

    let buffer = terminal.backend().buffer();
    let rows: Vec<String> = (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer.cell((x, y)).expect("cell should exist").symbol())
                .collect()
        })
        .collect();
    let title = rows
        .iter()
        .enumerate()
        .find_map(|(row, text)| {
            text.find(" Diff ")
                .map(|column| (row, text[..column].width()))
        })
        .expect("floating diff menu should render title");

    assert!(title.0 > 4 && title.0 < 12, "title row was {}", title.0);
    assert!(title.1 > 30 && title.1 < 48, "title column was {}", title.1);
    assert!(
        rows.iter()
            .any(|row| row.contains("│  All changes") && !row.contains("1 │"))
    );
    assert!(
        rows.iter()
            .any(|row| row.contains("│  Unstaged") && !row.contains("1 │"))
    );
    assert!(
        rows.iter()
            .any(|row| row.contains("│  Staged") && !row.contains("2 │"))
    );
}

#[test]
fn diff_menu_mouse_selects_visible_centered_choice() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.open_diff_menu();
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 20))
        .expect("test terminal should be created");

    terminal
        .draw(|frame| crate::render::draw(frame, &mut app))
        .expect("diff menu draw should succeed");

    let buffer = terminal.backend().buffer();
    let (row, column) = (0..buffer.area.height)
        .find_map(|y| {
            let text: String = (0..buffer.area.width)
                .map(|x| buffer.cell((x, y)).expect("cell should exist").symbol())
                .collect();
            text.find("Unstaged").map(|x| (y, x as u16))
        })
        .expect("unstaged choice should be visible");

    app.handle_click(column, row);

    assert!(!app.overlays.diff_menu_is_open());
    let load = app
        .jobs
        .pending_diff_load
        .as_ref()
        .expect("visible click should queue diff load");
    assert_eq!(
        load.options.source,
        DiffSource::Worktree {
            scope: DiffScope::Unstaged
        }
    );
    assert_eq!(load.options.worktree_scope(), Some(DiffScope::Unstaged));
}

#[test]
fn diff_menu_mouse_ignores_old_top_left_choice_coordinates() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.open_diff_menu();
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 20))
        .expect("test terminal should be created");

    terminal
        .draw(|frame| crate::render::draw(frame, &mut app))
        .expect("diff menu draw should succeed");

    app.handle_click(1, 1);

    assert!(!app.overlays.diff_menu_is_open());
    assert!(app.jobs.pending_diff_load.is_none());
}

#[test]
fn options_menu_toggles_setting_on_enter() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);

    app.open_options_menu();
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .expect("enter should toggle layout");

    assert!(app.overlays.options_menu_is_open());
    assert_eq!(app.viewport.layout, DiffLayoutMode::Split);
}

#[test]
fn options_menu_draft_persists_only_changed_setting() {
    let dir = temp_test_dir("settings-menu-persist-changed-only");
    let path = dir.join("config.toml");
    fs::create_dir_all(&dir).expect("test dir should be created");
    fs::write(
        &path,
        r#"
mode = "enabled"
layout = "split"
live_reload = true
syntax_highlighting = true
line_wrapping = false
colorscheme = "system"

[notifications]
mode = "default"
corner = "top-right"
timeout_ms = 1500
max_visible = 3

[diff]
line_background = "none"
context_expand = 7
"#,
    )
    .expect("settings file should be written");

    persist_options_menu_draft_to_path(
        &path,
        OptionsDraft {
            layout: LayoutSetting::Split,
            live_updates_enabled: false,
            context_expansion: DiffContextExpansion::Full,
            syntax_enabled: false,
            line_wrapping: true,
            color_scheme: ColorSchemeChoice::Tokyonight,
            ..default_options_draft()
        },
        OptionsMenuItem::LiveReload,
    )
    .expect("settings draft should persist");

    let saved = fs::read_to_string(&path).expect("settings file should be readable");
    let saved: toml::Value = toml::from_str(&saved).expect("settings should stay valid toml");
    let diff = saved["diff"].as_table().expect("diff should stay a table");
    let notifications = saved["notifications"]
        .as_table()
        .expect("notifications should stay a table");

    assert_eq!(saved["mode"].as_str(), Some("enabled"));
    assert_eq!(saved["layout"].as_str(), Some("split"));
    assert_eq!(saved["live_reload"].as_bool(), Some(false));
    assert_eq!(saved["syntax_highlighting"].as_bool(), Some(true));
    assert_eq!(saved["line_wrapping"].as_bool(), Some(false));
    assert_eq!(saved["colorscheme"].as_str(), Some("system"));
    assert_eq!(
        diff.get("line_background").and_then(toml::Value::as_str),
        Some("none")
    );
    assert_eq!(
        diff.get("context_expand").and_then(toml::Value::as_integer),
        Some(7)
    );
    assert_eq!(
        notifications.get("mode").and_then(toml::Value::as_str),
        Some("default")
    );
    assert_eq!(
        notifications.get("corner").and_then(toml::Value::as_str),
        Some("top-right")
    );
    assert_eq!(
        notifications
            .get("timeout_ms")
            .and_then(toml::Value::as_integer),
        Some(1_500)
    );
    assert_eq!(
        notifications
            .get("max_visible")
            .and_then(toml::Value::as_integer),
        Some(3)
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn options_menu_context_persistence_removes_context_aliases() {
    let dir = temp_test_dir("settings-menu-persist-context-aliases");
    let path = dir.join("config.toml");
    fs::create_dir_all(&dir).expect("test dir should be created");
    fs::write(
        &path,
        r#"
[diff]
line_background = "none"
context_expansion = 5
context_lines = 7
expand_context = 9
"#,
    )
    .expect("settings file should be written");

    persist_options_menu_draft_to_path(
        &path,
        OptionsDraft {
            layout: LayoutSetting::Split,
            live_updates_enabled: false,
            context_expansion: DiffContextExpansion::Full,
            syntax_enabled: false,
            line_wrapping: true,
            color_scheme: ColorSchemeChoice::Tokyonight,
            ..default_options_draft()
        },
        OptionsMenuItem::ContextExpansion,
    )
    .expect("settings draft should persist");

    let saved = fs::read_to_string(&path).expect("settings file should be readable");
    let saved: toml::Value = toml::from_str(&saved).expect("settings should stay valid toml");
    let diff = saved["diff"].as_table().expect("diff should stay a table");

    assert_eq!(
        diff.get("line_background").and_then(toml::Value::as_str),
        Some("none")
    );
    assert_eq!(
        diff.get("context_expand").and_then(toml::Value::as_str),
        Some("full")
    );
    assert!(diff.get("context_expansion").is_none());
    assert!(diff.get("context_lines").is_none());
    assert!(diff.get("expand_context").is_none());

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn options_menu_line_wrapping_persistence_removes_line_wrapping_aliases() {
    let dir = temp_test_dir("settings-menu-persist-line-wrapping-aliases");
    let path = dir.join("config.toml");
    fs::create_dir_all(&dir).expect("test dir should be created");
    fs::write(
        &path,
        r#"
word_wrap = false
wrap_lines = false
"#,
    )
    .expect("settings file should be written");

    persist_options_menu_draft_to_path(
        &path,
        OptionsDraft {
            layout: LayoutSetting::Split,
            live_updates_enabled: false,
            context_expansion: DiffContextExpansion::Full,
            syntax_enabled: false,
            line_wrapping: true,
            color_scheme: ColorSchemeChoice::Tokyonight,
            ..default_options_draft()
        },
        OptionsMenuItem::LineWrapping,
    )
    .expect("settings draft should persist");

    let saved = fs::read_to_string(&path).expect("settings file should be readable");
    let saved: toml::Value = toml::from_str(&saved).expect("settings should stay valid toml");

    assert_eq!(saved["line_wrapping"].as_bool(), Some(true));
    assert!(saved.get("word_wrap").is_none());
    assert!(saved.get("wrap_lines").is_none());

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn options_menu_notification_persistence_updates_nested_setting() {
    let dir = temp_test_dir("settings-menu-persist-notifications");
    let path = dir.join("config.toml");
    fs::create_dir_all(&dir).expect("test dir should be created");
    fs::write(
        &path,
        r#"
[notifications]
mode = "default"
corner = "top-right"
timeout_ms = 1500
max_visible = 3
"#,
    )
    .expect("settings file should be written");

    persist_options_menu_draft_to_path(
        &path,
        OptionsDraft {
            notification_mode: NotificationMode::Debug,
            toast_corner: ToastCorner::BottomLeft,
            toast_timeout_ms: 5_000,
            toast_max_visible: 5,
            ..default_options_draft()
        },
        OptionsMenuItem::ToastCorner,
    )
    .expect("settings draft should persist");

    let saved = fs::read_to_string(&path).expect("settings file should be readable");
    let saved: toml::Value = toml::from_str(&saved).expect("settings should stay valid toml");
    let notifications = saved["notifications"]
        .as_table()
        .expect("notifications should be a table");

    assert_eq!(
        notifications.get("mode").and_then(toml::Value::as_str),
        Some("default")
    );
    assert_eq!(
        notifications.get("corner").and_then(toml::Value::as_str),
        Some("bottom-left")
    );
    assert_eq!(
        notifications
            .get("timeout_ms")
            .and_then(toml::Value::as_integer),
        Some(1_500)
    );
    assert_eq!(
        notifications
            .get("max_visible")
            .and_then(toml::Value::as_integer),
        Some(3)
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn options_menu_plain_letters_filter_input() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);

    app.open_options_menu();
    app.handle_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE))
        .expect("x should filter settings");

    assert!(app.overlays.options_menu_is_open());
    assert_eq!(app.overlays.options_menu.input, "x");
    assert_eq!(app.viewport.layout, DiffLayoutMode::Unified);
    assert_eq!(app.highlighted_option(), Some(OptionsMenuItem::LiveReload));
}

#[test]
fn options_menu_toggles_syntax_highlighting() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new_with_syntax(
        DiffOptions::default(),
        changeset,
        DiffLayoutMode::Unified,
        SyntaxStartupMode::Disabled,
    );
    app.config.syntax = Some(syntax_runtime_with_queue(SyntaxWorkerQueue::new(
        1,
        app.document.generation,
    )));

    app.open_options_menu();
    app.move_options_menu_selection(2);
    assert_eq!(
        app.highlighted_option(),
        Some(OptionsMenuItem::SyntaxHighlighting)
    );
    assert_eq!(app.option_value(OptionsMenuItem::SyntaxHighlighting), "[x]");

    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .expect("enter should toggle syntax highlighting");

    assert!(app.config.syntax.is_none());
    assert_eq!(app.option_value(OptionsMenuItem::SyntaxHighlighting), "[ ]");
}

#[test]
fn options_menu_persists_post_apply_syntax_state_when_enable_fails() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new_with_syntax(
        DiffOptions::default(),
        changeset,
        DiffLayoutMode::Unified,
        SyntaxStartupMode::Languages(Vec::new()),
    );

    app.open_options_menu();
    app.move_options_menu_selection(2);
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .expect("enter should try to enable syntax highlighting");

    assert!(app.config.syntax.is_none());
    assert!(!app.overlays.options_menu_draft.syntax_enabled);
    assert_eq!(
        app.config.last_persisted_options_menu_draft,
        Some((
            OptionsDraft {
                syntax_enabled: false,
                ..app.overlays.options_menu_draft
            },
            OptionsMenuItem::SyntaxHighlighting,
        ))
    );
}

#[test]
fn options_menu_toggles_line_wrapping_and_clamps_horizontal_scroll() {
    let changeset = changeset_with_line_text(&"a".repeat(120));
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_width(40);
    app.set_horizontal_scroll(HORIZONTAL_SCROLL_STEP);
    assert_eq!(app.viewport.horizontal_scroll, HORIZONTAL_SCROLL_STEP);

    app.open_options_menu();
    app.move_options_menu_selection(3);
    assert_eq!(
        app.highlighted_option(),
        Some(OptionsMenuItem::LineWrapping)
    );
    assert_eq!(app.option_value(OptionsMenuItem::LineWrapping), "[ ]");

    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .expect("enter should toggle line wrapping");

    assert!(app.viewport.line_wrapping);
    assert_eq!(app.viewport.horizontal_scroll, 0);
    assert_eq!(app.max_horizontal_scroll(), 0);
    assert_eq!(app.option_value(OptionsMenuItem::LineWrapping), "[x]");
}

#[test]
fn options_menu_cycles_notification_settings() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);

    app.open_options_menu();
    app.set_options_menu_selection(5);
    assert_eq!(
        app.highlighted_option(),
        Some(OptionsMenuItem::NotificationMode)
    );
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .expect("enter should toggle notification mode");
    assert_eq!(
        app.config.syntax_settings.notifications.mode(),
        NotificationMode::Debug
    );
    assert!(app.notifications.toasts.debug_enabled());

    app.set_options_menu_selection(6);
    assert_eq!(app.highlighted_option(), Some(OptionsMenuItem::ToastCorner));
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .expect("enter should cycle toast corner");
    assert_eq!(
        app.config.syntax_settings.notifications.corner(),
        ToastCorner::BottomRight
    );
    assert_eq!(app.notifications.toasts.corner(), ToastCorner::BottomRight);

    app.set_options_menu_selection(7);
    assert_eq!(
        app.highlighted_option(),
        Some(OptionsMenuItem::ToastTimeout)
    );
    app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE))
        .expect("right should cycle toast timeout");
    assert_eq!(app.config.syntax_settings.notifications.timeout_ms(), 2_500);

    app.set_options_menu_selection(8);
    assert_eq!(
        app.highlighted_option(),
        Some(OptionsMenuItem::ToastMaxVisible)
    );
    app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE))
        .expect("right should cycle toast max visible");
    assert_eq!(app.config.syntax_settings.notifications.max_visible(), 4);
}

#[test]
fn options_menu_cycles_custom_notification_values_to_nearest_choices() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.config.syntax_settings.notifications = NotificationSettings::new(
        app.config.syntax_settings.notifications.mode(),
        app.config.syntax_settings.notifications.corner(),
        2_000,
        10,
    );

    app.open_options_menu();
    app.set_options_menu_selection(7);
    assert_eq!(
        app.highlighted_option(),
        Some(OptionsMenuItem::ToastTimeout)
    );
    app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE))
        .expect("right should cycle custom toast timeout to next choice");
    assert_eq!(app.config.syntax_settings.notifications.timeout_ms(), 2_500);
    assert_eq!(
        app.config
            .last_persisted_options_menu_draft
            .expect("timeout change should be persisted")
            .0
            .toast_timeout_ms,
        2_500
    );

    app.set_options_menu_selection(8);
    assert_eq!(
        app.highlighted_option(),
        Some(OptionsMenuItem::ToastMaxVisible)
    );
    app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE))
        .expect("right should cycle custom toast max visible to nearest choice");
    assert_eq!(app.config.syntax_settings.notifications.max_visible(), 5);
    assert_eq!(
        app.config
            .last_persisted_options_menu_draft
            .expect("max visible change should be persisted")
            .0
            .toast_max_visible,
        5
    );
}

#[test]
fn options_menu_clamps_selection_after_toggle_leaves_filter() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new_with_syntax(
        DiffOptions::default(),
        changeset,
        DiffLayoutMode::Unified,
        SyntaxStartupMode::Disabled,
    );
    app.config.syntax = Some(syntax_runtime_with_queue(SyntaxWorkerQueue::new(
        1,
        app.document.generation,
    )));

    app.open_options_menu();
    for character in ['[', 'x', ']'] {
        app.handle_key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE))
            .expect("typing should filter checked settings");
    }
    assert_eq!(
        app.filtered_options_menu_items(),
        vec![
            OptionsMenuItem::LiveReload,
            OptionsMenuItem::SyntaxHighlighting,
        ]
    );
    app.set_options_menu_selection(1);
    assert_eq!(
        app.highlighted_option(),
        Some(OptionsMenuItem::SyntaxHighlighting)
    );

    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .expect("enter should toggle syntax highlighting");

    assert!(app.config.syntax.is_none());
    assert_eq!(
        app.filtered_options_menu_items(),
        vec![OptionsMenuItem::LiveReload]
    );
    assert_eq!(app.overlays.options_menu.selected, 0);
    assert_eq!(app.highlighted_option(), Some(OptionsMenuItem::LiveReload));

    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .expect("enter should activate the rendered highlighted setting");
    assert!(!app.jobs.live_updates.enabled());
}

#[test]
fn options_menu_colorscheme_input_selects_draft_and_applies_on_enter() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.config.color_scheme = ColorSchemeChoice::System;
    app.config.theme = DiffTheme::system();

    app.open_options_menu();
    app.move_options_menu_selection(4);
    assert_eq!(app.highlighted_option(), Some(OptionsMenuItem::ColorScheme));

    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .expect("enter should open colorscheme input");
    assert!(app.overlays.color_scheme_picker_is_open());
    for character in ['t', 'o', 'k', 'y', 'o'] {
        app.handle_key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE))
            .expect("typing should filter colorschemes");
    }
    assert_eq!(app.config.color_scheme, ColorSchemeChoice::Tokyonight);
    assert_eq!(
        app.config.theme.background,
        DiffTheme::tokyonight().background
    );
    assert_eq!(
        app.overlays.options_menu_draft.color_scheme,
        ColorSchemeChoice::System
    );
    assert_eq!(
        app.filtered_color_schemes(),
        vec![ColorSchemeChoice::Tokyonight]
    );

    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .expect("enter should select colorscheme draft");
    assert!(!app.overlays.color_scheme_picker_is_open());
    assert!(app.overlays.options_menu_is_open());
    assert_eq!(
        app.overlays.options_menu_draft.color_scheme,
        ColorSchemeChoice::Tokyonight
    );
    assert_eq!(app.config.color_scheme, ColorSchemeChoice::Tokyonight);
    assert_eq!(
        app.config.theme.background,
        DiffTheme::tokyonight().background
    );
    assert!(app.jobs.pending_diff_load.is_none());
}

#[test]
fn colorscheme_picker_mouse_selection_persists_draft() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.config.color_scheme = ColorSchemeChoice::System;
    app.config.theme = DiffTheme::system();

    app.open_options_menu();
    app.move_options_menu_selection(4);
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .expect("enter should open colorscheme picker");
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 20))
        .expect("test terminal should be created");

    terminal
        .draw(|frame| crate::render::draw(frame, &mut app))
        .expect("colorscheme picker draw should succeed");

    let rows = buffer_rows(terminal.backend().buffer());
    let (row, column) = rows
        .iter()
        .enumerate()
        .find_map(|(row, text)| {
            text.find("gruvbox-dark")
                .map(|column| (row as u16, column as u16))
        })
        .expect("target colorscheme row should render");
    app.config.last_persisted_options_menu_draft = None;

    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    })
    .expect("mouse click should select colorscheme");

    assert!(!app.overlays.color_scheme_picker_is_open());
    assert_eq!(app.config.color_scheme, ColorSchemeChoice::GruvboxDark);
    assert_eq!(
        app.overlays.options_menu_draft.color_scheme,
        ColorSchemeChoice::GruvboxDark
    );
    let (draft, changed_item) = app
        .config
        .last_persisted_options_menu_draft
        .expect("mouse-selected colorscheme should be persisted");
    assert_eq!(changed_item, OptionsMenuItem::ColorScheme);
    assert_eq!(draft.color_scheme, ColorSchemeChoice::GruvboxDark);
}

#[test]
fn colorscheme_picker_mouse_dismiss_keeps_options_menu_open() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);

    app.open_options_menu();
    app.move_options_menu_selection(4);
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .expect("enter should open colorscheme picker");
    assert!(app.overlays.color_scheme_picker_is_open());

    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 0,
        row: 0,
        modifiers: KeyModifiers::NONE,
    })
    .expect("mouse click should dismiss colorscheme picker");

    assert!(!app.overlays.color_scheme_picker_is_open());
    assert!(app.overlays.options_menu_is_open());
}

#[test]
fn options_menu_omits_branch_options_for_branch_diff() {
    let options = DiffOptions {
        source: DiffSource::Branch {
            base: "main".into(),
            head: "feature".into(),
        },
        ..DiffOptions::default()
    };
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(options, changeset, DiffLayoutMode::Unified);
    app.refs.branch_base = Some("main".to_owned());
    app.refs.branch_head = Some("feature".to_owned());
    app.refs.current_head = Some("feature".to_owned());
    app.refs.comparison_branches = branch_names(&["main", "feature"]);

    app.open_options_menu();

    assert_eq!(app.highlighted_option(), Some(OptionsMenuItem::Layout));
    assert_eq!(
        app.options_menu_items(),
        [
            OptionsMenuItem::Layout,
            OptionsMenuItem::LiveReload,
            OptionsMenuItem::SyntaxHighlighting,
            OptionsMenuItem::LineWrapping,
            OptionsMenuItem::ColorScheme,
            OptionsMenuItem::NotificationMode,
            OptionsMenuItem::ToastCorner,
            OptionsMenuItem::ToastTimeout,
            OptionsMenuItem::ToastMaxVisible,
        ]
    );
}

#[test]
fn options_menu_does_not_open_branch_picker_for_branch_diff() {
    let options = DiffOptions {
        source: DiffSource::Branch {
            base: "main".into(),
            head: "feature".into(),
        },
        ..DiffOptions::default()
    };
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(options, changeset, DiffLayoutMode::Unified);
    app.refs.branch_base = Some("main".to_owned());
    app.refs.branch_head = Some("feature".to_owned());
    app.refs.current_head = Some("feature".to_owned());
    app.refs.comparison_branches = branch_names(&["main", "feature"]);

    app.open_options_menu();
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .expect("enter should toggle first setting");
    assert!(app.overlays.options_menu_is_open());
    assert!(app.refs.branch_menu_open().is_none());
}

#[test]
fn options_menu_live_reload_toggles_without_reloading_diff() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    assert!(app.jobs.live_updates.enabled());

    app.open_options_menu();
    app.move_options_menu_selection(1);
    assert_eq!(app.highlighted_option(), Some(OptionsMenuItem::LiveReload));
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .expect("enter should toggle live reload");

    assert!(!app.jobs.live_updates.enabled());
    assert!(app.jobs.pending_diff_load.is_none());
}

#[test]
fn options_menu_reenabling_live_reload_reloads_diff() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.jobs.live_updates = LiveUpdatesState::DisabledByUser;

    app.open_options_menu();
    app.move_options_menu_selection(1);
    assert_eq!(app.highlighted_option(), Some(OptionsMenuItem::LiveReload));
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .expect("enter should toggle live reload");

    assert!(app.jobs.live_updates.enabled());
    let load = app
        .jobs
        .pending_diff_load
        .as_ref()
        .expect("reenabling live reload should queue a fresh load");
    assert_eq!(load.options, DiffOptions::default());
}

#[test]
fn options_menu_does_not_enable_live_reload_when_watch_is_disabled() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.jobs.live_updates = LiveUpdatesState::DisabledByCli;

    app.open_options_menu();
    app.move_options_menu_selection(1);
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .expect("enter should be handled");

    assert!(!app.overlays.options_menu_draft.live_updates_enabled);
    assert_eq!(
        app.notifications.error_log.as_deref(),
        Some("live reload disabled by --no-watch")
    );
}

#[test]
fn options_menu_draws_centered_floating_menu() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.open_options_menu();
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 20))
        .expect("test terminal should be created");

    terminal
        .draw(|frame| crate::render::draw(frame, &mut app))
        .expect("options menu draw should succeed");

    let buffer = terminal.backend().buffer();
    let rows: Vec<String> = (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer.cell((x, y)).expect("cell should exist").symbol())
                .collect()
        })
        .collect();
    let title = rows
        .iter()
        .enumerate()
        .find_map(|(row, text)| {
            text.find("Settings")
                .map(|column| (row, text[..column].width()))
        })
        .expect("floating options menu should render title");

    assert!(title.0 >= 3 && title.0 < 12, "title row was {}", title.0);
    assert!(title.1 > 30 && title.1 < 48, "title column was {}", title.1);
    assert!(
        rows.iter()
            .any(|row| row.contains(&format!("> {INPUT_CURSOR}")))
    );
    assert!(rows.iter().any(|row| row.contains("Layout")));
    assert!(rows.iter().any(|row| row.contains("Live reload")));
    assert!(rows.iter().any(|row| row.contains("Syntax highlighting")));
    assert!(rows.iter().any(|row| row.contains("Colorscheme")));

    let layout_row = rows
        .iter()
        .find(|row| row.contains("Layout") && row.contains("[dynamic]"))
        .expect("layout row should show current value");
    let label_column = layout_row.find("Layout").expect("label should render");
    let value_column = layout_row.rfind("[dynamic]").expect("value should render");
    assert!(
        value_column > label_column + 20,
        "setting value should be right aligned: {layout_row}"
    );
}

#[test]
fn options_menu_scrolls_selected_setting_into_short_terminal() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.open_options_menu();
    app.set_options_menu_selection(4);
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 5))
        .expect("test terminal should be created");

    terminal
        .draw(|frame| crate::render::draw(frame, &mut app))
        .expect("options menu draw should succeed");

    let rows = buffer_rows(terminal.backend().buffer());
    assert!(app.overlays.options_menu.scroll > 0);
    assert!(rows.iter().any(|row| row.contains("Colorscheme")));
    assert!(
        !rows
            .iter()
            .any(|row| row.contains("Layout") && row.contains("[unified]"))
    );
}

#[test]
fn scrollable_menu_draws_thin_scrollbar() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.open_options_menu();
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 5))
        .expect("test terminal should be created");

    terminal
        .draw(|frame| crate::render::draw(frame, &mut app))
        .expect("options menu draw should succeed");

    let rows = buffer_rows(terminal.backend().buffer());
    assert!(
        rows.iter().any(|row| row.contains("┃")),
        "scrollable menu should show a thin scrollbar:\n{}",
        rows.join("\n")
    );
}

#[test]
fn scrollable_menu_scrollbar_reaches_bottom_at_last_page() {
    let options = DiffOptions {
        source: DiffSource::Base("branch-00".into()),
        ..DiffOptions::default()
    };
    let mut app = DiffApp::new(
        options,
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.refs.branch_base = Some("branch-00".to_owned());
    app.refs.branch_head = Some("branch-01".to_owned());
    app.refs.current_head = Some("branch-01".to_owned());
    app.refs.comparison_branches = (0..59)
        .map(|index| format!("branch-{index:02}").into())
        .collect();
    app.toggle_branch_menu(BranchMenu::Base);
    app.set_branch_selection(usize::MAX);

    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 60))
        .expect("test terminal should be created");
    terminal
        .draw(|frame| crate::render::draw(frame, &mut app))
        .expect("branch menu draw should succeed");

    let menu_area = app
        .overlays
        .rendered_branch_menu_area
        .expect("branch menu should render");
    let inner = branch_menu_block(app.config.theme, BranchMenu::Base).inner(menu_area);
    let scrollbar_column = inner.x.saturating_add(inner.width).saturating_sub(1);
    let scrollbar_bottom = inner.y.saturating_add(inner.height).saturating_sub(1);
    let symbol = terminal
        .backend()
        .buffer()
        .cell((scrollbar_column, scrollbar_bottom))
        .expect("scrollbar cell should exist")
        .symbol();

    assert_eq!(symbol, "┃");
}

#[test]
fn selector_menus_do_not_render_footers() {
    let keymap = Keymap::parse(
        r#"
        [keymap.menu]
        up = "u"
        down = "d"
        select = "x"
        confirm = "a"
        close = "z"
        "#,
    )
    .expect("keymap should parse");
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.config.keymap = keymap;

    app.open_options_menu();
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 20))
        .expect("test terminal should be created");
    terminal
        .draw(|frame| crate::render::draw(frame, &mut app))
        .expect("options menu draw should succeed");
    let rows = buffer_rows(terminal.backend().buffer());
    assert!(!rows.iter().any(|row| row.contains("toggle/open")));
    assert!(!rows.iter().any(|row| row.contains("apply/open")));

    app.close_options_menu();
    app.open_diff_menu();
    terminal
        .draw(|frame| crate::render::draw(frame, &mut app))
        .expect("diff menu draw should succeed");
    let rows = buffer_rows(terminal.backend().buffer());
    assert!(
        rows.iter()
            .any(|row| row.contains(&format!("> {INPUT_CURSOR}")))
    );
    assert!(
        rows.iter()
            .any(|row| row.contains("1") && row.contains("All changes"))
    );
    assert!(!rows.iter().any(|row| row.contains("d/u move")));
    assert!(!rows.iter().any(|row| row.contains("Enter apply")));
}

#[test]
fn colorscheme_picker_draws_input_dropdown() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.open_options_menu();
    app.move_options_menu_selection(4);
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .expect("enter should open colorscheme picker");
    app.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE))
        .expect("typing should filter colorschemes");
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 20))
        .expect("test terminal should be created");

    terminal
        .draw(|frame| crate::render::draw(frame, &mut app))
        .expect("colorscheme picker draw should succeed");

    let buffer = terminal.backend().buffer();
    let rows: Vec<String> = (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer.cell((x, y)).expect("cell should exist").symbol())
                .collect()
        })
        .collect();

    assert!(rows.iter().any(|row| row.contains("Colorscheme")));
    assert!(
        rows.iter()
            .any(|row| row.contains(&format!("> g{INPUT_CURSOR}")))
    );
    assert!(rows.iter().any(|row| row.contains("system")));
    assert!(rows.iter().any(|row| row.contains("gruvbox-dark")));
    assert!(!rows.iter().any(|row| row.contains("current")));

    let (row, column) = rows
        .iter()
        .enumerate()
        .find_map(|(row, text)| {
            text.find("system")
                .map(|column| (row as u16, column as u16))
        })
        .expect("current colorscheme should render");
    assert_eq!(
        buffer.cell((column, row)).expect("cell should exist").fg,
        app.config.theme.muted
    );
}

#[test]
fn colorscheme_picker_navigation_keeps_expanded_rows_stable_in_tall_terminal() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_terminal_area(Rect {
        x: 0,
        y: 0,
        width: 120,
        height: 60,
    });
    app.overlays.options_menu_draft.color_scheme = ColorSchemeChoice::System;
    app.open_color_scheme_picker();

    app.move_color_scheme_selection(9);

    assert_eq!(app.overlays.color_scheme_picker.selected, 9);
    assert_eq!(app.overlays.color_scheme_picker.scroll, 0);
}

#[test]
fn colorscheme_picker_previews_hovered_theme_and_reverts_on_close() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.open_options_menu();
    app.move_options_menu_selection(4);
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .expect("enter should open colorscheme picker");
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 20))
        .expect("test terminal should be created");

    terminal
        .draw(|frame| crate::render::draw(frame, &mut app))
        .expect("colorscheme picker draw should succeed");

    let buffer = terminal.backend().buffer();
    let (row, column) = (0..buffer.area.height)
        .find_map(|y| {
            let text: String = (0..buffer.area.width)
                .map(|x| buffer.cell((x, y)).expect("cell should exist").symbol())
                .collect();
            text.find("gruvbox-dark").map(|column| (y, column as u16))
        })
        .expect("gruvbox row should render");

    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Moved,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    })
    .expect("hover should preview colorscheme");

    assert_eq!(app.config.color_scheme, ColorSchemeChoice::GruvboxDark);
    assert_eq!(
        app.config.theme.background,
        DiffTheme::gruvbox_dark().background
    );

    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 0,
        row: 0,
        modifiers: KeyModifiers::NONE,
    })
    .expect("outside click should close colorscheme picker");

    assert!(!app.overlays.color_scheme_picker_is_open());
    assert_eq!(app.config.color_scheme, ColorSchemeChoice::System);
    assert_eq!(app.config.theme, DiffTheme::system());
}

#[test]
fn colorscheme_picker_previews_first_hovered_theme() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.config.color_scheme = ColorSchemeChoice::System;
    app.config.theme = DiffTheme::system();
    app.open_options_menu();
    app.move_options_menu_selection(4);
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .expect("enter should open colorscheme picker");
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 20))
        .expect("test terminal should be created");

    terminal
        .draw(|frame| crate::render::draw(frame, &mut app))
        .expect("colorscheme picker draw should succeed");

    let rows = buffer_rows(terminal.backend().buffer());
    let (row, column) = rows
        .iter()
        .enumerate()
        .find_map(|(row, text)| {
            text.find("catppuccin-latte")
                .map(|column| (row as u16, column as u16))
        })
        .expect("first colorscheme row should render");
    assert_eq!(app.overlays.color_scheme_picker.selected, 0);

    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Moved,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    })
    .expect("hover should preview first colorscheme");

    assert_eq!(app.overlays.color_scheme_picker.selected, 0);
    assert_eq!(app.config.color_scheme, ColorSchemeChoice::CatppuccinLatte);
    assert_eq!(
        app.config.theme.background,
        DiffTheme::catppuccin_latte().background
    );
}

#[test]
fn tab_from_review_diff_cycles_to_all_changes() {
    let options = DiffOptions {
        source: DiffSource::Patch(PatchSource::Review {
            label: "review owner/repo#123".into(),
            patch: Arc::from(&b""[..]),
        }),
        ..DiffOptions::default()
    };
    let mut app = DiffApp::new(
        options,
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );

    let should_quit = app
        .handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
        .expect("tab should cycle from review to all changes");

    assert!(!should_quit);
    let load = app
        .jobs
        .pending_diff_load
        .as_ref()
        .expect("tab should queue all-changes diff load");
    assert_eq!(
        load.options.source,
        DiffSource::Worktree {
            scope: DiffScope::All
        }
    );
    assert_eq!(load.options.worktree_scope(), Some(DiffScope::All));
}

#[test]
fn tab_from_pending_review_load_cancels_to_current_all_changes() {
    let mut app = DiffApp::new(
        DiffOptions::default(),
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.jobs.pending_review_load = Some(pending_review_load());

    let should_quit = app
        .handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
        .expect("tab should cycle pending review to all changes");

    assert!(!should_quit);
    assert!(app.jobs.pending_review_load.is_none());
    assert!(app.jobs.pending_diff_load.is_none());
    assert_eq!(app.document.options, DiffOptions::default());
}

#[test]
fn diff_menu_options_preserve_repo_and_untracked_setting() {
    let options = DiffOptions {
        repo: Some(PathBuf::from("/repo").into()),
        local_untracked: mark_diff::UntrackedMode::Exclude,
        ..DiffOptions::default()
    };
    let mut app = DiffApp::new(
        options.clone(),
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.refs.branch_base = Some("origin/main".to_owned());
    app.refs.branch_head = Some("feature/ui".to_owned());
    app.refs.current_head = Some("feature/ui".to_owned());

    let staged = app.options_for_choice(DiffChoice::Staged).unwrap();
    assert_eq!(staged.repo, options.repo);
    assert!(!staged.include_untracked());
    assert_eq!(
        staged.source,
        DiffSource::Worktree {
            scope: DiffScope::Staged
        }
    );
    assert_eq!(staged.worktree_scope(), Some(DiffScope::Staged));

    let branch = app.options_for_choice(DiffChoice::Branch).unwrap();
    assert_eq!(branch.source, DiffSource::Base("origin/main".into()));
    assert_eq!(branch.worktree_scope(), None);
}

#[test]
fn branch_choice_survives_switching_to_worktree_scope() {
    let options = DiffOptions {
        source: DiffSource::Base("origin/main".into()),
        ..DiffOptions::default()
    };
    let mut app = DiffApp::new(
        options,
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.refs.branch_base = Some("origin/main".to_owned());
    app.refs.branch_head = Some("feature/header".to_owned());

    app.replace_loaded_diff(DiffOptions::default(), changeset_with_context_lines(1));

    assert_eq!(app.refs.branch_base.as_deref(), Some("origin/main"));
    assert_eq!(app.refs.branch_head.as_deref(), Some("feature/header"));
    assert_eq!(
        app.options_for_choice(DiffChoice::Branch)
            .map(|options| options.source),
        Some(DiffSource::Branch {
            base: "origin/main".into(),
            head: "feature/header".into(),
        })
    );
}

#[test]
fn branch_header_exposes_head_and_base_selectors() {
    let options = DiffOptions {
        source: DiffSource::Base("origin/main".into()),
        ..DiffOptions::default()
    };
    let mut app = DiffApp::new(
        options,
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.refs.branch_head = Some("feature/ui".to_owned());
    app.refs.branch_base = Some("origin/main".to_owned());
    app.refs.current_head = Some("feature/ui".to_owned());

    assert_eq!(
        app.branch_selector_text(BranchMenu::Head).as_deref(),
        Some("● feature/ui ▾")
    );
    assert_eq!(
        app.branch_selector_text(BranchMenu::Base).as_deref(),
        Some("⌂ origin/main ▾")
    );
    assert_eq!(
        app.branch_selector_at(diff_selector_width(&app.document.options)),
        None
    );
    assert_eq!(
        app.branch_selector_at(
            diff_selector_width(&app.document.options) + STATUSLINE_SELECTOR_GAP.width() as u16
        ),
        Some(BranchMenu::Head)
    );

    app.toggle_branch_menu(BranchMenu::Head);
    let empty_input = app.branch_selector_text(BranchMenu::Head).unwrap();
    assert_eq!(empty_input, "● feature/ui ▾");
    app.push_branch_input('f');
    let typed_input = app.branch_selector_text(BranchMenu::Head).unwrap();
    assert_eq!(typed_input, "● feature/ui ▾");
    app.close_branch_menu();
    assert_eq!(
        app.branch_selector_text(BranchMenu::Head).as_deref(),
        Some("● feature/ui ▾")
    );
}

#[test]
fn branch_menu_draws_centered_floating_filter() {
    let options = DiffOptions {
        source: DiffSource::Base("main".into()),
        ..DiffOptions::default()
    };
    let mut app = DiffApp::new(
        options,
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.refs.branch_base = Some("main".to_owned());
    app.refs.branch_head = Some("feature".to_owned());
    app.refs.current_head = Some("feature".to_owned());
    app.refs.comparison_branches = branch_names(&["main", "feature"]);
    app.toggle_branch_menu(BranchMenu::Base);
    app.push_branch_input('m');

    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 20))
        .expect("test terminal should be created");
    terminal
        .draw(|frame| crate::render::draw(frame, &mut app))
        .expect("branch menu draw should succeed");

    let buffer = terminal.backend().buffer();
    let rows: Vec<String> = (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer.cell((x, y)).expect("cell should exist").symbol())
                .collect()
        })
        .collect();
    let title = rows
        .iter()
        .enumerate()
        .find_map(|(row, text)| text.find("base branch").map(|column| (row, column)))
        .expect("floating branch menu should render title");

    assert!(title.0 > 4 && title.0 < 12, "title row was {}", title.0);
    assert!(
        rows.iter()
            .any(|row| row.contains(&format!("> m{INPUT_CURSOR}")))
    );
    assert!(rows.iter().any(|row| row.contains("main")));
    assert!(
        !rows
            .iter()
            .any(|row| row.contains("main") && row.contains("1 │"))
    );
}

#[test]
fn branch_menu_number_keys_filter_branch_names() {
    let options = DiffOptions {
        source: DiffSource::Base("main".into()),
        ..DiffOptions::default()
    };
    let mut app = DiffApp::new(
        options,
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.refs.branch_base = Some("main".to_owned());
    app.refs.branch_head = Some("feature".to_owned());
    app.refs.current_head = Some("feature".to_owned());
    app.refs.comparison_branches = branch_names(&["release/2026", "release/2025", "topic-a"]);

    app.toggle_branch_menu(BranchMenu::Head);
    for character in "release/".chars() {
        app.push_branch_input(character);
    }
    app.handle_key(KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE))
        .expect("2 should filter branch names");

    assert_eq!(app.refs.branch_menu_open(), Some(BranchMenu::Head));
    assert_eq!(app.refs.branch_menu.input, "release/2");
    assert_eq!(
        app.filtered_branches(),
        vec!["release/2026", "release/2025"]
    );
    assert!(app.jobs.pending_diff_load.is_none());
}

#[test]
fn branch_menu_ctrl_n_and_ctrl_p_cycle_selection_from_input() {
    let options = DiffOptions {
        source: DiffSource::Base("main".into()),
        ..DiffOptions::default()
    };
    let mut app = DiffApp::new(
        options,
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.refs.branch_base = Some("main".to_owned());
    app.refs.branch_head = Some("feature".to_owned());
    app.refs.current_head = Some("feature".to_owned());
    app.refs.comparison_branches = branch_names(&["main", "feature", "topic"]);

    app.toggle_branch_menu(BranchMenu::Base);
    app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL))
        .expect("ctrl-n should move branch selection");
    assert_eq!(app.refs.branch_menu.selected, 1);

    app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL))
        .expect("ctrl-p should move branch selection");
    assert_eq!(app.refs.branch_menu.selected, 0);
    assert!(app.refs.branch_menu.input.is_empty());
}

#[test]
fn branch_menu_scrolls_visible_branch_window() {
    let options = DiffOptions {
        source: DiffSource::Base("branch-00".into()),
        ..DiffOptions::default()
    };
    let mut app = DiffApp::new(
        options,
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.refs.comparison_branches = (0..12)
        .map(|index| format!("branch-{index:02}").into())
        .collect();

    assert_eq!(app.max_branch_menu_scroll(), 0);

    app.move_branch_selection(99);
    assert_eq!(app.refs.branch_menu.selected, 10);
    assert_eq!(app.refs.branch_menu.scroll, 0);

    app.move_branch_selection(-1);
    assert_eq!(app.refs.branch_menu.selected, 9);
    assert_eq!(app.refs.branch_menu.scroll, 0);
}

#[test]
fn branch_menu_expands_to_show_long_branch_when_terminal_allows() {
    let options = DiffOptions {
        source: DiffSource::Base("main".into()),
        ..DiffOptions::default()
    };
    let mut app = DiffApp::new(
        options,
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    let long_branch = "feature/really-long-branch-name-that-should-fit-without-being-cut-off";
    app.refs.branch_base = Some("main".to_owned());
    app.refs.branch_head = Some("feature".to_owned());
    app.refs.current_head = Some("feature".to_owned());
    app.refs.comparison_branches = vec!["main".into(), long_branch.into()];
    app.toggle_branch_menu(BranchMenu::Base);

    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 20))
        .expect("test terminal should be created");
    terminal
        .draw(|frame| crate::render::draw(frame, &mut app))
        .expect("branch menu draw should succeed");

    let rows = buffer_rows(terminal.backend().buffer());
    assert!(rows.iter().any(|row| row.contains(long_branch)));
}

#[test]
fn branch_menu_scrolls_to_rendered_rows_in_short_terminal() {
    let options = DiffOptions {
        source: DiffSource::Base("branch-00".into()),
        ..DiffOptions::default()
    };
    let mut app = DiffApp::new(
        options,
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.refs.branch_base = Some("branch-00".to_owned());
    app.refs.branch_head = Some("branch-01".to_owned());
    app.refs.current_head = Some("branch-01".to_owned());
    app.refs.comparison_branches = (0..12)
        .map(|index| format!("branch-{index:02}").into())
        .collect();
    app.toggle_branch_menu(BranchMenu::Base);
    app.move_branch_selection(5);
    assert_eq!(app.refs.branch_menu.scroll, 0);

    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 8))
        .expect("test terminal should be created");
    terminal
        .draw(|frame| crate::render::draw(frame, &mut app))
        .expect("branch menu draw should succeed");

    assert_eq!(app.refs.branch_menu.selected, 5);
    assert_eq!(app.refs.branch_menu.scroll, 3);
    let rows = buffer_rows(terminal.backend().buffer());
    assert!(rows.iter().any(|row| row.contains("branch-06")));
    assert!(
        !rows
            .iter()
            .any(|row| row.contains("branch-02") && row.contains("│"))
    );
}

#[test]
fn branch_menu_navigation_keeps_expanded_rows_stable_in_tall_terminal() {
    let options = DiffOptions {
        source: DiffSource::Base("branch-00".into()),
        ..DiffOptions::default()
    };
    let mut app = DiffApp::new(
        options,
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.set_terminal_area(Rect {
        x: 0,
        y: 0,
        width: 120,
        height: 60,
    });
    app.refs.branch_base = Some("branch-00".to_owned());
    app.refs.branch_head = Some("branch-01".to_owned());
    app.refs.current_head = Some("branch-01".to_owned());
    app.refs.comparison_branches = (0..40)
        .map(|index| format!("branch-{index:02}").into())
        .collect();
    app.toggle_branch_menu(BranchMenu::Base);

    app.move_branch_selection(20);
    assert_eq!(app.refs.branch_menu.selected, 20);
    assert_eq!(app.refs.branch_menu.scroll, 0);

    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 60))
        .expect("test terminal should be created");
    terminal
        .draw(|frame| crate::render::draw(frame, &mut app))
        .expect("branch menu draw should succeed");

    assert_eq!(app.refs.branch_menu.scroll, 0);
    let rows = buffer_rows(terminal.backend().buffer());
    assert!(rows.iter().any(|row| row.contains("branch-01")));
}

#[test]
fn commit_menu_scrolls_to_rendered_rows_and_highlights_selection() {
    let options = DiffOptions {
        source: DiffSource::Show("HEAD".into()),
        ..DiffOptions::default()
    };
    let mut app = DiffApp::new(
        options,
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.refs.show_rev = Some("ccccccc".to_owned());
    app.refs.comparison_commits = (0..12)
        .map(|index| GitCommit {
            sha: format!("{index:07x}").into(),
            subject: format!("commit-{index:02}"),
        })
        .collect();
    app.toggle_commit_menu();
    app.set_commit_selection(5);
    assert_eq!(app.refs.commit_menu.scroll, 0);

    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 8))
        .expect("test terminal should be created");
    terminal
        .draw(|frame| crate::render::draw(frame, &mut app))
        .expect("commit menu draw should succeed");

    assert_eq!(app.refs.commit_menu.selected, 5);
    assert!(app.refs.commit_menu.scroll > 0);
    let rows = buffer_rows(terminal.backend().buffer());
    assert!(
        rows.iter()
            .any(|row| row.contains("0000005") && row.contains("commit-05"))
    );
    assert!(
        !rows
            .iter()
            .any(|row| row.contains("0000001") && row.contains("commit-01") && row.contains("│"))
    );
}

#[test]
fn commit_menu_navigation_keeps_expanded_rows_stable_in_tall_terminal() {
    let options = DiffOptions {
        source: DiffSource::Show("HEAD".into()),
        ..DiffOptions::default()
    };
    let mut app = DiffApp::new(
        options,
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.set_terminal_area(Rect {
        x: 0,
        y: 0,
        width: 120,
        height: 60,
    });
    app.refs.show_rev = Some("0000000".to_owned());
    app.refs.comparison_commits = (0..40)
        .map(|index| GitCommit {
            sha: format!("{index:07x}").into(),
            subject: format!("commit-{index:02}"),
        })
        .collect();
    app.toggle_commit_menu();

    app.move_commit_selection(20);
    assert_eq!(app.refs.commit_menu.selected, 20);
    assert_eq!(app.refs.commit_menu.scroll, 0);

    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 60))
        .expect("test terminal should be created");
    terminal
        .draw(|frame| crate::render::draw(frame, &mut app))
        .expect("commit menu draw should succeed");

    assert_eq!(app.refs.commit_menu.scroll, 0);
    let rows = buffer_rows(terminal.backend().buffer());
    assert!(
        rows.iter()
            .any(|row| row.contains("0000001") && row.contains("commit-01"))
    );
}

#[test]
fn mouse_wheel_over_commit_menu_scrolls_menu_not_diff() {
    let options = DiffOptions {
        source: DiffSource::Show("HEAD".into()),
        ..DiffOptions::default()
    };
    let mut app = DiffApp::new(
        options,
        changeset_with_context_lines(60),
        DiffLayoutMode::Unified,
    );
    app.set_viewport_rows(10);
    assert!(app.max_scroll() > 0);
    app.refs.comparison_commits = (0..12)
        .map(|index| GitCommit {
            sha: format!("{index:07x}").into(),
            subject: format!("commit-{index:02}"),
        })
        .collect();
    app.toggle_commit_menu();

    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: 0,
        row: 0,
        modifiers: KeyModifiers::NONE,
    })
    .expect("mouse wheel should be handled");

    assert_eq!(app.viewport.scroll, 0);
    assert_eq!(app.refs.commit_menu.selected, 1);
}

#[test]
fn branch_combo_input_filters_and_completes() {
    let options = DiffOptions {
        source: DiffSource::Base("main".into()),
        ..DiffOptions::default()
    };
    let mut app = DiffApp::new(
        options,
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.refs.comparison_branches = branch_names(&["main", "feature/header", "fix/footer"]);

    app.push_branch_input('h');
    assert_eq!(app.filtered_branches(), vec!["feature/header"]);

    app.clear_branch_input();
    app.push_branch_input('f');
    app.push_branch_input('h');
    assert_eq!(app.filtered_branches(), vec!["feature/header"]);

    app.refs.open_branch_menu(BranchMenu::Head);
    app.cycle_branch_completion(1);
    assert_eq!(app.refs.branch_menu.selected, 0);
    assert_eq!(app.refs.branch_menu.input, "fh");

    app.clear_branch_input();
    app.push_branch_input('f');
    assert_eq!(
        app.filtered_branches(),
        vec!["fix/footer", "feature/header"]
    );
    app.cycle_branch_completion(1);
    assert_eq!(app.refs.branch_menu.selected, 1);
    app.cycle_branch_completion(-1);
    assert_eq!(app.refs.branch_menu.selected, 0);

    app.clear_branch_input();
    assert!(app.refs.branch_menu.input.is_empty());
}

#[test]
fn branch_combo_pins_current_head_and_base_before_recent_order() {
    let options = DiffOptions {
        source: DiffSource::Base("release".into()),
        ..DiffOptions::default()
    };
    let mut app = DiffApp::new(
        options,
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.refs.branch_head = Some("feature/header".to_owned());
    app.refs.current_head = Some("feature/header".to_owned());
    app.refs.branch_base = Some("release".to_owned());
    app.refs.comparison_branches =
        branch_names(&["recent", "old", "origin/main", "release", "feature/header"]);

    app.refs.open_branch_menu(BranchMenu::Base);
    assert_eq!(
        app.filtered_branches(),
        vec!["feature/header", "recent", "old", "origin/main"]
    );

    app.refs.open_branch_menu(BranchMenu::Head);
    assert_eq!(
        app.filtered_branches(),
        vec!["release", "recent", "old", "origin/main"]
    );
}

#[test]
fn branch_combo_close_clears_input_without_changing_selection() {
    let options = DiffOptions {
        source: DiffSource::Base("main".into()),
        ..DiffOptions::default()
    };
    let mut app = DiffApp::new(
        options,
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.refs.branch_base = Some("main".to_owned());
    app.refs.branch_head = Some("feature/header".to_owned());
    app.refs.comparison_branches = branch_names(&["main", "feature/header"]);

    app.toggle_branch_menu(BranchMenu::Base);
    app.push_branch_input('f');
    app.close_branch_menu();

    assert!(app.refs.branch_menu_open().is_none());
    assert!(app.refs.branch_menu.input.is_empty());
    assert_eq!(app.refs.branch_base.as_deref(), Some("main"));
    assert_eq!(app.refs.branch_head.as_deref(), Some("feature/header"));
    assert_eq!(app.document.options.source, DiffSource::Base("main".into()));
}

#[test]
fn fit_helpers_use_terminal_display_width() {
    assert_eq!(fit("界a", 2), "界");
    assert_eq!(fit_padded("e\u{301}", 2), "e\u{301} ");
    assert_eq!(fit_padded_from("abcdef", 2, 3), "cde");
    assert_eq!(skip_display_prefix("abcdef", 2), ("cdef", 2));
    assert_eq!(skip_display_prefix("e\u{301}f", 1), ("f", 1));
    assert_eq!(fit_with_ellipsis("abcdef", 5), "ab...");
}

#[test]
fn color_overrides_layer_on_colorscheme() {
    let theme = DiffTheme::system()
        .with_color_overrides(&ColorOverrides {
            bg: Some("#010203".to_owned()),
            addition_bg: Some("#123456".to_owned()),
            deletion_fg: Some("bright-red".to_owned()),
            cursor: Some("white".to_owned()),
            cursor_line_bg: Some("#0a0b0c".to_owned()),
            search_match_fg: Some("#112233".to_owned()),
            search_match_bg: Some("#223344".to_owned()),
            statusline_accent_bg: Some("#334455".to_owned()),
            statusline_info_fg: Some("#445566".to_owned()),
            keyword: Some("ansi-13".to_owned()),
            ..ColorOverrides::default()
        })
        .expect("color overrides should parse");

    assert_eq!(theme.background, Color::Rgb(1, 2, 3));
    assert_eq!(
        row_bg(DiffLineKind::Addition, theme),
        Color::Rgb(0x12, 0x34, 0x56)
    );
    assert_eq!(theme.deletion_fg, Color::LightRed);
    assert_eq!(theme.cursor, Color::White);
    assert_eq!(theme.cursor_line_bg, Color::Rgb(0x0a, 0x0b, 0x0c));
    assert_eq!(theme.search_match_fg, Color::Rgb(0x11, 0x22, 0x33));
    assert_eq!(theme.search_match_bg, Color::Rgb(0x22, 0x33, 0x44));
    assert_eq!(theme.statusline_accent_bg, Color::Rgb(0x33, 0x44, 0x55));
    assert_eq!(theme.statusline_info_fg, Color::Rgb(0x44, 0x55, 0x66));
    assert_eq!(
        theme.syntax.color(SyntaxClass::Keyword),
        Some(Color::Indexed(13))
    );
}

#[test]
fn color_scheme_picker_lists_supported_builtin_themes_only() {
    assert_eq!(
        COLOR_SCHEME_CHOICES,
        &[
            ColorSchemeChoice::System,
            ColorSchemeChoice::CatppuccinLatte,
            ColorSchemeChoice::CatppuccinFrappe,
            ColorSchemeChoice::CatppuccinMacchiato,
            ColorSchemeChoice::CatppuccinMocha,
            ColorSchemeChoice::GruvboxDark,
            ColorSchemeChoice::GruvboxLight,
            ColorSchemeChoice::GithubDark,
            ColorSchemeChoice::GithubDarkHighContrast,
            ColorSchemeChoice::GithubLight,
            ColorSchemeChoice::GithubLightHighContrast,
            ColorSchemeChoice::Tokyonight,
        ]
    );
}

#[test]
fn branch_full_file_source_uses_merge_base_and_head_revision() {
    let repo = std::env::temp_dir();
    let file = mark_diff::DiffFile {
        change: mark_diff::FileChange::from_status(
            mark_diff::FileStatus::Renamed,
            Some("old.rs".to_owned()),
            Some("new.rs".to_owned()),
        ),
        additions: 0,
        deletions: 0,
        body: mark_diff::DiffFileBody::Text { hunks: Vec::new() },
    };
    let base = "origin/main".to_owned();
    let head = "feature/full-file".to_owned();
    let branch = DiffOptions {
        source: DiffSource::Branch {
            base: base.clone().into(),
            head: head.clone().into(),
        },
        ..DiffOptions::default()
    };

    assert_eq!(
        full_file_source(&repo, &branch, &file, DiffSide::Old)
            .unwrap()
            .kind,
        FullFileSourceKind::GitMergeBase {
            base: base.into(),
            head: head.clone().into(),
            path: "old.rs".into(),
        }
    );
    assert_eq!(
        full_file_source(&repo, &branch, &file, DiffSide::New)
            .unwrap()
            .kind,
        FullFileSourceKind::GitRevision {
            rev: head.into(),
            path: "new.rs".into(),
        }
    );
}

#[test]
fn git_full_file_helpers_do_not_treat_revisions_as_options() {
    let repo = temp_test_dir("git-option-boundary");
    fs::create_dir_all(&repo).expect("repo directory should be created");
    git(&repo, &["init", "-q"]);
    git(&repo, &["config", "user.email", "test@example.com"]);
    git(&repo, &["config", "user.name", "Test"]);
    fs::write(repo.join("file.rs"), "fn main() {}\n").expect("file should be written");
    git(&repo, &["add", "file.rs"]);
    git(&repo, &["commit", "-q", "-m", "init"]);
    let output_path = repo.join("poc.txt");
    let output_arg = format!("--output={}", output_path.display());

    assert!(git_blob(&repo, &output_arg).is_err());
    assert!(!output_path.exists());
    assert!(git_merge_base(&repo, &output_arg, "HEAD").is_err());
    assert!(!output_path.exists());

    fs::remove_dir_all(repo).expect("repo directory should be removed");
}
