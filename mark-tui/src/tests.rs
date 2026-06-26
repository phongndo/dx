use crate::render::{
    diff::{
        SplitCellRender, SplitLineRender, SplitSide, build_diff_viewport_lines,
        content_spans_at_scroll, context_hide_line, context_show_line, empty_diff_fill_from,
        inline_bg, render_row, render_row_with_focus, render_row_wrapped_with_focus,
        render_split_context_line_wrapped, render_split_line_with_focus,
        render_unified_line_at_scroll, row_bg, split_cell_spans_at_scroll, syntax_fg,
    },
    grep::{
        grep_highlight_target_for_columns, highlighted_grep_text_line,
        highlighted_mouse_diff_content_line, unified_content_start_column,
    },
    headers::{file_header_line, file_separator_line, hunk_header_line, hunk_header_spans},
    menus::{
        branch_menu_block, diff_comparison_label, diff_selector_text, diff_selector_width,
        help_menu_bg, help_menu_content_rows, help_menu_lines, help_menu_list_visible_rows,
        help_menu_row_line, help_menu_row_spans, help_menu_title_color,
    },
    sidebar::file_sidebar_lines,
    statusline::{
        error_log_header_line, error_log_height, error_log_separator, filter_bar_line,
        filter_bar_visible, statusline_file_count_label, statusline_header_line,
    },
    style::base_bg,
    text::{
        fit, fit_padded, fit_padded_from, fit_with_ellipsis, format_count, progress_label,
        skip_display_prefix,
    },
};
use crate::{
    app::*, controls::*, editor::*, keymap::*, live_diff::*, model::*, syntax::*, theme::*,
    toast::*,
};
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use mark_core::MarkError;
use mark_diff::{
    Changeset, DiffLine, DiffLineKind, DiffOptions, DiffScope, DiffSource, FileStatus, PatchSource,
};
use mark_syntax::{
    ColorOverrides, DiffContextExpansion, DiffSettings, HighlightedLine, LayoutSetting,
    MAX_NOTIFICATION_TIMEOUT_MS, NotificationMode, NotificationSettings, SyntaxClass,
    SyntaxLanguageSet, SyntaxLimits, SyntaxSettings, SyntaxThemeConfig, SyntaxThemeSource,
    ToastCorner,
};
use ratatui::layout::Rect;
use ratatui::prelude::{Color, Line, Modifier, Span, Style};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, oneshot};
use unicode_width::UnicodeWidthStr;

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
fn max_scroll_stops_at_last_full_viewport() {
    assert_eq!(max_scroll_for_viewport(10, 1), 9);
    assert_eq!(max_scroll_for_viewport(10, 4), 6);
    assert_eq!(max_scroll_for_viewport(3, 10), 0);
    assert_eq!(max_scroll_for_viewport(10, 10), 0);
    assert_eq!(max_scroll_for_viewport(10, 0), 9);
}

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
fn app_clamps_scroll_to_last_full_viewport() {
    let changeset = changeset_with_context_lines(10);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);

    app.set_viewport_rows(5);
    app.set_scroll(usize::MAX);

    assert_eq!(app.scroll, app.model.len() - 5);
    assert_eq!(app.viewport_focus_row(), app.model.len() - 1);

    app.set_viewport_rows(usize::MAX);

    assert_eq!(app.scroll, 0);
}

#[test]
fn app_clamps_horizontal_scroll_to_diff_content() {
    let changeset = changeset_with_line_text("abcdefghijkl");
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);

    app.set_viewport_width(18);
    assert_eq!(diff_content_width(app.layout, app.viewport_width), 4);

    app.scroll_horizontally_by(HORIZONTAL_SCROLL_STEP as isize);
    assert_eq!(app.horizontal_scroll, 8);

    app.scroll_horizontally_by(HORIZONTAL_SCROLL_STEP as isize);
    assert_eq!(app.horizontal_scroll, 8);

    app.scroll_horizontally_by(-(HORIZONTAL_SCROLL_STEP as isize));
    assert_eq!(app.horizontal_scroll, 0);

    app.set_horizontal_scroll(8);
    app.set_viewport_width(80);
    assert_eq!(app.horizontal_scroll, 0);
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
    assert_eq!(app.scroll, app.max_scroll());
}

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
        &app.changeset,
        app.model.row(annotated_row).expect("annotated row"),
    )
    .expect("annotation key");
    app.annotations.insert(key, "one\ntwo\nthree".to_owned());
    app.set_scroll(annotated_row.saturating_sub(1));

    let rendered_hunks: Vec<_> = plan_diff_viewport_rows(&app, app.viewport_rows)
        .into_iter()
        .filter_map(|slot| match slot.kind {
            ViewportSlotKind::DiffVisual { model_row, .. } => {
                app.model.row(model_row).and_then(|row| row.hunk_key())
            }
            _ => None,
        })
        .collect();
    assert!(rendered_hunks.contains(&(0, 0)));
    assert!(!rendered_hunks.contains(&(0, 1)));
    assert_eq!(
        app.focused_hunk_for_viewport(app.viewport_rows),
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
        &app.changeset,
        app.model.row(annotated_row).expect("annotated row"),
    )
    .expect("annotation key");
    app.annotations.insert(key, "one\ntwo\nthree".to_owned());
    app.set_scroll(annotated_row.saturating_sub(1));

    assert_eq!(
        app.focused_hunk_for_viewport(app.viewport_rows),
        Some((0, 0))
    );

    app.next_hunk();

    let target_hunk_row = app
        .model
        .hunk_start_row(0, 1)
        .expect("target hunk should have a header row");
    let rendered_rows: Vec<_> = plan_diff_viewport_rows(&app, app.viewport_rows)
        .into_iter()
        .filter_map(|slot| match slot.kind {
            ViewportSlotKind::DiffVisual { model_row, .. } => Some(model_row),
            _ => None,
        })
        .collect();
    assert!(rendered_rows.contains(&target_hunk_row));
    assert_eq!(app.manual_hunk_focus, Some((0, 1)));
    assert_eq!(
        app.focused_hunk_for_viewport(app.viewport_rows),
        Some((0, 1))
    );

    app.next_hunk();
    assert_eq!(
        app.focused_hunk_for_viewport(app.viewport_rows),
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
    app.set_viewport_rows(app.model.len());

    let annotated_row = app
        .model
        .rows
        .iter()
        .position(|row| matches!(row, UiRow::UnifiedLine { .. }))
        .expect("unified line");
    let key = AnnotationKey::from_ui_row(
        &app.changeset,
        app.model.row(annotated_row).expect("annotated row"),
    )
    .expect("annotation key");
    app.annotations.insert(key, "note".to_owned());

    assert!(app.max_scroll() > annotated_row);
    app.set_scroll(app.max_scroll());
    let rendered_rows: Vec<_> = plan_diff_viewport_rows(&app, app.viewport_rows)
        .into_iter()
        .filter_map(|slot| match slot.kind {
            ViewportSlotKind::DiffVisual { model_row, .. } => Some(model_row),
            _ => None,
        })
        .collect();
    assert!(rendered_rows.contains(&(app.model.len() - 1)));
}

#[test]
fn max_scroll_for_short_annotation_at_end_avoids_blank_viewport() {
    use crate::annotation::AnnotationKey;
    use crate::render::viewport_plan::{ViewportSlotKind, plan_diff_viewport_rows};

    let lines: Vec<&str> = (0..100).map(|_| "line").collect();
    let changeset = changeset_with_line_texts(&lines);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_width(40);
    app.set_viewport_rows(app.model.len());

    let annotated_row = app
        .model
        .rows
        .iter()
        .rposition(|row| matches!(row, UiRow::UnifiedLine { .. }))
        .expect("last unified line");
    let key = AnnotationKey::from_ui_row(
        &app.changeset,
        app.model.row(annotated_row).expect("annotated row"),
    )
    .expect("annotation key");
    app.annotations.insert(key, "note".to_owned());

    assert!(app.max_scroll() < annotated_row);
    app.set_scroll(usize::MAX);
    let plans = plan_diff_viewport_rows(&app, app.viewport_rows);

    assert_eq!(plans.len(), app.viewport_rows);
    assert!(plans.iter().any(|slot| matches!(
        slot.kind,
        ViewportSlotKind::DiffVisual { model_row, .. } if model_row == annotated_row
    )));
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
        .model
        .rows
        .iter()
        .position(|row| matches!(row, UiRow::UnifiedLine { .. }))
        .expect("unified line");
    let key = AnnotationKey::from_ui_row(
        &app.changeset,
        app.model.row(annotated_row).expect("annotated row"),
    )
    .expect("annotation key");
    app.annotations.insert(key, "note".to_owned());

    assert_eq!(app.max_scroll(), 0);
    app.set_scroll(usize::MAX);
    assert_eq!(app.scroll, 0);
}

#[test]
fn hunk_focus_moves_between_hunks_when_diff_fits_viewport() {
    let changeset = changeset_with_hunks_at(PathBuf::from("/repo"), &[1, 2, 3]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(20);

    assert_eq!(app.max_scroll(), 0);
    assert_eq!(app.focused_hunk_for_viewport(20), Some((0, 0)));

    app.next_hunk();
    assert_eq!(app.scroll, 0);
    assert_eq!(app.focused_hunk_for_viewport(20), Some((0, 1)));

    app.next_hunk();
    assert_eq!(app.scroll, 0);
    assert_eq!(app.focused_hunk_for_viewport(20), Some((0, 2)));

    app.previous_hunk();
    assert_eq!(app.focused_hunk_for_viewport(20), Some((0, 1)));

    app.previous_hunk();
    assert_eq!(app.scroll, 0);
    assert_eq!(app.focused_hunk_for_viewport(20), Some((0, 0)));
}

#[test]
fn layout_toggle_resets_manual_hunk_focus_when_diff_fits_viewport() {
    let changeset = changeset_with_hunks_at(PathBuf::from("/repo"), &[1, 2, 3]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(20);

    assert_eq!(app.max_scroll(), 0);
    assert_eq!(app.focused_hunk_for_viewport(20), Some((0, 0)));

    app.next_hunk();
    assert_eq!(app.scroll, 0);
    assert_eq!(app.manual_hunk_focus, Some((0, 1)));
    assert_eq!(app.focused_hunk_for_viewport(20), Some((0, 1)));

    app.toggle_layout();

    assert_eq!(app.layout, DiffLayoutMode::Split);
    assert_eq!(app.scroll, 0);
    assert_eq!(app.manual_hunk_focus, None);
    assert_eq!(app.focused_hunk_for_viewport(20), Some((0, 0)));
}

#[test]
fn scroll_change_clears_manual_hunk_focus() {
    let changeset = changeset_with_hunks_at(PathBuf::from("/repo"), &[1, 2, 3]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(5);

    app.next_hunk();
    assert_eq!(app.manual_hunk_focus, Some((0, 1)));
    assert_eq!(app.focused_hunk_for_viewport(5), Some((0, 1)));

    app.set_scroll(0);

    assert_eq!(app.scroll, 0);
    assert_eq!(app.manual_hunk_focus, None);
    assert_eq!(app.focused_hunk_for_viewport(5), Some((0, 0)));
}

#[test]
fn model_rebuild_clears_manual_hunk_focus_when_scroll_is_unchanged() {
    let changeset = changeset_with_files(&["a.rs", "b.rs", "c.rs"]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(20);

    app.next_hunk();
    assert_eq!(app.scroll, 0);
    assert_eq!(app.manual_hunk_focus, Some((1, 0)));

    app.file_filter = "a.rs".to_owned();
    app.apply_filters(false);

    assert_eq!(app.scroll, 0);
    assert_eq!(app.selected_file, 0);
    assert_eq!(app.manual_hunk_focus, None);
    assert_eq!(app.focused_hunk_for_viewport(20), Some((0, 0)));
}

#[test]
fn model_rebuild_preserves_valid_manual_hunk_focus_when_scroll_is_unchanged() {
    let changeset = changeset_with_hunks_at(PathBuf::from("/repo"), &[1, 2, 3]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(20);

    app.next_hunk();
    app.next_hunk();
    assert_eq!(app.scroll, 0);
    assert_eq!(app.manual_hunk_focus, Some((0, 2)));

    app.file_filter = "file.rs".to_owned();
    app.apply_filters(false);

    assert_eq!(app.scroll, 0);
    assert_eq!(app.manual_hunk_focus, Some((0, 2)));
    assert_eq!(app.focused_hunk_for_viewport(20), Some((0, 2)));
}

#[test]
fn model_rebuild_preserves_valid_manual_hunk_focus_when_scroll_changes() {
    let changeset = changeset_with_files(&["a.rs", "b.rs", "c.rs"]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(3);

    app.select_file(2);
    assert!(app.scroll > 0);
    assert_eq!(app.manual_hunk_focus, Some((2, 0)));

    app.file_filter = "c.rs".to_owned();
    app.apply_filters(false);

    assert_eq!(app.scroll, 0);
    assert_eq!(app.selected_file, 2);
    assert_eq!(app.manual_hunk_focus, Some((2, 0)));
    assert_eq!(app.focused_hunk_for_viewport(3), Some((2, 0)));
}

#[test]
fn j_and_k_move_hunk_focus_when_diff_fits_viewport() {
    let changeset = changeset_with_hunks_at(PathBuf::from("/repo"), &[1, 2, 3]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(20);

    assert_eq!(app.max_scroll(), 0);
    assert_eq!(app.focused_hunk_for_viewport(20), Some((0, 0)));

    app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE))
        .expect("j should be handled");
    assert_eq!(app.scroll, 0);
    assert_eq!(app.focused_hunk_for_viewport(20), Some((0, 1)));

    app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE))
        .expect("j should be handled");
    assert_eq!(app.scroll, 0);
    assert_eq!(app.focused_hunk_for_viewport(20), Some((0, 2)));

    app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE))
        .expect("k should be handled");
    assert_eq!(app.focused_hunk_for_viewport(20), Some((0, 1)));

    app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE))
        .expect("k should be handled");
    assert_eq!(app.scroll, 0);
    assert_eq!(app.focused_hunk_for_viewport(20), Some((0, 0)));
}

#[test]
fn arrow_keys_move_hunk_focus_when_diff_fits_viewport() {
    assert_key_pair_moves_hunk_focus_when_diff_fits_viewport(KeyCode::Down, KeyCode::Up);
}

#[test]
fn page_keys_move_hunk_focus_when_diff_fits_viewport() {
    assert_key_pair_moves_hunk_focus_when_diff_fits_viewport(KeyCode::PageDown, KeyCode::PageUp);
    assert_key_pair_moves_hunk_focus_when_diff_fits_viewport(
        KeyCode::Char('d'),
        KeyCode::Char('u'),
    );
}

#[test]
fn arrow_keys_scroll_then_move_hunk_focus_at_edges_in_scrollable_diff() {
    assert_key_pair_scrolls_then_moves_hunk_focus_at_edges(KeyCode::Down, KeyCode::Up, 1);
}

#[test]
fn page_keys_scroll_then_move_hunk_focus_at_edges_in_scrollable_diff() {
    assert_key_pair_scrolls_then_moves_hunk_focus_at_edges(KeyCode::PageDown, KeyCode::PageUp, 20);
    assert_key_pair_scrolls_then_moves_hunk_focus_at_edges(
        KeyCode::Char('d'),
        KeyCode::Char('u'),
        20,
    );
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
    assert_eq!(app.scroll, 1);
    assert_eq!(app.manual_hunk_focus, None);

    app.set_scroll(0);
    let top_hunks = visible_hunk_keys(&app);
    assert!(top_hunks.len() >= 2);
    app.manual_hunk_focus = Some(top_hunks[1]);
    app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE))
        .expect("k should be handled");
    assert_eq!(app.scroll, 0);
    assert_eq!(app.selected_file, top_hunks[0].0);
    assert_eq!(
        app.focused_hunk_for_viewport(app.viewport_rows),
        Some(top_hunks[0])
    );

    while app.scroll < app.max_scroll() {
        app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE))
            .expect("j should be handled");
    }
    let bottom_scroll = app.scroll;
    let bottom_hunks = visible_hunk_keys(&app);
    assert!(bottom_hunks.len() >= 2);
    let previous = bottom_hunks[bottom_hunks.len() - 2];
    let next = bottom_hunks[bottom_hunks.len() - 1];
    app.manual_hunk_focus = Some(previous);

    app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE))
        .expect("j should be handled");

    assert_eq!(app.scroll, bottom_scroll);
    assert_eq!(app.selected_file, next.0);
    assert_eq!(app.focused_hunk_for_viewport(app.viewport_rows), Some(next));
}

#[test]
fn mouse_wheel_moves_hunk_focus_when_diff_fits_viewport() {
    let changeset = changeset_with_hunks_at(PathBuf::from("/repo"), &[1, 2, 3]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(20);

    assert_eq!(app.max_scroll(), 0);
    assert_eq!(app.focused_hunk_for_viewport(20), Some((0, 0)));

    for _ in 0..2 {
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        })
        .expect("mouse wheel should be handled");
        assert_eq!(app.focused_hunk_for_viewport(20), Some((0, 0)));
    }
    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: 0,
        row: 0,
        modifiers: KeyModifiers::NONE,
    })
    .expect("mouse wheel should be handled");
    assert_eq!(app.scroll, 0);
    assert_eq!(app.focused_hunk_for_viewport(20), Some((0, 1)));

    for _ in 0..2 {
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        })
        .expect("mouse wheel should be handled");
        assert_eq!(app.focused_hunk_for_viewport(20), Some((0, 1)));
    }
    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::ScrollUp,
        column: 0,
        row: 0,
        modifiers: KeyModifiers::NONE,
    })
    .expect("mouse wheel should be handled");
    assert_eq!(app.scroll, 0);
    assert_eq!(app.focused_hunk_for_viewport(20), Some((0, 0)));
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
        if app.scroll == app.max_scroll() {
            break;
        }
        mouse_scroll(&mut app, MouseEventKind::ScrollDown);
    }
    assert_eq!(app.scroll, app.max_scroll());

    let bottom_hunks = visible_hunk_keys(&app);
    assert!(bottom_hunks.len() >= 2);
    let previous = bottom_hunks[bottom_hunks.len() - 2];
    let next = bottom_hunks[bottom_hunks.len() - 1];
    app.manual_hunk_focus = Some(previous);

    for _ in 0..2 {
        mouse_scroll(&mut app, MouseEventKind::ScrollDown);
        assert_eq!(app.scroll, app.max_scroll());
        assert_eq!(
            app.focused_hunk_for_viewport(app.viewport_rows),
            Some(previous)
        );
    }
    mouse_scroll(&mut app, MouseEventKind::ScrollDown);

    assert_eq!(app.scroll, app.max_scroll());
    assert_eq!(app.focused_hunk_for_viewport(app.viewport_rows), Some(next));
}

#[test]
fn bracket_hunk_navigation_uses_focused_hunk_in_scrollable_diff() {
    let changeset = changeset_with_hunks_at(PathBuf::from("/repo"), &[1, 2, 3]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(5);

    assert!(app.max_scroll() > 0);
    assert_eq!(app.focused_hunk_for_viewport(5), Some((0, 0)));

    app.next_hunk();
    assert_eq!(app.focused_hunk_for_viewport(5), Some((0, 1)));

    app.next_hunk();
    assert_eq!(app.focused_hunk_for_viewport(5), Some((0, 2)));

    app.previous_hunk();
    assert_eq!(app.focused_hunk_for_viewport(5), Some((0, 1)));
}

#[test]
fn bracket_hunk_navigation_focuses_visible_hunk_when_scroll_is_clamped() {
    let changeset = changeset_with_files(&[
        "a.rs", "b.rs", "c.rs", "d.rs", "e.rs", "f.rs", "g.rs", "h.rs", "i.rs", "j.rs",
    ]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(20);

    assert!(app.max_scroll() > 0);
    assert_eq!(app.scroll, 0);
    assert_eq!(app.focused_hunk_for_viewport(20), Some((0, 0)));

    app.next_hunk();
    assert_eq!(app.scroll, 0);
    assert_eq!(app.focused_hunk_for_viewport(20), Some((1, 0)));

    app.next_hunk();
    assert_eq!(app.scroll, 0);
    assert_eq!(app.focused_hunk_for_viewport(20), Some((2, 0)));

    app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE))
        .expect("k should be handled");
    assert_eq!(app.scroll, 0);
    assert_eq!(app.focused_hunk_for_viewport(20), Some((1, 0)));
}

#[test]
fn bracket_hunk_navigation_can_return_to_first_hunk_in_short_scrollable_diff() {
    let changeset = changeset_with_files(&["a.rs", "b.rs", "c.rs", "d.rs", "e.rs", "f.rs"]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(20);

    assert!(app.max_scroll() > 0);
    assert_eq!(app.focused_hunk_for_viewport(20), Some((0, 0)));

    for _ in 0..10 {
        app.previous_hunk();
    }
    assert_eq!(app.scroll, 0);
    assert_eq!(app.focused_hunk_for_viewport(20), Some((0, 0)));

    app.next_hunk();
    assert_eq!(app.focused_hunk_for_viewport(20), Some((1, 0)));

    app.previous_hunk();
    assert_eq!(app.focused_hunk_for_viewport(20), Some((0, 0)));
}

#[test]
fn bracket_hunk_navigation_centers_hunk_that_fits_viewport() {
    let changeset =
        changeset_with_hunk_line_counts(PathBuf::from("/repo"), &[(1, 8), (20, 4), (40, 10)]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(9);
    let range = app
        .model
        .hunk_row_range(0, 1)
        .expect("target hunk should have rows");

    app.next_hunk();

    let hunk_center = range
        .start
        .saturating_add(range.end.saturating_sub(range.start).saturating_sub(1) / 2);
    assert_eq!(
        app.scroll + viewport_center_offset(app.viewport_rows),
        hunk_center
    );
    assert_eq!(app.focused_hunk_for_viewport(9), Some((0, 1)));
}

#[test]
fn bracket_hunk_navigation_places_oversized_hunk_at_top() {
    let changeset =
        changeset_with_hunk_line_counts(PathBuf::from("/repo"), &[(1, 2), (20, 20), (60, 2)]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(9);
    let range = app
        .model
        .hunk_row_range(0, 1)
        .expect("target hunk should have rows");

    app.next_hunk();

    assert_eq!(app.scroll, range.start - 1);
    assert!(matches!(
        app.model.row(app.scroll),
        Some(UiRow::Collapsed { .. })
    ));
    assert_eq!(
        app.model.row(app.scroll + 1),
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
        .model
        .hunk_row_range(0, 1)
        .expect("target hunk should have rows");
    let range_start = range.start - 1;
    let range_end = range.end + 1;
    assert!(matches!(
        app.model.row(range_start),
        Some(UiRow::Collapsed { .. })
    ));
    assert!(matches!(
        app.model.row(range.end),
        Some(UiRow::Collapsed { .. })
    ));

    app.next_hunk();

    let center = range_start.saturating_add(range_end.saturating_sub(range_start + 1) / 2);
    assert_eq!(
        app.scroll + viewport_center_offset(app.viewport_rows),
        center
    );
    assert_eq!(app.focused_hunk_for_viewport(9), Some((0, 1)));
}

#[test]
fn hunk_navigation_keeps_target_visible_after_expanded_pre_hunk_context() {
    let changeset = changeset_with_hunks_at(PathBuf::from("/repo"), &[50, 100]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(9);
    app.context_expansions.insert(
        ContextKey { file: 0, hunk: 1 },
        default_context_expand_step(),
    );
    app.model = UiModel::new(&app.changeset, app.layout, &app.context_expansions);
    let hunk_row = app
        .model
        .hunk_start_row(0, 1)
        .expect("target hunk should have a header row");
    assert!(matches!(
        app.model.row(hunk_row - 1),
        Some(UiRow::Collapsed { .. })
    ));
    assert!(matches!(
        app.model.row(hunk_row - 2),
        Some(UiRow::ContextLine { .. })
    ));

    app.next_hunk();

    assert!(hunk_row >= app.scroll);
    assert!(hunk_row < app.scroll + app.viewport_rows);
    assert_eq!(app.focused_hunk_for_viewport(9), Some((0, 1)));
}

#[test]
fn hunk_navigation_includes_expanded_context_before_hunk() {
    let changeset = changeset_with_hunks_at(PathBuf::from("/repo"), &[50, 100, 150]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(8);
    app.context_expansions
        .insert(ContextKey { file: 0, hunk: 1 }, 3);
    app.model = UiModel::new(&app.changeset, app.layout, &app.context_expansions);
    let context_start = app
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
        .model
        .hunk_start_row(0, 1)
        .expect("target hunk should have a header row");

    app.next_hunk();

    assert_eq!(app.scroll, context_start);
    assert!(hunk_row >= app.scroll);
    assert!(hunk_row < app.scroll + app.viewport_rows);
    assert_eq!(
        app.model.row(app.scroll),
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
        .model
        .hunk_row_range(1, 0)
        .expect("selected file hunk should have rows");
    let hunk_center = range
        .start
        .saturating_add(range.end.saturating_sub(range.start).saturating_sub(1) / 2);
    assert_eq!(
        app.scroll + viewport_center_offset(app.viewport_rows),
        hunk_center
    );
    assert_eq!(app.selected_file, 1);
    assert_eq!(app.focused_hunk_for_viewport(7), Some((1, 0)));
}

#[test]
fn j_and_k_file_navigation_focuses_first_hunk_and_updates_selected_file() {
    let changeset = changeset_with_files(&["a.rs", "b.rs", "c.rs"]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(7);

    app.handle_key(KeyEvent::new(KeyCode::Char('J'), KeyModifiers::NONE))
        .expect("J should be handled");

    assert_eq!(app.selected_file, 1);
    assert_eq!(app.focused_hunk_for_viewport(7), Some((1, 0)));

    app.handle_key(KeyEvent::new(KeyCode::Char('J'), KeyModifiers::NONE))
        .expect("J should be handled");
    assert_eq!(app.selected_file, 2);
    assert_eq!(app.focused_hunk_for_viewport(7), Some((2, 0)));

    app.handle_key(KeyEvent::new(KeyCode::Char('K'), KeyModifiers::NONE))
        .expect("K should be handled");
    assert_eq!(app.selected_file, 1);
    assert_eq!(app.focused_hunk_for_viewport(7), Some((1, 0)));
}

#[test]
fn n_and_p_do_not_navigate_without_grep_filter() {
    let changeset = changeset_with_files(&["a.rs", "b.rs", "c.rs"]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(7);

    app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE))
        .expect("n should be ignored without grep");
    assert_eq!(app.selected_file, 0);
    assert_eq!(app.focused_hunk_for_viewport(7), Some((0, 0)));

    app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE))
        .expect("p should be ignored without grep");
    assert_eq!(app.selected_file, 0);
    assert_eq!(app.focused_hunk_for_viewport(7), Some((0, 0)));
}

#[test]
fn selecting_current_file_preserves_focused_hunk() {
    let changeset = changeset_with_hunks_at(PathBuf::from("/repo"), &[1, 2, 3]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(20);

    app.next_hunk();
    assert_eq!(app.manual_hunk_focus, Some((0, 1)));

    app.select_file(0);

    assert_eq!(app.selected_file, 0);
    assert_eq!(app.manual_hunk_focus, Some((0, 1)));
    assert_eq!(app.focused_hunk_for_viewport(20), Some((0, 1)));
}

#[test]
fn hunk_navigation_keeps_adjacent_file_header_with_oversized_hunk() {
    let changeset = changeset_with_hunk_line_counts(PathBuf::from("/repo"), &[(1, 20)]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(9);
    app.set_scroll(5);

    let hunk_row = app
        .model
        .hunk_start_row(0, 0)
        .expect("target hunk should have a header row");
    app.focus_hunk_row(hunk_row);

    assert_eq!(app.scroll, app.model.file_start_row(0).unwrap());
    assert_eq!(app.model.row(app.scroll), Some(UiRow::FileHeader(0)));
    assert_eq!(app.focused_hunk_for_viewport(9), Some((0, 0)));
}

#[test]
fn hunk_navigation_keeps_file_header_before_collapsed_context() {
    let changeset = changeset_with_hunk_line_counts(PathBuf::from("/repo"), &[(233, 20)]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(9);
    app.set_scroll(5);

    let hunk_row = app
        .model
        .hunk_start_row(0, 0)
        .expect("target hunk should have a header row");
    app.focus_hunk_row(hunk_row);

    assert_eq!(app.scroll, app.model.file_start_row(0).unwrap());
    assert_eq!(app.model.row(app.scroll), Some(UiRow::FileHeader(0)));
    assert!(matches!(
        app.model.row(app.scroll + 1),
        Some(UiRow::Collapsed { .. })
    ));
    assert_eq!(
        app.model.row(app.scroll + 2),
        Some(UiRow::HunkHeader { file: 0, hunk: 0 })
    );
    assert_eq!(app.focused_hunk_for_viewport(9), Some((0, 0)));
}

#[test]
fn focused_hunk_editor_target_uses_new_path_and_visible_line() {
    let repo = PathBuf::from("/repo");
    let changeset = changeset_with_context_lines_at(repo.clone(), 1, 100);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(11);
    app.set_scroll(40);

    assert_eq!(
        app.focused_hunk_editor_target(),
        Some(EditorTarget {
            path: repo.join("file.rs"),
            line: 44,
        })
    );
}

#[test]
fn focused_hunk_editor_target_uses_manual_focus_when_diff_fits_viewport() {
    let repo = PathBuf::from("/repo");
    let changeset = changeset_with_hunks_at(repo.clone(), &[10, 20, 30]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(20);

    app.next_hunk();
    app.next_hunk();

    assert_eq!(
        app.focused_hunk_editor_target(),
        Some(EditorTarget {
            path: repo.join("file.rs"),
            line: 30,
        })
    );
}

#[test]
fn replace_loaded_diff_clears_manual_hunk_focus() {
    let repo = PathBuf::from("/repo");
    let changeset = changeset_with_hunks_at(repo.clone(), &[10, 20, 30]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(20);

    app.next_hunk();
    app.next_hunk();
    assert_eq!(app.manual_hunk_focus, Some((0, 2)));
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

    assert_eq!(app.manual_hunk_focus, None);
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
fn ctrl_g_without_editable_target_does_not_scroll_to_top() {
    let mut changeset = changeset_with_hunk_at(PathBuf::from("/repo"), 20);
    changeset.files[0].new_path = None;
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(1);
    app.set_scroll(1);

    let should_quit = app
        .handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL))
        .expect("Ctrl-G should be handled");

    assert!(!should_quit);
    assert_eq!(app.scroll, 1);
    assert_eq!(app.toasts.latest_text(), Some("no editable focused hunk"));
}

#[test]
fn ctrl_g_without_editor_launch_preserves_queued_events() {
    let mut changeset = changeset_with_hunk_at(PathBuf::from("/repo"), 20);
    changeset.files[0].new_path = None;
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
    .expect("Ctrl-G should be handled");

    assert!(!should_quit);
    assert_eq!(events.try_read().unwrap(), Some(queued_quit));
}

#[test]
fn debug_notifications_emit_terminal_event_toasts() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.toasts = Toasts::new(NotificationSettings {
        mode: NotificationMode::Debug,
        corner: ToastCorner::TopRight,
        timeout_ms: 1_500,
        max_visible: 3,
    });
    let (_tx, rx) = mpsc::channel(1);
    let mut events = crate::event_reader::TerminalEventReader::from_receiver(rx);
    let mut live_diff = None;

    let should_quit = handle_event(
        &mut app,
        Event::Resize(120, 40),
        &mut live_diff,
        &mut events,
    )
    .expect("resize should be handled");

    assert!(!should_quit);
    assert_eq!(app.toasts.latest_text(), Some("event: resize 120x40"));
}

#[test]
fn editable_hunk_without_configured_editor_sets_notice() {
    let changeset = changeset_with_hunk_at(PathBuf::from("/repo"), 20);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(5);

    assert!(!app.prepare_focused_hunk_editor_for_test(None));
    assert_eq!(
        app.toasts.latest_text(),
        Some("set $VISUAL, $GIT_EDITOR, or $EDITOR to edit focused hunk")
    );
    assert!(app.error_log.is_none());
    assert!(app.dirty);
}

#[test]
fn post_editor_quit_key_guard_ignores_only_transient_quit_keys() {
    let changeset = changeset_with_hunk_at(PathBuf::from("/repo"), 20);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    let now = Instant::now();
    app.post_editor_quit_key_ignore_until = Some(now + Duration::from_millis(250));

    assert!(app.ignore_post_editor_quit_key(
        KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        now
    ));
    assert!(
        app.ignore_post_editor_quit_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE), now)
    );
    assert!(!app.ignore_post_editor_quit_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), now));
    assert!(!app.ignore_post_editor_quit_key(
        KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL),
        now
    ));
    assert!(!app.ignore_post_editor_quit_key(
        KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
        now + Duration::from_millis(251)
    ));
}

#[test]
fn post_editor_quit_key_guard_swallows_configured_single_quit_key_event() {
    let changeset = changeset_with_hunk_at(PathBuf::from("/repo"), 20);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.keymap = Keymap::parse(
        r#"
        [keymap.global]
        quit = "q"
        "#,
    )
    .expect("keymap should parse");
    app.post_editor_quit_key_ignore_until = Some(Instant::now() + Duration::from_millis(250));

    let should_quit = handle_test_key_event(
        &mut app,
        KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
    );

    assert!(!should_quit);
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

    app.options.source = DiffSource::Base("main".to_owned());
    assert_eq!(
        app.editor_reload_behavior(true, Some(Path::new("src/file.rs"))),
        EditorReloadBehavior::ScopedAsync
    );

    app.options.source = DiffSource::Branch {
        base: "main".to_owned(),
        head: "feature".to_owned(),
    };
    assert_eq!(
        app.editor_reload_behavior(true, Some(Path::new("src/file.rs"))),
        EditorReloadBehavior::None
    );
}

#[test]
fn queue_editor_scoped_reload_marks_dirty_for_terminal_repaint() {
    let changeset = changeset_with_hunk_at(PathBuf::from("/repo"), 20);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.dirty = false;

    app.queue_editor_scoped_reload(EditorReloadRequest {
        path: PathBuf::from("src/file.rs"),
        pathspecs: vec![PathBuf::from("src/file.rs")],
    });

    assert!(app.dirty);
    assert!(app.pending_editor_reload.is_some());
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
fn syntax_settings_load_error_falls_back_with_visible_diagnostic() {
    let (settings, error_log) =
        syntax_settings_for_diff(Err(MarkError::Usage("bad syntax config".to_owned())));

    assert_eq!(settings, SyntaxSettings::default());
    let error_log = error_log.expect("settings error should be visible");
    assert!(error_log.contains("syntax settings ignored"));
    assert!(error_log.contains("bad syntax config"));
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
fn syntax_runtime_start_error_disables_syntax_with_visible_diagnostic() {
    let mut error_log = Some("syntax settings ignored: bad theme".to_owned());

    let syntax = syntax_runtime_for_diff(
        Err(MarkError::Usage("bad tree-sitter config".to_owned())),
        &mut error_log,
    );

    assert!(syntax.is_none());
    assert_eq!(
        error_log.as_deref(),
        Some("syntax settings ignored: bad theme\nsyntax disabled: bad tree-sitter config")
    );
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
    assert_eq!(app.changeset.files[0].hunks[0].lines[0].text, "line 0");
    assert_eq!(app.changeset.files[1].hunks[0].lines[0].text, "line 0");
    assert_eq!(app.changeset.files[2].hunks[0].lines[0].text, "line 2");
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
fn file_separator_line_draws_rule_across_full_width() {
    let theme = DiffTheme::default();
    let line = file_separator_line(DiffLayoutMode::Unified, 24, theme);
    let text = line_text(&line);

    assert_eq!(text.width(), 24);
    assert_eq!(text, "────────────────────────");
    assert_eq!(line.spans[0].style.bg, Some(base_bg(theme)));
    assert_eq!(line.spans[0].style.fg, Some(theme.empty_diff));
}

#[test]
fn ui_model_expands_context_before_hunk_from_nearest_lines() {
    let step = default_context_expand_step();
    let changeset = changeset_with_hunk_at(PathBuf::from("/repo"), 50);
    let mut expansions = HashMap::new();

    let model = UiModel::new(&changeset, DiffLayoutMode::Unified, &expansions);
    assert_eq!(
        model.row(1),
        Some(UiRow::Collapsed {
            file: 0,
            hunk: 0,
            old_start: 1,
            new_start: 1,
            lines: 49,
            expanded: 0,
        })
    );

    expansions.insert(ContextKey { file: 0, hunk: 0 }, step);
    let model = UiModel::new(&changeset, DiffLayoutMode::Unified, &expansions);

    assert_eq!(
        model.row(1),
        Some(UiRow::Collapsed {
            file: 0,
            hunk: 0,
            old_start: 1,
            new_start: 1,
            lines: 29,
            expanded: step,
        })
    );
    assert_eq!(
        model.row(2),
        Some(UiRow::ContextLine {
            file: 0,
            old_line: 30,
            new_line: 30,
        })
    );
    assert_eq!(
        model.row(22),
        Some(UiRow::ContextHide {
            file: 0,
            hunk: 0,
            lines: step,
        })
    );
    assert_eq!(model.row(23), Some(UiRow::HunkHeader { file: 0, hunk: 0 }));
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
            old_start: 51,
            new_start: 51,
            lines: 29,
            expanded: step,
        })
    );
    assert_eq!(model.row(26), Some(UiRow::HunkHeader { file: 0, hunk: 1 }));
}

#[test]
fn full_context_expansion_config_shows_all_remaining_lines() {
    let repo = temp_test_dir("full-context-expansion");
    fs::create_dir_all(&repo).expect("repo directory should be created");
    let text = (1..=80)
        .map(|line| format!("line {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(repo.join("file.rs"), text).expect("context file should be written");
    let changeset = changeset_with_hunk_at(repo, 50);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.theme.diff.context_expansion = DiffContextExpansion::Full;

    assert_eq!(app.context_expand_count(49), 49);
    assert!(app.expand_context_at_row(1));
    assert_eq!(
        app.context_expansions.get(&ContextKey { file: 0, hunk: 0 }),
        Some(&49)
    );
    assert_eq!(
        app.model.row(1),
        Some(UiRow::ContextLine {
            file: 0,
            old_line: 1,
            new_line: 1,
        })
    );
    assert_eq!(
        app.model.row(50),
        Some(UiRow::ContextHide {
            file: 0,
            hunk: 0,
            lines: 49,
        })
    );
}

#[test]
fn clicking_collapsed_context_expands_more_on_each_click() {
    let step = default_context_expand_step();
    let repo = temp_test_dir("expand-context");
    fs::create_dir_all(&repo).expect("repo directory should be created");
    let text = (1..=80)
        .map(|line| format!("line {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(repo.join("file.rs"), text).expect("context file should be written");
    let changeset = changeset_with_hunk_at(repo, 50);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);

    assert!(app.expand_context_at_row(1));
    assert_eq!(
        app.context_expansions.get(&ContextKey { file: 0, hunk: 0 }),
        Some(&step)
    );
    assert_eq!(
        app.model.row(1),
        Some(UiRow::Collapsed {
            file: 0,
            hunk: 0,
            old_start: 1,
            new_start: 1,
            lines: 29,
            expanded: step,
        })
    );
    let row = app.model.row(2).expect("expanded context row should exist");
    assert_eq!(
        row,
        UiRow::ContextLine {
            file: 0,
            old_line: 30,
            new_line: 30,
        }
    );
    let rendered = render_row(&mut app, 2, row, 80);
    assert!(line_text(&rendered).contains("line 30"));

    assert!(app.expand_context_at_row(1));
    assert_eq!(
        app.context_expansions.get(&ContextKey { file: 0, hunk: 0 }),
        Some(&(step * 2))
    );
    assert_eq!(
        app.model.row(2),
        Some(UiRow::ContextLine {
            file: 0,
            old_line: 10,
            new_line: 10,
        })
    );

    assert!(app.hide_context(0, 0));
    assert!(
        !app.context_expansions
            .contains_key(&ContextKey { file: 0, hunk: 0 })
    );
    assert_eq!(
        app.model.row(1),
        Some(UiRow::Collapsed {
            file: 0,
            hunk: 0,
            old_start: 1,
            new_start: 1,
            lines: 49,
            expanded: 0,
        })
    );
}

#[test]
fn clicking_collapsed_context_between_hunks_expands_downward() {
    let step = default_context_expand_step();
    let repo = temp_test_dir("expand-context-downward");
    fs::create_dir_all(&repo).expect("repo directory should be created");
    let text = (1..=120)
        .map(|line| format!("line {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(repo.join("file.rs"), text).expect("context file should be written");
    let changeset = changeset_with_hunks_at(repo, &[50, 100]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);

    assert!(app.expand_context_at_row(4));
    assert_eq!(
        app.context_expansions.get(&ContextKey { file: 0, hunk: 1 }),
        Some(&step)
    );
    assert_eq!(
        app.model.row(4),
        Some(UiRow::ContextHide {
            file: 0,
            hunk: 1,
            lines: step,
        })
    );
    let row = app.model.row(5).expect("expanded context row should exist");
    assert_eq!(
        row,
        UiRow::ContextLine {
            file: 0,
            old_line: 51,
            new_line: 51,
        }
    );
    let rendered = render_row(&mut app, 5, row, 80);
    assert!(line_text(&rendered).contains("line 51"));

    assert!(app.expand_context_at_row(25));
    assert_eq!(
        app.context_expansions.get(&ContextKey { file: 0, hunk: 1 }),
        Some(&(step * 2))
    );
    assert_eq!(
        app.model.row(25),
        Some(UiRow::ContextLine {
            file: 0,
            old_line: 71,
            new_line: 71,
        })
    );

    assert!(app.hide_context(0, 1));
    assert!(
        !app.context_expansions
            .contains_key(&ContextKey { file: 0, hunk: 1 })
    );
    assert_eq!(
        app.model.row(4),
        Some(UiRow::Collapsed {
            file: 0,
            hunk: 1,
            old_start: 51,
            new_start: 51,
            lines: 49,
            expanded: 0,
        })
    );
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
    let line = context_show_line(20, false, 72, theme);
    let text = line_text(&line);

    assert_eq!(text.width(), 72);
    assert!(text.starts_with(DIFF_INDICATOR));
    assert!(text.contains("▾ show 20 lines"));
    assert_eq!(line.spans[1].style.fg, Some(theme.muted));
    assert_eq!(
        line.spans[0].style.bg,
        Some(line_gutter_bg(DiffLineKind::Meta, theme))
    );

    let hide = context_hide_line(20, 24, theme);
    let hide_text = line_text(&hide);
    assert!(hide_text.contains("▴ hide 20 lines"));
    assert_eq!(hide.spans[1].style.fg, Some(theme.muted));
}

#[test]
fn responsive_layout_preserves_valid_horizontal_scroll() {
    let long_line = "a".repeat(120);
    let changeset = changeset_with_line_text(&long_line);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_width(80);
    app.set_horizontal_scroll(40);

    app.apply_responsive_layout(MIN_SPLIT_WIDTH);

    assert_eq!(app.layout, DiffLayoutMode::Split);
    assert_eq!(app.horizontal_scroll, 40);
}

#[test]
fn responsive_layout_clamps_horizontal_scroll_without_layout_change() {
    let long_line = "a".repeat(100);
    let changeset = changeset_with_line_text(&long_line);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Split);

    app.apply_responsive_layout(MIN_SPLIT_WIDTH);
    assert_eq!(app.layout, DiffLayoutMode::Split);
    app.set_horizontal_scroll(usize::MAX);
    let previous_scroll = app.horizontal_scroll;

    app.apply_responsive_layout(MIN_SPLIT_WIDTH + 40);

    assert_eq!(app.layout, DiffLayoutMode::Split);
    assert!(app.max_horizontal_scroll() < previous_scroll);
    assert_eq!(app.horizontal_scroll, app.max_horizontal_scroll());
}

#[test]
fn responsive_layout_preserves_manual_unified_choice_on_wide_resize() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Split);

    app.toggle_layout();
    assert_eq!(app.layout, DiffLayoutMode::Unified);

    app.apply_responsive_layout(MIN_SPLIT_WIDTH + 40);

    assert_eq!(app.layout, DiffLayoutMode::Unified);
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
    assert_eq!(app.layout, DiffLayoutMode::Unified);
    assert_eq!(app.layout_override, Some(DiffLayoutMode::Unified));

    app.apply_responsive_layout(MIN_SPLIT_WIDTH + 40);

    assert_eq!(app.layout, DiffLayoutMode::Unified);
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

    assert_eq!(app.layout_override, None);
    assert_eq!(app.layout, DiffLayoutMode::Split);

    app.apply_responsive_layout(MIN_SPLIT_WIDTH - 1);
    assert_eq!(app.layout, DiffLayoutMode::Unified);

    app.apply_responsive_layout(MIN_SPLIT_WIDTH + 40);
    assert_eq!(app.layout, DiffLayoutMode::Split);
}

#[test]
fn responsive_layout_preserves_manual_split_choice_on_narrow_resize() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);

    app.toggle_layout();
    assert_eq!(app.layout, DiffLayoutMode::Split);
    assert_eq!(app.layout_override, Some(DiffLayoutMode::Split));

    app.apply_responsive_layout(MIN_SPLIT_WIDTH - 1);
    assert_eq!(app.layout, DiffLayoutMode::Split);
    assert_eq!(app.layout_override, Some(DiffLayoutMode::Split));

    app.apply_responsive_layout(MIN_SPLIT_WIDTH + 40);
    assert_eq!(app.layout, DiffLayoutMode::Split);
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

    assert_eq!(app.layout, DiffLayoutMode::Split);
    assert_eq!(app.layout_override, Some(DiffLayoutMode::Split));
    assert_eq!(app.options_menu_draft.layout, LayoutSetting::Split);

    app.apply_responsive_layout(MIN_SPLIT_WIDTH - 1);

    assert_eq!(app.layout, DiffLayoutMode::Split);
    assert_eq!(app.layout_override, Some(DiffLayoutMode::Split));
}

#[test]
fn b_key_toggles_file_sidebar() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);

    assert!(!app.file_sidebar_open);

    let should_quit = app
        .handle_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE))
        .expect("b should be handled");
    assert!(!should_quit);
    assert!(app.file_sidebar_open);

    app.handle_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE))
        .expect("b should be handled");
    assert!(!app.file_sidebar_open);
}

#[test]
fn b_clears_file_sidebar_resize_state() {
    let changeset = changeset_with_files(&["a.rs"]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.file_sidebar_open = true;
    app.file_sidebar_render_width = 30;
    app.viewport_width = 70;

    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 29,
        row: 1,
        modifiers: KeyModifiers::NONE,
    })
    .expect("resize should start");

    assert!(app.file_sidebar_resizing);
    assert_eq!(app.file_sidebar_width, Some(30));

    app.handle_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE))
        .expect("b should be handled");

    assert!(!app.file_sidebar_open);
    assert!(!app.file_sidebar_resizing);

    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Drag(MouseButton::Left),
        column: 49,
        row: 1,
        modifiers: KeyModifiers::NONE,
    })
    .expect("drag should no longer resize after sidebar closes");

    assert_eq!(app.file_sidebar_width, Some(30));
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

    assert!(app.live_reload_invalidated);
    assert!(app.live_reload_pending);
    app.dirty = false;

    reload_tx
        .try_send(LiveDiffReload::Loaded(Ok(changeset)))
        .expect("loaded reload should send");
    drain_live_reloads(&mut app, Some(&mut reload_rx));

    assert!(!app.live_reload_invalidated);
    assert!(!app.live_reload_pending);
    assert!(app.dirty);
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
    assert!(!app.diff_cache.is_empty());

    app.mark_live_reload_invalidated();

    assert!(app.live_reload_invalidated);
    assert!(!app.live_reload_pending);
    assert!(app.diff_cache.is_empty());

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

    assert!(app.pending_diff_load.is_some());
    assert_eq!(app.changeset.files[0].display_path(), "src/lib.rs");

    for _ in 0..100 {
        app.drain_pending_diff_load();
        if app.pending_diff_load.is_none() {
            break;
        }
        thread::sleep(Duration::from_millis(1));
    }

    assert!(app.pending_diff_load.is_none());
    assert_eq!(app.changeset.files[0].display_path(), "other.rs");
    assert_eq!(app.options.source, DiffSource::Patch(PatchSource::Text {
        label: "test patch".to_owned(),
        patch: Arc::<[u8]>::from(
            b"diff --git a/other.rs b/other.rs\n--- a/other.rs\n+++ b/other.rs\n@@ -1 +1 @@\n-old\n+new\n"
                .as_slice(),
        ),
    }));
}

#[test]
fn f_key_filters_files_and_escape_clears_filter() {
    let changeset = changeset_with_files(&["src/lib.rs", "README.md", "mark-tui/src/lib.rs"]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);

    app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE))
        .expect("f should open file filter");
    assert!(line_text(&filter_bar_line(&app, 40)).starts_with(&format!("@{INPUT_CURSOR}")));
    let generation_before_input = app.generation;
    for character in "tui".chars() {
        app.handle_key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE))
            .expect("file filter input should be handled");
    }

    assert_eq!(app.file_filter, "tui");
    assert_eq!(app.file_filter_input, "tui");
    assert_eq!(visible_paths(&app), vec!["mark-tui/src/lib.rs"]);
    assert_eq!(app.generation, generation_before_input);
    assert!(line_text(&filter_bar_line(&app, 40)).starts_with("@tui"));

    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .expect("enter should keep file filter");

    assert_eq!(app.generation, generation_before_input);
    assert_eq!(app.file_filter, "tui");
    assert_eq!(visible_paths(&app), vec!["mark-tui/src/lib.rs"]);
    assert_eq!(statusline_file_count_label(&app), "1/3 files");
    assert!(line_text(&filter_bar_line(&app, 40)).starts_with("@tui"));
    assert!(!line_text(&statusline_header_line(&app, 120)).contains("f:tui"));
    assert!(app.filter_input.is_none());
    assert!(filter_bar_visible(&app));
    assert!(line_text(&filter_bar_line(&app, 40)).starts_with("@tui"));

    app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE))
        .expect("f should reopen file filter");
    assert_eq!(app.file_filter, "");
    assert_eq!(app.file_filter_input, "");
    assert_eq!(
        visible_paths(&app),
        vec!["src/lib.rs", "README.md", "mark-tui/src/lib.rs"]
    );

    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
        .expect("escape should clear file filter");

    assert_eq!(app.file_filter, "");
    assert_eq!(
        visible_paths(&app),
        vec!["src/lib.rs", "README.md", "mark-tui/src/lib.rs"]
    );
    assert!(app.filter_input.is_none());
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

    assert_eq!(app.grep_filter, "line 1");
    assert_eq!(app.grep_filter_input, "line 1");
    assert_eq!(visible_paths(&app), vec!["b.rs"]);
    assert!(line_text(&filter_bar_line(&app, 40)).starts_with("/line 1"));

    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .expect("enter should keep grep filter");

    assert_eq!(app.grep_filter, "line 1");
    assert_eq!(visible_paths(&app), vec!["b.rs"]);
    assert_eq!(app.grep_matches.len(), 1);
    assert_eq!(app.current_grep_match_row(), Some(2));
    assert!(line_text(&filter_bar_line(&app, 40)).starts_with("/line 1"));
    assert!(!line_text(&statusline_header_line(&app, 120)).contains("/:line 1"));
    assert!(app.filter_input.is_none());
    assert!(filter_bar_visible(&app));
    assert!(line_text(&filter_bar_line(&app, 40)).starts_with("/line 1"));

    app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE))
        .expect("slash should reopen grep filter");
    assert_eq!(app.grep_filter, "");
    assert_eq!(app.grep_filter_input, "");
    assert_eq!(visible_paths(&app), vec!["a.rs", "b.rs", "c.rs"]);

    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
        .expect("escape should clear grep filter");

    assert_eq!(app.grep_filter, "");
    assert!(app.grep_matches.is_empty());
    assert_eq!(app.current_grep_match_row(), None);
    assert_eq!(visible_paths(&app), vec!["a.rs", "b.rs", "c.rs"]);
    assert!(app.filter_input.is_none());

    let selected_file = app.selected_file;
    let scroll = app.scroll;
    app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE))
        .expect("n should not navigate after grep is cleared");
    app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE))
        .expect("p should not navigate after grep is cleared");
    assert_eq!(app.selected_file, selected_file);
    assert_eq!(app.scroll, scroll);
}

#[test]
fn slash_does_not_match_file_paths() {
    let changeset = changeset_with_files(&["unique_name.rs", "other.rs"]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);

    app.grep_filter = "unique_name".to_owned();
    app.apply_filters(true);

    assert!(visible_paths(&app).is_empty());
    assert!(app.grep_matches.is_empty());
}

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

    let addition = DiffLine {
        kind: DiffLineKind::Addition,
        old_line: None,
        new_line: Some(1),
        text: "changed".to_owned(),
    };
    let prefixed = TextMatcher::new("+changed").expect("matcher should be created");
    assert!(diff_line_grep_text_matches(&addition, &prefixed));
}

#[test]
fn file_filter_and_grep_filter_compose_and_render_together() {
    let changeset = changeset_with_files(&["a.rs", "b.rs", "c.rs"]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);

    app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE))
        .expect("f should open file filter");
    app.handle_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE))
        .expect("file filter input should be handled");
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .expect("enter should keep file filter");

    app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE))
        .expect("slash should open grep filter");
    for character in "line 1".chars() {
        app.handle_key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE))
            .expect("grep filter input should be handled");
    }
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .expect("enter should keep grep filter");

    assert_eq!(visible_paths(&app), vec!["b.rs"]);
    assert!(line_text(&filter_bar_line(&app, 80)).starts_with("@b  /line 1"));

    let should_quit = app
        .handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
        .expect("escape should clear active filters");
    assert!(!should_quit);
    assert_eq!(app.file_filter, "");
    assert_eq!(app.grep_filter, "");
    assert_eq!(visible_paths(&app), vec!["a.rs", "b.rs", "c.rs"]);
    assert!(!filter_bar_visible(&app));
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
    assert_eq!(app.viewport_rows, 9);

    app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE))
        .expect("f should open file filter");
    terminal
        .draw(|frame| crate::render::draw(frame, &mut app))
        .expect("file filter draw should succeed");
    assert_eq!(app.viewport_rows, 8);

    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
        .expect("escape should close file filter");
    terminal
        .draw(|frame| crate::render::draw(frame, &mut app))
        .expect("draw after closing filter should succeed");
    assert_eq!(app.viewport_rows, 9);

    app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE))
        .expect("slash should open grep filter");
    terminal
        .draw(|frame| crate::render::draw(frame, &mut app))
        .expect("grep filter draw should succeed");
    assert_eq!(app.viewport_rows, 8);
}

#[test]
fn file_filter_edit_with_active_grep_preserves_current_grep_match() {
    let changeset = changeset_with_files(&["a.rs", "b.rs", "c.rs"]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(5);
    app.grep_filter = "line".to_owned();
    app.apply_filters(true);
    app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE))
        .expect("n should move to next grep match");
    let scroll_before_file_filter = app.scroll;

    app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE))
        .expect("f should open file filter");
    for character in "rs".chars() {
        app.handle_key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE))
            .expect("file filter input should be handled");
    }

    assert_eq!(visible_paths(&app), vec!["a.rs", "b.rs", "c.rs"]);
    assert_eq!(app.current_grep_match_row(), Some(6));
    assert_eq!(app.scroll, scroll_before_file_filter);
}

#[test]
fn n_and_p_navigate_grep_matches_when_grep_filter_is_active() {
    let changeset = changeset_with_files(&["a.rs", "b.rs", "c.rs"]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(5);
    app.grep_filter = "line".to_owned();
    app.apply_filters(true);

    assert_eq!(app.grep_matches.len(), 3);
    assert_eq!(app.current_grep_match_row(), Some(2));
    let row = app.model.row(2).unwrap();
    let rendered = render_row(&mut app, 2, row, 40);
    assert!(
        rendered
            .spans
            .iter()
            .any(|span| span.content.as_ref() == "line"
                && span.style.bg == Some(app.theme.search_match_bg)),
        "grep text should be highlighted"
    );
    assert!(
        rendered
            .spans
            .iter()
            .any(|span| span.content.as_ref() == "line"
                && span.style.fg == Some(app.theme.search_match_fg))
    );
    assert_ne!(rendered.spans[0].style.bg, Some(app.theme.search_match_bg));

    app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE))
        .expect("n should move to next grep match");
    assert_eq!(app.current_grep_match_row(), Some(6));
    assert_eq!(app.scroll + viewport_center_offset(app.viewport_rows), 6);

    app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE))
        .expect("p should move to previous grep match");
    assert_eq!(app.current_grep_match_row(), Some(2));
    assert_eq!(app.scroll + viewport_center_offset(app.viewport_rows), 2);
}

#[test]
fn n_and_p_navigate_grep_by_line_not_match_count() {
    let changeset = changeset_with_line_texts(&[
        "line line line",
        "line",
        "other 2",
        "other 3",
        "other 4",
        "other 5",
        "other 6",
        "other 7",
    ]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(5);
    app.grep_filter = "line".to_owned();
    app.apply_filters(true);

    assert_eq!(app.grep_matches, vec![2, 3]);
    assert_eq!(app.current_grep_match_row(), Some(2));
    assert_eq!(app.scroll, 0);

    app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE))
        .expect("n should move to next matching line");

    assert_eq!(app.current_grep_match_row(), Some(3));
    assert_eq!(app.scroll + viewport_center_offset(app.viewport_rows), 3);
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
        &app.changeset,
        app.model.row(annotated_row).expect("annotated row"),
    )
    .expect("annotation key");
    app.annotations.insert(key, "note".to_owned());

    app.grep_filter = "needle".to_owned();
    app.apply_filters(true);

    let match_row = app
        .current_grep_match_row()
        .expect("grep match should be selected");
    let rendered_rows: Vec<_> = plan_diff_viewport_rows(&app, app.viewport_rows)
        .into_iter()
        .filter_map(|slot| match slot.kind {
            ViewportSlotKind::DiffVisual { model_row, .. } => Some(model_row),
            _ => None,
        })
        .collect();
    assert!(rendered_rows.contains(&match_row));
}

#[test]
fn wrapped_grep_selection_stays_on_visible_continuation_row() {
    let changeset = changeset_with_line_texts(&["needle abcdefghijkl", "other", "needle second"]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.line_wrapping = true;
    app.set_viewport_width(18);
    app.set_viewport_rows(2);
    app.grep_filter = "needle".to_owned();
    app.apply_filters(true);

    assert_eq!(app.grep_matches.len(), 2);
    let first = app.grep_matches[0];
    let second = app.grep_matches[1];
    let continuation_scroll = app
        .wrapped_visual_scroll_for_model_row(first)
        .saturating_add(1);

    app.set_scroll(continuation_scroll);

    assert_eq!(app.current_grep_match_row(), Some(first));

    app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE))
        .expect("n should move to the next grep match");

    assert_eq!(app.current_grep_match_row(), Some(second));
}

#[test]
fn grep_match_stays_centered_after_viewport_rows_are_known() {
    let changeset = changeset_with_line_texts(&[
        "other 0", "other 1", "other 2", "other 3", "other 4", "needle", "other 6", "other 7",
    ]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.grep_filter = "needle".to_owned();
    app.apply_filters(true);

    assert_eq!(app.viewport_rows, 1);
    assert_eq!(app.current_grep_match_row(), Some(7));
    assert_eq!(app.scroll, 7);

    app.set_viewport_rows(5);

    assert_eq!(app.current_grep_match_row(), Some(7));
    assert_eq!(app.scroll + viewport_center_offset(app.viewport_rows), 7);
}

#[test]
fn grep_highlight_uses_logical_text_across_rendered_spans() {
    let theme = DiffTheme::default();
    let line = Line::from(vec![
        Span::styled("fo", Style::default().add_modifier(Modifier::BOLD)),
        Span::styled("obar", Style::default()),
    ]);
    let target = grep_highlight_target_for_columns("foobar".to_owned(), &line.spans, 0, 6, 0)
        .expect("target should map rendered spans to logical text");

    let rendered = highlighted_grep_text_line(line, "foobar", vec![target], theme);

    assert_eq!(line_text(&rendered), "foobar");
    assert_eq!(rendered.spans[0].content.as_ref(), "fo");
    assert_eq!(rendered.spans[0].style.bg, Some(theme.search_match_bg));
    assert_eq!(rendered.spans[0].style.fg, Some(theme.search_match_fg));
    assert!(
        rendered.spans[0]
            .style
            .add_modifier
            .contains(Modifier::BOLD)
    );
    assert_eq!(rendered.spans[1].content.as_ref(), "obar");
    assert_eq!(rendered.spans[1].style.bg, Some(theme.search_match_bg));
}

#[test]
fn grep_highlight_ignores_unified_gutter_numbers() {
    let changeset = changeset_with_line_text("abc");
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.grep_filter = "1".to_owned();
    app.apply_filters(false);

    let row = app.model.row(2).expect("diff line should be visible");
    let rendered = render_row(&mut app, 2, row, 32);

    assert!(line_text(&rendered).contains('1'));
    assert!(
        rendered
            .spans
            .iter()
            .all(|span| span.style.bg != Some(app.theme.search_match_bg)),
        "grep should not highlight line numbers when only the gutter matches"
    );
}

#[test]
fn wrapped_split_context_line_highlights_grep_on_continuation_rows() {
    let theme = DiffTheme::default();
    let line = DiffLine {
        kind: DiffLineKind::Context,
        old_line: Some(12),
        new_line: Some(12),
        text: "prefix needle suffix".to_owned(),
    };

    let lines = render_split_context_line_wrapped(&line, None, 0, 30, theme, "needle");
    let highlighted_line = lines
        .iter()
        .find(|line| line_text(line).contains("needle"))
        .expect("wrapped context line should render the grep match");

    assert!(
        highlighted_line
            .spans
            .iter()
            .any(|span| span.content.contains("needle")
                && span.style.bg == Some(theme.search_match_bg)),
        "grep match should be highlighted in wrapped split context lines"
    );
}

#[test]
fn question_mark_key_opens_help_menu_and_filters_when_open() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);

    assert!(!app.help_menu_open);

    let should_quit = app
        .handle_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::SHIFT))
        .expect("? should be handled");
    assert!(!should_quit);
    assert!(app.help_menu_open);

    app.handle_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::SHIFT))
        .expect("? should filter help");
    assert!(app.help_menu_open);
    assert_eq!(app.help_menu_input, "?");

    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
        .expect("Esc should close help");
    assert!(!app.help_menu_open);
}

#[test]
fn configured_help_key_filters_help_menu_when_open() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.keymap = Keymap::parse(
        r#"
        [keymap.global]
        help = "h"
        "#,
    )
    .expect("keymap should parse");

    app.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE))
        .expect("configured help key should open help");
    assert!(app.help_menu_open);

    app.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE))
        .expect("configured help key should filter help");
    assert!(app.help_menu_open);
    assert_eq!(app.help_menu_input, "h");
}

#[test]
fn configured_leader_help_key_filters_help_menu_when_open() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.keymap = Keymap::parse(
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
    assert!(app.help_menu_open);

    app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE))
        .expect("space should filter while help is open");
    assert!(!app.leader_pending);
    app.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE))
        .expect("h should filter while help is open");
    assert!(app.help_menu_open);
    assert_eq!(app.help_menu_input, " h");
    assert!(!app.leader_pending);
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
fn leader_q_does_not_quit() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);

    let should_quit = app
        .handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE))
        .expect("leader should be handled");
    assert!(!should_quit);
    assert!(app.leader_pending);

    let should_quit = app
        .handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE))
        .expect("leader q should be handled");

    assert!(!should_quit);
    assert!(!app.leader_pending);
}

#[test]
fn leader_escape_cancels() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);

    app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE))
        .expect("leader should be handled");
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
        .expect("escape should cancel leader");

    assert!(!app.leader_pending);
}

#[test]
fn flat_action_keys_are_unmapped_under_leader() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);

    app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE))
        .expect("leader should be handled");
    app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE))
        .expect("leader f should be handled");
    assert!(app.filter_input.is_none());

    app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE))
        .expect("leader should be handled");
    app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE))
        .expect("leader slash should be handled");
    assert!(app.filter_input.is_none());

    app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE))
        .expect("leader should be handled");
    app.handle_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::SHIFT))
        .expect("leader question mark should be handled");
    assert!(!app.help_menu_open);
}

#[test]
fn leader_m_opens_diff_source_menu() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);

    app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE))
        .expect("leader should be handled");
    app.handle_key(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE))
        .expect("leader m should be handled");

    assert!(app.diff_menu_open);
    assert_eq!(app.highlighted_diff_choice(), Some(DiffChoice::Show));
}

#[test]
fn leader_o_opens_options_menu() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);

    app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE))
        .expect("leader should be handled");
    app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE))
        .expect("leader o should be handled");

    assert!(app.options_menu_open);
    assert_eq!(app.highlighted_option(), Some(OptionsMenuItem::Layout));
}

#[test]
fn configured_keymap_changes_leader_actions_and_flat_keys() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.keymap = Keymap::parse(
        r#"
        [keymap.global]
        leader = ","
        diff_menu = ", d"
        options_menu = ", o"
        file_filter = "ctrl-f"
        "#,
    )
    .expect("keymap should parse");

    app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE))
        .expect("unmapped f should be handled");
    assert!(app.filter_input.is_none());

    app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL))
        .expect("configured file filter should be handled");
    assert_eq!(app.filter_input, Some(DiffFilterKind::File));

    app.filter_input = None;
    app.handle_key(KeyEvent::new(KeyCode::Char(','), KeyModifiers::NONE))
        .expect("configured leader should be handled");
    app.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE))
        .expect("configured diff menu should be handled");
    assert!(app.diff_menu_open);

    app.close_diff_menu();
    app.handle_key(KeyEvent::new(KeyCode::Char(','), KeyModifiers::NONE))
        .expect("configured leader should be handled");
    app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE))
        .expect("configured options menu should be handled");
    assert!(app.options_menu_open);
}

#[test]
fn leader_e_is_unmapped() {
    let mut changeset = changeset_with_hunk_at(PathBuf::from("/repo"), 20);
    changeset.files[0].new_path = None;
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);

    app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE))
        .expect("leader should be handled");
    let should_quit = app
        .handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE))
        .expect("leader e should be ignored");

    assert!(!should_quit);
    assert!(!app.leader_pending);
    assert!(app.error_log.is_none());
}

#[test]
fn configured_leader_diff_type_bindings_cycle_choices() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.keymap = Keymap::parse(
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
        .pending_diff_load
        .as_ref()
        .expect("leader n should queue diff load");
    assert_eq!(load.options.source, DiffSource::Show("HEAD".to_owned()));
    assert_eq!(load.options.scope, DiffScope::All);
    assert!(!app.leader_pending);

    app.pending_diff_load = None;
    app.options = DiffOptions {
        source: DiffSource::Show("HEAD".to_owned()),
        ..DiffOptions::default()
    };
    app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE))
        .expect("leader should be handled");
    app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE))
        .expect("leader p should cycle diff type");
    let load = app
        .pending_diff_load
        .as_ref()
        .expect("leader p should queue diff load");
    assert_eq!(load.options.source, DiffSource::Worktree);
    assert_eq!(load.options.scope, DiffScope::All);
    assert!(!app.leader_pending);
}

#[test]
fn edit_hunk_remap_disables_default_ctrl_g() {
    let mut changeset = changeset_with_hunk_at(PathBuf::from("/repo"), 20);
    changeset.files[0].new_path = None;
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.keymap = Keymap::parse(
        r#"
        [keymap.global]
        edit_hunk = "e"
        "#,
    )
    .expect("keymap should parse");
    app.set_viewport_rows(1);
    app.set_scroll(1);

    app.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL))
        .expect("unmapped Ctrl-G should be handled");
    assert_eq!(app.scroll, 1);
    assert!(app.error_log.is_none());

    app.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE))
        .expect("configured edit key should be handled");
    assert!(app.error_log.is_none());
}

#[test]
fn ctrl_c_force_quit_wins_over_configured_edit_hunk_key() {
    let changeset = changeset_with_hunk_at(PathBuf::from("/repo"), 20);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.keymap = Keymap::parse(
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
    assert!(app.error_log.is_none());
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
fn configured_edit_hunk_key_does_not_bypass_open_menus() {
    let mut changeset = changeset_with_hunk_at(PathBuf::from("/repo"), 20);
    changeset.files[0].new_path = None;
    let mut app = DiffApp::new(
        DiffOptions::default(),
        changeset.clone(),
        DiffLayoutMode::Unified,
    );
    app.keymap = Keymap::parse(
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
    assert!(app.diff_menu_open);
    assert_eq!(app.diff_menu_input, "j");
    assert!(app.error_log.is_none());

    let mut app = DiffApp::new(
        DiffOptions::default(),
        changeset.clone(),
        DiffLayoutMode::Unified,
    );
    app.keymap = Keymap::parse(
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
    assert!(app.options_menu_open);
    assert_eq!(app.layout, DiffLayoutMode::Split);
    assert!(app.error_log.is_none());

    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.keymap = Keymap::parse(
        r#"
        [keymap.global]
        edit_hunk = "j"
        "#,
    )
    .expect("keymap should parse");
    app.branch_menu_open = Some(BranchMenu::Head);

    let should_quit = handle_test_key_event(
        &mut app,
        KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE),
    );

    assert!(!should_quit);
    assert_eq!(app.branch_menu_open, Some(BranchMenu::Head));
    assert_eq!(app.branch_menu_input, "j");
    assert!(app.error_log.is_none());
}

#[test]
fn question_mark_key_filters_branch_menu() {
    let options = DiffOptions {
        source: DiffSource::Base("main".to_owned()),
        ..DiffOptions::default()
    };
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(options, changeset, DiffLayoutMode::Unified);
    app.branch_menu_open = Some(BranchMenu::Head);
    app.comparison_branches = vec!["main".to_owned(), "feature/header".to_owned()];

    let should_quit = app
        .handle_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::SHIFT))
        .expect("? should be handled by branch filter");

    assert!(!should_quit);
    assert!(!app.help_menu_open);
    assert_eq!(app.branch_menu_input, "?");
}

#[test]
fn help_menu_esc_closes_without_quitting() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.help_menu_open = true;
    app.dirty = false;

    let should_quit = app
        .handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
        .expect("Esc should close help");

    assert!(!should_quit);
    assert!(!app.help_menu_open);
    assert!(app.dirty);
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

    assert!(app.error_log.is_some());
    assert_eq!(error_log_height(&app, 20), ERROR_LOG_DEFAULT_HEIGHT);

    let should_quit = app
        .handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
        .expect("Esc should close error log");

    assert!(!should_quit);
    assert!(app.error_log.is_none());
}

#[test]
fn esc_closes_error_log_and_clears_pending_leader() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_error_log("reload failed");

    app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE))
        .expect("leader should be handled");
    assert!(app.leader_pending);

    let should_quit = app
        .handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
        .expect("Esc should close error log");

    assert!(!should_quit);
    assert!(app.error_log.is_none());
    assert!(!app.leader_pending);

    let should_quit = app
        .handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE))
        .expect("q should be handled as a fresh quit key");

    assert!(should_quit);
}

#[test]
fn esc_closes_diff_menu_before_error_log() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.diff_menu_open = true;
    app.set_error_log("reload failed");

    let should_quit = app
        .handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
        .expect("Esc should close topmost menu");

    assert!(!should_quit);
    assert!(!app.diff_menu_open);
    assert!(app.error_log.is_some());
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

    assert!(app.error_log_resizing);

    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Drag(MouseButton::Left),
        column: 0,
        row: separator_row.saturating_sub(2),
        modifiers: KeyModifiers::NONE,
    })
    .expect("drag should resize");

    assert_eq!(app.error_log_height, ERROR_LOG_DEFAULT_HEIGHT + 2);

    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: 0,
        row: separator_row.saturating_sub(2),
        modifiers: KeyModifiers::NONE,
    })
    .expect("resize should stop");

    assert!(!app.error_log_resizing);
}

#[test]
fn file_sidebar_position_is_limited_to_rendered_body_rows() {
    let changeset = changeset_with_files(&["a.rs", "b.rs", "c.rs", "d.rs"]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.file_sidebar_open = true;
    app.file_sidebar_render_width = 20;
    app.set_viewport_rows(3);

    assert!(app.is_file_sidebar_position(0, 3));
    assert!(!app.is_file_sidebar_position(0, 4));
}

#[test]
fn error_log_separator_fills_width() {
    assert_eq!(error_log_separator(0), "");
    assert_eq!(error_log_separator(4), "erro");
    assert_eq!(error_log_separator(12), "error ──────");
    assert_eq!(error_log_separator(12).width(), 12);
}

#[test]
fn error_log_header_shows_copy_command_on_right() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_error_log("reload failed");

    let line = error_log_header_line(&app, 40);
    let text = line_text(&line);

    assert_eq!(text.width(), 40);
    assert!(text.starts_with("error "));
    assert!(text.ends_with("[Copy All (Ctrl-Shift-C)]"));
    assert_eq!(line.spans[2].style.fg, Some(app.theme.deletion_fg));
}

#[test]
fn error_log_header_uses_configured_copy_command() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.keymap = Keymap::parse(
        r#"
        [keymap.global]
        copy_error_log = "space y"
        "#,
    )
    .expect("keymap should parse");
    app.set_error_log("reload failed");

    let text = line_text(&error_log_header_line(&app, 32));

    assert_eq!(text.width(), 32);
    assert!(text.ends_with("[Copy All (Space y)]"));
}

#[test]
fn copy_error_log_key_ignores_absent_error_log() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.keymap = Keymap::parse(
        r#"
        [keymap.global]
        copy_error_log = "e"
        "#,
    )
    .expect("keymap should parse");

    let should_quit = app
        .handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE))
        .expect("copy key without error log should be handled");

    assert!(!should_quit);
    assert!(app.error_log.is_none());
    assert!(app.toasts.is_empty());
}

#[test]
fn copy_error_log_key_does_not_preempt_filter_input() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.keymap = Keymap::parse(
        r#"
        [keymap.global]
        copy_error_log = "e"
        "#,
    )
    .expect("keymap should parse");
    app.set_error_log("reload failed");
    app.open_filter_input(DiffFilterKind::File);

    let should_quit = app
        .handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE))
        .expect("copy key should be handled as filter input");

    assert!(!should_quit);
    assert_eq!(app.file_filter_input, "e");
    assert_eq!(app.file_filter, "e");
    assert!(app.toasts.is_empty());
}

#[test]
fn copy_error_log_key_does_not_preempt_branch_menu_input() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.keymap = Keymap::parse(
        r#"
        [keymap.global]
        copy_error_log = "e"
        "#,
    )
    .expect("keymap should parse");
    app.set_error_log("reload failed");
    app.branch_menu_open = Some(BranchMenu::Head);

    let should_quit = app
        .handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE))
        .expect("copy key should be handled as branch input");

    assert!(!should_quit);
    assert_eq!(app.branch_menu_input, "e");
    assert!(app.toasts.is_empty());
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
    assert_eq!(app.toasts.latest_text(), Some("error log copied"));
    assert_eq!(
        app.error_log.as_deref(),
        Some("reload failed:\nfatal: bad revision")
    );
}

#[test]
fn copy_error_log_without_log_shows_notice_without_writing() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    let mut output = Vec::new();

    app.copy_error_log_to_writer(&mut output);

    assert!(output.is_empty());
    assert_eq!(app.toasts.latest_text(), Some("no error log to copy"));
}

#[test]
fn copy_marks_writes_structured_json_to_clipboard_sequence() {
    use crate::annotation::AnnotationKey;

    let mut changeset = changeset_with_line_text("hello");
    changeset.files[0].hunks[0].lines[0].kind = DiffLineKind::Addition;
    changeset.files[0].hunks[0].lines[0].old_line = None;
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    let code_row = app
        .model
        .rows
        .iter()
        .position(|row| matches!(row, UiRow::UnifiedLine { .. }))
        .expect("unified line");
    let row = app.model.row(code_row).expect("row");
    let key = AnnotationKey::from_ui_row(&app.changeset, row).expect("key");
    app.annotations
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
    assert_eq!(app.toasts.latest_text(), Some("marks copied"));
}

#[test]
fn copy_marks_omits_annotations_without_current_diff_line() {
    use crate::annotation::{AnnotationKey, AnnotationSide};

    let changeset = changeset_with_replacement_pair();
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    let new_row = app
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
        &app.changeset,
        app.model.row(new_row).expect("new diff row"),
    )
    .expect("new-side key");
    assert_eq!(old_key.side, AnnotationSide::Old);
    assert_eq!(new_key.side, AnnotationSide::New);
    app.annotations
        .insert(old_key.clone(), "old note".to_owned());
    app.annotations.insert(new_key, "new note".to_owned());

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
        app.annotations.get(&old_key).map(String::as_str),
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
        .model
        .rows
        .iter()
        .copied()
        .find(|row| matches!(row, UiRow::ContextLine { .. }))
        .expect("expanded context line");
    let key = AnnotationKey::from_ui_row(&app.changeset, context_row).expect("context key");
    app.annotations
        .insert(key.clone(), "context note".to_owned());

    assert!(app.hide_context(0, 0));
    assert!(
        !app.model.rows.iter().any(|row| matches!(
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
fn copy_marks_without_marks_shows_notice_without_writing() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    let mut output = Vec::new();

    app.copy_marks_to_writer(&mut output);

    assert!(output.is_empty());
    assert_eq!(app.toasts.latest_text(), Some("no marks to copy"));
}

#[test]
fn osc52_clipboard_sequence_base64_encodes_text() {
    assert_eq!(osc52_clipboard_sequence("abc"), "\x1b]52;c;YWJj\x07");
    assert_eq!(osc52_clipboard_sequence("mark"), "\x1b]52;c;bWFyaw==\x07");
}

#[test]
fn notices_expire_after_ttl() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);

    app.set_notice("reloaded");
    let expires_at = app.toasts.visible().next().unwrap().expires_at;
    assert_eq!(app.toasts.latest_text().unwrap(), "reloaded");
    app.dirty = false;

    app.expire_toasts(expires_at - Duration::from_millis(1));
    assert!(!app.toasts.is_empty());
    assert!(!app.dirty);

    app.expire_toasts(expires_at);
    assert!(app.toasts.is_empty());
    assert!(app.dirty);
}

#[test]
fn notices_mark_dirty_when_configured_max_visible_is_zero() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.toasts = Toasts::new(NotificationSettings {
        mode: NotificationMode::Default,
        corner: ToastCorner::TopRight,
        timeout_ms: 1_500,
        max_visible: 0,
    });
    app.dirty = false;
    app.terminal_clear_requested = true;

    app.set_notice("editor closed");

    assert!(app.dirty);
    assert!(app.terminal_clear_requested);
    assert_eq!(app.toasts.latest_text(), Some("editor closed"));
}

#[test]
fn notices_clamp_oversized_timeout() {
    let mut toasts = Toasts::new(NotificationSettings {
        mode: NotificationMode::Default,
        corner: ToastCorner::TopRight,
        timeout_ms: u64::MAX,
        max_visible: 3,
    });

    let before = Instant::now();
    assert!(toasts.push(ToastLevel::Info, "saved"));
    let after = Instant::now();

    let expires_at = toasts.visible().next().unwrap().expires_at;
    assert!(expires_at >= before);
    assert!(expires_at <= after + Duration::from_millis(MAX_NOTIFICATION_TIMEOUT_MS));
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
    assert!(text.iter().any(|line| line.contains("]/[")));
    assert!(text.iter().any(|line| line.contains("Ctrl-G")));
    assert!(text.iter().any(|line| line.contains("Ctrl-Shift-C")));
    assert_eq!(keymap.global_action_label(GlobalAction::FileBrowser), "b");
    assert!(text.iter().any(|line| line.contains("toggle file sidebar")));
    assert!(text.iter().any(|line| line.contains("Space s")));
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
            .any(|line| line.contains(",") && line.contains("leader"))
    );
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

    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 60))
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
    assert!(
        rows.iter()
            .any(|row| row.contains("edit active mark in editor")),
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
    assert!(app.help_menu_visible_rows > 1);

    app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL))
        .expect("ctrl-n should scroll help");
    assert_eq!(app.help_menu_scroll, 1);

    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
        .expect("tab should not scroll help");
    assert_eq!(app.help_menu_scroll, 1);
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
    let page = app.help_menu_visible_rows;
    assert!(page > 1);

    app.handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE))
        .expect("page down should scroll help");
    let max_scroll = app.filtered_help_menu_rows().len().saturating_sub(page);
    assert_eq!(app.help_menu_scroll, page.min(max_scroll));
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
fn file_sidebar_tracks_selected_file() {
    let changeset = changeset_with_files(&["a.rs", "b.rs", "c.rs", "d.rs", "e.rs"]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.file_sidebar_open = true;
    app.set_viewport_rows(4);

    app.selected_file = 4;
    app.ensure_file_sidebar_selection_visible(app.visible_file_sidebar_rows());

    assert_eq!(app.selected_file, 4);
    assert_eq!(app.file_sidebar_scroll, 1);

    app.selected_file = 1;
    app.ensure_file_sidebar_selection_visible(app.visible_file_sidebar_rows());

    assert_eq!(app.selected_file, 1);
    assert_eq!(app.file_sidebar_scroll, 1);
}

#[test]
fn diff_scroll_does_not_move_file_sidebar_scroll() {
    let changeset = changeset_with_files(&["a.rs", "b.rs", "c.rs", "d.rs", "e.rs"]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.file_sidebar_open = true;
    app.set_viewport_rows(4);
    app.set_file_sidebar_scroll(1);

    app.set_scroll(0);

    assert_eq!(app.selected_file, 0);
    assert_eq!(app.file_sidebar_scroll, 1);
}

#[test]
fn replace_changeset_keeps_remapped_file_sidebar_selection_visible() {
    let changeset = changeset_with_files(&["a.rs", "b.rs", "c.rs", "d.rs", "e.rs"]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.file_sidebar_open = true;
    app.set_viewport_rows(2);
    app.selected_file = 4;
    app.ensure_file_sidebar_selection_visible(app.visible_file_sidebar_rows());

    assert_eq!(app.file_sidebar_scroll, 3);

    app.replace_changeset(changeset_with_files(&[
        "new.rs",
        "other.rs",
        "third.rs",
        "fourth.rs",
        "fifth.rs",
    ]));

    assert_eq!(app.selected_file, 0);
    assert_eq!(app.file_sidebar_scroll, 0);
}

#[test]
fn mouse_wheel_over_file_sidebar_scrolls_sidebar_only() {
    let changeset = changeset_with_files(&["a.rs", "b.rs", "c.rs", "d.rs", "e.rs"]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.file_sidebar_open = true;
    app.file_sidebar_render_width = 20;
    app.set_viewport_rows(4);

    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: 1,
        row: 1,
        modifiers: KeyModifiers::NONE,
    })
    .expect("sidebar scroll should be handled");

    assert_eq!(app.scroll, 0);
    assert_eq!(app.file_sidebar_scroll, 1);
}

#[test]
fn horizontal_mouse_wheel_over_file_sidebar_is_ignored() {
    let changeset = changeset_with_line_text("abcdefghijkl");
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.file_sidebar_open = true;
    app.file_sidebar_render_width = 20;
    app.set_viewport_width(18);

    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::ScrollRight,
        column: 1,
        row: 1,
        modifiers: KeyModifiers::NONE,
    })
    .expect("sidebar horizontal scroll should be ignored");

    assert_eq!(app.horizontal_scroll, 0);

    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::ScrollRight,
        column: 21,
        row: 1,
        modifiers: KeyModifiers::NONE,
    })
    .expect("diff horizontal scroll should still work");

    assert_eq!(app.horizontal_scroll, HORIZONTAL_SCROLL_STEP);
}

#[test]
fn file_sidebar_renders_changed_file_summary() {
    let mut changeset = changeset_with_files(&["src/lib.rs", "README.md"]);
    changeset.files[1].status = FileStatus::Added;
    changeset.files[1].additions = 12;
    changeset.files[1].deletions = 0;
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.file_sidebar_open = true;
    app.selected_file = 1;

    let lines = file_sidebar_lines(&app, 24, 2);
    let additions = lines[1]
        .spans
        .iter()
        .find(|span| span.content.as_ref() == "+12")
        .expect("additions should render as their own span");
    let deletions = lines[1]
        .spans
        .iter()
        .find(|span| span.content.as_ref() == "-0")
        .expect("deletions should render as their own span");

    assert!(line_text(&lines[0]).contains(" M src/lib.rs"));
    assert!(line_text(&lines[1]).contains(" A README.md"));
    assert!(line_text(&lines[1]).contains("+12 -0"));
    assert_eq!(lines[0].spans[0].content.as_ref(), " M ");
    assert_eq!(lines[0].spans[0].style.fg, Some(DiffTheme::default().hunk));
    assert!(
        lines[0].spans[0]
            .style
            .add_modifier
            .contains(Modifier::BOLD)
    );
    assert_eq!(
        lines[0].spans[1].style.fg,
        Some(DiffTheme::default().foreground)
    );
    assert_eq!(additions.style.fg, Some(DiffTheme::default().addition_fg));
    assert_eq!(deletions.style.fg, Some(DiffTheme::default().deletion_fg));
    assert!(
        lines[1].spans[0]
            .style
            .add_modifier
            .contains(Modifier::BOLD)
    );
}

#[test]
fn file_sidebar_truncates_long_paths_before_stats() {
    let mut changeset =
        changeset_with_files(&["src/runtime/test_runner/expect/toMatchInlineSnapshot.rs"]);
    changeset.files[0].status = FileStatus::Added;
    changeset.files[0].additions = 1290;
    changeset.files[0].deletions = 3910;
    let app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);

    let lines = file_sidebar_lines(&app, 32, 1);
    let text = line_text(&lines[0]);

    assert_eq!(text.width(), 32);
    assert!(text.contains("..."));
    assert!(text.contains("+1290 -3910"));
}

#[test]
fn file_sidebar_separator_drag_resizes_sidebar() {
    let changeset = changeset_with_files(&["a.rs"]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.file_sidebar_open = true;
    app.file_sidebar_render_width = 30;
    app.viewport_width = 70;

    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 29,
        row: 1,
        modifiers: KeyModifiers::NONE,
    })
    .expect("resize should start");

    assert!(app.file_sidebar_resizing);

    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Drag(MouseButton::Left),
        column: 49,
        row: 1,
        modifiers: KeyModifiers::NONE,
    })
    .expect("drag should resize");

    assert_eq!(app.file_sidebar_width, Some(50));
    assert!(app.dirty);

    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: 49,
        row: 1,
        modifiers: KeyModifiers::NONE,
    })
    .expect("resize should end");

    assert!(!app.file_sidebar_resizing);
}

#[test]
fn progress_label_is_bounded() {
    assert_eq!(progress_label(0, 0), "100%");
    assert_eq!(progress_label(0, 20), "0%");
    assert_eq!(progress_label(10, 20), "50%");
    assert_eq!(progress_label(100, 20), "100%");
}

#[test]
fn diff_header_labels_describe_selected_scope() {
    let mut options = DiffOptions::default();

    assert_eq!(diff_selector_text(&options), " All changes ");
    assert_eq!(diff_comparison_label(&options), "HEAD → working tree");

    options.scope = DiffScope::Unstaged;
    assert_eq!(diff_selector_text(&options), " Unstaged ");
    assert_eq!(diff_comparison_label(&options), "index → working tree");

    options.scope = DiffScope::Staged;
    assert_eq!(diff_selector_text(&options), " Staged ");
    assert_eq!(diff_comparison_label(&options), "HEAD → index");

    options.source = DiffSource::Base("origin/main".to_owned());
    options.scope = DiffScope::All;
    assert_eq!(diff_selector_text(&options), " Branch ");
    assert_eq!(diff_comparison_label(&options), "HEAD → origin/main");

    options.source = DiffSource::Branch {
        base: "origin/main".to_owned(),
        head: "feature/ui".to_owned(),
    };
    assert_eq!(diff_comparison_label(&options), "feature/ui → origin/main");
}

#[test]
fn statusline_header_right_aligns_current_file() {
    let changeset = changeset_with_files(&["src/lib.rs", "README.md", "docs/guide.md"]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(3);
    app.select_file(1);

    let line = statusline_header_line(&app, 80);
    let text = line_text(&line);

    assert_eq!(text.width(), 80);
    assert!(text.starts_with(" All changes  HEAD → working tree"));
    assert!(text.contains("README.md 2/3"));
    assert!(text.ends_with("% "));

    let selector = line.spans.first().expect("selector block should render");
    assert_eq!(selector.style.fg, Some(app.theme.statusline_accent_fg));
    assert_eq!(selector.style.bg, Some(app.theme.statusline_accent_bg));

    let file = line.spans.last().expect("file block should render");
    assert_eq!(file.style.fg, Some(app.theme.statusline_info_fg));
    assert_eq!(file.style.bg, Some(app.theme.statusline_info_bg));
    assert!(file.style.add_modifier.contains(Modifier::BOLD));
}

#[test]
fn statusline_header_uses_theme_statusline_colors() {
    let changeset = changeset_with_files(&["src/lib.rs"]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.theme.statusline_accent_fg = Color::Rgb(1, 2, 3);
    app.theme.statusline_accent_bg = Color::Rgb(4, 5, 6);
    app.theme.statusline_info_fg = Color::Rgb(7, 8, 9);
    app.theme.statusline_info_bg = Color::Rgb(10, 11, 12);

    let line = statusline_header_line(&app, 80);
    let selector = line.spans.first().expect("selector block should render");
    let file = line.spans.last().expect("file block should render");

    assert_eq!(selector.style.fg, Some(Color::Rgb(1, 2, 3)));
    assert_eq!(selector.style.bg, Some(Color::Rgb(4, 5, 6)));
    assert_eq!(file.style.fg, Some(Color::Rgb(7, 8, 9)));
    assert_eq!(file.style.bg, Some(Color::Rgb(10, 11, 12)));
}

#[test]
fn statusline_header_shows_pending_diff_load() {
    let changeset = changeset_with_files(&["src/lib.rs", "README.md", "docs/guide.md"]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    let options = DiffOptions {
        scope: DiffScope::Staged,
        ..DiffOptions::default()
    };
    app.pending_diff_load = Some(pending_diff_load(options));

    let line = statusline_header_line(&app, 80);
    let text = line_text(&line);

    assert_eq!(text.width(), 80);
    assert!(text.contains("loading diff"));
}

#[test]
fn notice_toasts_render_as_overlay() {
    let changeset = changeset_with_files(&["src/lib.rs", "README.md", "docs/guide.md"]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 10))
        .expect("test terminal should be created");

    app.set_notice("editor closed; reloading");

    let line = statusline_header_line(&app, 120);
    let text = line_text(&line);
    assert_eq!(text.width(), 120);
    assert!(!text.contains("editor closed; reloading"));

    terminal
        .draw(|frame| crate::render::draw(frame, &mut app))
        .expect("draw should succeed");
    let rows = buffer_rows(terminal.backend().buffer());
    assert!(
        rows.iter()
            .any(|row| row.contains("editor closed; reloading"))
    );
}

#[test]
fn statusline_header_shows_pending_live_reload() {
    let changeset = changeset_with_files(&["src/lib.rs", "README.md", "docs/guide.md"]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.mark_live_reload_pending();

    let line = statusline_header_line(&app, 80);
    let text = line_text(&line);

    assert_eq!(text.width(), 80);
    assert!(text.contains("refreshing diff"));
    assert!(!text.contains("loading diff"));
}

#[test]
fn commit_match_score_matches_sha_and_subject() {
    let commit = GitCommit {
        sha: "abcdef0123456789".to_owned(),
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
    app.current_head = Some("feature".to_owned());
    assert_eq!(app.show_rev_menu_detail(), "feature");
    app.current_head = Some("a1b2c3d".to_owned());
    assert_eq!(app.show_rev_menu_detail(), "a1b2c3d");
    app.show_rev = Some("HEAD~1".to_owned());
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
    assert!(!app.diff_menu_open);
    assert!(!app.commit_menu_open);
    let load = app
        .pending_diff_load
        .as_ref()
        .expect("show choice should queue diff load");
    assert_eq!(load.options.source, DiffSource::Show("HEAD".to_owned()));
    assert_eq!(load.options.scope, DiffScope::All);
}

#[test]
fn diff_menu_lists_all_changes_first() {
    let mut app = DiffApp::new(
        DiffOptions::default(),
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.branch_base = Some("origin/main".to_owned());

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
    app.branch_base = Some("main".to_owned());
    app.branch_head = Some("feature".to_owned());
    app.current_head = Some("main".to_owned());

    assert!(app.diff_menu_choices().is_empty());
}

#[test]
fn tab_does_not_switch_range_diff_to_branch_or_worktree() {
    let options = DiffOptions {
        source: DiffSource::Range {
            left: "main".to_owned(),
            right: "feature".to_owned(),
        },
        ..DiffOptions::default()
    };
    let mut app = DiffApp::new(
        options.clone(),
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.branch_base = Some("main".to_owned());
    app.branch_head = Some("feature".to_owned());
    app.current_head = Some("main".to_owned());

    let should_quit = app
        .handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
        .expect("tab should be handled");

    assert!(!should_quit);
    assert!(app.pending_diff_load.is_none());
    assert_eq!(app.options, options);
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
    assert!(!app.diff_menu_open);
    let load = app
        .pending_diff_load
        .as_ref()
        .expect("menu selection should queue diff load");
    assert_eq!(load.options.source, DiffSource::Worktree);
    assert_eq!(load.options.scope, DiffScope::Unstaged);
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

    assert!(!app.diff_menu_open);
    assert!(app.review_input_open);
    assert_eq!(app.review_input, "");
}

#[test]
fn review_input_url_queues_review_load() {
    let mut app = DiffApp::new(
        DiffOptions::default(),
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );

    app.open_review_input();
    app.review_input = "https://github.com/owner/repo/pull/1".to_owned();
    app.review_input_cursor = app.review_input.len();
    let mut submitted_target = None;
    app.submit_review_input_for_test(|app, target| {
        submitted_target = Some(target);
        app.pending_review_load = Some(pending_review_load());
    });

    assert!(!app.review_input_open);
    assert_eq!(
        submitted_target.as_deref(),
        Some("https://github.com/owner/repo/pull/1")
    );
    assert!(app.pending_review_load.is_some());
}

#[test]
fn review_input_number_queues_review_load() {
    let mut app = DiffApp::new(
        DiffOptions::default(),
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );

    app.open_review_input();
    app.review_input = " 123 ".to_owned();
    app.review_input_cursor = app.review_input.len();
    let mut submitted_target = None;
    app.submit_review_input_for_test(|app, target| {
        submitted_target = Some(target);
        app.pending_review_load = Some(pending_review_load());
    });

    assert!(!app.review_input_open);
    assert_eq!(submitted_target.as_deref(), Some("123"));
    assert!(app.pending_review_load.is_some());
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
        source: DiffSource::Patch(PatchSource::Text {
            label: "review owner/repo#123".to_owned(),
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
        include_untracked: true,
        ..DiffOptions::default()
    };
    let mut app = DiffApp::new(
        options,
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    let review_options = DiffOptions {
        source: DiffSource::Patch(PatchSource::Text {
            label: "review owner/repo#123".to_owned(),
            patch: Arc::from(&b""[..]),
        }),
        include_untracked: false,
        ..DiffOptions::default()
    };
    let (tx, rx) = oneshot::channel();
    let _ = tx.send(Ok((review_options, changeset_with_context_lines(1))));
    app.pending_review_load = Some(PendingReviewLoad {
        error_prefix: "review unavailable".to_owned(),
        rx,
    });

    app.drain_pending_review_load();

    assert!(app.options.include_untracked);
    assert!(
        app.options_for_choice(DiffChoice::All)
            .expect("all choice should be available")
            .include_untracked
    );
}

#[test]
fn diff_menu_uses_configured_menu_keymap() {
    let mut app = DiffApp::new(
        DiffOptions::default(),
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.branch_base = Some("main".to_owned());
    app.branch_head = Some("feature".to_owned());
    app.current_head = Some("feature".to_owned());
    app.keymap = Keymap::parse(
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
    assert_eq!(app.diff_menu_input, "");
    assert_eq!(app.highlighted_diff_choice(), Some(DiffChoice::Show));

    app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE))
        .expect("configured up key should move menu selection");
    assert_eq!(app.diff_menu_input, "");
    assert_eq!(app.highlighted_diff_choice(), Some(DiffChoice::Branch));

    app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE))
        .expect("configured close key should close menu");
    assert!(!app.diff_menu_open);
    assert_eq!(app.diff_menu_input, "");

    app.open_diff_menu();
    app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE))
        .expect("configured confirm key should select menu item");

    assert!(!app.diff_menu_open);
    let load = app
        .pending_diff_load
        .as_ref()
        .expect("menu selection should queue diff load");
    assert_eq!(load.options.source, DiffSource::Base("main".to_owned()));
    assert_eq!(load.options.scope, DiffScope::All);
}

#[test]
fn branch_menu_uses_configured_menu_keymap() {
    let options = DiffOptions {
        source: DiffSource::Base("main".to_owned()),
        ..DiffOptions::default()
    };
    let mut app = DiffApp::new(
        options,
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.branch_base = Some("main".to_owned());
    app.branch_head = Some("feature".to_owned());
    app.current_head = Some("feature".to_owned());
    app.comparison_branches = vec!["main".to_owned(), "feature".to_owned(), "topic".to_owned()];
    app.keymap = Keymap::parse(
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
    assert_eq!(app.branch_menu_selected, 0);

    app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE))
        .expect("configured down key should move branch selection");
    assert_eq!(app.branch_menu_input, "");
    assert_eq!(app.branch_menu_selected, 1);

    app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE))
        .expect("configured up key should move branch selection");
    assert_eq!(app.branch_menu_input, "");
    assert_eq!(app.branch_menu_selected, 0);

    app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE))
        .expect("configured close key should close branch menu");
    assert!(app.branch_menu_open.is_none());
    assert_eq!(app.branch_menu_input, "");

    app.toggle_branch_menu(BranchMenu::Head);
    app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE))
        .expect("configured down key should move branch selection");
    app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE))
        .expect("configured confirm key should select branch");

    assert!(app.branch_menu_open.is_none());
    let load = app
        .pending_diff_load
        .as_ref()
        .expect("branch selection should queue diff load");
    assert_eq!(
        load.options.source,
        DiffSource::Branch {
            base: "main".to_owned(),
            head: "topic".to_owned()
        }
    );
    assert_eq!(load.options.scope, DiffScope::All);
}

#[test]
fn diff_menu_ctrl_n_and_ctrl_p_move_selection() {
    let mut app = DiffApp::new(
        DiffOptions::default(),
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.branch_base = Some("main".to_owned());

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

    assert_eq!(app.diff_menu_input, "j");
    assert!(app.diff_menu_open);
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

    assert!(app.diff_menu_open);
    assert!(!app.leader_pending);
    assert_eq!(app.diff_menu_input, " ");
    assert!(app.pending_diff_load.is_none());
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
    assert!(app.diff_menu_open);
    assert_eq!(app.diff_menu_input, "q");
}

#[test]
fn diff_menu_branch_keys_do_not_open_branch_picker() {
    let mut app = DiffApp::new(
        DiffOptions::default(),
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.branch_base = Some("main".to_owned());
    app.branch_head = Some("feature".to_owned());
    app.current_head = Some("feature".to_owned());
    app.comparison_branches = vec!["main".to_owned(), "feature".to_owned()];

    app.open_diff_menu();
    assert_eq!(app.highlighted_diff_choice(), Some(DiffChoice::Branch));
    app.handle_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE))
        .expect("b should filter diff menu");

    assert!(app.diff_menu_open);
    assert!(app.branch_menu_open.is_none());
    assert_eq!(app.diff_menu_input, "b");

    app.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE))
        .expect("h should filter diff menu");

    assert!(app.diff_menu_open);
    assert!(app.branch_menu_open.is_none());
    assert_eq!(app.diff_menu_input, "bh");
}

#[test]
fn diff_menu_number_keys_filter_input() {
    let mut app = DiffApp::new(
        DiffOptions::default(),
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.branch_base = Some("main".to_owned());
    app.branch_head = Some("feature".to_owned());
    app.current_head = Some("feature".to_owned());

    app.open_diff_menu();
    app.handle_key(KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE))
        .expect("2 should filter diff choices");

    assert!(app.diff_menu_open);
    assert_eq!(app.diff_menu_input, "2");
    assert!(app.pending_diff_load.is_none());
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

    assert!(!app.diff_menu_open);
    let load = app
        .pending_diff_load
        .as_ref()
        .expect("visible click should queue diff load");
    assert_eq!(load.options.source, DiffSource::Worktree);
    assert_eq!(load.options.scope, DiffScope::Unstaged);
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

    assert!(!app.diff_menu_open);
    assert!(app.pending_diff_load.is_none());
}

#[test]
fn options_menu_toggles_setting_on_enter() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);

    app.open_options_menu();
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .expect("enter should toggle layout");

    assert!(app.options_menu_open);
    assert_eq!(app.layout, DiffLayoutMode::Split);
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

    assert!(app.options_menu_open);
    assert_eq!(app.options_menu_input, "x");
    assert_eq!(app.layout, DiffLayoutMode::Unified);
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
    app.syntax = Some(syntax_runtime_with_queue(SyntaxWorkerQueue::new(
        1,
        app.generation,
    )));

    app.open_options_menu();
    app.move_options_menu_selection(3);
    assert_eq!(
        app.highlighted_option(),
        Some(OptionsMenuItem::SyntaxHighlighting)
    );
    assert_eq!(app.option_value(OptionsMenuItem::SyntaxHighlighting), "[x]");

    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .expect("enter should toggle syntax highlighting");

    assert!(app.syntax.is_none());
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
    app.move_options_menu_selection(3);
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .expect("enter should try to enable syntax highlighting");

    assert!(app.syntax.is_none());
    assert!(!app.options_menu_draft.syntax_enabled);
    assert_eq!(
        app.last_persisted_options_menu_draft,
        Some((
            OptionsDraft {
                syntax_enabled: false,
                ..app.options_menu_draft
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
    assert_eq!(app.horizontal_scroll, HORIZONTAL_SCROLL_STEP);

    app.open_options_menu();
    app.move_options_menu_selection(4);
    assert_eq!(
        app.highlighted_option(),
        Some(OptionsMenuItem::LineWrapping)
    );
    assert_eq!(app.option_value(OptionsMenuItem::LineWrapping), "[ ]");

    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .expect("enter should toggle line wrapping");

    assert!(app.line_wrapping);
    assert_eq!(app.horizontal_scroll, 0);
    assert_eq!(app.max_horizontal_scroll(), 0);
    assert_eq!(app.option_value(OptionsMenuItem::LineWrapping), "[x]");
}

#[test]
fn options_menu_cycles_notification_settings() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);

    app.open_options_menu();
    app.set_options_menu_selection(6);
    assert_eq!(
        app.highlighted_option(),
        Some(OptionsMenuItem::NotificationMode)
    );
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .expect("enter should toggle notification mode");
    assert_eq!(
        app.syntax_settings.notifications.mode,
        NotificationMode::Debug
    );
    assert!(app.toasts.debug_enabled());

    app.set_options_menu_selection(7);
    assert_eq!(app.highlighted_option(), Some(OptionsMenuItem::ToastCorner));
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .expect("enter should cycle toast corner");
    assert_eq!(
        app.syntax_settings.notifications.corner,
        ToastCorner::BottomRight
    );
    assert_eq!(app.toasts.corner(), ToastCorner::BottomRight);

    app.set_options_menu_selection(8);
    assert_eq!(
        app.highlighted_option(),
        Some(OptionsMenuItem::ToastTimeout)
    );
    app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE))
        .expect("right should cycle toast timeout");
    assert_eq!(app.syntax_settings.notifications.timeout_ms, 2_500);

    app.set_options_menu_selection(9);
    assert_eq!(
        app.highlighted_option(),
        Some(OptionsMenuItem::ToastMaxVisible)
    );
    app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE))
        .expect("right should cycle toast max visible");
    assert_eq!(app.syntax_settings.notifications.max_visible, 4);
}

#[test]
fn options_menu_cycles_custom_notification_values_to_nearest_choices() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.syntax_settings.notifications.timeout_ms = 2_000;
    app.syntax_settings.notifications.max_visible = 10;

    app.open_options_menu();
    app.set_options_menu_selection(8);
    assert_eq!(
        app.highlighted_option(),
        Some(OptionsMenuItem::ToastTimeout)
    );
    app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE))
        .expect("right should cycle custom toast timeout to next choice");
    assert_eq!(app.syntax_settings.notifications.timeout_ms, 2_500);
    assert_eq!(
        app.last_persisted_options_menu_draft
            .expect("timeout change should be persisted")
            .0
            .toast_timeout_ms,
        2_500
    );

    app.set_options_menu_selection(9);
    assert_eq!(
        app.highlighted_option(),
        Some(OptionsMenuItem::ToastMaxVisible)
    );
    app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE))
        .expect("right should cycle custom toast max visible to nearest choice");
    assert_eq!(app.syntax_settings.notifications.max_visible, 5);
    assert_eq!(
        app.last_persisted_options_menu_draft
            .expect("max visible change should be persisted")
            .0
            .toast_max_visible,
        5
    );
}

#[test]
fn line_wrapping_wraps_long_unified_rows() {
    let changeset = changeset_with_line_text("abcdefghijkl");
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.line_wrapping = true;

    let row_index = 2;
    let row = app.model.row(row_index).expect("diff line should exist");
    let lines = render_row_wrapped_with_focus(&mut app, row_index, row, 18, None);
    let rendered = lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>();

    assert_eq!(rendered.len(), 3);
    assert!(rendered[0].contains("abcd"));
    assert!(rendered[1].contains("efgh"));
    assert!(rendered[2].contains("ijkl"));
}

#[test]
fn line_wrapping_preserves_wide_glyphs_at_unified_wrap_boundary() {
    let changeset = changeset_with_line_text("abc界def");
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.line_wrapping = true;

    let row_index = 2;
    let row = app.model.row(row_index).expect("diff line should exist");
    let lines = render_row_wrapped_with_focus(&mut app, row_index, row, 18, None);
    let rendered = lines.iter().map(line_text).collect::<Vec<_>>();

    assert_eq!(rendered.len(), 3);
    assert!(rendered[0].contains("abc"));
    assert!(rendered[1].contains("界de"));
    assert!(rendered[2].contains('f'));
}

#[test]
fn line_wrapping_preserves_wide_glyphs_at_split_wrap_boundary() {
    let changeset = changeset_with_line_text("abc界def");
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Split);
    app.line_wrapping = true;

    let row_index = 2;
    let row = app.model.row(row_index).expect("diff line should exist");
    let lines = render_row_wrapped_with_focus(&mut app, row_index, row, 24, None);
    let rendered = lines.iter().map(line_text).collect::<Vec<_>>();

    assert_eq!(rendered.len(), 3);
    assert!(rendered[0].contains("abc"));
    assert!(rendered[1].contains("界de"));
    assert!(rendered[2].contains('f'));
}

#[test]
fn line_wrapping_scrolls_through_continuation_rows() {
    let changeset = changeset_with_line_text("abcdefghijklmnopqrstuvwx");
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.line_wrapping = true;
    app.set_viewport_width(18);
    app.set_viewport_rows(4);

    assert_eq!(app.model.len(), 3);
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
    app.line_wrapping = true;
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
    app.line_wrapping = true;
    app.set_viewport_width(18);
    app.set_viewport_rows(4);
    app.set_scroll(app.max_scroll());

    let previous_scroll = app.scroll;
    assert!(previous_scroll > 0);

    app.apply_responsive_layout(80);

    assert!(previous_scroll > app.max_scroll());
    assert_eq!(app.scroll, app.max_scroll());
    let lines = build_diff_viewport_lines(&mut app, 80, 4);
    assert!(!lines.is_empty());
    assert!(
        lines
            .iter()
            .any(|line| line_text(line).contains("abcdefghijklmnopqrstuvwx"))
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
        .model
        .rows
        .iter()
        .position(|row| matches!(row, UiRow::UnifiedLine { .. }))
        .expect("unified line");
    let key = AnnotationKey::from_ui_row(
        &app.changeset,
        app.model.row(code_row).expect("annotated row"),
    )
    .expect("annotation key");
    app.annotations.insert(
        key,
        "one two three four five six seven eight nine ten".to_owned(),
    );

    app.set_scroll(app.max_scroll());
    let narrow_scroll = app.scroll;
    assert!(narrow_scroll > 0);

    app.set_viewport_width(80);

    assert!(narrow_scroll > app.max_scroll());
    assert_eq!(app.scroll, app.max_scroll());
    let lines = build_diff_viewport_lines(&mut app, 80, 6);
    assert!(lines.iter().any(|line| line_text(line).contains("hello")));
}

#[test]
fn select_file_scrolls_to_visual_file_start_for_wrapped_no_hunk_file() {
    let mut changeset = changeset_with_wrapped_leading_file();
    changeset.files[1].hunks.clear();
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.line_wrapping = true;
    app.set_viewport_width(18);

    app.select_file(1);

    assert_eq!(app.selected_file, 1);
    assert_eq!(app.scroll, wrapped_file_start_scroll(&app, 1));
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
    replacement.files[1].hunks[0].lines[0].text = "updated target".to_owned();

    app.replace_loaded_diff(DiffOptions::default(), replacement);

    assert_eq!(
        app.scroll,
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
        app.scroll,
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
        scope: DiffScope::Staged,
        ..DiffOptions::default()
    };
    let mut replacement = changeset_with_wrapped_leading_file();
    replacement.files[1].hunks[0].lines[0].text = "cached target".to_owned();

    app.replace_cached_diff(
        options.clone(),
        diff_cache_entry(options, replacement),
        false,
    );

    assert_eq!(
        app.scroll,
        wrapped_file_start_scroll(&app, 1).saturating_add(relative_scroll)
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
    app.syntax = Some(syntax_runtime_with_queue(SyntaxWorkerQueue::new(
        1,
        app.generation,
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

    assert!(app.syntax.is_none());
    assert_eq!(
        app.filtered_options_menu_items(),
        vec![OptionsMenuItem::LiveReload]
    );
    assert_eq!(app.options_menu_selected, 0);
    assert_eq!(app.highlighted_option(), Some(OptionsMenuItem::LiveReload));

    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .expect("enter should activate the rendered highlighted setting");
    assert!(!app.live_updates_enabled);
}

#[test]
fn options_menu_colorscheme_input_selects_draft_and_applies_on_enter() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.color_scheme = ColorSchemeChoice::System;
    app.theme = DiffTheme::system();

    app.open_options_menu();
    app.move_options_menu_selection(5);
    assert_eq!(app.highlighted_option(), Some(OptionsMenuItem::ColorScheme));

    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .expect("enter should open colorscheme input");
    assert!(app.color_scheme_picker_open);
    for character in ['t', 'o', 'k', 'y', 'o'] {
        app.handle_key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE))
            .expect("typing should filter colorschemes");
    }
    assert_eq!(app.color_scheme, ColorSchemeChoice::Tokyonight);
    assert_eq!(app.theme.background, DiffTheme::tokyonight().background);
    assert_eq!(
        app.options_menu_draft.color_scheme,
        ColorSchemeChoice::System
    );
    assert_eq!(
        app.filtered_color_schemes(),
        vec![ColorSchemeChoice::Tokyonight]
    );

    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .expect("enter should select colorscheme draft");
    assert!(!app.color_scheme_picker_open);
    assert!(app.options_menu_open);
    assert_eq!(
        app.options_menu_draft.color_scheme,
        ColorSchemeChoice::Tokyonight
    );
    assert_eq!(app.color_scheme, ColorSchemeChoice::Tokyonight);
    assert_eq!(app.theme.background, DiffTheme::tokyonight().background);
    assert!(app.pending_diff_load.is_none());
}

#[test]
fn colorscheme_picker_mouse_dismiss_keeps_options_menu_open() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);

    app.open_options_menu();
    app.move_options_menu_selection(5);
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .expect("enter should open colorscheme picker");
    assert!(app.color_scheme_picker_open);

    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 0,
        row: 0,
        modifiers: KeyModifiers::NONE,
    })
    .expect("mouse click should dismiss colorscheme picker");

    assert!(!app.color_scheme_picker_open);
    assert!(app.options_menu_open);
}

#[test]
fn options_menu_omits_branch_options_for_branch_diff() {
    let options = DiffOptions {
        source: DiffSource::Branch {
            base: "main".to_owned(),
            head: "feature".to_owned(),
        },
        ..DiffOptions::default()
    };
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(options, changeset, DiffLayoutMode::Unified);
    app.branch_base = Some("main".to_owned());
    app.branch_head = Some("feature".to_owned());
    app.current_head = Some("feature".to_owned());
    app.comparison_branches = vec!["main".to_owned(), "feature".to_owned()];

    app.open_options_menu();

    assert_eq!(app.highlighted_option(), Some(OptionsMenuItem::Layout));
    assert_eq!(
        app.options_menu_items(),
        [
            OptionsMenuItem::Layout,
            OptionsMenuItem::LiveReload,
            OptionsMenuItem::ContextExpansion,
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
            base: "main".to_owned(),
            head: "feature".to_owned(),
        },
        ..DiffOptions::default()
    };
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(options, changeset, DiffLayoutMode::Unified);
    app.branch_base = Some("main".to_owned());
    app.branch_head = Some("feature".to_owned());
    app.current_head = Some("feature".to_owned());
    app.comparison_branches = vec!["main".to_owned(), "feature".to_owned()];

    app.open_options_menu();
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .expect("enter should toggle first setting");
    assert!(app.options_menu_open);
    assert!(app.branch_menu_open.is_none());
}

#[test]
fn options_menu_live_reload_toggles_without_reloading_diff() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    assert!(app.live_updates_enabled);

    app.open_options_menu();
    app.move_options_menu_selection(1);
    assert_eq!(app.highlighted_option(), Some(OptionsMenuItem::LiveReload));
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .expect("enter should toggle live reload");

    assert!(!app.live_updates_enabled);
    assert!(app.pending_diff_load.is_none());
}

#[test]
fn options_menu_reenabling_live_reload_reloads_diff() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.live_updates_enabled = false;

    app.open_options_menu();
    app.move_options_menu_selection(1);
    assert_eq!(app.highlighted_option(), Some(OptionsMenuItem::LiveReload));
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .expect("enter should toggle live reload");

    assert!(app.live_updates_enabled);
    let load = app
        .pending_diff_load
        .as_ref()
        .expect("reenabling live reload should queue a fresh load");
    assert_eq!(load.options, DiffOptions::default());
}

#[test]
fn options_menu_does_not_enable_live_reload_when_watch_is_disabled() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.live_updates_allowed = false;
    app.live_updates_enabled = false;

    app.open_options_menu();
    app.move_options_menu_selection(1);
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .expect("enter should be handled");

    assert!(!app.options_menu_draft.live_updates_enabled);
    assert_eq!(
        app.error_log.as_deref(),
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
    assert!(rows.iter().any(|row| row.contains("> │")));
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
    app.set_options_menu_selection(5);
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 5))
        .expect("test terminal should be created");

    terminal
        .draw(|frame| crate::render::draw(frame, &mut app))
        .expect("options menu draw should succeed");

    let rows = buffer_rows(terminal.backend().buffer());
    assert!(app.options_menu_scroll > 0);
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
        source: DiffSource::Base("branch-00".to_owned()),
        ..DiffOptions::default()
    };
    let mut app = DiffApp::new(
        options,
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.branch_base = Some("branch-00".to_owned());
    app.branch_head = Some("branch-01".to_owned());
    app.current_head = Some("branch-01".to_owned());
    app.comparison_branches = (0..59).map(|index| format!("branch-{index:02}")).collect();
    app.toggle_branch_menu(BranchMenu::Base);
    app.set_branch_selection(usize::MAX);

    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 60))
        .expect("test terminal should be created");
    terminal
        .draw(|frame| crate::render::draw(frame, &mut app))
        .expect("branch menu draw should succeed");

    let menu_area = app
        .rendered_branch_menu_area
        .expect("branch menu should render");
    let inner = branch_menu_block(app.theme, BranchMenu::Base).inner(menu_area);
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
    app.keymap = keymap;

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
    assert!(rows.iter().any(|row| row.contains("> │")));
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
    app.move_options_menu_selection(5);
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
    assert!(rows.iter().any(|row| row.contains("> g│")));
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
        app.theme.muted
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
    app.options_menu_draft.color_scheme = ColorSchemeChoice::System;
    app.open_color_scheme_picker();

    app.move_color_scheme_selection(9);

    assert_eq!(app.color_scheme_selected, 9);
    assert_eq!(app.color_scheme_scroll, 0);
}

#[test]
fn colorscheme_picker_previews_hovered_theme_and_reverts_on_close() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.open_options_menu();
    app.move_options_menu_selection(5);
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

    assert_eq!(app.color_scheme, ColorSchemeChoice::GruvboxDark);
    assert_eq!(app.theme.background, DiffTheme::gruvbox_dark().background);

    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 0,
        row: 0,
        modifiers: KeyModifiers::NONE,
    })
    .expect("outside click should close colorscheme picker");

    assert!(!app.color_scheme_picker_open);
    assert_eq!(app.color_scheme, ColorSchemeChoice::System);
    assert_eq!(app.theme, DiffTheme::system());
}

#[test]
fn colorscheme_picker_previews_first_hovered_theme() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.color_scheme = ColorSchemeChoice::System;
    app.theme = DiffTheme::system();
    app.open_options_menu();
    app.move_options_menu_selection(5);
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
    assert_eq!(app.color_scheme_selected, 0);

    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Moved,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    })
    .expect("hover should preview first colorscheme");

    assert_eq!(app.color_scheme_selected, 0);
    assert_eq!(app.color_scheme, ColorSchemeChoice::CatppuccinLatte);
    assert_eq!(
        app.theme.background,
        DiffTheme::catppuccin_latte().background
    );
}

#[test]
fn number_keys_do_not_switch_diff_choice() {
    let mut app = DiffApp::new(
        DiffOptions::default(),
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.branch_base = Some("origin/main".to_owned());
    app.current_head = Some("feature".to_owned());

    let should_quit = app
        .handle_key(KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE))
        .expect("number key should be handled");

    assert!(!should_quit);
    assert!(app.pending_diff_load.is_none());
    assert_eq!(app.options.source, DiffSource::Worktree);
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
        .pending_diff_load
        .as_ref()
        .expect("tab should queue diff load");
    assert_eq!(load.options.source, DiffSource::Show("HEAD".to_owned()));
    assert_eq!(load.options.scope, DiffScope::All);

    app.pending_diff_load = None;
    app.options = DiffOptions {
        source: DiffSource::Show("HEAD".to_owned()),
        ..DiffOptions::default()
    };
    app.handle_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT))
        .expect("shift-tab should cycle diff type backwards");

    let load = app
        .pending_diff_load
        .as_ref()
        .expect("shift-tab should queue diff load");
    assert_eq!(load.options.source, DiffSource::Worktree);
    assert_eq!(load.options.scope, DiffScope::All);
}

#[test]
fn tab_from_review_diff_cycles_to_all_changes() {
    let options = DiffOptions {
        source: DiffSource::Patch(PatchSource::Text {
            label: "review owner/repo#123".to_owned(),
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
        .pending_diff_load
        .as_ref()
        .expect("tab should queue all-changes diff load");
    assert_eq!(load.options.source, DiffSource::Worktree);
    assert_eq!(load.options.scope, DiffScope::All);
}

#[test]
fn tab_from_pending_review_load_cancels_to_current_all_changes() {
    let mut app = DiffApp::new(
        DiffOptions::default(),
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.pending_review_load = Some(pending_review_load());

    let should_quit = app
        .handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
        .expect("tab should cycle pending review to all changes");

    assert!(!should_quit);
    assert!(app.pending_review_load.is_none());
    assert!(app.pending_diff_load.is_none());
    assert_eq!(app.options, DiffOptions::default());
}

#[test]
fn cached_tab_key_switches_diff_choice_without_loading() {
    let mut app = DiffApp::new(
        DiffOptions::default(),
        changeset_with_files(&["all.rs"]),
        DiffLayoutMode::Unified,
    );
    let unstaged = DiffOptions {
        scope: DiffScope::Unstaged,
        ..DiffOptions::default()
    };
    let cached_changeset = changeset_with_files(&["unstaged.rs"]);
    app.cache_loaded_diff(unstaged.clone(), cached_changeset.clone());

    app.select_diff_choice(DiffChoice::Unstaged);

    assert!(app.pending_diff_load.is_none());
    assert_eq!(app.options, unstaged);
    assert_eq!(app.base_changeset, cached_changeset);
    assert_eq!(visible_paths(&app), vec!["unstaged.rs"]);
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

    app.file_filter = "filtered".to_owned();
    app.apply_filters(false);
    assert_eq!(visible_paths(&app), vec!["filtered.rs"]);

    app.file_filter.clear();
    app.file_filter_input.clear();
    app.filter_searching = true;

    app.select_diff_choice(DiffChoice::Unstaged);
    assert_eq!(app.options, unstaged);
    assert_eq!(visible_paths(&app), vec!["unstaged.rs"]);

    app.select_diff_choice(DiffChoice::All);
    assert_eq!(app.options, DiffOptions::default());
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
    app.live_updates_allowed = false;
    app.live_updates_enabled = false;

    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
        .expect("tab should cycle to show");
    app.pending_diff_load = None;
    app.options = DiffOptions {
        source: DiffSource::Show("HEAD".to_owned()),
        ..DiffOptions::default()
    };

    let should_quit = app
        .handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
        .expect("tab should cycle diff type");

    assert!(!should_quit);
    let load = app
        .pending_diff_load
        .as_ref()
        .expect("tab should queue a fresh diff load");
    assert_eq!(load.options, unstaged);
    assert_eq!(app.options.source, DiffSource::Show("HEAD".to_owned()));
    assert_eq!(visible_paths(&app), vec!["all.rs"]);
    assert!(app.diff_cache.is_empty());
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
    app.pending_diff_load = None;
    app.options = DiffOptions {
        source: DiffSource::Show("HEAD".to_owned()),
        ..DiffOptions::default()
    };

    let should_quit = app
        .handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
        .expect("tab should cycle diff type");

    assert!(!should_quit);
    let load = app
        .pending_diff_load
        .as_ref()
        .expect("tab should queue a fresh diff load");
    assert_eq!(load.options, unstaged);
    assert_eq!(app.options.source, DiffSource::Show("HEAD".to_owned()));
    assert_eq!(visible_paths(&app), vec!["all.rs"]);
    assert!(app.diff_cache.is_empty());
    assert!(app.live_reload_pending);
}

#[test]
fn diff_prefetch_skips_when_live_reload_is_disabled() {
    let mut app = DiffApp::new(
        DiffOptions::default(),
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.live_updates_allowed = false;
    app.live_updates_enabled = false;

    app.start_diff_prefetches();

    assert!(app.pending_diff_prefetch.is_none());
    assert!(app.diff_prefetch_queue.is_empty());
    assert!(!app.diff_prefetch_started);
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

    assert!(app.pending_diff_prefetch.is_none());
    assert!(app.diff_prefetch_queue.is_empty());
    assert!(!app.diff_prefetch_started);
}

#[test]
fn repeated_tab_uses_pending_diff_choice_for_next_target() {
    let mut app = DiffApp::new(
        DiffOptions::default(),
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );

    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
        .expect("tab should queue show");
    app.pending_diff_load = None;
    app.options = DiffOptions {
        source: DiffSource::Show("HEAD".to_owned()),
        ..DiffOptions::default()
    };
    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
        .expect("tab should queue unstaged");
    app.pending_diff_load = None;
    app.options = DiffOptions {
        scope: DiffScope::Unstaged,
        ..DiffOptions::default()
    };
    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
        .expect("tab should advance to staged");

    let load = app
        .pending_diff_load
        .as_ref()
        .expect("third tab should queue diff load");
    assert_eq!(load.options.source, DiffSource::Worktree);
    assert_eq!(load.options.scope, DiffScope::Staged);
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
    assert!(app.pending_diff_load.is_some());

    app.handle_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT))
        .expect("shift-tab should return to current diff type");

    assert_eq!(app.options, DiffOptions::default());
    assert!(app.pending_diff_load.is_none());
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

    assert!(app.diff_cache.is_empty());
    assert!(app.pending_diff_prefetch.is_none());
    assert!(app.diff_prefetch_queue.is_empty());
    assert!(!app.diff_prefetch_started);
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
    app.pending_diff_load = Some(pending_diff_load(pending_options.clone()));

    app.invalidate_diff_cache();

    assert_eq!(
        app.pending_diff_load.as_ref().map(|load| &load.options),
        Some(&pending_options)
    );
}

#[test]
fn number_key_does_not_switch_show_source_diff_choice() {
    let options = DiffOptions {
        source: DiffSource::Show("HEAD".to_owned()),
        ..DiffOptions::default()
    };
    let mut app = DiffApp::new(
        options.clone(),
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.branch_base = Some("origin/main".to_owned());
    app.current_head = Some("feature".to_owned());

    let should_quit = app
        .handle_key(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE))
        .expect("number key should be handled");

    assert!(!should_quit);
    assert!(app.pending_diff_load.is_none());
    assert_eq!(app.options, options);
}

#[test]
fn diff_menu_options_preserve_repo_and_untracked_setting() {
    let options = DiffOptions {
        repo: Some(PathBuf::from("/repo")),
        include_untracked: false,
        ..DiffOptions::default()
    };
    let mut app = DiffApp::new(
        options.clone(),
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.branch_base = Some("origin/main".to_owned());
    app.branch_head = Some("feature/ui".to_owned());
    app.current_head = Some("feature/ui".to_owned());

    let staged = app.options_for_choice(DiffChoice::Staged).unwrap();
    assert_eq!(staged.repo, options.repo);
    assert!(!staged.include_untracked);
    assert_eq!(staged.source, DiffSource::Worktree);
    assert_eq!(staged.scope, DiffScope::Staged);

    let branch = app.options_for_choice(DiffChoice::Branch).unwrap();
    assert_eq!(branch.source, DiffSource::Base("origin/main".to_owned()));
    assert_eq!(branch.scope, DiffScope::All);
}

#[test]
fn branch_choice_survives_switching_to_worktree_scope() {
    let options = DiffOptions {
        source: DiffSource::Base("origin/main".to_owned()),
        ..DiffOptions::default()
    };
    let mut app = DiffApp::new(
        options,
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.branch_base = Some("origin/main".to_owned());
    app.branch_head = Some("feature/header".to_owned());

    app.replace_loaded_diff(DiffOptions::default(), changeset_with_context_lines(1));

    assert_eq!(app.branch_base.as_deref(), Some("origin/main"));
    assert_eq!(app.branch_head.as_deref(), Some("feature/header"));
    assert_eq!(
        app.options_for_choice(DiffChoice::Branch)
            .map(|options| options.source),
        Some(DiffSource::Branch {
            base: "origin/main".to_owned(),
            head: "feature/header".to_owned(),
        })
    );
}

#[test]
fn branch_header_exposes_head_and_base_selectors() {
    let options = DiffOptions {
        source: DiffSource::Base("origin/main".to_owned()),
        ..DiffOptions::default()
    };
    let mut app = DiffApp::new(
        options,
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.branch_head = Some("feature/ui".to_owned());
    app.branch_base = Some("origin/main".to_owned());
    app.current_head = Some("feature/ui".to_owned());

    assert_eq!(
        app.branch_selector_text(BranchMenu::Head).as_deref(),
        Some("● feature/ui ▾")
    );
    assert_eq!(
        app.branch_selector_text(BranchMenu::Base).as_deref(),
        Some("⌂ origin/main ▾")
    );
    assert_eq!(
        app.branch_selector_at(diff_selector_width(&app.options)),
        None
    );
    assert_eq!(
        app.branch_selector_at(
            diff_selector_width(&app.options) + STATUSLINE_SELECTOR_GAP.width() as u16
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
        source: DiffSource::Base("main".to_owned()),
        ..DiffOptions::default()
    };
    let mut app = DiffApp::new(
        options,
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.branch_base = Some("main".to_owned());
    app.branch_head = Some("feature".to_owned());
    app.current_head = Some("feature".to_owned());
    app.comparison_branches = vec!["main".to_owned(), "feature".to_owned()];
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
    assert!(rows.iter().any(|row| row.contains("> m│")));
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
        source: DiffSource::Base("main".to_owned()),
        ..DiffOptions::default()
    };
    let mut app = DiffApp::new(
        options,
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.branch_base = Some("main".to_owned());
    app.branch_head = Some("feature".to_owned());
    app.current_head = Some("feature".to_owned());
    app.comparison_branches = vec![
        "release/2026".to_owned(),
        "release/2025".to_owned(),
        "topic-a".to_owned(),
    ];

    app.toggle_branch_menu(BranchMenu::Head);
    for character in "release/".chars() {
        app.push_branch_input(character);
    }
    app.handle_key(KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE))
        .expect("2 should filter branch names");

    assert_eq!(app.branch_menu_open, Some(BranchMenu::Head));
    assert_eq!(app.branch_menu_input, "release/2");
    assert_eq!(
        app.filtered_branches(),
        vec!["release/2026", "release/2025"]
    );
    assert!(app.pending_diff_load.is_none());
}

#[test]
fn branch_menu_ctrl_n_and_ctrl_p_cycle_selection_from_input() {
    let options = DiffOptions {
        source: DiffSource::Base("main".to_owned()),
        ..DiffOptions::default()
    };
    let mut app = DiffApp::new(
        options,
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.branch_base = Some("main".to_owned());
    app.branch_head = Some("feature".to_owned());
    app.current_head = Some("feature".to_owned());
    app.comparison_branches = vec!["main".to_owned(), "feature".to_owned(), "topic".to_owned()];

    app.toggle_branch_menu(BranchMenu::Base);
    app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL))
        .expect("ctrl-n should move branch selection");
    assert_eq!(app.branch_menu_selected, 1);

    app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL))
        .expect("ctrl-p should move branch selection");
    assert_eq!(app.branch_menu_selected, 0);
    assert!(app.branch_menu_input.is_empty());
}

#[test]
fn branch_menu_scrolls_visible_branch_window() {
    let options = DiffOptions {
        source: DiffSource::Base("branch-00".to_owned()),
        ..DiffOptions::default()
    };
    let mut app = DiffApp::new(
        options,
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.comparison_branches = (0..12).map(|index| format!("branch-{index:02}")).collect();

    assert_eq!(app.max_branch_menu_scroll(), 0);

    app.move_branch_selection(99);
    assert_eq!(app.branch_menu_selected, 10);
    assert_eq!(app.branch_menu_scroll, 0);

    app.move_branch_selection(-1);
    assert_eq!(app.branch_menu_selected, 9);
    assert_eq!(app.branch_menu_scroll, 0);
}

#[test]
fn branch_menu_expands_to_show_long_branch_when_terminal_allows() {
    let options = DiffOptions {
        source: DiffSource::Base("main".to_owned()),
        ..DiffOptions::default()
    };
    let mut app = DiffApp::new(
        options,
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    let long_branch = "feature/really-long-branch-name-that-should-fit-without-being-cut-off";
    app.branch_base = Some("main".to_owned());
    app.branch_head = Some("feature".to_owned());
    app.current_head = Some("feature".to_owned());
    app.comparison_branches = vec!["main".to_owned(), long_branch.to_owned()];
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
        source: DiffSource::Base("branch-00".to_owned()),
        ..DiffOptions::default()
    };
    let mut app = DiffApp::new(
        options,
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.branch_base = Some("branch-00".to_owned());
    app.branch_head = Some("branch-01".to_owned());
    app.current_head = Some("branch-01".to_owned());
    app.comparison_branches = (0..12).map(|index| format!("branch-{index:02}")).collect();
    app.toggle_branch_menu(BranchMenu::Base);
    app.move_branch_selection(5);
    assert_eq!(app.branch_menu_scroll, 0);

    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 8))
        .expect("test terminal should be created");
    terminal
        .draw(|frame| crate::render::draw(frame, &mut app))
        .expect("branch menu draw should succeed");

    assert_eq!(app.branch_menu_selected, 5);
    assert_eq!(app.branch_menu_scroll, 3);
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
        source: DiffSource::Base("branch-00".to_owned()),
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
    app.branch_base = Some("branch-00".to_owned());
    app.branch_head = Some("branch-01".to_owned());
    app.current_head = Some("branch-01".to_owned());
    app.comparison_branches = (0..40).map(|index| format!("branch-{index:02}")).collect();
    app.toggle_branch_menu(BranchMenu::Base);

    app.move_branch_selection(20);
    assert_eq!(app.branch_menu_selected, 20);
    assert_eq!(app.branch_menu_scroll, 0);

    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 60))
        .expect("test terminal should be created");
    terminal
        .draw(|frame| crate::render::draw(frame, &mut app))
        .expect("branch menu draw should succeed");

    assert_eq!(app.branch_menu_scroll, 0);
    let rows = buffer_rows(terminal.backend().buffer());
    assert!(rows.iter().any(|row| row.contains("branch-01")));
}

#[test]
fn commit_menu_scrolls_to_rendered_rows_and_highlights_selection() {
    let options = DiffOptions {
        source: DiffSource::Show("HEAD".to_owned()),
        ..DiffOptions::default()
    };
    let mut app = DiffApp::new(
        options,
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.show_rev = Some("ccccccc".to_owned());
    app.comparison_commits = (0..12)
        .map(|index| GitCommit {
            sha: format!("{index:07x}"),
            subject: format!("commit-{index:02}"),
        })
        .collect();
    app.toggle_commit_menu();
    app.set_commit_selection(5);
    assert_eq!(app.commit_menu_scroll, 0);

    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 8))
        .expect("test terminal should be created");
    terminal
        .draw(|frame| crate::render::draw(frame, &mut app))
        .expect("commit menu draw should succeed");

    assert_eq!(app.commit_menu_selected, 5);
    assert!(app.commit_menu_scroll > 0);
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
        source: DiffSource::Show("HEAD".to_owned()),
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
    app.show_rev = Some("0000000".to_owned());
    app.comparison_commits = (0..40)
        .map(|index| GitCommit {
            sha: format!("{index:07x}"),
            subject: format!("commit-{index:02}"),
        })
        .collect();
    app.toggle_commit_menu();

    app.move_commit_selection(20);
    assert_eq!(app.commit_menu_selected, 20);
    assert_eq!(app.commit_menu_scroll, 0);

    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 60))
        .expect("test terminal should be created");
    terminal
        .draw(|frame| crate::render::draw(frame, &mut app))
        .expect("commit menu draw should succeed");

    assert_eq!(app.commit_menu_scroll, 0);
    let rows = buffer_rows(terminal.backend().buffer());
    assert!(
        rows.iter()
            .any(|row| row.contains("0000001") && row.contains("commit-01"))
    );
}

#[test]
fn mouse_wheel_over_commit_menu_scrolls_menu_not_diff() {
    let options = DiffOptions {
        source: DiffSource::Show("HEAD".to_owned()),
        ..DiffOptions::default()
    };
    let mut app = DiffApp::new(
        options,
        changeset_with_context_lines(60),
        DiffLayoutMode::Unified,
    );
    app.set_viewport_rows(10);
    assert!(app.max_scroll() > 0);
    app.comparison_commits = (0..12)
        .map(|index| GitCommit {
            sha: format!("{index:07x}"),
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

    assert_eq!(app.scroll, 0);
    assert_eq!(app.commit_menu_selected, 1);
}

#[test]
fn branch_combo_input_filters_and_completes() {
    let options = DiffOptions {
        source: DiffSource::Base("main".to_owned()),
        ..DiffOptions::default()
    };
    let mut app = DiffApp::new(
        options,
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.comparison_branches = vec![
        "main".to_owned(),
        "feature/header".to_owned(),
        "fix/footer".to_owned(),
    ];

    app.push_branch_input('h');
    assert_eq!(app.filtered_branches(), vec!["feature/header"]);

    app.clear_branch_input();
    app.push_branch_input('f');
    app.push_branch_input('h');
    assert_eq!(app.filtered_branches(), vec!["feature/header"]);

    app.branch_menu_open = Some(BranchMenu::Head);
    app.cycle_branch_completion(1);
    assert_eq!(app.branch_menu_selected, 0);
    assert_eq!(app.branch_menu_input, "fh");

    app.clear_branch_input();
    app.push_branch_input('f');
    assert_eq!(
        app.filtered_branches(),
        vec!["fix/footer", "feature/header"]
    );
    app.cycle_branch_completion(1);
    assert_eq!(app.branch_menu_selected, 1);
    app.cycle_branch_completion(-1);
    assert_eq!(app.branch_menu_selected, 0);

    app.clear_branch_input();
    assert!(app.branch_menu_input.is_empty());
}

#[test]
fn branch_combo_pins_current_head_and_base_before_recent_order() {
    let options = DiffOptions {
        source: DiffSource::Base("release".to_owned()),
        ..DiffOptions::default()
    };
    let mut app = DiffApp::new(
        options,
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.branch_head = Some("feature/header".to_owned());
    app.current_head = Some("feature/header".to_owned());
    app.branch_base = Some("release".to_owned());
    app.comparison_branches = vec![
        "recent".to_owned(),
        "old".to_owned(),
        "origin/main".to_owned(),
        "release".to_owned(),
        "feature/header".to_owned(),
    ];

    app.branch_menu_open = Some(BranchMenu::Base);
    assert_eq!(
        app.filtered_branches(),
        vec!["feature/header", "recent", "old", "origin/main"]
    );

    app.branch_menu_open = Some(BranchMenu::Head);
    assert_eq!(
        app.filtered_branches(),
        vec!["release", "recent", "old", "origin/main"]
    );
}

#[test]
fn branch_combo_close_clears_input_without_changing_selection() {
    let options = DiffOptions {
        source: DiffSource::Base("main".to_owned()),
        ..DiffOptions::default()
    };
    let mut app = DiffApp::new(
        options,
        changeset_with_context_lines(1),
        DiffLayoutMode::Unified,
    );
    app.branch_base = Some("main".to_owned());
    app.branch_head = Some("feature/header".to_owned());
    app.comparison_branches = vec!["main".to_owned(), "feature/header".to_owned()];

    app.toggle_branch_menu(BranchMenu::Base);
    app.push_branch_input('f');
    app.close_branch_menu();

    assert!(app.branch_menu_open.is_none());
    assert!(app.branch_menu_input.is_empty());
    assert_eq!(app.branch_base.as_deref(), Some("main"));
    assert_eq!(app.branch_head.as_deref(), Some("feature/header"));
    assert_eq!(app.options.source, DiffSource::Base("main".to_owned()));
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
fn fit_helpers_use_terminal_display_width() {
    assert_eq!(fit("界a", 2), "界");
    assert_eq!(fit_padded("e\u{301}", 2), "e\u{301} ");
    assert_eq!(fit_padded_from("abcdef", 2, 3), "cde");
    assert_eq!(skip_display_prefix("abcdef", 2), ("cdef", 2));
    assert_eq!(skip_display_prefix("e\u{301}f", 1), ("f", 1));
    assert_eq!(fit_with_ellipsis("abcdef", 5), "ab...");
}

#[test]
fn file_header_truncates_path_before_delta() {
    let file = mark_diff::DiffFile {
        old_path: Some("src/runtime/test_runner/expect/toMatchInlineSnapshot.rs".to_owned()),
        new_path: Some("src/runtime/test_runner/expect/toMatchInlineSnapshot.rs".to_owned()),
        status: FileStatus::Modified,
        hunks: Vec::new(),
        additions: 1290,
        deletions: 3910,
        is_binary: false,
    };

    let theme = DiffTheme::default();
    let line = file_header_line(&file, 32, theme);
    let text = line_text(&line);

    assert_eq!(text.width(), 32);
    assert!(text.starts_with("M "));
    assert!(text.contains("..."));
    assert!(text.ends_with("+1290 -3910"));
    assert_eq!(line.spans[0].content.as_ref(), "M");
    assert_eq!(line.spans[0].style.fg, Some(theme.hunk));
    assert!(line.spans[0].style.add_modifier.contains(Modifier::BOLD));
    assert_eq!(line.spans[2].style.fg, Some(theme.foreground));

    let additions = line
        .spans
        .iter()
        .find(|span| span.content.as_ref() == "+1290")
        .expect("additions should render as a separate span");
    let deletions = line
        .spans
        .iter()
        .find(|span| span.content.as_ref() == "-3910")
        .expect("deletions should render as a separate span");

    assert_eq!(additions.style.fg, Some(theme.addition_fg));
    assert_eq!(deletions.style.fg, Some(theme.deletion_fg));
}

#[test]
fn hunk_header_uses_raw_location_context_and_delta() {
    let hunk = mark_diff::DiffHunk {
        header: "@@ -200,2 +211,3 @@ render_diff_hunk".to_owned(),
        old_start: 200,
        old_count: 2,
        new_start: 211,
        new_count: 3,
        lines: vec![
            DiffLine {
                kind: DiffLineKind::Context,
                old_line: Some(200),
                new_line: Some(211),
                text: "context".to_owned(),
            },
            DiffLine {
                kind: DiffLineKind::Deletion,
                old_line: Some(201),
                new_line: None,
                text: "old".to_owned(),
            },
            DiffLine {
                kind: DiffLineKind::Addition,
                old_line: None,
                new_line: Some(212),
                text: "new".to_owned(),
            },
            DiffLine {
                kind: DiffLineKind::Addition,
                old_line: None,
                new_line: Some(213),
                text: "again".to_owned(),
            },
        ],
    };

    let theme = DiffTheme::default();
    let text = line_text(&Line::from(hunk_header_spans(
        &hunk,
        48,
        theme,
        line_gutter_bg(DiffLineKind::Meta, theme),
    )));

    assert_eq!(text.width(), 48);
    assert!(text.starts_with("@@ -200,2 +211,3 @@ render_diff_hunk"));
    assert!(text.ends_with("+2 -1"));
}

#[test]
fn hunk_header_line_matches_unified_gutter() {
    let hunk = mark_diff::DiffHunk {
        header: "@@ -200,2 +211,3 @@ render_diff_hunk".to_owned(),
        old_start: 200,
        old_count: 2,
        new_start: 211,
        new_count: 3,
        lines: vec![DiffLine {
            kind: DiffLineKind::Addition,
            old_line: None,
            new_line: Some(211),
            text: "new".to_owned(),
        }],
    };

    let theme = DiffTheme::default();
    let line = hunk_header_line(&hunk, 64, theme);
    let text = line_text(&line);

    assert_eq!(text.width(), 64);
    assert!(text.starts_with(&format!("{DIFF_INDICATOR} @@ -200,2 +211,3 @@")));
    assert!(text.contains("@@ -200,2 +211,3 @@ render_diff_hunk"));
    assert!(text.ends_with("+1"));
    let old_range = line
        .spans
        .iter()
        .find(|span| span.content.as_ref() == "-200,2")
        .expect("old range should render as a separate span");
    let new_range = line
        .spans
        .iter()
        .find(|span| span.content.as_ref() == "+211,3")
        .expect("new range should render as a separate span");
    let context = line
        .spans
        .iter()
        .find(|span| span.content.as_ref().contains("render_diff_hunk"))
        .expect("context should render as a separate span");
    let additions = line
        .spans
        .iter()
        .find(|span| span.content.as_ref() == "+1")
        .expect("additions should render as a separate span");

    assert_eq!(old_range.style.fg, Some(theme.deletion_fg));
    assert_eq!(new_range.style.fg, Some(theme.addition_fg));
    assert_eq!(context.style.fg, Some(theme.foreground));
    assert_eq!(additions.style.fg, Some(theme.addition_fg));
    assert!(
        line.spans
            .iter()
            .all(|span| span.style.bg == Some(line_gutter_bg(DiffLineKind::Meta, theme)))
    );
}

#[test]
fn hunk_header_truncates_context_before_delta() {
    let hunk = mark_diff::DiffHunk {
        header: "@@ -1 +1 @@ render_diff_hunk_with_a_really_long_name".to_owned(),
        old_start: 1,
        old_count: 1,
        new_start: 1,
        new_count: 1,
        lines: vec![
            DiffLine {
                kind: DiffLineKind::Deletion,
                old_line: Some(1),
                new_line: None,
                text: "old".to_owned(),
            },
            DiffLine {
                kind: DiffLineKind::Addition,
                old_line: None,
                new_line: Some(1),
                text: "new".to_owned(),
            },
        ],
    };

    let theme = DiffTheme::default();
    let text = line_text(&Line::from(hunk_header_spans(
        &hunk,
        32,
        theme,
        line_gutter_bg(DiffLineKind::Meta, theme),
    )));

    assert_eq!(text.width(), 32);
    assert!(text.starts_with("@@ -1 +1 @@ render"));
    assert!(text.contains("..."));
    assert!(text.ends_with("+1 -1"));
}

#[test]
fn hunk_header_truncates_location_without_collapsing_range_styles() {
    let hunk = mark_diff::DiffHunk {
        header: "@@ -200,2 +211,3 @@ render_diff_hunk".to_owned(),
        old_start: 200,
        old_count: 2,
        new_start: 211,
        new_count: 3,
        lines: vec![DiffLine {
            kind: DiffLineKind::Context,
            old_line: Some(200),
            new_line: Some(211),
            text: "context".to_owned(),
        }],
    };

    let theme = DiffTheme::default();
    let line = Line::from(hunk_header_spans(
        &hunk,
        17,
        theme,
        line_gutter_bg(DiffLineKind::Meta, theme),
    ));
    let text = line_text(&line);

    assert_eq!(text, "@@ -200,2 +211...");
    assert_eq!(text.width(), 17);

    let old_range = line
        .spans
        .iter()
        .find(|span| span.content.as_ref() == "-200,2")
        .expect("old range should keep its own span when truncated");
    let new_range = line
        .spans
        .iter()
        .find(|span| span.content.as_ref() == "+211")
        .expect("new range should keep its own span when truncated");

    assert_eq!(old_range.style.fg, Some(theme.deletion_fg));
    assert_eq!(new_range.style.fg, Some(theme.addition_fg));
}

#[test]
fn content_spans_fall_back_when_syntax_text_mismatches_diff_text() {
    let syntax = HighlightedLine {
        segments: vec![mark_syntax::SyntaxSegment {
            byte_start: 0,
            byte_end: 5,
            text: "wrong".to_owned(),
            class: Some(SyntaxClass::Keyword),
        }],
    };

    let spans = content_spans_at_scroll(
        "right",
        Some(&syntax),
        &[],
        DiffLineKind::Addition,
        8,
        DiffTheme::default(),
        0,
    );
    let text = spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect::<String>();

    assert_eq!(text, "right   ");
    assert_eq!(spans.len(), 1);
}

#[test]
fn empty_diff_fill_draws_shifted_diagonal_pattern() {
    assert_eq!(empty_diff_fill_from(8, 0, 0), "╱  ╱  ╱ ");
    assert_eq!(empty_diff_fill_from(8, 1, 0), "  ╱  ╱  ");
    assert_eq!(empty_diff_fill_from(8, 2, 0), " ╱  ╱  ╱");
}

#[test]
fn split_empty_cells_use_default_gutter_and_hatched_fill() {
    let spans = split_cell_spans_at_scroll(
        None,
        None,
        &[],
        SplitCellRender {
            side: SplitSide::Old,
            row_index: 0,
            width: 12,
            theme: DiffTheme::default(),
        },
        0,
    );

    assert_eq!(span_text(&spans), "▌        ╱  ");
    assert_eq!(spans[0].content.as_ref(), DIFF_INDICATOR);
    assert_eq!(spans[0].style.fg, Some(DiffTheme::default().muted));
    assert_eq!(spans[0].style.bg, Some(DiffTheme::default().gutter_bg));
    assert_eq!(spans[1].content.as_ref(), "       ");
    assert_eq!(spans[1].style.bg, Some(DiffTheme::default().gutter_bg));
    assert_eq!(spans[2].style.fg, Some(DiffTheme::default().empty_diff));
}

#[test]
fn split_wrapped_empty_cells_follow_visual_rows() {
    let changeset = Changeset {
        repo: PathBuf::from("/repo"),
        title: "test".to_owned(),
        files: vec![mark_diff::DiffFile {
            old_path: Some("file.rs".to_owned()),
            new_path: Some("file.rs".to_owned()),
            status: FileStatus::Modified,
            hunks: vec![mark_diff::DiffHunk {
                header: "@@ -0,0 +1,2 @@".to_owned(),
                old_start: 0,
                old_count: 0,
                new_start: 1,
                new_count: 2,
                lines: vec![
                    DiffLine {
                        kind: DiffLineKind::Addition,
                        old_line: None,
                        new_line: Some(1),
                        text: "abcdefgh".to_owned(),
                    },
                    DiffLine {
                        kind: DiffLineKind::Addition,
                        old_line: None,
                        new_line: Some(2),
                        text: "ijkl".to_owned(),
                    },
                ],
            }],
            additions: 2,
            deletions: 0,
            is_binary: false,
        }],
        raw_patch: Vec::new(),
    };
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Split);
    app.line_wrapping = true;
    app.set_viewport_width(24);

    let first_row = app.model.row(2).expect("first addition row should exist");
    let second_row = app.model.row(3).expect("second addition row should exist");
    let first = render_row_wrapped_with_focus(&mut app, 2, first_row, 24, None);
    let second = render_row_wrapped_with_focus(&mut app, 3, second_row, 24, None);

    let left_width = 12usize;
    let content_offset = 1 + GUTTER_WIDTH.min(left_width.saturating_sub(1));
    let content_width = split_cell_content_width(left_width);
    let left_fill = |line: &Line<'_>| {
        line_text(line)
            .chars()
            .skip(content_offset)
            .take(content_width)
            .collect::<String>()
    };
    let first_visual_row = app.wrapped_visual_scroll_for_model_row(2);
    let second_visual_row = app.wrapped_visual_scroll_for_model_row(3);

    assert_eq!(first.len(), 2);
    assert_eq!(second.len(), 1);
    assert_eq!(first_visual_row, 2);
    assert_eq!(second_visual_row, 4);
    assert_eq!(
        left_fill(&first[0]),
        empty_diff_fill_from(content_width, first_visual_row, content_offset)
    );
    assert_eq!(
        left_fill(&first[1]),
        empty_diff_fill_from(content_width, first_visual_row + 1, content_offset)
    );
    assert_eq!(
        left_fill(&second[0]),
        empty_diff_fill_from(content_width, second_visual_row, content_offset)
    );
    assert_ne!(left_fill(&first[0]), left_fill(&first[1]));
}

#[test]
fn line_gutters_use_theme_background() {
    let theme = DiffTheme::default();
    let line = DiffLine {
        kind: DiffLineKind::Context,
        old_line: Some(7),
        new_line: Some(7),
        text: "same".to_owned(),
    };

    let rendered = render_unified_line_at_scroll(&line, None, &[], 0, 24, theme, 0);

    assert_eq!(rendered.spans[0].style.fg, Some(theme.muted));
    assert_eq!(rendered.spans[0].style.bg, Some(theme.gutter_bg));
    assert_eq!(rendered.spans[1].style.fg, Some(theme.foreground));
    assert_eq!(rendered.spans[1].style.bg, Some(theme.gutter_bg));
}

#[test]
fn changed_line_gutters_use_delta_colors_and_bold_signs() {
    let theme = DiffTheme::default();
    let line = DiffLine {
        kind: DiffLineKind::Addition,
        old_line: None,
        new_line: Some(7),
        text: "added".to_owned(),
    };

    let rendered = render_unified_line_at_scroll(&line, None, &[], 0, 24, theme, 0);

    assert_eq!(rendered.spans[0].style.bg, Some(theme.addition_gutter_bg));
    assert_eq!(rendered.spans[1].style.fg, Some(theme.addition_fg));
    assert_eq!(rendered.spans[1].style.bg, Some(theme.addition_gutter_bg));
    assert_eq!(rendered.spans[2].content.as_ref(), "+");
    assert_eq!(rendered.spans[2].style.fg, Some(theme.addition_fg));
    assert_eq!(rendered.spans[2].style.bg, Some(theme.addition_gutter_bg));
    assert!(
        rendered.spans[2]
            .style
            .add_modifier
            .contains(Modifier::BOLD)
    );
    assert_eq!(rendered.spans[3].style.fg, Some(theme.foreground));
    assert_eq!(rendered.spans[3].style.bg, Some(theme.addition_bg));
}

#[test]
fn split_view_uses_right_indicator_as_separator() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Split);

    let rendered = render_split_line_with_focus(
        &mut app,
        SplitLineRender {
            file: 0,
            hunk: 0,
            left: Some(0),
            right: Some(0),
            row_index: 0,
            width: 24,
            focused: false,
        },
    );
    let text = line_text(&rendered);

    assert!(!text.contains('│'));
    assert_eq!(text.chars().nth(12), Some('▌'));
}

#[test]
fn unified_diff_content_scrolls_horizontally() {
    let line = DiffLine {
        kind: DiffLineKind::Context,
        old_line: Some(1),
        new_line: Some(1),
        text: "abcdef".to_owned(),
    };

    let rendered = render_unified_line_at_scroll(&line, None, &[], 0, 18, DiffTheme::default(), 2);

    assert!(line_text(&rendered).ends_with("cdef"));
}

#[test]
fn split_diff_content_scrolls_horizontally() {
    let changeset = changeset_with_line_text("abcdef");
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Split);
    app.horizontal_scroll = 2;

    let rendered = render_split_line_with_focus(
        &mut app,
        SplitLineRender {
            file: 0,
            hunk: 0,
            left: Some(0),
            right: Some(0),
            row_index: 0,
            width: 24,
            focused: false,
        },
    );

    assert_eq!(line_text(&rendered), "▌    1  cdef▌    1  cdef");
}

#[test]
fn diff_lines_start_with_change_indicator() {
    let line = DiffLine {
        kind: DiffLineKind::Addition,
        old_line: None,
        new_line: Some(3),
        text: "new".to_owned(),
    };

    let rendered = render_unified_line_at_scroll(&line, None, &[], 0, 24, DiffTheme::default(), 0);

    assert_eq!(rendered.spans[0].content.as_ref(), DIFF_INDICATOR);
    assert_eq!(
        rendered.spans[0].style.fg,
        Some(DiffTheme::default().addition_fg)
    );
    assert!(!line_text(&rendered).contains(EMPTY_DIFF_FILL));
}

#[test]
fn highlighted_mouse_diff_content_line_highlights_only_code_columns() {
    let line = DiffLine {
        kind: DiffLineKind::Context,
        old_line: Some(1),
        new_line: Some(1),
        text: "code".to_owned(),
    };
    let width = 24;
    let theme = DiffTheme::default();
    let rendered = render_unified_line_at_scroll(&line, None, &[], 0, width, theme, 0);
    let highlighted = highlighted_mouse_diff_content_line(
        rendered.clone(),
        DiffLayoutMode::Unified,
        width,
        theme,
    );

    assert_eq!(highlighted.spans[0].style, rendered.spans[0].style);
    let original_code = rendered
        .spans
        .iter()
        .find(|span| span.content.as_ref().contains("code"))
        .expect("content span");
    let highlighted_code = highlighted
        .spans
        .iter()
        .find(|span| span.content.as_ref().contains("code"))
        .expect("content span");
    assert_ne!(highlighted_code.style.bg, original_code.style.bg);
    assert_eq!(highlighted_code.style.bg, Some(theme.cursor_line_bg));
    assert!(unified_content_start_column(width) > 0);
}

#[test]
fn annotation_add_button_opens_input_under_hovered_line() {
    use crate::annotation::AnnotationKey;

    let changeset = changeset_with_line_text("hello");
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_rendered_diff_area(Rect {
        x: 0,
        y: 1,
        width: 40,
        height: 10,
    });
    app.set_viewport_width(40);
    app.set_viewport_rows(5);
    app.scroll = 0;

    let code_row = app
        .model
        .rows
        .iter()
        .position(|row| matches!(row, UiRow::UnifiedLine { .. }))
        .expect("unified line");
    app.scroll = code_row;
    app.update_diff_mouse_hover(38, 1);

    let hover_lines = crate::render::diff::build_diff_viewport_lines(&mut app, 40, 3);
    let button_span = hover_lines[0].spans.last().expect("add button span");
    assert_eq!(button_span.style.bg, Some(app.theme.cursor_line_bg));

    assert!(app.handle_diff_click(38, 1));
    assert!(app.annotation_draft.is_some());

    let lines = crate::render::diff::build_diff_viewport_lines(&mut app, 40, 6);
    assert_eq!(lines.len(), 4);
    assert!(line_text(&lines[1]).starts_with("file.rs R1 "));
    assert!(line_text(&lines[1]).ends_with("[x]"));
    assert!(
        lines[1]
            .spans
            .iter()
            .any(|span| span.style.fg == Some(app.theme.hunk))
    );
    assert!(line_text(&lines[2]).contains(INPUT_CURSOR));
    let footer = line_text(&lines[3]);
    assert!(footer.starts_with("─"));
    assert!(footer.ends_with("[✓]"));
    assert_eq!(
        lines[3].spans.last().and_then(|span| span.style.fg),
        Some(app.theme.addition_fg)
    );

    app.handle_annotation_input_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
    app.handle_annotation_input_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));
    app.handle_annotation_input_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE));
    app.handle_annotation_input_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
    assert!(app.handle_diff_click(38, 4));

    assert!(app.annotation_draft.is_none());
    let row = app.model.row(code_row).expect("row");
    let key = AnnotationKey::from_ui_row(&app.changeset, row).expect("key");
    assert_eq!(app.annotations.get(&key).map(String::as_str), Some("note"));

    let lines = crate::render::diff::build_diff_viewport_lines(&mut app, 40, 6);
    let close_row = lines
        .iter()
        .position(|line| line_text(line).ends_with("[x]"))
        .expect("saved annotation close row");
    let body_row = lines
        .iter()
        .position(|line| line_text(line).contains("note"))
        .expect("saved annotation body row");
    let edit_row = lines
        .iter()
        .position(|line| line_text(line).ends_with("[↻]"))
        .expect("saved annotation edit row");
    assert!(close_row < body_row);
    assert!(body_row < edit_row);
    assert_eq!(
        lines[edit_row].spans.last().and_then(|span| span.style.fg),
        Some(app.theme.search_match_bg)
    );

    let diff_y = app.rendered_diff_area.expect("diff area").y;
    assert!(app.handle_diff_click(38, diff_y.saturating_add(edit_row as u16)));
    assert!(app.annotation_draft.is_some());
    app.handle_annotation_input_key(KeyEvent::new(KeyCode::Char('!'), KeyModifiers::NONE));
    app.handle_annotation_input_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
    assert_eq!(app.annotations.get(&key).map(String::as_str), Some("note!"));

    let lines = crate::render::diff::build_diff_viewport_lines(&mut app, 40, 6);
    let close_row = lines
        .iter()
        .position(|line| line_text(line).ends_with("[x]"))
        .expect("saved annotation close row");
    app.update_diff_mouse_hover(38, diff_y.saturating_add(close_row as u16));
    assert!(app.handle_diff_click(38, diff_y.saturating_add(close_row as u16)));
    assert!(!app.annotations.contains_key(&key));
}

#[test]
fn split_annotation_add_button_uses_current_side_for_paired_row() {
    use crate::annotation::{AnnotationKey, AnnotationSide};

    let changeset = changeset_with_replacement_pair();
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
        .model
        .rows
        .iter()
        .position(|row| matches!(row, UiRow::SplitLine { .. }))
        .expect("split line");
    app.scroll = split_row;

    let row = app.model.row(split_row).expect("row");
    let keys = AnnotationKey::candidates_from_ui_row(&app.changeset, row);
    assert_eq!(keys.len(), 1);
    let key = keys[0].clone();
    assert_eq!(key.side, AnnotationSide::New);
    let left_width = app.viewport_width / 2;
    let old_button_column = (left_width - 1) as u16;
    app.update_diff_mouse_hover(old_button_column, 1);

    let hover_lines = crate::render::diff::build_diff_viewport_lines(&mut app, 60, 3);
    let hover_text = line_text(&hover_lines[0]);
    let (old_button_text, _) = skip_display_prefix(&hover_text, left_width - 4);
    assert!(
        !old_button_text.starts_with(" [+]"),
        "paired row should not expose an old-side add button: {hover_text:?}"
    );
    assert!(hover_text.ends_with(" [+]"));
    assert!(!app.handle_diff_click(old_button_column, 1));
    assert!(app.annotation_draft.is_none());

    let new_button_column = (app.viewport_width - 1) as u16;
    app.update_diff_mouse_hover(new_button_column, 1);
    let hover_lines = crate::render::diff::build_diff_viewport_lines(&mut app, 60, 3);
    assert!(line_text(&hover_lines[0]).ends_with(" [+]"));

    assert!(app.handle_diff_click(new_button_column, 1));
    let draft = app.annotation_draft.as_ref().expect("draft");
    assert_eq!(draft.key, key);

    for character in "new note".chars() {
        app.handle_annotation_input_key(KeyEvent::new(
            KeyCode::Char(character),
            KeyModifiers::NONE,
        ));
    }
    app.handle_annotation_input_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));

    assert_eq!(
        app.annotations.get(&key).map(String::as_str),
        Some("new note")
    );
    let json = app.marks_clipboard_json().expect("marks JSON");
    assert!(json.contains("\"old_line\": 1"));
    assert!(json.contains("\"new_line\": 1"));
}

#[test]
fn annotation_pairing_spans_no_newline_meta_line() {
    use crate::annotation::{AnnotationKey, AnnotationSide};

    let mut changeset = changeset_with_replacement_pair();
    changeset.files[0].hunks[0].lines.insert(
        1,
        DiffLine {
            kind: DiffLineKind::Meta,
            old_line: None,
            new_line: None,
            text: "\\ No newline at end of file".to_owned(),
        },
    );

    let mut app = DiffApp::new(
        DiffOptions::default(),
        changeset.clone(),
        DiffLayoutMode::Unified,
    );
    let deletion_row = app
        .model
        .rows
        .iter()
        .position(|row| matches!(row, UiRow::UnifiedLine { line: 0, .. }))
        .expect("deletion line");
    let addition_row = app
        .model
        .rows
        .iter()
        .position(|row| matches!(row, UiRow::UnifiedLine { line: 2, .. }))
        .expect("addition line");

    assert_eq!(
        AnnotationKey::from_ui_row(&app.changeset, app.model.row(deletion_row).expect("row")),
        None
    );
    let key = AnnotationKey::from_ui_row(&app.changeset, app.model.row(addition_row).expect("row"))
        .expect("addition key");
    assert_eq!(key.side, AnnotationSide::New);
    app.annotations.insert(key, "note".to_owned());
    let json = app.marks_clipboard_json().expect("marks JSON");
    assert!(json.contains("\"old_line\": 1"));
    assert!(json.contains("\"new_line\": 1"));

    let split_app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Split);
    let split_deletion_row = split_app
        .model
        .rows
        .iter()
        .position(|row| {
            matches!(
                row,
                UiRow::SplitLine {
                    left: Some(0),
                    right: None,
                    ..
                }
            )
        })
        .expect("split deletion row");
    assert_eq!(
        AnnotationKey::from_ui_row(
            &split_app.changeset,
            split_app.model.row(split_deletion_row).expect("row"),
        ),
        None
    );
}

#[test]
fn split_annotation_add_button_uses_right_edge_for_deletion_only_row() {
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
        .model
        .rows
        .iter()
        .position(|row| matches!(row, UiRow::SplitLine { .. }))
        .expect("split line");
    app.scroll = split_row;

    let row = app.model.row(split_row).expect("row");
    let keys = AnnotationKey::candidates_from_ui_row(&app.changeset, row);
    assert_eq!(keys.len(), 1);
    let key = keys[0].clone();
    assert_eq!(key.side, AnnotationSide::Old);

    let left_width = app.viewport_width / 2;
    let old_button_column = (left_width - 1) as u16;
    app.update_diff_mouse_hover(old_button_column, 1);
    let hover_lines = crate::render::diff::build_diff_viewport_lines(&mut app, 60, 3);
    let hover_text = line_text(&hover_lines[0]);
    let (old_button_text, _) = skip_display_prefix(&hover_text, left_width - 4);
    assert!(
        !old_button_text.starts_with(" [+]"),
        "deletion-only row should not expose a left-side add button: {hover_text:?}"
    );
    assert!(hover_text.ends_with(" [+]"));
    assert!(!app.handle_diff_click(old_button_column, 1));
    assert!(app.annotation_draft.is_none());

    let new_button_column = (app.viewport_width - 1) as u16;
    app.update_diff_mouse_hover(new_button_column, 1);
    assert!(app.handle_diff_click(new_button_column, 1));
    let draft = app.annotation_draft.as_ref().expect("draft");
    assert_eq!(draft.key, key);
}

#[test]
fn annotation_save_preserves_body_whitespace_but_deletes_blank_drafts() {
    use crate::annotation::{AnnotationDraft, AnnotationKey};

    let changeset = changeset_with_line_text("hello");
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    let code_row = app
        .model
        .rows
        .iter()
        .position(|row| matches!(row, UiRow::UnifiedLine { .. }))
        .expect("unified line");
    let row = app.model.row(code_row).expect("row");
    let key = AnnotationKey::from_ui_row(&app.changeset, row).expect("key");

    let body = "  indented\n    code  ";
    app.annotation_draft = Some(AnnotationDraft {
        key: key.clone(),
        model_row_index: code_row,
        input: body.to_owned(),
        cursor: body.len(),
    });
    let save_key = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL);
    assert!(app.handle_annotation_input_key(save_key));
    assert_eq!(app.annotations.get(&key).map(String::as_str), Some(body));
    assert!(
        app.marks_clipboard_json()
            .expect("marks JSON")
            .contains("\"body\": \"  indented\\n    code  \"")
    );

    let blank = " \n\t ";
    app.annotation_draft = Some(AnnotationDraft {
        key: key.clone(),
        model_row_index: code_row,
        input: blank.to_owned(),
        cursor: blank.len(),
    });
    let save_key = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL);
    assert!(app.handle_annotation_input_key(save_key));
    assert!(!app.annotations.contains_key(&key));
}

#[test]
fn annotation_button_hit_tests_use_diff_relative_columns() {
    let changeset = changeset_with_line_text("hello");
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_rendered_diff_area(Rect {
        x: 10,
        y: 1,
        width: 40,
        height: 5,
    });
    app.set_viewport_width(40);
    app.set_viewport_rows(5);
    let code_row = app
        .model
        .rows
        .iter()
        .position(|row| matches!(row, UiRow::UnifiedLine { .. }))
        .expect("unified line");
    app.scroll = code_row;
    app.update_diff_mouse_hover(48, 1);

    assert!(!app.handle_diff_click(38, 1));
    assert!(app.annotation_draft.is_none());

    assert!(app.handle_diff_click(48, 1));
    assert!(app.annotation_draft.is_some());
}

#[test]
fn annotation_draft_opened_on_last_row_scrolls_block_into_view() {
    let lines: Vec<&str> = (0..10).map(|_| "line").collect();
    let changeset = changeset_with_line_texts(&lines);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_rendered_diff_area(Rect {
        x: 0,
        y: 1,
        width: 40,
        height: 4,
    });
    app.set_viewport_width(40);
    app.set_viewport_rows(4);
    let code_row = app
        .model
        .rows
        .iter()
        .rposition(|row| matches!(row, UiRow::UnifiedLine { .. }))
        .expect("unified line");
    app.set_scroll(app.max_scroll());
    let previous_scroll = app.scroll;
    let viewport_row = code_row.saturating_sub(app.scroll) as u16;

    app.update_diff_mouse_hover(38, viewport_row.saturating_add(1));
    assert!(app.handle_diff_click(38, viewport_row.saturating_add(1)));

    let draft_row = app
        .annotation_draft
        .as_ref()
        .map(|draft| draft.model_row_index)
        .expect("draft");
    assert_eq!(draft_row, code_row);
    assert!(app.scroll > previous_scroll);
    assert!(
        crate::render::viewport_plan::compose_block_bottom_viewport_row(&app, draft_row).is_some()
    );

    let rendered = crate::render::diff::build_diff_viewport_lines(&mut app, 40, 4);
    assert!(
        rendered
            .iter()
            .any(|line| line_text(line).contains(INPUT_CURSOR))
    );
    assert!(rendered.iter().any(|line| line_text(line).ends_with("[✓]")));
}

#[test]
fn annotation_hidden_compose_footer_is_not_submit_target() {
    use crate::annotation::AnnotationKey;

    let changeset = changeset_with_line_text("hello");
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_rendered_diff_area(Rect {
        x: 0,
        y: 1,
        width: 40,
        height: 2,
    });
    app.set_viewport_width(40);
    app.set_viewport_rows(2);
    let code_row = app
        .model
        .rows
        .iter()
        .position(|row| matches!(row, UiRow::UnifiedLine { .. }))
        .expect("unified line");
    app.scroll = code_row;
    app.update_diff_mouse_hover(38, 1);
    assert!(app.handle_diff_click(38, 1));

    let draft_row = app
        .annotation_draft
        .as_ref()
        .map(|draft| draft.model_row_index)
        .expect("draft");
    assert_eq!(
        crate::render::viewport_plan::compose_block_bottom_viewport_row(&app, draft_row),
        None
    );
    for character in "note".chars() {
        app.handle_annotation_input_key(KeyEvent::new(
            KeyCode::Char(character),
            KeyModifiers::NONE,
        ));
    }

    let rendered = crate::render::diff::build_diff_viewport_lines(&mut app, 40, 2);
    assert!(line_text(&rendered[1]).ends_with("[x]"));
    assert!(!rendered.iter().any(|line| line_text(line).ends_with("[✓]")));
    assert!(app.handle_diff_click(38, 2));

    assert!(app.annotation_draft.is_none());
    let row = app.model.row(code_row).expect("row");
    let key = AnnotationKey::from_ui_row(&app.changeset, row).expect("key");
    assert!(!app.annotations.contains_key(&key));
}

#[test]
fn annotation_input_scrolls_back_to_draft_above_viewport() {
    let lines: Vec<&str> = (0..12).map(|_| "line").collect();
    let changeset = changeset_with_line_texts(&lines);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_rendered_diff_area(Rect {
        x: 0,
        y: 1,
        width: 40,
        height: 4,
    });
    app.set_viewport_width(40);
    app.set_viewport_rows(4);
    let code_row = app
        .model
        .rows
        .iter()
        .position(|row| matches!(row, UiRow::UnifiedLine { .. }))
        .expect("unified line");
    app.scroll = code_row;
    app.update_diff_mouse_hover(38, 1);
    assert!(app.handle_diff_click(38, 1));
    let draft_row = app
        .annotation_draft
        .as_ref()
        .map(|draft| draft.model_row_index)
        .expect("draft");

    app.set_scroll(code_row.saturating_add(5));
    assert_eq!(
        crate::render::viewport_plan::compose_block_bottom_viewport_row(&app, draft_row),
        None
    );
    app.handle_annotation_input_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));

    assert!(
        crate::render::viewport_plan::compose_block_bottom_viewport_row(&app, draft_row).is_some()
    );
    let rendered = crate::render::diff::build_diff_viewport_lines(&mut app, 40, 4);
    assert!(
        rendered
            .iter()
            .any(|line| line_text(line).contains(INPUT_CURSOR))
    );
}

#[test]
fn long_annotation_draft_stays_visible_when_footer_cannot_fit() {
    let lines: Vec<&str> = (0..20).map(|_| "line").collect();
    let changeset = changeset_with_line_texts(&lines);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_rendered_diff_area(Rect {
        x: 0,
        y: 1,
        width: 40,
        height: 4,
    });
    app.set_viewport_width(40);
    app.set_viewport_rows(4);
    let code_row = app
        .model
        .rows
        .iter()
        .position(|row| matches!(row, UiRow::UnifiedLine { .. }))
        .expect("unified line");
    app.scroll = code_row;
    app.update_diff_mouse_hover(38, 1);
    assert!(app.handle_diff_click(38, 1));
    let draft_row = app
        .annotation_draft
        .as_ref()
        .map(|draft| draft.model_row_index)
        .expect("draft");

    app.handle_annotation_input_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
    app.handle_annotation_input_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(app.scroll, draft_row);
    assert_eq!(
        crate::render::viewport_plan::compose_block_bottom_viewport_row(&app, draft_row),
        None
    );
    assert!(
        crate::render::viewport_plan::compose_block_top_viewport_row(&app, draft_row).is_some()
    );
    let rendered = crate::render::diff::build_diff_viewport_lines(&mut app, 40, 4);
    assert!(
        rendered
            .iter()
            .any(|line| line_text(line).contains(INPUT_CURSOR))
    );
}

#[test]
fn filter_input_blocks_annotation_hover_drafts() {
    use crate::annotation::AnnotationKey;

    let changeset = changeset_with_line_text("hello");
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
        .model
        .rows
        .iter()
        .position(|row| matches!(row, UiRow::UnifiedLine { .. }))
        .expect("unified line");
    app.scroll = code_row;
    app.update_diff_mouse_hover(38, 1);
    assert_eq!(app.mouse_hover, Some((38, 0)));

    app.open_filter_input(DiffFilterKind::File);
    assert_eq!(app.mouse_hover, None);
    assert!(!app.handle_diff_click(38, 1));

    let key = AnnotationKey::from_ui_row(
        &app.changeset,
        app.model.row(code_row).expect("annotated row"),
    )
    .expect("key");
    app.annotations.insert(key.clone(), "note".to_owned());
    assert!(!app.handle_diff_click(38, 2));
    assert_eq!(app.annotations.get(&key).map(String::as_str), Some("note"));
    assert!(!app.handle_diff_click(38, 4));

    assert!(app.annotation_draft.is_none());
    assert_eq!(app.filter_input, Some(DiffFilterKind::File));
}

#[test]
fn diff_modals_suppress_stale_mouse_hover_highlight() {
    type ModalOpener = (&'static str, fn(&mut DiffApp));
    let modal_openers: [ModalOpener; 7] = [
        ("help menu", |app| app.toggle_help_menu()),
        ("options menu", |app| app.open_options_menu()),
        ("diff menu", |app| app.open_diff_menu()),
        ("review input", |app| app.open_review_input()),
        ("branch menu", |app| {
            app.comparison_branches = vec!["main".to_owned(), "topic".to_owned()];
            app.toggle_branch_menu(BranchMenu::Head);
        }),
        ("commit menu", |app| {
            app.comparison_commits = vec![GitCommit {
                sha: "abcdef0".to_owned(),
                subject: "commit".to_owned(),
            }];
            app.toggle_commit_menu();
        }),
        ("color scheme picker", |app| {
            app.open_options_menu();
            app.open_color_scheme_picker();
        }),
    ];

    for (label, open_modal) in modal_openers {
        let changeset = changeset_with_line_text("hello");
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
            .model
            .rows
            .iter()
            .position(|row| matches!(row, UiRow::UnifiedLine { .. }))
            .expect("unified line");
        app.scroll = code_row;
        app.update_diff_mouse_hover(38, 1);

        let hovered_lines = crate::render::diff::build_diff_viewport_lines(&mut app, 40, 5);
        assert!(
            line_text(&hovered_lines[0]).contains("[+]"),
            "{label} baseline should show hover add button"
        );

        open_modal(&mut app);
        assert!(
            app.diff_modal_blocks_mouse_hover(),
            "{label} should block diff mouse hover"
        );

        let modal_lines = crate::render::diff::build_diff_viewport_lines(&mut app, 40, 5);
        assert!(
            !line_text(&modal_lines[0]).contains("[+]"),
            "{label} should hide stale hover add button"
        );
        assert!(
            !modal_lines[0]
                .spans
                .iter()
                .any(|span| span.style.bg == Some(app.theme.cursor_line_bg)),
            "{label} should hide stale hover highlight"
        );
    }
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
        .model
        .rows
        .iter()
        .position(|row| matches!(row, UiRow::SplitLine { .. }))
        .expect("split line");
    app.scroll = split_row;
    let key = AnnotationKey::from_ui_row(&app.changeset, app.model.row(split_row).expect("row"))
        .expect("key");
    assert_eq!(key.side, AnnotationSide::Old);
    app.annotations.insert(key.clone(), "old note".to_owned());

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
    let draft = app.annotation_draft.as_ref().expect("draft");
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
        .model
        .rows
        .iter()
        .position(|row| matches!(row, UiRow::UnifiedLine { line: 1, .. }))
        .expect("addition line");
    assert_eq!(
        AnnotationKey::from_ui_row(
            &app.changeset,
            app.model.row(deletion_row).expect("deletion row"),
        ),
        None
    );
    let new_key = AnnotationKey::from_ui_row(
        &app.changeset,
        app.model.row(addition_row).expect("addition row"),
    )
    .expect("new-side key");
    assert_eq!(new_key.path, "new.rs");
    assert_eq!(new_key.side, AnnotationSide::New);

    let split_app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Split);
    let split_row = split_app
        .model
        .rows
        .iter()
        .find(|row| matches!(row, UiRow::SplitLine { .. }))
        .copied()
        .expect("split row");
    let split_keys = AnnotationKey::candidates_from_ui_row(&split_app.changeset, split_row);
    assert!(!split_keys.contains(&old_key));
    assert!(split_keys.contains(&new_key));

    let mut export_app = split_app;
    export_app
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
        .model
        .rows
        .iter()
        .position(|row| matches!(row, UiRow::MetaLine { .. }))
        .expect("meta line");
    app.scroll = meta_row;
    app.update_diff_mouse_hover(38, 1);

    let rendered = crate::render::diff::build_diff_viewport_lines(&mut app, 40, 5);
    assert!(!line_text(&rendered[0]).contains("[+]"));
    assert!(!app.handle_diff_click(38, 1));
    assert!(app.annotation_draft.is_none());
}

#[test]
fn annotations_are_keyed_by_path_and_line_across_model_changes() {
    use crate::annotation::AnnotationKey;

    let changeset = changeset_with_line_text("hello");
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_width(40);
    app.set_viewport_rows(6);
    let code_row = app
        .model
        .rows
        .iter()
        .position(|row| matches!(row, UiRow::UnifiedLine { .. }))
        .expect("unified line");
    let row = app.model.row(code_row).expect("row");
    let key = AnnotationKey::from_ui_row(&app.changeset, row).expect("key");
    app.annotations.insert(key, "note".to_owned());

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
        .model
        .rows
        .iter()
        .position(|row| matches!(row, UiRow::UnifiedLine { .. }))
        .expect("unified line");
    app.scroll = code_row;
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
    assert!(app.annotation_draft.is_some());

    app.handle_annotation_input_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));

    assert!(app.annotation_draft.is_none());
    let row = app.model.row(code_row).expect("row");
    let key = AnnotationKey::from_ui_row(&app.changeset, row).expect("key");
    assert_eq!(
        app.annotations.get(&key).map(String::as_str),
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
        .model
        .rows
        .iter()
        .position(|row| matches!(row, UiRow::UnifiedLine { .. }))
        .expect("unified line");
    let key = AnnotationKey::from_ui_row(
        &app.changeset,
        app.model.row(code_row).expect("annotated row"),
    )
    .expect("annotation key");
    app.annotations
        .insert(key, "  indented  code\na\t\tb".to_owned());
    app.scroll = code_row;

    let rendered: Vec<String> = crate::render::diff::build_diff_viewport_lines(&mut app, 12, 8)
        .iter()
        .map(line_text)
        .collect();

    assert!(rendered.iter().any(|line| line.starts_with("  indented  ")));
    assert!(rendered.iter().any(|line| line.starts_with("code")));
    assert!(rendered.iter().any(|line| line.contains("a\t\tb")));
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

    assert_eq!(app.file_filter_input, ">alpha beta!");

    app.handle_filter_input_key(KeyEvent::new(KeyCode::Delete, KeyModifiers::SUPER));
    assert!(app.file_filter_input.is_empty());

    for character in "done".chars() {
        app.handle_filter_input_key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE));
    }
    app.handle_filter_input_key(KeyEvent::new(KeyCode::Left, KeyModifiers::ALT));
    app.handle_filter_input_key(KeyEvent::new(KeyCode::Char('!'), KeyModifiers::NONE));
    assert_eq!(app.file_filter_input, "!done");
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
        .model
        .rows
        .iter()
        .position(|row| matches!(row, UiRow::UnifiedLine { .. }))
        .expect("unified line");
    app.scroll = code_row;
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

    let row = app.model.row(code_row).expect("row");
    let key = AnnotationKey::from_ui_row(&app.changeset, row).expect("key");
    assert_eq!(
        app.annotations.get(&key).map(String::as_str),
        Some(">hello world!\n")
    );
}

#[test]
fn annotation_save_and_cancel_use_configured_keybindings() {
    use crate::annotation::AnnotationKey;

    let changeset = changeset_with_line_text("hello");
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.keymap = Keymap::parse(
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
        .model
        .rows
        .iter()
        .position(|row| matches!(row, UiRow::UnifiedLine { .. }))
        .expect("unified line");
    app.scroll = code_row;
    app.update_diff_mouse_hover(38, 1);
    assert!(app.handle_diff_click(38, 1));
    app.handle_annotation_input_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
    app.handle_annotation_input_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
    assert!(app.annotation_draft.is_some());
    app.handle_annotation_input_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL));

    let row = app.model.row(code_row).expect("row");
    let key = AnnotationKey::from_ui_row(&app.changeset, row).expect("key");
    assert_eq!(app.annotations.get(&key).map(String::as_str), Some("n"));

    let rendered = build_diff_viewport_lines(&mut app, 40, 8);
    let edit_row = rendered
        .iter()
        .position(|line| line_text(line).ends_with("[↻]"))
        .expect("saved annotation edit footer") as u16
        + app.rendered_diff_area.expect("diff area").y;
    assert!(app.handle_diff_click(38, edit_row));
    assert!(app.annotation_draft.is_some());
    app.handle_annotation_input_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL));
    assert!(app.annotation_draft.is_none());
    assert_eq!(app.annotations.get(&key).map(String::as_str), Some("n"));
}

#[test]
fn annotation_cancel_binding_preempts_overlapping_edit_hunk_shortcut() {
    let changeset = changeset_with_line_text("hello");
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.keymap = Keymap::parse(
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
        .model
        .rows
        .iter()
        .position(|row| matches!(row, UiRow::UnifiedLine { .. }))
        .expect("unified line");
    app.scroll = code_row;
    app.update_diff_mouse_hover(38, 1);
    assert!(app.handle_diff_click(38, 1));
    assert!(app.annotation_draft.is_some());

    assert!(!handle_test_key_event(
        &mut app,
        KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL)
    ));

    assert!(app.annotation_draft.is_none());
    assert!(app.annotations.is_empty());
}

#[test]
fn annotation_save_binding_preempts_overlapping_edit_hunk_shortcut() {
    use crate::annotation::{AnnotationDraft, AnnotationKey};

    let changeset = changeset_with_line_text("hello");
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.keymap = Keymap::parse(
        r#"
        [keymap.global]
        edit_hunk = "ctrl-g"
        save_mark = "ctrl-g"
        "#,
    )
    .expect("keymap should parse");
    let code_row = app
        .model
        .rows
        .iter()
        .position(|row| matches!(row, UiRow::UnifiedLine { .. }))
        .expect("unified line");
    let key = AnnotationKey::from_ui_row(
        &app.changeset,
        app.model.row(code_row).expect("annotated row"),
    )
    .expect("annotation key");
    app.annotation_draft = Some(AnnotationDraft {
        key: key.clone(),
        model_row_index: code_row,
        input: "note".to_owned(),
        cursor: "note".len(),
    });

    assert!(!handle_test_key_event(
        &mut app,
        KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL)
    ));

    assert!(app.annotation_draft.is_none());
    assert_eq!(app.annotations.get(&key).map(String::as_str), Some("note"));
}

#[test]
fn annotation_draft_bindings_preempt_hard_quit_key() {
    use crate::annotation::{AnnotationDraft, AnnotationKey};

    let changeset = changeset_with_line_text("hello");
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.keymap = Keymap::parse(
        r#"
        [keymap.global]
        save_mark = "ctrl-c"
        "#,
    )
    .expect("keymap should parse");
    let code_row = app
        .model
        .rows
        .iter()
        .position(|row| matches!(row, UiRow::UnifiedLine { .. }))
        .expect("unified line");
    let key = AnnotationKey::from_ui_row(
        &app.changeset,
        app.model.row(code_row).expect("annotated row"),
    )
    .expect("annotation key");
    app.annotation_draft = Some(AnnotationDraft {
        key: key.clone(),
        model_row_index: code_row,
        input: "note".to_owned(),
        cursor: "note".len(),
    });

    assert!(!handle_test_key_event(
        &mut app,
        KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)
    ));

    assert!(app.annotation_draft.is_none());
    assert_eq!(app.annotations.get(&key).map(String::as_str), Some("note"));

    app.keymap = Keymap::parse(
        r#"
        [keymap.global]
        cancel_mark = "ctrl-c"
        "#,
    )
    .expect("keymap should parse");
    app.annotation_draft = Some(AnnotationDraft {
        key: key.clone(),
        model_row_index: code_row,
        input: "discard".to_owned(),
        cursor: "discard".len(),
    });

    assert!(!handle_test_key_event(
        &mut app,
        KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)
    ));

    assert!(app.annotation_draft.is_none());
    assert_eq!(app.annotations.get(&key).map(String::as_str), Some("note"));
}

#[test]
fn annotation_compose_scrolls_with_annotated_line() {
    let lines: Vec<&str> = (0..12).map(|_| "line").collect();
    let changeset = changeset_with_line_texts(&lines);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_width(40);
    app.set_viewport_rows(6);
    let code_row = app
        .model
        .rows
        .iter()
        .position(|row| matches!(row, UiRow::UnifiedLine { .. }))
        .expect("unified line");
    app.scroll = code_row;
    app.set_rendered_diff_area(Rect {
        x: 0,
        y: 1,
        width: 40,
        height: 6,
    });
    let compose_viewport_row = (code_row - app.scroll) as u16;
    app.update_diff_mouse_hover(38, compose_viewport_row.saturating_add(1));
    assert!(app.handle_diff_click(38, compose_viewport_row.saturating_add(1)));
    assert!(app.annotation_draft.is_some());

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
    assert_eq!(app.mouse_hover, Some((12, 2)));
    assert_eq!(
        app.diff_mouse_highlight_visual_row(),
        crate::render::viewport_plan::visual_scroll_for_viewport_row(&app, 2)
    );

    app.update_diff_mouse_hover(22, 5);
    app.update_diff_mouse_hover(0, 5);
    assert_eq!(app.mouse_hover, None);
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
    assert_eq!(app.mouse_hover, Some((38, 0)));

    app.set_rendered_diff_area(area);
    assert_eq!(app.mouse_hover, Some((38, 0)));

    app.set_rendered_diff_area(Rect {
        x: 10,
        y: area.y,
        width: area.width,
        height: area.height,
    });

    assert_eq!(app.mouse_hover, None);
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
        .model
        .rows
        .iter()
        .position(|row| matches!(row, UiRow::UnifiedLine { .. }))
        .expect("unified line");
    app.scroll = code_row;
    app.update_diff_mouse_hover(38, 1);

    let before = crate::render::diff::build_diff_viewport_lines(&mut app, 40, 5);
    assert!(line_text(&before[0]).contains("[+]"));

    app.scroll_by(1);

    assert_eq!(app.mouse_hover, None);
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
        .model
        .rows
        .iter()
        .position(|row| matches!(row, UiRow::UnifiedLine { .. }))
        .expect("unified line");
    app.scroll = code_row;

    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: 38,
        row: 1,
        modifiers: KeyModifiers::NONE,
    })
    .expect("mouse scroll");

    assert_eq!(app.mouse_hover, Some((38, 0)));
    let after = crate::render::diff::build_diff_viewport_lines(&mut app, 40, 5);
    assert!(line_text(&after[0]).contains("[+]"));
}

#[test]
fn focused_hunk_highlights_diff_indicators() {
    let changeset = changeset_with_context_lines(1);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    let theme = app.theme;

    let header = render_row_with_focus(
        &mut app,
        1,
        UiRow::HunkHeader { file: 0, hunk: 0 },
        24,
        Some((0, 0)),
    );
    assert_eq!(header.spans[0].style.fg, Some(theme.hunk));
    assert!(header.spans[0].style.add_modifier.contains(Modifier::BOLD));

    let row = app.model.row(2).expect("diff line should be visible");
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
fn ansi_theme_uses_terminal_palette_indices() {
    let theme = diff_theme_from_config(&SyntaxThemeConfig {
        source: SyntaxThemeSource::Ansi,
        name: None,
        path: None,
    })
    .expect("ansi theme should load");

    assert_eq!(theme.addition_fg, Color::Indexed(2));
    assert_eq!(
        theme.syntax.color(SyntaxClass::Keyword),
        Some(Color::Indexed(13))
    );
}

#[test]
fn system_theme_preserves_terminal_base_and_uses_owned_diff_colors() {
    let theme = builtin_diff_theme(Some("system")).expect("system theme should load");

    assert_eq!(theme.foreground, Color::Reset);
    assert_eq!(theme.background, Color::Reset);
    assert_eq!(theme.file, Color::Reset);
    assert_eq!(theme.cursor, Color::Reset);
    assert_eq!(theme.cursor_line_bg, Color::Indexed(237));
    assert_ne!(theme.addition_fg, Color::Indexed(2));
    assert_ne!(theme.deletion_fg, Color::Indexed(1));
    assert_eq!(row_bg(DiffLineKind::Addition, theme), theme.addition_bg);
    assert_eq!(
        inline_bg(DiffLineKind::Addition, theme),
        theme.addition_inline_bg
    );
    assert_eq!(
        line_gutter_bg(DiffLineKind::Addition, theme),
        theme.addition_gutter_bg
    );
    assert_eq!(
        theme.syntax.color(SyntaxClass::String),
        SyntaxPalette::ansi().color(SyntaxClass::String)
    );
}

#[test]
fn default_theme_alias_uses_system_theme() {
    let theme = builtin_diff_theme(Some("default")).expect("default theme should load");

    assert_eq!(theme, DiffTheme::system());
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
fn packaged_builtin_themes_are_available() {
    for name in [
        "system",
        "catppuccin-latte",
        "catppuccin-frappe",
        "catppuccin-macchiato",
        "catppuccin-mocha",
        "gruvbox-dark",
        "gruvbox-light",
        "github-dark",
        "github-dark-high-contrast",
        "github-light",
        "github-light-high-contrast",
        "tokyonight",
    ] {
        let theme = builtin_diff_theme(Some(name)).expect("built-in theme should load");

        assert_ne!(theme.statusline_accent_bg, Color::Reset);
        assert!(
            theme.syntax.color(SyntaxClass::Keyword).is_some(),
            "{name} should set syntax keyword foreground"
        );
    }
}

#[test]
fn builtin_syntax_palettes_match_upstream_theme_scopes() {
    // These expectations mirror the upstream TextMate scopes used by the
    // Catppuccin, Gruvbox, and GitHub VS Code/Shiki themes for the closest
    // matching Mark syntax classes.
    for (theme, comment, operator, tag, attribute) in [
        (
            DiffTheme::catppuccin_latte(),
            Color::Rgb(0x7c, 0x7f, 0x93),
            Color::Rgb(0x17, 0x92, 0x99),
            Color::Rgb(0x1e, 0x66, 0xf5),
            Color::Rgb(0xdf, 0x8e, 0x1d),
        ),
        (
            DiffTheme::catppuccin_frappe(),
            Color::Rgb(0x94, 0x9c, 0xbb),
            Color::Rgb(0x81, 0xc8, 0xbe),
            Color::Rgb(0x8c, 0xaa, 0xee),
            Color::Rgb(0xe5, 0xc8, 0x90),
        ),
        (
            DiffTheme::catppuccin_macchiato(),
            Color::Rgb(0x93, 0x9a, 0xb7),
            Color::Rgb(0x8b, 0xd5, 0xca),
            Color::Rgb(0x8a, 0xad, 0xf4),
            Color::Rgb(0xee, 0xd4, 0x9f),
        ),
        (
            DiffTheme::catppuccin_mocha(),
            Color::Rgb(0x93, 0x99, 0xb2),
            Color::Rgb(0x94, 0xe2, 0xd5),
            Color::Rgb(0x89, 0xb4, 0xfa),
            Color::Rgb(0xf9, 0xe2, 0xaf),
        ),
    ] {
        assert_eq!(theme.syntax.color(SyntaxClass::Comment), Some(comment));
        assert_eq!(theme.syntax.color(SyntaxClass::Operator), Some(operator));
        assert_eq!(theme.syntax.color(SyntaxClass::Property), Some(operator));
        assert_eq!(theme.syntax.color(SyntaxClass::Tag), Some(tag));
        assert_eq!(theme.syntax.color(SyntaxClass::Attribute), Some(attribute));
        assert_eq!(theme.syntax.color(SyntaxClass::Module), Some(attribute));
        assert_eq!(theme.syntax.color(SyntaxClass::Variable), None);
    }

    for (theme, constant, function, operator, variable, punctuation, property) in [
        (
            DiffTheme::gruvbox_dark(),
            Color::Rgb(0xd3, 0x86, 0x9b),
            Color::Rgb(0xfa, 0xbd, 0x2f),
            Color::Rgb(0x8e, 0xc0, 0x7c),
            Color::Rgb(0x83, 0xa5, 0x98),
            Color::Rgb(0xa8, 0x99, 0x84),
            Color::Rgb(0x68, 0x9d, 0x6a),
        ),
        (
            DiffTheme::gruvbox_light(),
            Color::Rgb(0x8f, 0x3f, 0x71),
            Color::Rgb(0xb5, 0x76, 0x14),
            Color::Rgb(0x42, 0x7b, 0x58),
            Color::Rgb(0x07, 0x66, 0x78),
            Color::Rgb(0x7c, 0x6f, 0x64),
            Color::Rgb(0x68, 0x9d, 0x6a),
        ),
    ] {
        assert_eq!(
            theme.syntax.color(SyntaxClass::Comment),
            Some(Color::Rgb(0x92, 0x83, 0x74))
        );
        assert_eq!(theme.syntax.color(SyntaxClass::Constant), Some(constant));
        assert_eq!(theme.syntax.color(SyntaxClass::Function), Some(function));
        assert_eq!(theme.syntax.color(SyntaxClass::Type), Some(function));
        assert_eq!(theme.syntax.color(SyntaxClass::Operator), Some(operator));
        assert_eq!(theme.syntax.color(SyntaxClass::Tag), Some(operator));
        assert_eq!(theme.syntax.color(SyntaxClass::Variable), Some(variable));
        assert_eq!(
            theme.syntax.color(SyntaxClass::Punctuation),
            Some(punctuation)
        );
        assert_eq!(theme.syntax.color(SyntaxClass::Property), Some(property));
    }

    for (theme, comment, constant, function, tag, string, variable) in [
        (
            DiffTheme::github_light(),
            Color::Rgb(0x6e, 0x77, 0x81),
            Color::Rgb(0x05, 0x50, 0xae),
            Color::Rgb(0x82, 0x50, 0xdf),
            Color::Rgb(0x11, 0x63, 0x29),
            Color::Rgb(0x0a, 0x30, 0x69),
            Color::Rgb(0x95, 0x38, 0x00),
        ),
        (
            DiffTheme::github_dark(),
            Color::Rgb(0x8b, 0x94, 0x9e),
            Color::Rgb(0x79, 0xc0, 0xff),
            Color::Rgb(0xd2, 0xa8, 0xff),
            Color::Rgb(0x7e, 0xe7, 0x87),
            Color::Rgb(0xa5, 0xd6, 0xff),
            Color::Rgb(0xff, 0xa6, 0x57),
        ),
        (
            DiffTheme::github_light_high_contrast(),
            Color::Rgb(0x66, 0x70, 0x7b),
            Color::Rgb(0x02, 0x3b, 0x95),
            Color::Rgb(0x62, 0x2c, 0xbc),
            Color::Rgb(0x02, 0x4c, 0x1a),
            Color::Rgb(0x03, 0x25, 0x63),
            Color::Rgb(0x70, 0x2c, 0x00),
        ),
        (
            DiffTheme::github_dark_high_contrast(),
            Color::Rgb(0xbd, 0xc4, 0xcc),
            Color::Rgb(0x91, 0xcb, 0xff),
            Color::Rgb(0xdb, 0xb7, 0xff),
            Color::Rgb(0x72, 0xf0, 0x88),
            Color::Rgb(0xad, 0xdc, 0xff),
            Color::Rgb(0xff, 0xb7, 0x57),
        ),
    ] {
        assert_eq!(theme.syntax.color(SyntaxClass::Attribute), None);
        assert_eq!(theme.syntax.color(SyntaxClass::Comment), Some(comment));
        assert_eq!(theme.syntax.color(SyntaxClass::Constant), Some(constant));
        assert_eq!(theme.syntax.color(SyntaxClass::Function), Some(function));
        assert_eq!(theme.syntax.color(SyntaxClass::Tag), Some(tag));
        assert_eq!(theme.syntax.color(SyntaxClass::String), Some(string));
        assert_eq!(theme.syntax.color(SyntaxClass::Variable), Some(variable));
        assert_eq!(theme.syntax.color(SyntaxClass::Type), Some(variable));
        assert_eq!(theme.syntax.color(SyntaxClass::Property), Some(constant));
        assert_eq!(theme.syntax.color(SyntaxClass::Punctuation), None);
    }

    let theme = DiffTheme::tokyonight();
    assert_eq!(
        theme.syntax.color(SyntaxClass::Comment),
        Some(Color::Rgb(0x51, 0x59, 0x7d))
    );
    assert_eq!(
        theme.syntax.color(SyntaxClass::Attribute),
        Some(Color::Rgb(0xbb, 0x9a, 0xf7))
    );
    assert_eq!(
        theme.syntax.color(SyntaxClass::Operator),
        Some(Color::Rgb(0x89, 0xdd, 0xff))
    );
    assert_eq!(
        theme.syntax.color(SyntaxClass::Property),
        Some(Color::Rgb(0x7d, 0xcf, 0xff))
    );
    assert_eq!(
        theme.syntax.color(SyntaxClass::Type),
        Some(Color::Rgb(0x0d, 0xb9, 0xd7))
    );
    assert_eq!(
        theme.syntax.color(SyntaxClass::Tag),
        Some(Color::Rgb(0xf7, 0x76, 0x8e))
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
fn transparent_background_resets_diff_and_inline_backgrounds() {
    let theme = DiffTheme::catppuccin_mocha().with_transparent_background(true);
    let spans = content_spans_at_scroll(
        "changed",
        None,
        &[InlineRange {
            byte_start: 0,
            byte_end: 7,
        }],
        DiffLineKind::Addition,
        8,
        theme,
        0,
    );

    assert_eq!(row_bg(DiffLineKind::Addition, theme), Color::Reset);
    assert_eq!(spans[0].style.bg, Some(Color::Reset));
    assert!(spans[0].style.add_modifier.contains(Modifier::BOLD));
}

#[test]
fn base16_theme_parser_accepts_yaml_or_toml_lines() {
    let scheme = parse_base16_scheme(
        r##"
base00: "#000000"
base01: "111111"
base02: "222222"
base03: "333333"
base04 = "444444"
base05 = "555555"
base06 = "666666"
base07 = "777777"
base08 = "888888"
base09 = "999999"
base0A = "aaaaaa"
base0B = "bbbbbb"
base0C = "cccccc"
base0D = "dddddd"
base0E = "eeeeee"
base0F = "ffffff"
"##,
    )
    .expect("base16 scheme should parse");
    let theme = DiffTheme::base16(scheme);

    assert_eq!(theme.muted, Color::Rgb(51, 51, 51));
    assert_eq!(
        theme.syntax.color(SyntaxClass::String),
        Some(Color::Rgb(187, 187, 187))
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
fn inline_emphasis_leaves_unpaired_changed_lines_to_line_style() {
    let lines = vec![DiffLine {
        kind: DiffLineKind::Deletion,
        old_line: Some(1),
        new_line: None,
        text: "removed line".to_owned(),
    }];

    let emphasis = compute_hunk_inline_emphasis(&lines);

    assert!(emphasis[0].ranges.is_empty());
}

#[test]
fn lazy_inline_emphasis_matches_eager_emphasis() {
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
        DiffLine {
            kind: DiffLineKind::Deletion,
            old_line: Some(2),
            new_line: None,
            text: "removed only".to_owned(),
        },
        DiffLine {
            kind: DiffLineKind::Context,
            old_line: Some(3),
            new_line: Some(2),
            text: "context".to_owned(),
        },
        DiffLine {
            kind: DiffLineKind::Addition,
            old_line: None,
            new_line: Some(3),
            text: "added only".to_owned(),
        },
        DiffLine {
            kind: DiffLineKind::Deletion,
            old_line: Some(4),
            new_line: None,
            text: "alpha beta".to_owned(),
        },
        DiffLine {
            kind: DiffLineKind::Deletion,
            old_line: Some(5),
            new_line: None,
            text: "gamma".to_owned(),
        },
        DiffLine {
            kind: DiffLineKind::Addition,
            old_line: None,
            new_line: Some(4),
            text: "alpha zeta".to_owned(),
        },
        DiffLine {
            kind: DiffLineKind::Addition,
            old_line: None,
            new_line: Some(5),
            text: "delta".to_owned(),
        },
    ];
    let expected = compute_hunk_inline_emphasis(&lines);
    let mut cache = InlineHunkEmphasisCache::new(&lines);

    for index in [7, 5, 2, 4, 0, 1, 8, 6, 3] {
        assert_eq!(
            cache.ranges_for_line(&lines, index),
            expected[index].ranges,
            "lazy emphasis should match eager emphasis for line {index}"
        );
        assert_eq!(
            cache.ranges_for_line(&lines, index),
            expected[index].ranges,
            "cached emphasis should be stable for line {index}"
        );
    }
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
fn inline_diff_skips_expensive_long_line_pairs() {
    let lines = vec![
        DiffLine {
            kind: DiffLineKind::Deletion,
            old_line: Some(1),
            new_line: None,
            text: "a".repeat(MAX_INLINE_DIFF_LINE_BYTES + 1),
        },
        DiffLine {
            kind: DiffLineKind::Addition,
            old_line: None,
            new_line: Some(1),
            text: "b".repeat(MAX_INLINE_DIFF_LINE_BYTES + 1),
        },
    ];

    let emphasis = compute_hunk_inline_emphasis(&lines);

    assert!(emphasis[0].ranges.is_empty());
    assert!(emphasis[1].ranges.is_empty());
}

#[test]
fn content_spans_layers_inline_emphasis_over_syntax() {
    let text = "let value = 2;";
    let number_start = text.find('2').unwrap();
    let syntax = HighlightedLine {
        segments: vec![
            mark_syntax::SyntaxSegment {
                byte_start: 0,
                byte_end: 12,
                text: "let value = ".to_owned(),
                class: Some(SyntaxClass::Keyword),
            },
            mark_syntax::SyntaxSegment {
                byte_start: 12,
                byte_end: 13,
                text: "2".to_owned(),
                class: Some(SyntaxClass::Number),
            },
            mark_syntax::SyntaxSegment {
                byte_start: 13,
                byte_end: 14,
                text: ";".to_owned(),
                class: Some(SyntaxClass::Punctuation),
            },
        ],
    };

    let spans = content_spans_at_scroll(
        text,
        Some(&syntax),
        &[InlineRange {
            byte_start: number_start,
            byte_end: number_start + 1,
        }],
        DiffLineKind::Addition,
        20,
        DiffTheme::default(),
        0,
    );
    let number = spans
        .iter()
        .find(|span| span.content.as_ref() == "2")
        .expect("number span should be split out for inline emphasis");

    assert_eq!(
        number.style.fg,
        syntax_fg(SyntaxClass::Number, DiffTheme::default())
    );
    assert_eq!(
        number.style.bg,
        Some(DiffTheme::default().addition_inline_bg)
    );
    assert!(number.style.add_modifier.contains(Modifier::BOLD));
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
fn highlight_queue_runs_visible_jobs_before_prefetch_jobs() {
    let queue = SyntaxWorkerQueue::new(8, 0);
    let prefetch = syntax_key(1);
    let visible = syntax_key(2);

    queue
        .try_push(syntax_job(prefetch), SyntaxPriority::Prefetch)
        .unwrap();
    queue
        .try_push(syntax_job(visible), SyntaxPriority::Visible)
        .unwrap();

    assert_eq!(queue.try_pop().map(|job| job.key), Some(visible));
    assert_eq!(queue.try_pop().map(|job| job.key), Some(prefetch));
}

#[test]
fn visible_highlight_job_can_evict_prefetch_when_queue_is_full() {
    let queue = SyntaxWorkerQueue::new(1, 0);
    let prefetch = syntax_key(1);
    let visible = syntax_key(2);

    queue
        .try_push(syntax_job(prefetch), SyntaxPriority::Prefetch)
        .unwrap();
    let pushed = queue
        .try_push(syntax_job(visible), SyntaxPriority::Visible)
        .unwrap();

    assert_eq!(pushed.dropped, Some(prefetch));
    assert_eq!(queue.len(), 1);
    assert_eq!(queue.try_pop().map(|job| job.key), Some(visible));
}

#[test]
fn stale_highlight_jobs_are_dropped_on_generation_change() {
    let queue = SyntaxWorkerQueue::new(8, 0);

    queue
        .try_push(syntax_job(syntax_key(1)), SyntaxPriority::Prefetch)
        .unwrap();
    queue.set_generation(1);

    assert_eq!(queue.len(), 0);
    assert_eq!(
        queue.try_push(syntax_job(syntax_key(2)), SyntaxPriority::Visible),
        Err(SyntaxQueueError::Stale)
    );

    let fresh = syntax_key_with_generation(1, 0);
    queue
        .try_push(syntax_job(fresh), SyntaxPriority::Visible)
        .unwrap();
    assert_eq!(queue.try_pop().map(|job| job.key), Some(fresh));
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
fn oversized_lines_disable_hunk_highlighting() {
    let limits = SyntaxLimits::default();
    let lines = vec![
        DiffLine {
            kind: DiffLineKind::Context,
            old_line: Some(1),
            new_line: Some(1),
            text: "x".repeat(limits.max_line_bytes + 1),
        },
        DiffLine {
            kind: DiffLineKind::Context,
            old_line: Some(2),
            new_line: Some(2),
            text: "let value = 1;".to_owned(),
        },
    ];

    assert_eq!(
        build_hunk_source(&lines, DiffSide::New, limits).unwrap_err(),
        SyntaxSkipReason::TooLarge
    );
}

#[test]
fn hunk_source_excludes_diff_meta_lines_and_preserves_empty_lines() {
    let lines = vec![
        DiffLine {
            kind: DiffLineKind::Context,
            old_line: Some(1),
            new_line: Some(1),
            text: "let a = 1;".to_owned(),
        },
        DiffLine {
            kind: DiffLineKind::Meta,
            old_line: None,
            new_line: None,
            text: "\\ No newline at end of file".to_owned(),
        },
        DiffLine {
            kind: DiffLineKind::Addition,
            old_line: None,
            new_line: Some(2),
            text: String::new(),
        },
    ];

    let source = build_hunk_source(&lines, DiffSide::New, SyntaxLimits::default()).unwrap();

    assert_eq!(source.text, "let a = 1;\n");
    assert_eq!(source.line_map, vec![Some(0), None, Some(1)]);
    assert_eq!(source.source_lines, 2);
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

#[test]
fn hunk_source_preserves_leading_empty_lines() {
    let lines = vec![
        DiffLine {
            kind: DiffLineKind::Addition,
            old_line: None,
            new_line: Some(1),
            text: String::new(),
        },
        DiffLine {
            kind: DiffLineKind::Addition,
            old_line: None,
            new_line: Some(2),
            text: "let value = 1;".to_owned(),
        },
    ];

    let source = build_hunk_source(&lines, DiffSide::New, SyntaxLimits::default()).unwrap();

    assert_eq!(source.text, "\nlet value = 1;");
    assert_eq!(source.line_map, vec![Some(0), Some(1)]);
    assert_eq!(source.source_lines, 2);
}

#[test]
fn full_file_line_map_uses_absolute_line_numbers() {
    let lines = vec![
        DiffLine {
            kind: DiffLineKind::Deletion,
            old_line: Some(10),
            new_line: None,
            text: "old".to_owned(),
        },
        DiffLine {
            kind: DiffLineKind::Addition,
            old_line: None,
            new_line: Some(11),
            text: "new".to_owned(),
        },
        DiffLine {
            kind: DiffLineKind::Context,
            old_line: Some(12),
            new_line: Some(12),
            text: "same".to_owned(),
        },
    ];

    assert_eq!(
        build_full_file_line_map(&lines, DiffSide::Old).unwrap(),
        vec![Some(9), None, Some(11)]
    );
    assert_eq!(
        build_full_file_line_map(&lines, DiffSide::New).unwrap(),
        vec![None, Some(10), Some(11)]
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
fn branch_full_file_source_uses_merge_base_and_head_revision() {
    let repo = std::env::temp_dir();
    let file = mark_diff::DiffFile {
        old_path: Some("old.rs".to_owned()),
        new_path: Some("new.rs".to_owned()),
        status: mark_diff::FileStatus::Modified,
        hunks: Vec::new(),
        additions: 0,
        deletions: 0,
        is_binary: false,
    };
    let base = "origin/main".to_owned();
    let head = "feature/full-file".to_owned();
    let branch = DiffOptions {
        source: DiffSource::Branch {
            base: base.clone(),
            head: head.clone(),
        },
        scope: DiffScope::All,
        ..DiffOptions::default()
    };

    assert_eq!(
        full_file_source(&repo, &branch, &file, DiffSide::Old)
            .unwrap()
            .kind,
        FullFileSourceKind::GitMergeBase {
            base,
            head: head.clone(),
            path: "old.rs".to_owned(),
        }
    );
    assert_eq!(
        full_file_source(&repo, &branch, &file, DiffSide::New)
            .unwrap()
            .kind,
        FullFileSourceKind::GitRevision {
            rev: head,
            path: "new.rs".to_owned(),
        }
    );
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

#[test]
fn queue_close_wakes_blocked_pop() {
    let queue = SyntaxWorkerQueue::new(8, 0);
    let worker_queue = queue.clone();
    let worker = thread::spawn(move || worker_queue.pop());

    queue.close();

    assert!(worker.join().unwrap().is_none());
}

fn handle_test_key_event(app: &mut DiffApp, key: KeyEvent) -> bool {
    let (_tx, rx) = mpsc::channel(1);
    let mut events = crate::event_reader::TerminalEventReader::from_receiver(rx);
    let mut live_diff = None;

    handle_event(app, Event::Key(key), &mut live_diff, &mut events)
        .expect("key event should be handled")
}

fn changeset_with_context_lines(line_count: usize) -> Changeset {
    changeset_with_context_lines_at(PathBuf::from("/repo"), 1, line_count)
}

fn changeset_with_context_lines_at(repo: PathBuf, start: usize, line_count: usize) -> Changeset {
    let lines = (1..=line_count)
        .map(|line| DiffLine {
            kind: DiffLineKind::Context,
            old_line: Some(start.saturating_add(line - 1)),
            new_line: Some(start.saturating_add(line - 1)),
            text: format!("line {line}"),
        })
        .collect();

    Changeset {
        repo,
        title: "test".to_owned(),
        files: vec![mark_diff::DiffFile {
            old_path: Some("file.rs".to_owned()),
            new_path: Some("file.rs".to_owned()),
            status: mark_diff::FileStatus::Modified,
            hunks: vec![mark_diff::DiffHunk {
                header: format!("@@ -{start} +{start} @@"),
                old_start: start,
                old_count: line_count,
                new_start: start,
                new_count: line_count,
                lines,
            }],
            additions: 0,
            deletions: 0,
            is_binary: false,
        }],
        raw_patch: Vec::new(),
    }
}

fn changeset_with_line_text(text: &str) -> Changeset {
    changeset_with_line_texts(&[text])
}

fn changeset_with_line_texts(texts: &[&str]) -> Changeset {
    Changeset {
        repo: PathBuf::from("/repo"),
        title: "test".to_owned(),
        files: vec![mark_diff::DiffFile {
            old_path: Some("file.rs".to_owned()),
            new_path: Some("file.rs".to_owned()),
            status: mark_diff::FileStatus::Modified,
            hunks: vec![mark_diff::DiffHunk {
                header: "@@ -1 +1 @@".to_owned(),
                old_start: 1,
                old_count: texts.len(),
                new_start: 1,
                new_count: texts.len(),
                lines: texts
                    .iter()
                    .enumerate()
                    .map(|(index, text)| DiffLine {
                        kind: DiffLineKind::Context,
                        old_line: Some(index + 1),
                        new_line: Some(index + 1),
                        text: (*text).to_owned(),
                    })
                    .collect(),
            }],
            additions: 0,
            deletions: 0,
            is_binary: false,
        }],
        raw_patch: Vec::new(),
    }
}

fn changeset_with_replacement_pair() -> Changeset {
    Changeset {
        repo: PathBuf::from("/repo"),
        title: "test".to_owned(),
        files: vec![mark_diff::DiffFile {
            old_path: Some("file.rs".to_owned()),
            new_path: Some("file.rs".to_owned()),
            status: mark_diff::FileStatus::Modified,
            hunks: vec![mark_diff::DiffHunk {
                header: "@@ -1 +1 @@".to_owned(),
                old_start: 1,
                old_count: 1,
                new_start: 1,
                new_count: 1,
                lines: vec![
                    DiffLine {
                        kind: DiffLineKind::Deletion,
                        old_line: Some(1),
                        new_line: None,
                        text: "old".to_owned(),
                    },
                    DiffLine {
                        kind: DiffLineKind::Addition,
                        old_line: None,
                        new_line: Some(1),
                        text: "new".to_owned(),
                    },
                ],
            }],
            additions: 1,
            deletions: 1,
            is_binary: false,
        }],
        raw_patch: Vec::new(),
    }
}

fn changeset_with_wrapped_leading_file() -> Changeset {
    let mut changeset = changeset_with_files(&["wide.rs", "target.rs"]);
    changeset.files[0].hunks[0].lines[0].text = "a".repeat(96);
    changeset
}

fn set_wrapped_scroll_relative_to_file_start(
    app: &mut DiffApp,
    file: usize,
    relative_scroll: usize,
) {
    app.line_wrapping = true;
    app.set_viewport_width(18);
    app.set_scroll(wrapped_file_start_scroll(app, file).saturating_add(relative_scroll));
    assert_eq!(app.selected_file, file);
}

fn wrapped_file_start_scroll(app: &DiffApp, file: usize) -> usize {
    let row = app
        .model
        .file_start_row(file)
        .expect("file should be visible");
    app.wrapped_visual_scroll_for_model_row(row)
}

fn changeset_with_hunk_at(repo: PathBuf, line_number: usize) -> Changeset {
    changeset_with_hunks_at(repo, &[line_number])
}

fn changeset_with_hunks_at(repo: PathBuf, line_numbers: &[usize]) -> Changeset {
    let hunks = line_numbers
        .iter()
        .map(|line_number| mark_diff::DiffHunk {
            header: format!("@@ -{line_number} +{line_number} @@"),
            old_start: *line_number,
            old_count: 1,
            new_start: *line_number,
            new_count: 1,
            lines: vec![DiffLine {
                kind: DiffLineKind::Context,
                old_line: Some(*line_number),
                new_line: Some(*line_number),
                text: format!("line {line_number}"),
            }],
        })
        .collect();

    Changeset {
        repo,
        title: "test".to_owned(),
        files: vec![mark_diff::DiffFile {
            old_path: Some("file.rs".to_owned()),
            new_path: Some("file.rs".to_owned()),
            status: mark_diff::FileStatus::Modified,
            hunks,
            additions: 0,
            deletions: 0,
            is_binary: false,
        }],
        raw_patch: Vec::new(),
    }
}

fn changeset_with_hunk_line_counts(repo: PathBuf, hunks: &[(usize, usize)]) -> Changeset {
    let hunks = hunks
        .iter()
        .map(|(line_number, line_count)| mark_diff::DiffHunk {
            header: format!("@@ -{line_number},{line_count} +{line_number},{line_count} @@"),
            old_start: *line_number,
            old_count: *line_count,
            new_start: *line_number,
            new_count: *line_count,
            lines: (0..*line_count)
                .map(|offset| DiffLine {
                    kind: DiffLineKind::Context,
                    old_line: Some(line_number + offset),
                    new_line: Some(line_number + offset),
                    text: format!("line {}", line_number + offset),
                })
                .collect(),
        })
        .collect();

    Changeset {
        repo,
        title: "test".to_owned(),
        files: vec![mark_diff::DiffFile {
            old_path: Some("file.rs".to_owned()),
            new_path: Some("file.rs".to_owned()),
            status: mark_diff::FileStatus::Modified,
            hunks,
            additions: 0,
            deletions: 0,
            is_binary: false,
        }],
        raw_patch: Vec::new(),
    }
}

fn changeset_with_files(paths: &[&str]) -> Changeset {
    let files = paths
        .iter()
        .enumerate()
        .map(|(index, path)| mark_diff::DiffFile {
            old_path: Some((*path).to_owned()),
            new_path: Some((*path).to_owned()),
            status: mark_diff::FileStatus::Modified,
            hunks: vec![mark_diff::DiffHunk {
                header: "@@ -1 +1 @@".to_owned(),
                old_start: 1,
                old_count: 1,
                new_start: 1,
                new_count: 1,
                lines: vec![DiffLine {
                    kind: DiffLineKind::Context,
                    old_line: Some(1),
                    new_line: Some(1),
                    text: format!("line {index}"),
                }],
            }],
            additions: index + 1,
            deletions: index,
            is_binary: false,
        })
        .collect();

    Changeset {
        repo: PathBuf::from("/repo"),
        title: "test".to_owned(),
        files,
        raw_patch: Vec::new(),
    }
}

fn pending_diff_load(options: DiffOptions) -> PendingDiffLoad {
    let (_tx, rx) = oneshot::channel();
    PendingDiffLoad {
        options,
        error_prefix: "load failed".to_owned(),
        refresh_branch_metadata: false,
        rx,
    }
}

fn pending_review_load() -> PendingReviewLoad {
    let (_tx, rx) = oneshot::channel();
    PendingReviewLoad {
        error_prefix: "review unavailable".to_owned(),
        rx,
    }
}

fn syntax_key(file: usize) -> SyntaxKey {
    syntax_key_with_generation(0, file)
}

fn syntax_key_with_generation(generation: u64, file: usize) -> SyntaxKey {
    SyntaxKey {
        source: SyntaxSourceId {
            generation,
            file,
            side: DiffSide::New,
            kind: SyntaxSourceKind::HunkSide { hunk: 0 },
        },
        language_hash: 1,
        theme_id: SYNTAX_THEME_ID,
    }
}

fn syntax_job(key: SyntaxKey) -> SyntaxJob {
    SyntaxJob {
        key,
        language: "rust".to_owned(),
        source: SyntaxJobSource::Hunk(HunkSource {
            text: "fn main() {}".to_owned(),
            line_map: vec![Some(0)],
            source_lines: 1,
        }),
        limits: SyntaxLimits::default(),
    }
}

fn full_file_syntax_job_source() -> SyntaxJobSource {
    SyntaxJobSource::FullFile(FullFileSource {
        repo: PathBuf::from("/repo"),
        kind: FullFileSourceKind::Worktree {
            path: "file.rs".to_owned(),
        },
    })
}

fn syntax_runtime_with_queue(queue: SyntaxWorkerQueue) -> SyntaxRuntime {
    let (_result_tx, result_rx) = mpsc::channel(1);
    SyntaxRuntime {
        languages: SyntaxLanguageSet::from_enabled_languages(&[]),
        limits: SyntaxLimits::default(),
        result_rx,
        queue,
        cache: LruCache::new(8),
        pending: HashSet::new(),
        source_keys: HashMap::new(),
        position_keys: HashMap::new(),
        line_maps: HashMap::new(),
        skipped: HashMap::new(),
        skipped_sources: HashSet::new(),
        unavailable_full_files: HashSet::new(),
        failed: HashSet::new(),
        stats: SyntaxBenchmarkReport::default(),
        worker: None,
    }
}

fn range_texts(text: &str, ranges: &[InlineRange]) -> Vec<String> {
    ranges
        .iter()
        .map(|range| text[range.byte_start..range.byte_end].to_owned())
        .collect()
}

fn line_text(line: &Line<'_>) -> String {
    span_text(&line.spans)
}

fn buffer_rows(buffer: &ratatui::buffer::Buffer) -> Vec<String> {
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer.cell((x, y)).expect("cell should exist").symbol())
                .collect()
        })
        .collect()
}

fn visible_paths(app: &DiffApp) -> Vec<&str> {
    app.model
        .visible_files()
        .iter()
        .filter_map(|file| app.changeset.files.get(*file))
        .map(|file| file.display_path())
        .collect()
}

fn default_options_draft() -> OptionsDraft {
    OptionsDraft {
        layout: LayoutSetting::Dynamic,
        live_updates_enabled: true,
        context_expansion: DiffContextExpansion::Lines(20),
        syntax_enabled: true,
        line_wrapping: false,
        color_scheme: ColorSchemeChoice::System,
        notification_mode: NotificationMode::Default,
        toast_corner: ToastCorner::TopRight,
        toast_timeout_ms: 1_500,
        toast_max_visible: 3,
    }
}

fn span_text(spans: &[Span<'_>]) -> String {
    spans.iter().map(|span| span.content.as_ref()).collect()
}

fn visible_hunk_keys(app: &DiffApp) -> Vec<(usize, usize)> {
    let visible_end = app
        .scroll
        .saturating_add(app.viewport_rows)
        .min(app.model.len());
    let mut hunks = Vec::new();
    for row_index in app.scroll..visible_end {
        if let Some(hunk) = app.model.row(row_index).and_then(|row| row.hunk_key())
            && hunks.last().copied() != Some(hunk)
        {
            hunks.push(hunk);
        }
    }
    hunks
}

fn assert_key_pair_moves_hunk_focus_when_diff_fits_viewport(forward: KeyCode, backward: KeyCode) {
    let changeset = changeset_with_hunks_at(PathBuf::from("/repo"), &[1, 2, 3]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(20);

    assert_eq!(app.max_scroll(), 0);
    assert_eq!(app.focused_hunk_for_viewport(20), Some((0, 0)));

    app.handle_key(KeyEvent::new(forward, KeyModifiers::NONE))
        .expect("forward key should be handled");
    assert_eq!(app.scroll, 0);
    assert_eq!(app.focused_hunk_for_viewport(20), Some((0, 1)));

    app.handle_key(KeyEvent::new(forward, KeyModifiers::NONE))
        .expect("forward key should be handled");
    assert_eq!(app.scroll, 0);
    assert_eq!(app.focused_hunk_for_viewport(20), Some((0, 2)));

    app.handle_key(KeyEvent::new(backward, KeyModifiers::NONE))
        .expect("backward key should be handled");
    assert_eq!(app.focused_hunk_for_viewport(20), Some((0, 1)));

    app.handle_key(KeyEvent::new(backward, KeyModifiers::NONE))
        .expect("backward key should be handled");
    assert_eq!(app.scroll, 0);
    assert_eq!(app.focused_hunk_for_viewport(20), Some((0, 0)));
}

fn assert_key_pair_scrolls_then_moves_hunk_focus_at_edges(
    forward: KeyCode,
    backward: KeyCode,
    scroll_delta: usize,
) {
    let changeset = changeset_with_files(&[
        "a.rs", "b.rs", "c.rs", "d.rs", "e.rs", "f.rs", "g.rs", "h.rs",
    ]);
    let mut app = DiffApp::new(DiffOptions::default(), changeset, DiffLayoutMode::Unified);
    app.set_viewport_rows(6);

    assert!(app.max_scroll() >= scroll_delta);
    app.handle_key(KeyEvent::new(forward, KeyModifiers::NONE))
        .expect("forward key should be handled");
    assert_eq!(app.scroll, scroll_delta);
    assert_eq!(app.manual_hunk_focus, None);

    app.handle_key(KeyEvent::new(backward, KeyModifiers::NONE))
        .expect("backward key should be handled");
    assert_eq!(app.scroll, 0);
    assert_eq!(app.manual_hunk_focus, None);

    let top_hunks = visible_hunk_keys(&app);
    assert!(top_hunks.len() >= 2);
    app.manual_hunk_focus = Some(top_hunks[1]);
    app.handle_key(KeyEvent::new(backward, KeyModifiers::NONE))
        .expect("backward key should be handled");
    assert_eq!(app.scroll, 0);
    assert_eq!(app.selected_file, top_hunks[0].0);
    assert_eq!(
        app.focused_hunk_for_viewport(app.viewport_rows),
        Some(top_hunks[0])
    );

    app.set_scroll(app.max_scroll());
    let bottom_scroll = app.scroll;
    let bottom_hunks = visible_hunk_keys(&app);
    assert!(bottom_hunks.len() >= 2);
    let previous = bottom_hunks[bottom_hunks.len() - 2];
    let next = bottom_hunks[bottom_hunks.len() - 1];
    app.manual_hunk_focus = Some(previous);
    app.handle_key(KeyEvent::new(forward, KeyModifiers::NONE))
        .expect("forward key should be handled");
    assert_eq!(app.scroll, bottom_scroll);
    assert_eq!(app.selected_file, next.0);
    assert_eq!(app.focused_hunk_for_viewport(app.viewport_rows), Some(next));
}

fn mouse_scroll(app: &mut DiffApp, kind: MouseEventKind) {
    app.handle_mouse(MouseEvent {
        kind,
        column: 0,
        row: 0,
        modifiers: KeyModifiers::NONE,
    })
    .expect("mouse wheel should be handled");
}

fn default_context_expand_step() -> usize {
    match DiffSettings::default().context_expansion {
        DiffContextExpansion::Lines(lines) => lines,
        DiffContextExpansion::Full => panic!("default context expansion should be bounded"),
    }
}

fn temp_test_dir(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "mark-tui-{name}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos()
    ))
}

fn git(repo: &Path, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(repo)
        .args(args)
        .output()
        .expect("git should run");
    assert!(
        output.status.success(),
        "git failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
