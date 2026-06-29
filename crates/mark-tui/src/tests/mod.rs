use crate::render::{
    diff::{
        SplitCellRender, SplitLineRender, SplitSide, build_diff_viewport_lines,
        content_spans_at_scroll, context_expand_marker, context_hide_line, context_hide_marker,
        context_show_line, empty_diff_fill_from, inline_bg, render_row, render_row_with_focus,
        render_row_wrapped_with_focus, render_split_context_line_wrapped,
        render_split_line_with_focus, render_unified_line_at_scroll, row_bg,
        split_cell_spans_at_scroll, syntax_fg,
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
    BranchName, Changeset, DiffLine, DiffLineKind, DiffOptions, DiffScope, DiffSource, FileChange,
    FileStatus, HunkLineRanges, PatchSource,
};
use mark_syntax::{
    ColorOverrides, DiffContextExpansion, HighlightedLine, LayoutSetting,
    MAX_NOTIFICATION_TIMEOUT_MS, NotificationMode, NotificationSettings, SyntaxClass,
    SyntaxLanguageSet, SyntaxLimits, SyntaxSettings, SyntaxThemeConfig, ToastCorner,
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

mod annotations;
mod app;
mod diff;
mod input;
mod menus;
mod misc;
mod render;
mod syntax;

const FILE_0: FileIndex = FileIndex::new(0);
const FILE_1: FileIndex = FileIndex::new(1);
const FILE_2: FileIndex = FileIndex::new(2);
const FILE_4: FileIndex = FileIndex::new(4);
const HUNK_0: HunkIndex = HunkIndex::new(0);
const HUNK_1: HunkIndex = HunkIndex::new(1);
const HUNK_2: HunkIndex = HunkIndex::new(2);
const LINE_0: DiffLineIndex = DiffLineIndex::new(0);
const LINE_1: DiffLineIndex = DiffLineIndex::new(1);
const LINE_2: DiffLineIndex = DiffLineIndex::new(2);

fn typed_hunk_key((file, hunk): (usize, usize)) -> (FileIndex, HunkIndex) {
    (FileIndex::new(file), HunkIndex::new(hunk))
}

fn branch_names(names: &[&str]) -> Vec<BranchName> {
    names.iter().copied().map(BranchName::from).collect()
}

fn handle_test_key_event(app: &mut DiffApp, key: KeyEvent) -> bool {
    let (_tx, rx) = mpsc::channel(1);
    let mut events = crate::event_reader::TerminalEventReader::from_receiver(rx);
    let mut live_diff = None;

    handle_event(app, Event::Key(key), &mut live_diff, &mut events)
        .expect("key event should be handled")
}

fn set_test_file_modified(file: &mut mark_diff::DiffFile, path: &str) {
    file.change = FileChange::modified(path.to_owned());
}

fn set_test_file_added(file: &mut mark_diff::DiffFile) {
    let path = file
        .new_path()
        .or_else(|| file.old_path())
        .unwrap_or("file.rs")
        .to_owned();
    file.change = FileChange::Added { path: path.into() };
}

fn set_test_file_deleted(file: &mut mark_diff::DiffFile) {
    let path = file
        .old_path()
        .or_else(|| file.new_path())
        .unwrap_or("file.rs")
        .to_owned();
    file.change = FileChange::Deleted { path: path.into() };
}

fn set_test_file_renamed(file: &mut mark_diff::DiffFile, old_path: &str, new_path: &str) {
    file.change = FileChange::Renamed {
        old_path: old_path.to_owned().into(),
        new_path: new_path.to_owned().into(),
    };
}

fn changeset_with_context_lines(line_count: usize) -> Changeset {
    changeset_with_context_lines_at(PathBuf::from("/repo"), 1, line_count)
}

fn changeset_with_context_lines_at(repo: PathBuf, start: usize, line_count: usize) -> Changeset {
    let lines = (1..=line_count)
        .map(|line| {
            DiffLine::context(
                start.saturating_add(line - 1),
                start.saturating_add(line - 1),
                format!("line {line}"),
            )
        })
        .collect();

    Changeset {
        repo: repo.into(),
        title: "test".to_owned(),
        files: vec![mark_diff::DiffFile {
            change: mark_diff::FileChange::from_status(
                mark_diff::FileStatus::Modified,
                Some("file.rs".to_owned()),
                Some("file.rs".to_owned()),
            ),
            additions: 0,
            deletions: 0,
            body: mark_diff::DiffFileBody::Text {
                hunks: vec![mark_diff::DiffHunk {
                    header: format!("@@ -{start} +{start} @@"),
                    ranges: HunkLineRanges::new(start, line_count, start, line_count),
                    lines,
                }],
            },
        }],
        raw_patch: Vec::new(),
    }
}

fn changeset_with_line_text(text: &str) -> Changeset {
    changeset_with_line_texts(&[text])
}

fn changeset_with_line_texts(texts: &[&str]) -> Changeset {
    Changeset {
        repo: PathBuf::from("/repo").into(),
        title: "test".to_owned(),
        files: vec![mark_diff::DiffFile {
            change: mark_diff::FileChange::from_status(
                mark_diff::FileStatus::Modified,
                Some("file.rs".to_owned()),
                Some("file.rs".to_owned()),
            ),
            additions: 0,
            deletions: 0,
            body: mark_diff::DiffFileBody::Text {
                hunks: vec![mark_diff::DiffHunk {
                    header: "@@ -1 +1 @@".to_owned(),
                    ranges: HunkLineRanges::new(1, texts.len(), 1, texts.len()),
                    lines: texts
                        .iter()
                        .enumerate()
                        .map(|(index, text)| {
                            DiffLine::context(index + 1, index + 1, (*text).to_owned())
                        })
                        .collect(),
                }],
            },
        }],
        raw_patch: Vec::new(),
    }
}

fn changeset_with_replacement_pair() -> Changeset {
    Changeset {
        repo: PathBuf::from("/repo").into(),
        title: "test".to_owned(),
        files: vec![mark_diff::DiffFile {
            change: mark_diff::FileChange::from_status(
                mark_diff::FileStatus::Modified,
                Some("file.rs".to_owned()),
                Some("file.rs".to_owned()),
            ),
            additions: 1,
            deletions: 1,
            body: mark_diff::DiffFileBody::Text {
                hunks: vec![mark_diff::DiffHunk {
                    header: "@@ -1 +1 @@".to_owned(),
                    ranges: HunkLineRanges::new(1, 1, 1, 1),
                    lines: vec![
                        DiffLine::deletion(1, "old".to_owned()),
                        DiffLine::addition(1, "new".to_owned()),
                    ],
                }],
            },
        }],
        raw_patch: Vec::new(),
    }
}

fn changeset_with_wrapped_leading_file() -> Changeset {
    let mut changeset = changeset_with_files(&["wide.rs", "target.rs"]);
    *changeset.files[0].hunks_mut()[0].lines[0].text_mut() = "a".repeat(96);
    changeset
}

fn set_wrapped_scroll_relative_to_file_start(
    app: &mut DiffApp,
    file: usize,
    relative_scroll: usize,
) {
    app.viewport.line_wrapping = true;
    app.set_viewport_width(18);
    app.set_scroll(wrapped_file_start_scroll(app, file).saturating_add(relative_scroll));
    assert_eq!(app.sidebar.selected_file, FileIndex::new(file));
}

fn wrapped_file_start_scroll(app: &DiffApp, file: usize) -> usize {
    let row = app
        .document
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
            ranges: HunkLineRanges::new(*line_number, 1, *line_number, 1),
            lines: vec![DiffLine::context(
                *line_number,
                *line_number,
                format!("line {line_number}"),
            )],
        })
        .collect();

    Changeset {
        repo: repo.into(),
        title: "test".to_owned(),
        files: vec![mark_diff::DiffFile {
            change: mark_diff::FileChange::from_status(
                mark_diff::FileStatus::Modified,
                Some("file.rs".to_owned()),
                Some("file.rs".to_owned()),
            ),
            additions: 0,
            deletions: 0,
            body: mark_diff::DiffFileBody::Text { hunks },
        }],
        raw_patch: Vec::new(),
    }
}

fn changeset_with_hunk_line_counts(repo: PathBuf, hunks: &[(usize, usize)]) -> Changeset {
    let hunks = hunks
        .iter()
        .map(|(line_number, line_count)| mark_diff::DiffHunk {
            header: format!("@@ -{line_number},{line_count} +{line_number},{line_count} @@"),
            ranges: HunkLineRanges::new(*line_number, *line_count, *line_number, *line_count),
            lines: (0..*line_count)
                .map(|offset| {
                    DiffLine::context(
                        line_number + offset,
                        line_number + offset,
                        format!("line {}", line_number + offset),
                    )
                })
                .collect(),
        })
        .collect();

    Changeset {
        repo: repo.into(),
        title: "test".to_owned(),
        files: vec![mark_diff::DiffFile {
            change: mark_diff::FileChange::from_status(
                mark_diff::FileStatus::Modified,
                Some("file.rs".to_owned()),
                Some("file.rs".to_owned()),
            ),
            additions: 0,
            deletions: 0,
            body: mark_diff::DiffFileBody::Text { hunks },
        }],
        raw_patch: Vec::new(),
    }
}

fn changeset_with_files(paths: &[&str]) -> Changeset {
    let files = paths
        .iter()
        .enumerate()
        .map(|(index, path)| mark_diff::DiffFile {
            change: mark_diff::FileChange::from_status(
                mark_diff::FileStatus::Modified,
                Some((*path).to_owned()),
                Some((*path).to_owned()),
            ),
            additions: index + 1,
            deletions: index,
            body: mark_diff::DiffFileBody::Text {
                hunks: vec![mark_diff::DiffHunk {
                    header: "@@ -1 +1 @@".to_owned(),
                    ranges: HunkLineRanges::new(1, 1, 1, 1),
                    lines: vec![DiffLine::context(1, 1, format!("line {index}"))],
                }],
            },
        })
        .collect();

    Changeset {
        repo: PathBuf::from("/repo").into(),
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
        branch_metadata: BranchMetadataPolicy::Preserve,
        job: AsyncJob::new(rx),
    }
}

fn pending_review_load() -> PendingReviewLoad {
    let (_tx, rx) = oneshot::channel();
    PendingReviewLoad {
        error_prefix: "review unavailable".to_owned(),
        job: AsyncJob::new(rx),
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
        repo: PathBuf::from("/repo").into(),
        kind: FullFileSourceKind::Worktree {
            path: "file.rs".into(),
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
    app.document
        .model
        .visible_files()
        .iter()
        .filter_map(|file| app.document.changeset.files.get(file.get()))
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
        .viewport
        .scroll
        .saturating_add(app.viewport.viewport_rows)
        .min(app.document.model.len());
    let mut hunks = Vec::new();
    for row_index in app.viewport.scroll..visible_end {
        if let Some(hunk) = app
            .document
            .model
            .row(row_index)
            .and_then(|row| row.hunk_key())
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
    assert_eq!(app.focused_hunk_for_viewport(20), Some((FILE_0, HUNK_0)));

    app.handle_key(KeyEvent::new(forward, KeyModifiers::NONE))
        .expect("forward key should be handled");
    assert_eq!(app.viewport.scroll, 0);
    assert_eq!(app.focused_hunk_for_viewport(20), Some((FILE_0, HUNK_1)));

    app.handle_key(KeyEvent::new(forward, KeyModifiers::NONE))
        .expect("forward key should be handled");
    assert_eq!(app.viewport.scroll, 0);
    assert_eq!(app.focused_hunk_for_viewport(20), Some((FILE_0, HUNK_2)));

    app.handle_key(KeyEvent::new(backward, KeyModifiers::NONE))
        .expect("backward key should be handled");
    assert_eq!(app.focused_hunk_for_viewport(20), Some((FILE_0, HUNK_1)));

    app.handle_key(KeyEvent::new(backward, KeyModifiers::NONE))
        .expect("backward key should be handled");
    assert_eq!(app.viewport.scroll, 0);
    assert_eq!(app.focused_hunk_for_viewport(20), Some((FILE_0, HUNK_0)));
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
    assert_eq!(app.viewport.scroll, scroll_delta);
    assert_eq!(app.viewport.manual_hunk_focus, None);

    app.handle_key(KeyEvent::new(backward, KeyModifiers::NONE))
        .expect("backward key should be handled");
    assert_eq!(app.viewport.scroll, 0);
    assert_eq!(app.viewport.manual_hunk_focus, None);

    let top_hunks = visible_hunk_keys(&app);
    assert!(top_hunks.len() >= 2);
    app.viewport.manual_hunk_focus = Some(typed_hunk_key(top_hunks[1]));
    app.handle_key(KeyEvent::new(backward, KeyModifiers::NONE))
        .expect("backward key should be handled");
    assert_eq!(app.viewport.scroll, 0);
    assert_eq!(app.sidebar.selected_file, FileIndex::new(top_hunks[0].0));
    assert_eq!(
        app.focused_hunk_for_viewport(app.viewport.viewport_rows),
        Some(typed_hunk_key(top_hunks[0]))
    );

    app.set_scroll(app.max_scroll());
    let bottom_scroll = app.viewport.scroll;
    let bottom_hunks = visible_hunk_keys(&app);
    assert!(bottom_hunks.len() >= 2);
    let previous = bottom_hunks[bottom_hunks.len() - 2];
    let next = bottom_hunks[bottom_hunks.len() - 1];
    app.viewport.manual_hunk_focus = Some(typed_hunk_key(previous));
    app.handle_key(KeyEvent::new(forward, KeyModifiers::NONE))
        .expect("forward key should be handled");
    assert_eq!(app.viewport.scroll, bottom_scroll);
    assert_eq!(app.sidebar.selected_file, FileIndex::new(next.0));
    assert_eq!(
        app.focused_hunk_for_viewport(app.viewport.viewport_rows),
        Some(typed_hunk_key(next))
    );
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
    20
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
