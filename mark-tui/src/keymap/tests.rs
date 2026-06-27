use super::*;
use crossterm::event::KeyModifiers;

#[test]
fn keymap_parses_configured_global_and_menu_bindings() {
    let keymap = Keymap::parse(
        r#"
            [keymap.global]
            leader = ","
            diff_menu = ", d"
            quit = ", x"
            file_filter = "ctrl-f"
            head_branch = "m h"
            save_mark = "ctrl-enter"
            copy_marks = ", y"
            copy_error_log = "ctrl+shift+c"
            prev_diff_type = "shift-left"
            expand_context_up = []

            [keymap.menu]
            down = ["s", "down"]
            "#,
    )
    .expect("keymap should parse");

    let comma = KeyPress::from(KeyEvent::new(KeyCode::Char(','), KeyModifiers::NONE));
    assert!(keymap.is_prefix(KeyEvent::new(KeyCode::Char(','), KeyModifiers::NONE)));
    assert!(keymap.matches_prefix(
        GlobalAction::DiffMenu,
        comma,
        KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE)
    ));
    assert!(keymap.matches_single(
        GlobalAction::FileFilter,
        KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL)
    ));
    assert!(keymap.matches_single(
        GlobalAction::CopyErrorLog,
        KeyEvent::new(
            KeyCode::Char('C'),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT
        )
    ));
    assert!(keymap.matches_prefix(
        GlobalAction::CopyMarks,
        comma,
        KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE)
    ));
    assert!(keymap.is_prefix(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE)));
    assert!(keymap.matches_prefix(
        GlobalAction::HeadBranch,
        KeyPress::from(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE)),
        KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE)
    ));
    assert_eq!(
        keymap.global_action_label(GlobalAction::CopyErrorLog),
        "Ctrl-Shift-C"
    );
    assert!(keymap.matches_menu(
        MenuAction::Down,
        KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE)
    ));
    assert!(keymap.matches_help_menu_scroll(
        MenuAction::Down,
        KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE)
    ));
    assert!(!keymap.matches_help_menu_scroll(
        MenuAction::Down,
        KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)
    ));
    assert!(!keymap.matches_help_menu_scroll(
        MenuAction::Up,
        KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT)
    ));
    assert!(keymap.matches_single(
        GlobalAction::PreviousDiffType,
        KeyEvent::new(KeyCode::Left, KeyModifiers::SHIFT)
    ));
}

#[test]
fn keymap_preserves_shifted_character_bindings() {
    let keymap = Keymap::parse(
        r#"
            [keymap.global]
            quit = "shift-q"
            "#,
    )
    .expect("keymap should parse");

    assert!(keymap.matches_single(
        GlobalAction::Quit,
        KeyEvent::new(KeyCode::Char('Q'), KeyModifiers::SHIFT)
    ));
    assert!(!keymap.matches_single(
        GlobalAction::Quit,
        KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)
    ));
}

#[test]
fn default_copy_error_log_matches_hunk_diff_binding() {
    let keymap = Keymap::default();

    assert!(keymap.matches_single(
        GlobalAction::CopyErrorLog,
        KeyEvent::new(
            KeyCode::Char('C'),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT
        )
    ));
    assert!(keymap.matches_single(
        GlobalAction::CopyErrorLog,
        KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT
        )
    ));
    assert!(!keymap.matches_single(
        GlobalAction::CopyErrorLog,
        KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)
    ));
    assert_eq!(
        keymap.global_action_label(GlobalAction::CopyErrorLog),
        "Ctrl-Shift-C"
    );
}

#[test]
fn default_mark_bindings_are_configurable_actions() {
    let keymap = Keymap::default();

    assert!(keymap.matches_single(
        GlobalAction::SaveMark,
        KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL)
    ));
    assert!(keymap.matches_single(
        GlobalAction::CancelMark,
        KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)
    ));
    assert!(keymap.matches_single(
        GlobalAction::CopyMarks,
        KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE)
    ));
    assert_eq!(keymap.global_action_label(GlobalAction::CopyMarks), "y");
    assert!(!keymap.is_prefix(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE)));
}

#[test]
fn default_review_actions_use_mnemonic_keys() {
    let keymap = Keymap::default();

    assert!(keymap.is_prefix(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE)));
    assert!(!keymap.is_prefix(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE)));
    assert!(keymap.matches_prefix(
        GlobalAction::DiffMenu,
        KeyPress::from(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE)),
        KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE)
    ));
    assert!(keymap.matches_single(
        GlobalAction::OptionsMenu,
        KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE)
    ));
    assert!(keymap.matches_single(
        GlobalAction::Layout,
        KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE)
    ));
    assert!(!keymap.matches_single(
        GlobalAction::EditHunk,
        KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE)
    ));
    assert!(keymap.matches_single(
        GlobalAction::EditHunk,
        KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL)
    ));
    assert!(keymap.matches_single(
        GlobalAction::ClearFilters,
        KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL)
    ));
    assert!(keymap.matches_single(
        GlobalAction::NextAnnotation,
        KeyEvent::new(KeyCode::Char('}'), KeyModifiers::NONE)
    ));
    assert!(keymap.matches_single(
        GlobalAction::PreviousAnnotation,
        KeyEvent::new(KeyCode::Char('{'), KeyModifiers::NONE)
    ));
    assert!(keymap.matches_single(
        GlobalAction::PreviousFile,
        KeyEvent::new(KeyCode::Char('('), KeyModifiers::NONE)
    ));
    assert!(keymap.matches_single(
        GlobalAction::NextFile,
        KeyEvent::new(KeyCode::Char(')'), KeyModifiers::NONE)
    ));
    assert!(keymap.matches_single(
        GlobalAction::PreviousHunk,
        KeyEvent::new(KeyCode::Char('['), KeyModifiers::NONE)
    ));
    assert!(keymap.matches_single(
        GlobalAction::NextHunk,
        KeyEvent::new(KeyCode::Char(']'), KeyModifiers::NONE)
    ));
    assert!(keymap.matches_single(
        GlobalAction::ExpandContextUp,
        KeyEvent::new(KeyCode::Char(','), KeyModifiers::NONE)
    ));
    assert!(keymap.matches_single(
        GlobalAction::ExpandContextDown,
        KeyEvent::new(KeyCode::Char('.'), KeyModifiers::NONE)
    ));
    assert!(keymap.matches_single(
        GlobalAction::CollapseContextAll,
        KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE)
    ));
    assert!(keymap.matches_prefix(
        GlobalAction::HeadBranch,
        KeyPress::from(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE)),
        KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE)
    ));
    assert!(keymap.matches_prefix(
        GlobalAction::BaseBranch,
        KeyPress::from(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE)),
        KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE)
    ));
    assert!(keymap.matches_prefix(
        GlobalAction::CommitPicker,
        KeyPress::from(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE)),
        KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE)
    ));
    assert!(!keymap.is_prefix(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::NONE)));
}

#[test]
fn keymap_allows_global_bindings_that_overlap_mark_draft_bindings() {
    let keymap = Keymap::parse(
        r#"
            [keymap.global]
            reload = "ctrl-s"
            quit = "esc"
            "#,
    )
    .expect("draft-only bindings should not reject existing global bindings");

    assert!(keymap.matches_single(
        GlobalAction::Reload,
        KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL)
    ));
    assert!(keymap.matches_single(
        GlobalAction::SaveMark,
        KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL)
    ));
    assert!(keymap.matches_single(
        GlobalAction::Quit,
        KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)
    ));
    assert!(keymap.matches_single(
        GlobalAction::CancelMark,
        KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)
    ));
}

#[test]
fn keymap_allows_prefixes_that_overlap_default_mark_draft_bindings() {
    let ctrl_s_prefix = Keymap::parse(
        r#"
            [keymap.global]
            leader = "ctrl-s"
            copy_marks = "ctrl-s y"
            "#,
    )
    .expect("ctrl-s prefix should parse");

    assert!(ctrl_s_prefix.is_prefix(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL)));
    assert_eq!(
        ctrl_s_prefix.global_action_label(GlobalAction::SaveMark),
        "Ctrl-S"
    );

    let esc_prefix = Keymap::parse(
        r#"
            [keymap.global]
            leader = "esc"
            copy_marks = "esc y"
            "#,
    )
    .expect("esc prefix should parse");

    assert!(esc_prefix.is_prefix(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
    assert_eq!(
        esc_prefix.global_action_label(GlobalAction::CancelMark),
        "Esc"
    );
}

#[test]
fn keymap_rejects_multi_key_mark_draft_binding() {
    let error = Keymap::parse(
        r#"
            [keymap.global]
            save_mark = "ctrl-s y"
            "#,
    )
    .expect_err("configured draft binding should be single-key");

    assert!(error.contains("save_mark must be a single key"));
}

#[test]
fn keymap_rejects_conflicting_mark_draft_bindings() {
    let error = Keymap::parse(
        r#"
            [keymap.global]
            save_mark = "esc"
            cancel_mark = "esc"
            "#,
    )
    .expect_err("mark draft bindings should not conflict with each other");

    assert!(error.contains("keymap.global conflict"));
}

#[test]
fn keymap_allows_arbitrary_multi_key_global_binding() {
    let keymap = Keymap::parse(
        r#"
            [keymap.global]
            diff_menu = "z d"
            "#,
    )
    .expect("multi-key binding should parse");

    assert!(keymap.matches_prefix(
        GlobalAction::DiffMenu,
        KeyPress::from(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE)),
        KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE)
    ));
}

#[test]
fn keymap_clears_unconfigured_copy_marks_when_used_as_prefix() {
    let keymap = Keymap::parse(
        r#"
            [keymap.global]
            diff_menu = "y d"
            "#,
    )
    .expect("unconfigured copy_marks should not reserve y as a prefix");

    let y = KeyPress::from(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE));
    assert!(keymap.is_prefix(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE)));
    assert!(keymap.matches_prefix(
        GlobalAction::DiffMenu,
        y,
        KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE)
    ));
    assert_eq!(
        keymap.global_action_label(GlobalAction::CopyMarks),
        "unbound"
    );
}

#[test]
fn keymap_allows_direct_space_when_leader_is_unused() {
    let keymap = Keymap::parse(
        r#"
            [keymap.global]
            diff_menu = "space"
            "#,
    )
    .expect("space binding should parse without a leader sequence");

    assert!(keymap.matches_single(
        GlobalAction::DiffMenu,
        KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE)
    ));
    assert!(!keymap.is_prefix(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE)));
}

#[test]
fn keymap_uses_space_prefix_sequences() {
    let keymap = Keymap::parse(
        r#"
            [keymap.global]
            help = "space h"
            "#,
    )
    .expect("space prefix binding should parse");

    assert!(keymap.is_prefix(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE)));
    assert!(keymap.matches_prefix(
        GlobalAction::Help,
        KeyPress::from(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE)),
        KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE)
    ));
}

#[test]
fn keymap_does_not_reserve_unused_configured_leader() {
    let keymap = Keymap::parse(
        r#"
            [keymap.global]
            leader = "ctrl-g"
            "#,
    )
    .expect("unused leader should parse");

    assert!(!keymap.is_prefix(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL)));
    assert!(keymap.matches_single(
        GlobalAction::EditHunk,
        KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL)
    ));
}

#[test]
fn keymap_rejects_single_key_that_is_also_a_prefix() {
    let error = Keymap::parse(
        r#"
            [keymap.global]
            reload = "d"
            diff_menu = "d m"
            "#,
    )
    .expect_err("ambiguous prefix should fail");

    assert!(error.contains("is both a binding"));
}

#[test]
fn keymap_rejects_conflicting_bindings_in_same_context() {
    let error = Keymap::parse(
        r#"
            [keymap.global]
            help = "r"
            reload = "r"
            "#,
    )
    .expect_err("conflicting keymap should fail");

    assert!(error.contains("keymap.global conflict"));
}

#[test]
fn keymap_rejects_multi_key_editor_binding() {
    let error = Keymap::parse(
        r#"
            [keymap.global]
            edit_hunk = "space e"
            "#,
    )
    .expect_err("multi-key editor binding should fail");

    assert!(error.contains("edit_hunk must be a single key"));
}
