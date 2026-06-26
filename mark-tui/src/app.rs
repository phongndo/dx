use std::{
    cell::RefCell,
    collections::{HashMap, HashSet, VecDeque},
    fs,
    io::{self, Write},
    ops::Range,
    path::{Path, PathBuf},
    process,
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};

#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use mark_core::{MarkError, MarkResult};
use mark_diff::{Changeset, DiffOptions, DiffScope, DiffSource, DiffStats};
use mark_syntax::{
    ColorOverrides, DiffContextExpansion, HighlightedLine, LayoutSetting, SyntaxLimits,
    SyntaxSettings, SyntaxThemeConfig, SyntaxThemeSource,
};
use ratatui::layout::Rect;
use tempfile::TempDir;
use tokio::sync::{mpsc::Receiver, oneshot};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::{
    annotation::{
        AnnotationDraft, AnnotationKey, AnnotationSide, AnnotationStore,
        paired_old_line_for_addition,
    },
    controls::{
        BranchMenu, CrosstermTerminal, DiffChoice, DiffFilterKind, DiffLayoutMode, GitCommit,
        branch_base_from_options, branch_head_from_options, branch_match_score, commit_match_score,
        commit_menu_width, commit_short_sha, comparison_branches, comparison_commits,
        current_head_label, default_branch_base, default_layout_for_width, diff_stats_for_files,
        is_review_options, rev_display_label,
    },
    editor::{EditorTarget, configured_editor, open_editor, open_text_in_editor, repo_file_path},
    event_reader::TerminalEventReader,
    keymap::{GlobalAction, Keymap, MenuAction},
    live_diff::{LiveDiff, LiveDiffReload, live_diff_supported},
    model::{
        ContextExpansionDirection, ContextKey, ContextSourceEntry, ContextSourceKey, UiModel,
        UiRow, context_expansion_direction,
    },
    render::{
        annotations::{
            annotation_close_hit_at_column, annotation_compose_block_height,
            annotation_edit_hit_at_column, annotation_hit_at_column, annotation_saved_block_height,
            annotation_submit_hit_at_column,
        },
        draw,
        menus::{
            branch_menu_block, branch_menu_list_visible_rows, branch_menu_width,
            color_scheme_picker_block, color_scheme_picker_list_visible_rows, commit_menu_block,
            commit_menu_list_visible_rows, diff_menu_block, diff_selector_width,
            help_menu_list_visible_rows,
        },
        sidebar::max_file_sidebar_width,
        viewport_plan::{
            ViewportSlotKind, annotation_saved_key_at_bottom_border,
            annotation_saved_key_at_top_border, compose_block_bottom_viewport_row,
            compose_block_top_viewport_row, model_row_for_viewport_row,
            plan_diff_viewport_rows_at_scroll, visual_scroll_for_viewport_row,
        },
    },
    runtime,
    search::{DiffSearchIndex, DiffSearchResult, grep_match_rows},
    syntax::{
        DiffSide, InlineHunkEmphasisCache, InlineHunkKey, InlineRange, LruCache, SyntaxPosition,
        SyntaxPriority, SyntaxRuntime, available_context_lines, full_file_source,
        load_full_file_source, split_context_source_lines, unified_syntax_side,
    },
    theme::{
        BASE_BRANCH_MARKER, BRANCH_COMPARISON_SEPARATOR, CURRENT_BRANCH_MARKER, DiffTheme,
        EVENT_POLL, FILE_SIDEBAR_MIN_WIDTH, GUTTER_WIDTH, HELP_MENU_ROWS, HORIZONTAL_SCROLL_STEP,
        HelpMenuKey, HelpMenuRow, MAX_BRANCH_MENU_ROWS, MAX_INLINE_DIFF_CACHE_ENTRIES,
        MAX_READY_EVENTS_PER_FRAME, MAX_SYNTAX_RESULTS_PER_FRAME, MOUSE_SCROLL_ACCEL_A,
        MOUSE_SCROLL_ACCEL_TAU, MOUSE_SCROLL_HISTORY_SIZE, MOUSE_SCROLL_MAX_MULTIPLIER,
        MOUSE_SCROLL_MIN_TICK_INTERVAL, MOUSE_SCROLL_REFERENCE_INTERVAL_MS,
        MOUSE_SCROLL_STREAK_TIMEOUT, NOTICE_TTL, STATUSLINE_SELECTOR_GAP, SyntaxBenchmarkReport,
        UNIFIED_GUTTER_WIDTH, diff_theme_from_config,
    },
};

const MOUSE_HUNK_FOCUS_SCROLL_TICKS: isize = 3;
const EDITOR_RELOAD_POLL: Duration = Duration::from_millis(8);
const FILTER_DEBOUNCE: Duration = Duration::from_millis(120);
const DIFF_PREFETCH_POLL: Duration = Duration::from_millis(8);
const FILTER_WORKER_POLL: Duration = Duration::from_millis(8);
const MAX_LIVE_GREP_MATCHES: usize = 10_000;
const MAX_DIFF_CACHE_ENTRIES: usize = 4;
const MAX_COLOR_SCHEME_MENU_ROWS: usize = 9;
pub(crate) const ERROR_LOG_DEFAULT_HEIGHT: u16 = 6;
pub(crate) const ERROR_LOG_MIN_HEIGHT: u16 = 3;
pub(crate) const ERROR_LOG_MAX_HEIGHT: u16 = 40;
const POST_EDITOR_QUIT_KEY_IGNORE: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HunkFocusScrollBehavior {
    Preserve,
    ClearOnScroll,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HunkFocusModelBehavior {
    PreserveIfValid,
    Clear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditorReloadBehavior {
    None,
    ScopedAsync,
    Sync,
}

struct FocusedEditorLaunch {
    target: EditorTarget,
    editor: String,
}

pub(crate) struct EditorReloadWorker {
    generation: u64,
    rx: oneshot::Receiver<EditorScopedReload>,
}

impl std::fmt::Debug for EditorReloadWorker {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("EditorReloadWorker").finish()
    }
}

pub(crate) struct EditorScopedReload {
    path: PathBuf,
    changeset: MarkResult<Changeset>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PendingFilterApply {
    generation: u64,
    due_at: Instant,
    jump_to_grep: bool,
}

pub(crate) struct FilterWorker {
    generation: u64,
    file_filter: String,
    grep_filter: String,
    jump_to_grep: bool,
    rx: oneshot::Receiver<DiffSearchResult>,
}

impl std::fmt::Debug for FilterWorker {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FilterWorker")
            .field("generation", &self.generation)
            .field("file_filter", &self.file_filter)
            .field("grep_filter", &self.grep_filter)
            .field("jump_to_grep", &self.jump_to_grep)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub(crate) struct Notice {
    pub(crate) text: String,
    pub(crate) expires_at: Instant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MarkExport {
    path: String,
    old_line: Option<usize>,
    new_line: Option<usize>,
    body: String,
}

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub(crate) fn osc52_clipboard_sequence(text: &str) -> String {
    format!("\x1b]52;c;{}\x07", base64_encode(text.as_bytes()))
}

pub(crate) fn write_osc52_clipboard<W: Write>(writer: &mut W, text: &str) -> io::Result<()> {
    writer.write_all(osc52_clipboard_sequence(text).as_bytes())?;
    writer.flush()
}

fn base64_encode(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);

        encoded.push(BASE64[(first >> 2) as usize] as char);
        encoded.push(BASE64[(((first & 0b0000_0011) << 4) | (second >> 4)) as usize] as char);
        if chunk.len() > 1 {
            encoded.push(BASE64[(((second & 0b0000_1111) << 2) | (third >> 6)) as usize] as char);
        } else {
            encoded.push('=');
        }
        if chunk.len() > 2 {
            encoded.push(BASE64[(third & 0b0011_1111) as usize] as char);
        } else {
            encoded.push('=');
        }
    }
    encoded
}

fn json_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            character if character <= '\u{1f}' => {
                out.push_str("\\u00");
                let value = character as u8;
                out.push(hex_digit(value >> 4));
                out.push(hex_digit(value & 0x0f));
            }
            character => out.push(character),
        }
    }
    out.push('"');
    out
}

fn hex_digit(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        10..=15 => (b'a' + (value - 10)) as char,
        _ => '0',
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EditorReloadRequest {
    pub(crate) path: PathBuf,
    pub(crate) pathspecs: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FileFingerprint {
    len: u64,
    modified: Option<SystemTime>,
    #[cfg(unix)]
    changed: (i64, i64, u64),
}

impl FileFingerprint {
    pub(crate) fn read(path: &Path) -> Option<Self> {
        let metadata = fs::metadata(path).ok()?;
        Some(Self {
            len: metadata.len(),
            modified: metadata.modified().ok(),
            #[cfg(unix)]
            changed: (metadata.ctime(), metadata.ctime_nsec(), metadata.ino()),
        })
    }
}

pub(crate) fn file_changed_since(path: &Path, before: Option<FileFingerprint>) -> bool {
    let after = FileFingerprint::read(path);
    match (before, after) {
        (Some(before), Some(after)) => before != after,
        (None, None) => false,
        _ => true,
    }
}

pub(crate) async fn run_loop(
    terminal: &mut CrosstermTerminal,
    app: &mut DiffApp,
    live_updates: bool,
    live_diff: &mut Option<LiveDiff>,
) -> MarkResult<()> {
    let mut events = TerminalEventReader::start("mark-diff-events")?;

    loop {
        app.expire_notice(Instant::now());
        drain_live_diff_invalidation(app, live_diff.as_ref());
        sync_live_diff(live_diff, app, live_updates);
        drain_live_reloads(
            app,
            live_diff.as_mut().map(|live_diff| &mut live_diff.reload_rx),
        );
        app.drain_pending_diff_load();
        app.drain_diff_prefetch();
        app.start_due_filter_apply();
        app.drain_filter_worker();
        app.drain_syntax();
        if app.dirty {
            if app.terminal_clear_requested {
                terminal.clear()?;
                app.terminal_clear_requested = false;
            }
            terminal.draw(|frame| draw(frame, app))?;
            app.dirty = false;
            app.start_diff_prefetches();
        }
        app.start_pending_editor_reload();
        if app.drain_editor_reload() {
            continue;
        }

        if let Some(event) = events.read_timeout(app.event_poll()).await?
            && handle_ready_events(app, live_diff, event, &mut events)?
        {
            break;
        }
    }

    Ok(())
}

fn handle_ready_events(
    app: &mut DiffApp,
    live_diff: &mut Option<LiveDiff>,
    first_event: Event,
    events: &mut TerminalEventReader,
) -> MarkResult<bool> {
    if handle_event(app, first_event, live_diff, events)? {
        return Ok(true);
    }

    for _ in 1..MAX_READY_EVENTS_PER_FRAME {
        let Some(event) = events.try_read()? else {
            break;
        };
        if handle_event(app, event, live_diff, events)? {
            return Ok(true);
        }
    }

    Ok(false)
}

pub(crate) fn drain_live_diff_invalidation(app: &mut DiffApp, live_diff: Option<&LiveDiff>) {
    if live_diff.is_some_and(|live_diff| live_diff.take_invalidated()) {
        app.mark_live_reload_invalidated();
    }
}

pub(crate) fn sync_live_diff(
    live_diff: &mut Option<LiveDiff>,
    app: &mut DiffApp,
    live_updates: bool,
) {
    if !live_updates
        || !app.live_updates_allowed
        || !app.live_updates_enabled
        || !live_diff_supported(&app.options)
    {
        *live_diff = None;
        app.live_diff_failed_options = None;
        app.live_reload_invalidated = false;
        app.live_reload_pending = false;
        app.clear_cached_diff_choices();
        return;
    }

    if live_diff
        .as_ref()
        .is_some_and(|live_diff| live_diff.options == app.options)
    {
        return;
    }
    if app.live_diff_failed_options.as_ref() == Some(&app.options) {
        return;
    }

    match LiveDiff::start(app.options.clone(), &app.changeset.repo) {
        Ok(next_live_diff) => {
            app.live_diff_failed_options = None;
            app.live_reload_invalidated = false;
            app.live_reload_pending = false;
            *live_diff = Some(next_live_diff);
        }
        Err(error) => {
            *live_diff = None;
            app.live_diff_failed_options = Some(app.options.clone());
            app.live_reload_invalidated = false;
            app.live_reload_pending = false;
            app.clear_cached_diff_choices();
            app.set_error_log(format!("live reload unavailable: {error}"));
        }
    }
}

pub(crate) fn drain_live_reloads(
    app: &mut DiffApp,
    live_reload_rx: Option<&mut Receiver<LiveDiffReload>>,
) {
    let Some(live_reload_rx) = live_reload_rx else {
        return;
    };

    while let Ok(reload) = live_reload_rx.try_recv() {
        match reload {
            LiveDiffReload::Started => {
                if !app.live_reload_pending {
                    app.mark_live_reload_pending();
                }
            }
            LiveDiffReload::Loaded(Ok(changeset)) => app.replace_changeset(changeset),
            LiveDiffReload::Loaded(Err(error)) => {
                app.live_reload_invalidated = false;
                app.live_reload_pending = false;
                app.set_error_log(format!("live reload failed: {error}"));
            }
        }
    }
}

pub(crate) fn handle_event(
    app: &mut DiffApp,
    event: Event,
    live_diff: &mut Option<LiveDiff>,
    events: &mut TerminalEventReader,
) -> MarkResult<bool> {
    drain_live_diff_invalidation(app, live_diff.as_ref());

    match event {
        Event::Key(key) if app.ignore_post_editor_quit_key(key, Instant::now()) => Ok(false),
        Event::Key(key) if app.handle_annotation_save_or_cancel_key(key) => Ok(false),
        Event::Key(key) if is_quit_key(key) => Ok(true),
        Event::Key(key)
            if app.keymap.matches_single(GlobalAction::EditHunk, key)
                && app.editor_shortcut_available() =>
        {
            if app.annotation_draft.is_some() {
                let paused_events = events.pause();
                app.open_annotation_draft_in_editor();
                paused_events.resume()?;
            } else if let Some(editor) = app.prepare_focused_hunk_editor() {
                let paused_events = events.pause();
                app.open_prepared_hunk_in_editor(editor, Some(live_diff));
                paused_events.resume()?;
            }
            Ok(false)
        }
        Event::Key(key) if app.handle_key(key)? => Ok(true),
        Event::Mouse(mouse) => {
            app.handle_mouse(mouse)?;
            Ok(false)
        }
        Event::FocusLost => {
            app.clear_diff_mouse_hover();
            Ok(false)
        }
        Event::Resize(width, height) => {
            app.clear_diff_mouse_hover();
            app.set_terminal_area(Rect {
                x: 0,
                y: 0,
                width,
                height,
            });
            app.apply_responsive_layout(width);
            Ok(false)
        }
        _ => Ok(false),
    }
}

pub(crate) fn is_quit_key(key: KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL)
        && !key.modifiers.contains(KeyModifiers::SHIFT)
        && key.code == KeyCode::Char('c')
}

pub(crate) fn is_plain_char_key(key: KeyEvent, character: char) -> bool {
    key.code == KeyCode::Char(character)
        && !key.modifiers.contains(KeyModifiers::CONTROL)
        && !key.modifiers.contains(KeyModifiers::ALT)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextInputKeyResult {
    Ignored,
    Handled,
    Moved,
    Edited,
}

fn handle_text_input_key(
    input: &mut String,
    cursor: &mut usize,
    key: KeyEvent,
) -> TextInputKeyResult {
    clamp_text_cursor(input, cursor);
    if input.is_empty() {
        match key.code {
            KeyCode::Home
            | KeyCode::End
            | KeyCode::Left
            | KeyCode::Right
            | KeyCode::Backspace
            | KeyCode::Delete => return TextInputKeyResult::Ignored,
            KeyCode::Char('a' | 'e' | 'u' | 'k' | 'w')
                if key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                return TextInputKeyResult::Ignored;
            }
            _ => {}
        }
    }
    let before_input = input.clone();
    let before_cursor = *cursor;

    let handled = match key.code {
        KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            *cursor = line_start(input, *cursor);
            true
        }
        KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            *cursor = line_end(input, *cursor);
            true
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            delete_range(input, cursor, line_start(input, *cursor), *cursor);
            true
        }
        KeyCode::Char('k') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            delete_range(input, cursor, *cursor, line_end(input, *cursor));
            true
        }
        KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            delete_range(
                input,
                cursor,
                previous_word_boundary(input, *cursor),
                *cursor,
            );
            true
        }
        KeyCode::Home => {
            *cursor = line_start(input, *cursor);
            true
        }
        KeyCode::End => {
            *cursor = line_end(input, *cursor);
            true
        }
        KeyCode::Left if key_has_command_modifier(key.modifiers) => {
            *cursor = line_start(input, *cursor);
            true
        }
        KeyCode::Right if key_has_command_modifier(key.modifiers) => {
            *cursor = line_end(input, *cursor);
            true
        }
        KeyCode::Left if key_has_word_modifier(key.modifiers) => {
            *cursor = previous_word_boundary(input, *cursor);
            true
        }
        KeyCode::Right if key_has_word_modifier(key.modifiers) => {
            *cursor = next_word_boundary(input, *cursor);
            true
        }
        KeyCode::Left => {
            *cursor = previous_char_boundary(input, *cursor);
            true
        }
        KeyCode::Right => {
            *cursor = next_char_boundary(input, *cursor);
            true
        }
        KeyCode::Backspace | KeyCode::Delete if key_has_command_modifier(key.modifiers) => {
            delete_range(input, cursor, line_start(input, *cursor), *cursor);
            true
        }
        KeyCode::Backspace if key_has_word_modifier(key.modifiers) => {
            delete_range(
                input,
                cursor,
                previous_word_boundary(input, *cursor),
                *cursor,
            );
            true
        }
        KeyCode::Delete if key_has_word_modifier(key.modifiers) => {
            delete_range(input, cursor, *cursor, next_word_boundary(input, *cursor));
            true
        }
        KeyCode::Backspace => {
            delete_range(
                input,
                cursor,
                previous_char_boundary(input, *cursor),
                *cursor,
            );
            true
        }
        KeyCode::Delete => {
            delete_range(input, cursor, *cursor, next_char_boundary(input, *cursor));
            true
        }
        KeyCode::Char(character) if is_text_input_character(key.modifiers) => {
            input.insert(*cursor, character);
            *cursor += character.len_utf8();
            true
        }
        _ => false,
    };

    if !handled {
        TextInputKeyResult::Ignored
    } else if before_input != *input {
        TextInputKeyResult::Edited
    } else if before_cursor != *cursor {
        TextInputKeyResult::Moved
    } else {
        TextInputKeyResult::Handled
    }
}

fn is_text_input_character(modifiers: KeyModifiers) -> bool {
    !modifiers.intersects(
        KeyModifiers::CONTROL
            | KeyModifiers::ALT
            | KeyModifiers::SUPER
            | KeyModifiers::HYPER
            | KeyModifiers::META,
    )
}

fn key_has_command_modifier(modifiers: KeyModifiers) -> bool {
    modifiers.intersects(KeyModifiers::SUPER | KeyModifiers::META)
}

fn key_has_word_modifier(modifiers: KeyModifiers) -> bool {
    modifiers.intersects(KeyModifiers::ALT | KeyModifiers::CONTROL)
}

fn clamp_text_cursor(input: &str, cursor: &mut usize) {
    *cursor = (*cursor).min(input.len());
    while *cursor > 0 && !input.is_char_boundary(*cursor) {
        *cursor -= 1;
    }
}

fn delete_range(input: &mut String, cursor: &mut usize, start: usize, end: usize) {
    let start = start.min(input.len());
    let end = end.min(input.len());
    if start >= end || !input.is_char_boundary(start) || !input.is_char_boundary(end) {
        return;
    }
    input.replace_range(start..end, "");
    *cursor = start;
}

fn previous_char_boundary(input: &str, cursor: usize) -> usize {
    input[..cursor.min(input.len())]
        .char_indices()
        .last()
        .map(|(index, _)| index)
        .unwrap_or(0)
}

fn next_char_boundary(input: &str, cursor: usize) -> usize {
    let cursor = cursor.min(input.len());
    input[cursor..]
        .chars()
        .next()
        .map(|character| cursor + character.len_utf8())
        .unwrap_or(cursor)
}

fn line_start(input: &str, cursor: usize) -> usize {
    input[..cursor.min(input.len())]
        .rfind('\n')
        .map(|index| index + 1)
        .unwrap_or(0)
}

fn line_end(input: &str, cursor: usize) -> usize {
    let cursor = cursor.min(input.len());
    input[cursor..]
        .find('\n')
        .map(|index| cursor + index)
        .unwrap_or(input.len())
}

fn previous_word_boundary(input: &str, cursor: usize) -> usize {
    let mut index = cursor.min(input.len());
    while index > 0 {
        let prev = previous_char_boundary(input, index);
        let ch = input[prev..index].chars().next().unwrap_or_default();
        if !ch.is_whitespace() {
            break;
        }
        index = prev;
    }
    while index > 0 {
        let prev = previous_char_boundary(input, index);
        let ch = input[prev..index].chars().next().unwrap_or_default();
        if ch.is_whitespace() {
            break;
        }
        index = prev;
    }
    index
}

fn next_word_boundary(input: &str, cursor: usize) -> usize {
    let mut index = cursor.min(input.len());
    while index < input.len() {
        let next = next_char_boundary(input, index);
        let ch = input[index..next].chars().next().unwrap_or_default();
        if ch.is_whitespace() {
            break;
        }
        index = next;
    }
    while index < input.len() {
        let next = next_char_boundary(input, index);
        let ch = input[index..next].chars().next().unwrap_or_default();
        if !ch.is_whitespace() {
            break;
        }
        index = next;
    }
    index
}

fn rect_contains(area: Rect, column: u16, row: u16) -> bool {
    column >= area.x
        && column < area.x.saturating_add(area.width)
        && row >= area.y
        && row < area.y.saturating_add(area.height)
}

/// Keeps `selected` visible in a scrollable list of `item_count` rows.
pub(crate) fn ensure_selector_scroll(
    scroll: &mut usize,
    selected: usize,
    item_count: usize,
    visible_rows: usize,
) {
    if visible_rows == 0 {
        *scroll = 0;
        return;
    }

    let max_scroll = item_count.saturating_sub(visible_rows.max(1));
    if selected < *scroll {
        *scroll = selected;
    } else if selected >= scroll.saturating_add(visible_rows) {
        *scroll = selected.saturating_add(1).saturating_sub(visible_rows);
    }
    *scroll = (*scroll).min(max_scroll);
}

pub(crate) fn show_rev_from_options(options: &DiffOptions) -> Option<String> {
    match &options.source {
        DiffSource::Show(rev) if !rev.is_empty() => Some(rev.clone()),
        _ => None,
    }
}

pub(crate) fn diff_choice_for_options(options: &DiffOptions) -> Option<DiffChoice> {
    if is_review_options(options) {
        return Some(DiffChoice::Review);
    }

    match (&options.source, options.scope) {
        (DiffSource::Worktree, DiffScope::All) => Some(DiffChoice::All),
        (DiffSource::Worktree, DiffScope::Unstaged) => Some(DiffChoice::Unstaged),
        (DiffSource::Worktree, DiffScope::Staged) => Some(DiffChoice::Staged),
        (DiffSource::Base(_) | DiffSource::Branch { .. }, DiffScope::All) => {
            Some(DiffChoice::Branch)
        }
        (DiffSource::Show(_), DiffScope::All) => Some(DiffChoice::Show),
        _ => None,
    }
}

pub(crate) fn cacheable_diff_options(options: &DiffOptions) -> bool {
    !options.stat
        && !matches!(
            options.source,
            DiffSource::Patch(_) | DiffSource::Difftool { .. }
        )
}

pub(crate) fn next_context_expansion(expansion: DiffContextExpansion) -> DiffContextExpansion {
    match expansion {
        DiffContextExpansion::Lines(lines) if lines < 20 => DiffContextExpansion::Lines(20),
        DiffContextExpansion::Lines(lines) if lines < 50 => DiffContextExpansion::Lines(50),
        DiffContextExpansion::Lines(_) => DiffContextExpansion::Full,
        DiffContextExpansion::Full => DiffContextExpansion::Lines(5),
    }
}

pub(crate) fn previous_context_expansion(expansion: DiffContextExpansion) -> DiffContextExpansion {
    match expansion {
        DiffContextExpansion::Lines(lines) if lines <= 5 => DiffContextExpansion::Full,
        DiffContextExpansion::Lines(lines) if lines <= 20 => DiffContextExpansion::Lines(5),
        DiffContextExpansion::Lines(lines) if lines <= 50 => DiffContextExpansion::Lines(20),
        DiffContextExpansion::Lines(_) => DiffContextExpansion::Lines(50),
        DiffContextExpansion::Full => DiffContextExpansion::Lines(50),
    }
}

pub(crate) fn context_expansion_label(expansion: DiffContextExpansion) -> String {
    match expansion {
        DiffContextExpansion::Lines(lines) => lines.to_string(),
        DiffContextExpansion::Full => "full".to_owned(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ColorSchemeChoice {
    Custom,
    System,
    CatppuccinLatte,
    CatppuccinFrappe,
    CatppuccinMacchiato,
    CatppuccinMocha,
    GruvboxDark,
    GruvboxLight,
    GithubDark,
    GithubDarkHighContrast,
    GithubLight,
    GithubLightHighContrast,
    Tokyonight,
}

pub(crate) const COLOR_SCHEME_CHOICES: &[ColorSchemeChoice] = &[
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
];

pub(crate) fn color_scheme_label(choice: ColorSchemeChoice) -> &'static str {
    match choice {
        ColorSchemeChoice::Custom => "custom",
        ColorSchemeChoice::System => "system",
        ColorSchemeChoice::CatppuccinLatte => "catppuccin-latte",
        ColorSchemeChoice::CatppuccinFrappe => "catppuccin-frappe",
        ColorSchemeChoice::CatppuccinMacchiato => "catppuccin-macchiato",
        ColorSchemeChoice::CatppuccinMocha => "catppuccin-mocha",
        ColorSchemeChoice::GruvboxDark => "gruvbox-dark",
        ColorSchemeChoice::GruvboxLight => "gruvbox-light",
        ColorSchemeChoice::GithubDark => "github-dark",
        ColorSchemeChoice::GithubDarkHighContrast => "github-dark-high-contrast",
        ColorSchemeChoice::GithubLight => "github-light",
        ColorSchemeChoice::GithubLightHighContrast => "github-light-high-contrast",
        ColorSchemeChoice::Tokyonight => "tokyonight",
    }
}

pub(crate) fn color_scheme_from_config(config: &SyntaxThemeConfig) -> ColorSchemeChoice {
    match config.source {
        SyntaxThemeSource::Ansi | SyntaxThemeSource::Base16 => ColorSchemeChoice::Custom,
        SyntaxThemeSource::Builtin => color_scheme_from_name(config.name.as_deref()),
    }
}

pub(crate) fn color_scheme_from_name(name: Option<&str>) -> ColorSchemeChoice {
    match name
        .unwrap_or("system")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "system" | "default" | "" => ColorSchemeChoice::System,
        "catppuccin-latte" | "latte" => ColorSchemeChoice::CatppuccinLatte,
        "catppuccin-frappe" | "frappe" => ColorSchemeChoice::CatppuccinFrappe,
        "catppuccin-macchiato" | "macchiato" => ColorSchemeChoice::CatppuccinMacchiato,
        "catppuccin" | "catppuccin-mocha" | "mocha" => ColorSchemeChoice::CatppuccinMocha,
        "gruvbox" | "gruvbox-dark" => ColorSchemeChoice::GruvboxDark,
        "gruvbox-light" => ColorSchemeChoice::GruvboxLight,
        "github" | "github-dark" => ColorSchemeChoice::GithubDark,
        "github-dark-high-contrast" | "github-high-contrast" => {
            ColorSchemeChoice::GithubDarkHighContrast
        }
        "github-light" => ColorSchemeChoice::GithubLight,
        "github-light-high-contrast" => ColorSchemeChoice::GithubLightHighContrast,
        "tokyonight" | "tokyo-night" | "tokyonight-night" => ColorSchemeChoice::Tokyonight,
        _ => ColorSchemeChoice::Custom,
    }
}

pub(crate) fn color_scheme_config(choice: ColorSchemeChoice) -> Option<SyntaxThemeConfig> {
    match choice {
        ColorSchemeChoice::Custom => None,
        choice => Some(SyntaxThemeConfig {
            source: SyntaxThemeSource::Builtin,
            name: Some(color_scheme_label(choice).to_owned()),
            path: None,
        }),
    }
}

pub(crate) fn layout_override_from_setting(setting: LayoutSetting) -> Option<DiffLayoutMode> {
    match setting {
        LayoutSetting::Dynamic => None,
        LayoutSetting::Split => Some(DiffLayoutMode::Split),
        LayoutSetting::Unified => Some(DiffLayoutMode::Unified),
    }
}

pub(crate) fn layout_setting_from_override(
    layout_override: Option<DiffLayoutMode>,
) -> LayoutSetting {
    match layout_override {
        Some(DiffLayoutMode::Split) => LayoutSetting::Split,
        Some(DiffLayoutMode::Unified) => LayoutSetting::Unified,
        None => LayoutSetting::Dynamic,
    }
}

pub(crate) fn layout_setting_label(layout: LayoutSetting) -> &'static str {
    match layout {
        LayoutSetting::Dynamic => "dynamic",
        LayoutSetting::Split => "split",
        LayoutSetting::Unified => "unified",
    }
}

pub(crate) fn next_layout_setting(setting: LayoutSetting, delta: isize) -> LayoutSetting {
    let settings = [
        LayoutSetting::Dynamic,
        LayoutSetting::Split,
        LayoutSetting::Unified,
    ];
    let current = settings
        .iter()
        .position(|candidate| *candidate == setting)
        .unwrap_or_default();
    let next = (current as isize + delta).rem_euclid(settings.len() as isize) as usize;
    settings[next]
}

pub(crate) fn diff_cache_entry(options: DiffOptions, changeset: Changeset) -> DiffCacheEntry {
    let search_index = Arc::new(DiffSearchIndex::new(&changeset));
    let max_line_width = search_index.max_line_width();
    let total_stats = changeset.stats();
    let context_expansions = HashMap::new();
    let unified_model = UiModel::new(&changeset, DiffLayoutMode::Unified, &context_expansions);
    let split_model = UiModel::new(&changeset, DiffLayoutMode::Split, &context_expansions);
    DiffCacheEntry {
        options,
        changeset,
        search_index,
        total_stats,
        max_line_width,
        unified_model,
        split_model,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MouseScrollDirection {
    Up,
    Down,
}

#[derive(Debug, Default)]
pub(crate) struct MouseScroll {
    pub(crate) last_tick: Option<Instant>,
    pub(crate) direction: Option<MouseScrollDirection>,
    pub(crate) intervals: Vec<Duration>,
    pub(crate) pending_lines: f64,
    pub(crate) pending_hunk_focus_ticks: isize,
}

impl MouseScroll {
    pub(crate) fn scroll_delta(&mut self, direction: MouseScrollDirection, now: Instant) -> isize {
        let multiplier = self.multiplier(direction, now);
        self.pending_lines += multiplier;
        let lines = self.pending_lines.trunc() as isize;
        self.pending_lines -= lines as f64;

        match direction {
            MouseScrollDirection::Down => lines,
            MouseScrollDirection::Up => -lines,
        }
    }

    pub(crate) fn reset(&mut self) {
        self.last_tick = None;
        self.direction = None;
        self.intervals.clear();
        self.pending_lines = 0.0;
        self.pending_hunk_focus_ticks = 0;
    }

    pub(crate) fn reset_hunk_focus_ticks(&mut self) {
        self.pending_hunk_focus_ticks = 0;
    }

    pub(crate) fn hunk_focus_delta(&mut self, direction: MouseScrollDirection) -> isize {
        match direction {
            MouseScrollDirection::Down => self.pending_hunk_focus_ticks += 1,
            MouseScrollDirection::Up => self.pending_hunk_focus_ticks -= 1,
        }

        if self.pending_hunk_focus_ticks >= MOUSE_HUNK_FOCUS_SCROLL_TICKS {
            self.pending_hunk_focus_ticks -= MOUSE_HUNK_FOCUS_SCROLL_TICKS;
            1
        } else if self.pending_hunk_focus_ticks <= -MOUSE_HUNK_FOCUS_SCROLL_TICKS {
            self.pending_hunk_focus_ticks += MOUSE_HUNK_FOCUS_SCROLL_TICKS;
            -1
        } else {
            0
        }
    }

    pub(crate) fn multiplier(&mut self, direction: MouseScrollDirection, now: Instant) -> f64 {
        let Some(last_tick) = self.last_tick else {
            self.start_streak(direction, now);
            return 1.0;
        };

        let elapsed = now.saturating_duration_since(last_tick);
        if self.direction != Some(direction) || elapsed > MOUSE_SCROLL_STREAK_TIMEOUT {
            self.start_streak(direction, now);
            return 1.0;
        }

        if elapsed < MOUSE_SCROLL_MIN_TICK_INTERVAL {
            return 1.0;
        }

        self.last_tick = Some(now);
        self.intervals.push(elapsed);
        if self.intervals.len() > MOUSE_SCROLL_HISTORY_SIZE {
            self.intervals.remove(0);
        }

        let average_interval_ms = self
            .intervals
            .iter()
            .map(|interval| interval.as_secs_f64() * 1000.0)
            .sum::<f64>()
            / self.intervals.len() as f64;
        let velocity = MOUSE_SCROLL_REFERENCE_INTERVAL_MS / average_interval_ms;
        let multiplier =
            1.0 + MOUSE_SCROLL_ACCEL_A * ((velocity / MOUSE_SCROLL_ACCEL_TAU).exp() - 1.0);

        multiplier.min(MOUSE_SCROLL_MAX_MULTIPLIER)
    }

    pub(crate) fn start_streak(&mut self, direction: MouseScrollDirection, now: Instant) {
        self.last_tick = Some(now);
        self.direction = Some(direction);
        self.intervals.clear();
        self.pending_lines = 0.0;
        self.pending_hunk_focus_ticks = 0;
    }
}

#[derive(Debug)]
pub(crate) struct PendingDiffLoad {
    pub(crate) options: DiffOptions,
    pub(crate) error_prefix: String,
    pub(crate) refresh_branch_metadata: bool,
    pub(crate) rx: oneshot::Receiver<MarkResult<Changeset>>,
}

#[derive(Debug)]
pub(crate) struct PendingReviewLoad {
    pub(crate) error_prefix: String,
    pub(crate) rx: oneshot::Receiver<MarkResult<(DiffOptions, Changeset)>>,
}

#[derive(Debug)]
pub(crate) struct DiffCacheEntry {
    pub(crate) options: DiffOptions,
    pub(crate) changeset: Changeset,
    pub(crate) search_index: Arc<DiffSearchIndex>,
    pub(crate) total_stats: DiffStats,
    pub(crate) max_line_width: usize,
    pub(crate) unified_model: UiModel,
    pub(crate) split_model: UiModel,
}

#[derive(Debug)]
pub(crate) struct PendingDiffPrefetch {
    pub(crate) options: DiffOptions,
    pub(crate) rx: oneshot::Receiver<MarkResult<Changeset>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SyntaxStartupMode {
    Config,
    Disabled,
    Languages(Vec<String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HunkFocusSearch {
    FirstVisible,
    NearestTo(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RenderedDiffRow {
    viewport_row: usize,
    model_row: usize,
}

#[derive(Debug)]
struct AnnotationScratchFile {
    _dir: TempDir,
    path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OptionsMenuItem {
    Layout,
    LiveReload,
    ContextExpansion,
    SyntaxHighlighting,
    LineWrapping,
    ColorScheme,
}

pub(crate) const COMMON_OPTIONS_MENU_ITEMS: &[OptionsMenuItem] = &[
    OptionsMenuItem::Layout,
    OptionsMenuItem::LiveReload,
    OptionsMenuItem::ContextExpansion,
    OptionsMenuItem::SyntaxHighlighting,
    OptionsMenuItem::LineWrapping,
    OptionsMenuItem::ColorScheme,
];

pub(crate) fn option_label(item: OptionsMenuItem) -> &'static str {
    match item {
        OptionsMenuItem::Layout => "Layout",
        OptionsMenuItem::LiveReload => "Live reload",
        OptionsMenuItem::ContextExpansion => "Context expand",
        OptionsMenuItem::SyntaxHighlighting => "Syntax highlighting",
        OptionsMenuItem::LineWrapping => "Line wrapping",
        OptionsMenuItem::ColorScheme => "Colorscheme",
    }
}

fn checkbox(enabled: bool) -> String {
    if enabled { "[x]" } else { "[ ]" }.to_owned()
}

fn on_off_search(enabled: bool) -> String {
    if enabled { "on" } else { "off" }.to_owned()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OptionsDraft {
    pub(crate) layout: LayoutSetting,
    pub(crate) live_updates_enabled: bool,
    pub(crate) context_expansion: DiffContextExpansion,
    pub(crate) syntax_enabled: bool,
    pub(crate) line_wrapping: bool,
    pub(crate) color_scheme: ColorSchemeChoice,
}

pub(crate) fn persist_options_menu_draft_to_path(
    path: &Path,
    draft: OptionsDraft,
    changed_item: OptionsMenuItem,
) -> MarkResult<()> {
    let mut table = if path.exists() {
        let contents = fs::read_to_string(path)?;
        if contents.trim().is_empty() {
            toml::Table::new()
        } else {
            contents.parse::<toml::Table>().map_err(|error| {
                MarkError::Usage(format!("failed to parse {}: {error}", path.display()))
            })?
        }
    } else {
        toml::Table::new()
    };

    match changed_item {
        OptionsMenuItem::Layout => {
            table.insert(
                "layout".to_owned(),
                toml::Value::String(layout_setting_label(draft.layout).to_owned()),
            );
        }
        OptionsMenuItem::LiveReload => {
            table.insert(
                "live_reload".to_owned(),
                toml::Value::Boolean(draft.live_updates_enabled),
            );
        }
        OptionsMenuItem::ContextExpansion => {
            let mut diff = match table.remove("diff") {
                Some(toml::Value::Table(diff)) => diff,
                Some(_) => {
                    return Err(MarkError::Usage(format!(
                        "failed to update {}: diff must be a table",
                        path.display()
                    )));
                }
                None => toml::Table::new(),
            };
            diff.remove("context_expansion");
            diff.remove("context_lines");
            diff.remove("expand_context");
            diff.insert(
                "context_expand".to_owned(),
                context_expansion_config_value(draft.context_expansion),
            );
            table.insert("diff".to_owned(), toml::Value::Table(diff));
        }
        OptionsMenuItem::SyntaxHighlighting => {
            table.insert(
                "syntax_highlighting".to_owned(),
                toml::Value::Boolean(draft.syntax_enabled),
            );
        }
        OptionsMenuItem::LineWrapping => {
            table.remove("word_wrap");
            table.remove("wrap_lines");
            table.insert(
                "line_wrapping".to_owned(),
                toml::Value::Boolean(draft.line_wrapping),
            );
        }
        OptionsMenuItem::ColorScheme => {
            if let Some(config) = color_scheme_config(draft.color_scheme)
                && config.source == SyntaxThemeSource::Builtin
                && let Some(name) = config.name
            {
                table.insert("colorscheme".to_owned(), toml::Value::String(name));
            }
        }
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let contents = toml::to_string_pretty(&table)
        .map_err(|error| MarkError::Usage(format!("failed to serialize settings: {error}")))?;
    fs::write(path, contents)?;
    Ok(())
}

fn context_expansion_config_value(expansion: DiffContextExpansion) -> toml::Value {
    match expansion {
        DiffContextExpansion::Lines(lines) => toml::Value::Integer(lines as i64),
        DiffContextExpansion::Full => toml::Value::String("full".to_owned()),
    }
}

#[derive(Debug)]
struct WrappedVisualLayout {
    layout: DiffLayoutMode,
    viewport_width: usize,
    model_rows: usize,
    model_rows_ptr: usize,
    row_starts: Vec<usize>,
    total_rows: usize,
}

impl WrappedVisualLayout {
    fn matches(&self, app: &DiffApp) -> bool {
        self.layout == app.layout
            && self.viewport_width == app.viewport_width
            && self.model_rows == app.model.len()
            && self.model_rows_ptr == app.model.rows.as_ptr() as usize
    }
}

#[derive(Debug)]
pub(crate) struct DiffApp {
    pub(crate) options: DiffOptions,
    pub(crate) base_changeset: Changeset,
    pub(crate) changeset: Changeset,
    pub(crate) search_index: Arc<DiffSearchIndex>,
    pub(crate) total_stats: DiffStats,
    pub(crate) stats: DiffStats,
    pub(crate) model: UiModel,
    pub(crate) layout: DiffLayoutMode,
    pub(crate) layout_override: Option<DiffLayoutMode>,
    pub(crate) scroll: usize,
    pub(crate) horizontal_scroll: usize,
    pub(crate) line_wrapping: bool,
    pub(crate) viewport_rows: usize,
    pub(crate) viewport_width: usize,
    pub(crate) max_line_width: usize,
    wrapped_visual_layout: RefCell<Option<WrappedVisualLayout>>,
    pub(crate) manual_hunk_focus: Option<(usize, usize)>,
    pub(crate) selected_file: usize,
    pub(crate) file_sidebar_open: bool,
    pub(crate) file_sidebar_scroll: usize,
    pub(crate) file_sidebar_width: Option<u16>,
    pub(crate) file_sidebar_render_width: u16,
    pub(crate) file_sidebar_resizing: bool,
    pub(crate) rendered_diff_menu_area: Option<Rect>,
    pub(crate) rendered_branch_menu_area: Option<Rect>,
    pub(crate) rendered_commit_menu_area: Option<Rect>,
    pub(crate) rendered_review_input_area: Option<Rect>,
    pub(crate) rendered_color_scheme_picker_area: Option<Rect>,
    pub(crate) rendered_diff_area: Option<Rect>,
    pub(crate) mouse_hover: Option<(u16, u16)>,
    pub(crate) annotations: AnnotationStore,
    pub(crate) annotation_draft: Option<AnnotationDraft>,
    pub(crate) leader_pending: bool,
    pub(crate) help_menu_open: bool,
    pub(crate) help_menu_input: String,
    pub(crate) help_menu_input_cursor: usize,
    pub(crate) help_menu_scroll: usize,
    pub(crate) help_menu_visible_rows: usize,
    terminal_area: Rect,
    pub(crate) diff_menu_open: bool,
    pub(crate) diff_menu_input: String,
    pub(crate) diff_menu_input_cursor: usize,
    pub(crate) diff_menu_selected: usize,
    pub(crate) review_input_open: bool,
    pub(crate) review_input: String,
    pub(crate) review_input_cursor: usize,
    pub(crate) options_menu_open: bool,
    pub(crate) options_menu_input: String,
    pub(crate) options_menu_input_cursor: usize,
    pub(crate) options_menu_selected: usize,
    pub(crate) options_menu_scroll: usize,
    pub(crate) options_menu_draft: OptionsDraft,
    pub(crate) color_scheme_picker_open: bool,
    pub(crate) color_scheme_input: String,
    pub(crate) color_scheme_input_cursor: usize,
    pub(crate) color_scheme_scroll: usize,
    pub(crate) color_scheme_selected: usize,
    pub(crate) color_scheme_preview_original: Option<(ColorSchemeChoice, DiffTheme)>,
    pub(crate) filter_input: Option<DiffFilterKind>,
    pub(crate) file_filter: String,
    pub(crate) file_filter_input: String,
    pub(crate) file_filter_input_cursor: usize,
    pub(crate) grep_filter: String,
    pub(crate) grep_filter_input: String,
    pub(crate) grep_filter_input_cursor: usize,
    pub(crate) grep_matches: Vec<usize>,
    pub(crate) grep_matches_truncated: bool,
    pub(crate) selected_grep_match: Option<usize>,
    pub(crate) branch_menu_open: Option<BranchMenu>,
    pub(crate) branch_menu_input: String,
    pub(crate) branch_menu_input_cursor: usize,
    pub(crate) branch_menu_scroll: usize,
    pub(crate) branch_menu_selected: usize,
    pub(crate) branch_base: Option<String>,
    pub(crate) branch_head: Option<String>,
    pub(crate) current_head: Option<String>,
    pub(crate) comparison_branches: Vec<String>,
    pub(crate) commit_menu_open: bool,
    pub(crate) commit_menu_input: String,
    pub(crate) commit_menu_input_cursor: usize,
    pub(crate) commit_menu_scroll: usize,
    pub(crate) commit_menu_selected: usize,
    pub(crate) show_rev: Option<String>,
    pub(crate) comparison_commits: Vec<GitCommit>,
    pub(crate) live_diff_failed_options: Option<DiffOptions>,
    pub(crate) editor_reload: Option<EditorReloadWorker>,
    pub(crate) pending_editor_reload: Option<EditorReloadRequest>,
    pub(crate) post_editor_quit_key_ignore_until: Option<Instant>,
    pub(crate) live_updates_allowed: bool,
    pub(crate) live_updates_enabled: bool,
    pub(crate) live_reload_invalidated: bool,
    pub(crate) live_reload_pending: bool,
    pub(crate) pending_diff_load: Option<PendingDiffLoad>,
    pub(crate) pending_review_load: Option<PendingReviewLoad>,
    pub(crate) diff_cache: Vec<DiffCacheEntry>,
    pub(crate) pending_diff_prefetch: Option<PendingDiffPrefetch>,
    pub(crate) diff_prefetch_queue: VecDeque<DiffOptions>,
    pub(crate) diff_prefetch_started: bool,
    pub(crate) filter_generation: u64,
    pub(crate) pending_filter_apply: Option<PendingFilterApply>,
    pub(crate) filter_worker: Option<FilterWorker>,
    pub(crate) filter_searching: bool,
    pub(crate) error_log: Option<String>,
    pub(crate) error_log_height: u16,
    pub(crate) error_log_resizing: bool,
    pub(crate) rendered_error_log_separator_row: Option<u16>,
    pub(crate) notice: Option<Notice>,
    pub(crate) mouse_scroll: MouseScroll,
    pub(crate) keymap: Keymap,
    pub(crate) theme: DiffTheme,
    pub(crate) color_scheme: ColorSchemeChoice,
    pub(crate) theme_color_overrides: ColorOverrides,
    pub(crate) theme_transparent_background: bool,
    pub(crate) settings_persistence_enabled: bool,
    #[cfg(test)]
    pub(crate) last_persisted_options_menu_draft: Option<(OptionsDraft, OptionsMenuItem)>,
    pub(crate) context_expansions: HashMap<ContextKey, usize>,
    pub(crate) context_cache: HashMap<ContextSourceKey, ContextSourceEntry>,
    pub(crate) syntax_settings: SyntaxSettings,
    pub(crate) syntax_startup_mode: SyntaxStartupMode,
    pub(crate) syntax_limits: SyntaxLimits,
    pub(crate) syntax: Option<SyntaxRuntime>,
    pub(crate) inline_cache: LruCache<InlineHunkKey, InlineHunkEmphasisCache>,
    pub(crate) generation: u64,
    pub(crate) terminal_clear_requested: bool,
    pub(crate) dirty: bool,
}

pub(crate) fn load_syntax_settings_for_diff(
    load_user_settings: bool,
) -> (SyntaxSettings, Option<String>) {
    if !load_user_settings {
        return (SyntaxSettings::default(), None);
    }

    syntax_settings_for_diff(mark_syntax::load_settings())
}

pub(crate) fn syntax_settings_for_diff(
    result: MarkResult<SyntaxSettings>,
) -> (SyntaxSettings, Option<String>) {
    match result {
        Ok(settings) => (settings, None),
        Err(error) => (
            SyntaxSettings::default(),
            Some(format!("syntax settings ignored: {error}")),
        ),
    }
}

fn push_startup_error_log(error_log: &mut Option<String>, message: impl Into<String>) {
    match error_log {
        Some(error_log) => {
            error_log.push('\n');
            error_log.push_str(&message.into());
        }
        None => *error_log = Some(message.into()),
    }
}

pub(crate) fn syntax_runtime_for_diff(
    result: MarkResult<Option<SyntaxRuntime>>,
    error_log: &mut Option<String>,
) -> Option<SyntaxRuntime> {
    match result {
        Ok(syntax) => syntax,
        Err(error) => {
            push_startup_error_log(error_log, format!("syntax disabled: {error}"));
            None
        }
    }
}

pub(crate) fn load_keymap_for_diff(load_user_settings: bool) -> (Keymap, Option<String>) {
    if !load_user_settings {
        return (Keymap::default(), None);
    }

    match Keymap::load() {
        Ok(keymap) => (keymap, None),
        Err(error) => (Keymap::default(), Some(format!("keymap ignored: {error}"))),
    }
}

pub(crate) fn layout_override_from_settings(
    settings: &SyntaxSettings,
    honor_settings_layout: bool,
) -> Option<DiffLayoutMode> {
    honor_settings_layout
        .then_some(settings.layout)
        .flatten()
        .and_then(layout_override_from_setting)
}

impl DiffApp {
    #[cfg(test)]
    pub(crate) fn new(options: DiffOptions, changeset: Changeset, layout: DiffLayoutMode) -> Self {
        Self::new_with_syntax(options, changeset, layout, SyntaxStartupMode::Config)
    }

    pub(crate) fn new_with_syntax(
        options: DiffOptions,
        changeset: Changeset,
        layout: DiffLayoutMode,
        syntax_mode: SyntaxStartupMode,
    ) -> Self {
        Self::new_with_syntax_and_layout_settings(options, changeset, layout, syntax_mode, true)
    }

    pub(crate) fn new_with_explicit_layout(
        options: DiffOptions,
        changeset: Changeset,
        layout: DiffLayoutMode,
        syntax_mode: SyntaxStartupMode,
    ) -> Self {
        let mut app = Self::new_with_syntax_and_layout_settings(
            options,
            changeset,
            layout,
            syntax_mode,
            false,
        );
        app.layout_override = Some(layout);
        app.options_menu_draft.layout = layout_setting_from_override(app.layout_override);
        app
    }

    fn new_with_syntax_and_layout_settings(
        options: DiffOptions,
        changeset: Changeset,
        mut layout: DiffLayoutMode,
        syntax_mode: SyntaxStartupMode,
        honor_settings_layout: bool,
    ) -> Self {
        let context_expansions = HashMap::new();
        let context_cache = HashMap::new();
        let load_user_settings = matches!(
            syntax_mode,
            SyntaxStartupMode::Config | SyntaxStartupMode::Disabled
        ) && !cfg!(test);
        let (settings, mut startup_error_log) = load_syntax_settings_for_diff(load_user_settings);
        let layout_override = layout_override_from_settings(&settings, honor_settings_layout);
        if let Some(setting_layout) = layout_override {
            layout = setting_layout;
        }
        let model = UiModel::new(&changeset, layout, &context_expansions);
        let search_index = Arc::new(DiffSearchIndex::new(&changeset));
        let manual_hunk_focus = model
            .hunk_start_rows
            .first()
            .and_then(|row| model.row(*row).and_then(UiRow::hunk_key));
        let stats = changeset.stats();
        let total_stats = stats.clone();
        let branch_base = default_branch_base(&options, &changeset.repo);
        let current_head = current_head_label(&changeset.repo);
        let branch_head = branch_head_from_options(&options, current_head.as_deref());
        let comparison_branches = comparison_branches(
            &changeset.repo,
            &[
                current_head.as_deref(),
                branch_head.as_deref(),
                branch_base.as_deref(),
            ],
        );
        let show_rev = show_rev_from_options(&options);
        let comparison_commits = comparison_commits(&changeset.repo, show_rev.as_deref());
        let (keymap, keymap_notice) = load_keymap_for_diff(load_user_settings);
        if let Some(message) = keymap_notice {
            push_startup_error_log(&mut startup_error_log, message);
        }
        let mut color_scheme = color_scheme_from_config(&settings.theme);
        let theme = match diff_theme_from_config(&settings.theme).and_then(|theme| {
            theme
                .with_color_overrides(&settings.colors)
                .map(|theme| theme.with_transparent_background(settings.transparent_background))
        }) {
            Ok(theme) => theme.with_diff_settings(settings.diff),
            Err(error) => {
                push_startup_error_log(
                    &mut startup_error_log,
                    format!("syntax theme ignored: {error}"),
                );
                color_scheme = ColorSchemeChoice::System;
                DiffTheme::default()
                    .with_color_overrides(&settings.colors)
                    .unwrap_or_else(|_| DiffTheme::default())
                    .with_transparent_background(settings.transparent_background)
                    .with_diff_settings(settings.diff)
            }
        };
        let syntax_limits = settings.limits;
        let context_expansion = theme.diff.context_expansion;
        let theme_color_overrides = settings.colors.clone();
        let theme_transparent_background = settings.transparent_background;
        let syntax = match &syntax_mode {
            SyntaxStartupMode::Config if settings.syntax_highlighting => {
                syntax_runtime_for_diff(SyntaxRuntime::start(&settings), &mut startup_error_log)
            }
            SyntaxStartupMode::Config => None,
            SyntaxStartupMode::Disabled => None,
            SyntaxStartupMode::Languages(languages) => {
                SyntaxRuntime::start_with_languages(languages.clone(), syntax_limits)
            }
        };
        let max_line_width = search_index.max_line_width();
        Self {
            options,
            base_changeset: changeset.clone(),
            changeset,
            search_index,
            total_stats,
            stats,
            model,
            layout,
            layout_override,
            scroll: 0,
            horizontal_scroll: 0,
            line_wrapping: settings.line_wrapping,
            viewport_rows: 1,
            viewport_width: 1,
            max_line_width,
            wrapped_visual_layout: RefCell::new(None),
            manual_hunk_focus,
            selected_file: 0,
            file_sidebar_open: false,
            file_sidebar_scroll: 0,
            file_sidebar_width: None,
            file_sidebar_render_width: 0,
            file_sidebar_resizing: false,
            rendered_diff_menu_area: None,
            rendered_branch_menu_area: None,
            rendered_commit_menu_area: None,
            rendered_review_input_area: None,
            rendered_color_scheme_picker_area: None,
            rendered_diff_area: None,
            mouse_hover: None,
            annotations: AnnotationStore::default(),
            annotation_draft: None,
            leader_pending: false,
            help_menu_open: false,
            help_menu_input: String::new(),
            help_menu_input_cursor: 0,
            help_menu_scroll: 0,
            help_menu_visible_rows: 1,
            terminal_area: Rect::default(),
            diff_menu_open: false,
            diff_menu_input: String::new(),
            diff_menu_input_cursor: 0,
            diff_menu_selected: 0,
            review_input_open: false,
            review_input: String::new(),
            review_input_cursor: 0,
            options_menu_open: false,
            options_menu_input: String::new(),
            options_menu_input_cursor: 0,
            options_menu_selected: 0,
            options_menu_scroll: 0,
            options_menu_draft: OptionsDraft {
                layout: layout_setting_from_override(layout_override),
                live_updates_enabled: settings.live_reload,
                context_expansion,
                syntax_enabled: syntax.is_some(),
                line_wrapping: settings.line_wrapping,
                color_scheme,
            },
            color_scheme_picker_open: false,
            color_scheme_input: String::new(),
            color_scheme_input_cursor: 0,
            color_scheme_scroll: 0,
            color_scheme_selected: 0,
            color_scheme_preview_original: None,
            filter_input: None,
            file_filter: String::new(),
            file_filter_input: String::new(),
            file_filter_input_cursor: 0,
            grep_filter: String::new(),
            grep_filter_input: String::new(),
            grep_filter_input_cursor: 0,
            grep_matches: Vec::new(),
            grep_matches_truncated: false,
            selected_grep_match: None,
            branch_menu_open: None,
            branch_menu_input: String::new(),
            branch_menu_input_cursor: 0,
            branch_menu_scroll: 0,
            branch_menu_selected: 0,
            branch_base,
            branch_head,
            current_head,
            comparison_branches,
            commit_menu_open: false,
            commit_menu_input: String::new(),
            commit_menu_input_cursor: 0,
            commit_menu_scroll: 0,
            commit_menu_selected: 0,
            show_rev,
            comparison_commits,
            live_diff_failed_options: None,
            editor_reload: None,
            pending_editor_reload: None,
            post_editor_quit_key_ignore_until: None,
            live_updates_allowed: true,
            live_updates_enabled: settings.live_reload,
            live_reload_invalidated: false,
            live_reload_pending: false,
            pending_diff_load: None,
            pending_review_load: None,
            diff_cache: Vec::new(),
            pending_diff_prefetch: None,
            diff_prefetch_queue: VecDeque::new(),
            diff_prefetch_started: false,
            filter_generation: 0,
            pending_filter_apply: None,
            filter_worker: None,
            filter_searching: false,
            error_log: startup_error_log,
            error_log_height: ERROR_LOG_DEFAULT_HEIGHT,
            error_log_resizing: false,
            rendered_error_log_separator_row: None,
            notice: None,
            mouse_scroll: MouseScroll::default(),
            keymap,
            theme,
            color_scheme,
            theme_color_overrides,
            theme_transparent_background,
            settings_persistence_enabled: !cfg!(test),
            #[cfg(test)]
            last_persisted_options_menu_draft: None,
            context_expansions,
            context_cache,
            syntax_settings: settings,
            syntax_startup_mode: syntax_mode,
            syntax_limits,
            syntax,
            inline_cache: LruCache::new(MAX_INLINE_DIFF_CACHE_ENTRIES),
            generation: 0,
            terminal_clear_requested: false,
            dirty: true,
        }
    }

    pub(crate) fn handle_key(&mut self, key: KeyEvent) -> MarkResult<bool> {
        if self.handle_annotation_save_or_cancel_key(key) {
            return Ok(false);
        }
        if is_quit_key(key) {
            return Ok(true);
        }

        self.mouse_scroll.reset();

        if self.filter_input.is_some() && self.handle_filter_input_key(key) {
            return Ok(false);
        }

        if self.annotation_draft.is_some() && self.handle_annotation_input_key(key) {
            return Ok(false);
        }

        if self.help_menu_open {
            return self.handle_help_menu_key(key);
        }

        if self.branch_menu_open.is_some() {
            return self.handle_branch_menu_key(key);
        }

        if self.commit_menu_open {
            return self.handle_commit_menu_key(key);
        }

        if self.review_input_open {
            return self.handle_review_input_key(key);
        }

        if self.diff_menu_open {
            return self.handle_diff_menu_key(key);
        }

        if self.color_scheme_picker_open {
            return self.handle_color_scheme_picker_key(key);
        }

        if self.options_menu_open && !self.color_scheme_picker_open {
            return self.handle_options_menu_key(key);
        }

        if key.code == KeyCode::Esc && self.close_error_log() {
            return Ok(false);
        }

        if self.leader_pending {
            return self.handle_leader_key(key);
        }

        if self.keymap.matches_single(GlobalAction::Quit, key) {
            return Ok(true);
        }
        if self.keymap.matches_single(GlobalAction::Help, key) {
            self.toggle_help_menu();
            return Ok(false);
        }
        if self.keymap.matches_single(GlobalAction::Reload, key) {
            self.reload()?;
            return Ok(false);
        }
        if self.keymap.matches_single(GlobalAction::FileFilter, key) {
            self.open_filter_input(DiffFilterKind::File);
            return Ok(false);
        }
        if self.keymap.matches_single(GlobalAction::Grep, key) {
            self.open_filter_input(DiffFilterKind::Grep);
            return Ok(false);
        }
        if self.keymap.matches_single(GlobalAction::DiffMenu, key) {
            self.open_diff_menu();
            return Ok(false);
        }
        if self.keymap.matches_single(GlobalAction::OptionsMenu, key) {
            self.open_options_menu();
            return Ok(false);
        }
        if self.keymap.matches_single(GlobalAction::FileBrowser, key) {
            self.toggle_file_sidebar();
            return Ok(false);
        }
        if self.keymap.matches_single(GlobalAction::Layout, key) {
            self.toggle_layout();
            return Ok(false);
        }
        if self.keymap.matches_single(GlobalAction::EditHunk, key) {
            self.open_focused_hunk_in_editor();
            return Ok(false);
        }
        if self.keymap.matches_single(GlobalAction::CopyMarks, key) {
            self.copy_marks_to_terminal_clipboard();
            return Ok(false);
        }
        if self.error_log.is_some() && self.keymap.matches_single(GlobalAction::CopyErrorLog, key) {
            self.copy_error_log_to_terminal_clipboard();
            return Ok(false);
        }
        if self.keymap.matches_single(GlobalAction::NextDiffType, key) {
            self.cycle_diff_choice(1);
            return Ok(false);
        }
        if self
            .keymap
            .matches_single(GlobalAction::PreviousDiffType, key)
        {
            self.cycle_diff_choice(-1);
            return Ok(false);
        }

        if self.keymap.is_leader(key) {
            self.leader_pending = true;
            self.dirty = true;
            return Ok(false);
        }

        if self.error_log.is_some() {
            match key.code {
                KeyCode::Char('+') | KeyCode::Char('=') => {
                    self.resize_error_log(1);
                    return Ok(false);
                }
                KeyCode::Char('-') => {
                    self.resize_error_log(-1);
                    return Ok(false);
                }
                _ => {}
            }
        }

        match key.code {
            KeyCode::Esc if self.filters_active() => self.clear_all_filters(),
            KeyCode::Down | KeyCode::Char('j') => self.scroll_or_focus_hunk(1),
            KeyCode::Up | KeyCode::Char('k') => self.scroll_or_focus_hunk(-1),
            KeyCode::Left | KeyCode::Char('h') => {
                self.scroll_horizontally_by(-(HORIZONTAL_SCROLL_STEP as isize));
            }
            KeyCode::Right | KeyCode::Char('l') => {
                self.scroll_horizontally_by(HORIZONTAL_SCROLL_STEP as isize);
            }
            KeyCode::PageDown | KeyCode::Char('d') => self.scroll_or_focus_hunk(20),
            KeyCode::PageUp | KeyCode::Char('u') => self.scroll_or_focus_hunk(-20),
            KeyCode::Home => self.set_scroll(0),
            KeyCode::Char('g') if is_plain_char_key(key, 'g') => self.set_scroll(0),
            KeyCode::End | KeyCode::Char('G') => self.set_scroll(self.max_scroll()),
            KeyCode::Char('n') if !self.grep_filter.is_empty() => self.move_grep_match(1),
            KeyCode::Char('p') | KeyCode::Char('N') if !self.grep_filter.is_empty() => {
                self.move_grep_match(-1);
            }
            KeyCode::Char('n') | KeyCode::Char('p') | KeyCode::Char('N') => {}
            KeyCode::Char('J') => self.move_file(1),
            KeyCode::Char('K') => self.move_file(-1),
            KeyCode::Char(']') => self.next_hunk(),
            KeyCode::Char('[') => self.previous_hunk(),
            _ => {}
        }

        Ok(false)
    }

    pub(crate) fn handle_leader_key(&mut self, key: KeyEvent) -> MarkResult<bool> {
        self.leader_pending = false;

        if key.code == KeyCode::Esc {
            self.dirty = true;
            return Ok(false);
        }

        if self.keymap.matches_leader(GlobalAction::Quit, key) {
            return Ok(true);
        }
        if self.keymap.matches_leader(GlobalAction::Help, key) {
            self.toggle_help_menu();
            return Ok(false);
        }
        if self.keymap.matches_leader(GlobalAction::Reload, key) {
            self.reload()?;
            return Ok(false);
        }
        if self.keymap.matches_leader(GlobalAction::FileFilter, key) {
            self.open_filter_input(DiffFilterKind::File);
            return Ok(false);
        }
        if self.keymap.matches_leader(GlobalAction::Grep, key) {
            self.open_filter_input(DiffFilterKind::Grep);
            return Ok(false);
        }
        if self.keymap.matches_leader(GlobalAction::DiffMenu, key) {
            self.open_diff_menu();
            return Ok(false);
        }
        if self.keymap.matches_leader(GlobalAction::OptionsMenu, key) {
            self.open_options_menu();
            return Ok(false);
        }
        if self.keymap.matches_leader(GlobalAction::Layout, key) {
            self.toggle_layout();
            return Ok(false);
        }
        if self.keymap.matches_leader(GlobalAction::FileBrowser, key) {
            self.toggle_file_sidebar();
            return Ok(false);
        }
        if self.keymap.matches_leader(GlobalAction::CopyMarks, key) {
            self.copy_marks_to_terminal_clipboard();
            return Ok(false);
        }
        if self.error_log.is_some() && self.keymap.matches_leader(GlobalAction::CopyErrorLog, key) {
            self.copy_error_log_to_terminal_clipboard();
            return Ok(false);
        }
        if self.keymap.matches_leader(GlobalAction::NextDiffType, key) {
            self.cycle_diff_choice(1);
            return Ok(false);
        }
        if self
            .keymap
            .matches_leader(GlobalAction::PreviousDiffType, key)
        {
            self.cycle_diff_choice(-1);
            return Ok(false);
        }

        self.dirty = true;
        Ok(false)
    }

    pub(crate) fn handle_diff_menu_key(&mut self, key: KeyEvent) -> MarkResult<bool> {
        if self.keymap.matches_menu(MenuAction::Close, key) {
            self.close_diff_menu();
            return Ok(false);
        }

        if self.keymap.matches_menu(MenuAction::Down, key) {
            self.move_diff_menu_selection(1);
        } else if self.keymap.matches_menu(MenuAction::Up, key) {
            self.move_diff_menu_selection(-1);
        } else if self.keymap.matches_menu(MenuAction::Select, key)
            || self.keymap.matches_menu(MenuAction::Confirm, key)
        {
            self.select_highlighted_diff_choice();
        } else if !self.apply_diff_menu_input_key(key) {
            match key.code {
                KeyCode::Home => self.set_diff_menu_selection(0),
                KeyCode::End => self.set_diff_menu_selection(usize::MAX),
                _ => {}
            }
        }

        Ok(false)
    }

    pub(crate) fn handle_review_input_key(&mut self, key: KeyEvent) -> MarkResult<bool> {
        if self.keymap.matches_menu(MenuAction::Close, key) {
            self.close_review_input();
            return Ok(false);
        }

        if self.keymap.matches_menu(MenuAction::Select, key)
            || self.keymap.matches_menu(MenuAction::Confirm, key)
        {
            self.submit_review_input();
        } else {
            match handle_text_input_key(&mut self.review_input, &mut self.review_input_cursor, key)
            {
                TextInputKeyResult::Edited | TextInputKeyResult::Moved => self.dirty = true,
                TextInputKeyResult::Handled | TextInputKeyResult::Ignored => {}
            }
        }

        Ok(false)
    }

    pub(crate) fn handle_branch_menu_key(&mut self, key: KeyEvent) -> MarkResult<bool> {
        if self.keymap.matches_menu(MenuAction::Close, key) {
            self.close_branch_menu();
            return Ok(false);
        }

        if self.keymap.matches_menu(MenuAction::Down, key) {
            self.cycle_branch_completion(1);
        } else if self.keymap.matches_menu(MenuAction::Up, key) {
            self.cycle_branch_completion(-1);
        } else if self.keymap.matches_menu(MenuAction::Select, key)
            || self.keymap.matches_menu(MenuAction::Confirm, key)
        {
            self.select_highlighted_branch_match();
        } else if !self.apply_branch_input_key(key) {
            match key.code {
                KeyCode::PageDown => self.move_branch_selection(self.branch_menu_rows() as isize),
                KeyCode::PageUp => self.move_branch_selection(-(self.branch_menu_rows() as isize)),
                KeyCode::Home => self.set_branch_selection(0),
                KeyCode::End => self.set_branch_selection(usize::MAX),
                _ => {}
            }
        }

        Ok(false)
    }

    pub(crate) fn handle_commit_menu_key(&mut self, key: KeyEvent) -> MarkResult<bool> {
        if self.keymap.matches_menu(MenuAction::Close, key) {
            self.close_commit_menu();
            return Ok(false);
        }

        if self.keymap.matches_menu(MenuAction::Down, key) {
            self.cycle_commit_completion(1);
        } else if self.keymap.matches_menu(MenuAction::Up, key) {
            self.cycle_commit_completion(-1);
        } else if self.keymap.matches_menu(MenuAction::Select, key)
            || self.keymap.matches_menu(MenuAction::Confirm, key)
        {
            self.select_highlighted_commit_match();
        } else if !self.apply_commit_input_key(key) {
            match key.code {
                KeyCode::PageDown => self.move_commit_selection(self.commit_menu_rows() as isize),
                KeyCode::PageUp => self.move_commit_selection(-(self.commit_menu_rows() as isize)),
                KeyCode::Home => self.set_commit_selection(0),
                KeyCode::End => self.set_commit_selection(usize::MAX),
                _ => {}
            }
        }

        Ok(false)
    }

    fn help_menu_line_scroll_delta(&self, key: KeyEvent) -> Option<isize> {
        if self.keymap.matches_help_menu_scroll(MenuAction::Down, key) {
            Some(1)
        } else if self.keymap.matches_help_menu_scroll(MenuAction::Up, key) {
            Some(-1)
        } else {
            None
        }
    }

    pub(crate) fn handle_help_menu_key(&mut self, key: KeyEvent) -> MarkResult<bool> {
        if self.keymap.matches_menu(MenuAction::Close, key) {
            self.close_help_menu();
            return Ok(false);
        }

        if let Some(delta) = self.help_menu_line_scroll_delta(key) {
            self.scroll_help_menu(delta);
        } else if !self.apply_help_menu_input_key(key) {
            match key.code {
                KeyCode::PageDown => {
                    let page = self.help_menu_page_scroll_rows();
                    if page > 0 {
                        self.scroll_help_menu(page as isize);
                    }
                }
                KeyCode::PageUp => {
                    let page = self.help_menu_page_scroll_rows();
                    if page > 0 {
                        self.scroll_help_menu(-(page as isize));
                    }
                }
                KeyCode::Home => self.set_help_menu_scroll(0),
                KeyCode::End => self.set_help_menu_scroll(usize::MAX),
                _ => {}
            }
        }

        Ok(false)
    }

    pub(crate) fn handle_options_menu_key(&mut self, key: KeyEvent) -> MarkResult<bool> {
        if self.keymap.matches_menu(MenuAction::Close, key) {
            self.close_options_menu();
            return Ok(false);
        }

        if self.keymap.matches_menu(MenuAction::Down, key) {
            self.move_options_menu_selection(1);
        } else if self.keymap.matches_menu(MenuAction::Up, key) {
            self.move_options_menu_selection(-1);
        } else if self.keymap.matches_menu(MenuAction::Select, key)
            || self.keymap.matches_menu(MenuAction::Confirm, key)
        {
            self.activate_selected_option();
        } else if !self.apply_options_menu_input_key(key) {
            match key.code {
                KeyCode::Left => self.cycle_selected_option(-1),
                KeyCode::Right => self.cycle_selected_option(1),
                KeyCode::Home => self.set_options_menu_selection(0),
                KeyCode::End => self.set_options_menu_selection(usize::MAX),
                _ => {}
            }
        }

        Ok(false)
    }

    pub(crate) fn handle_color_scheme_picker_key(&mut self, key: KeyEvent) -> MarkResult<bool> {
        if self.keymap.matches_menu(MenuAction::Close, key) {
            self.close_color_scheme_picker();
            return Ok(false);
        }

        if self.keymap.matches_menu(MenuAction::Down, key) {
            self.move_color_scheme_selection(1);
        } else if self.keymap.matches_menu(MenuAction::Up, key) {
            self.move_color_scheme_selection(-1);
        } else if self.keymap.matches_menu(MenuAction::Select, key)
            || self.keymap.matches_menu(MenuAction::Confirm, key)
        {
            self.select_highlighted_color_scheme();
        } else if !self.apply_color_scheme_input_key(key) {
            match key.code {
                KeyCode::Home => self.set_color_scheme_selection(0),
                KeyCode::End => self.set_color_scheme_selection(usize::MAX),
                _ => {}
            }
        }

        Ok(false)
    }

    pub(crate) fn editor_shortcut_available(&self) -> bool {
        self.filter_input.is_none()
            && !self.help_menu_open
            && self.branch_menu_open.is_none()
            && !self.diff_menu_open
            && !self.review_input_open
            && !self.options_menu_open
            && !self.leader_pending
            && !self.color_scheme_picker_open
            && !self.commit_menu_open
    }

    pub(crate) fn event_poll(&self) -> Duration {
        let now = Instant::now();
        let mut poll = EVENT_POLL;
        if self.editor_reload.is_some() || self.pending_editor_reload.is_some() {
            poll = poll.min(EDITOR_RELOAD_POLL);
        }
        if self.filter_worker.is_some() {
            poll = poll.min(FILTER_WORKER_POLL);
        }
        if let Some(pending) = self.pending_filter_apply {
            poll = poll.min(pending.due_at.saturating_duration_since(now));
        }
        if self.pending_diff_prefetch.is_some() {
            poll = poll.min(DIFF_PREFETCH_POLL);
        }
        poll
    }

    pub(crate) fn ignore_post_editor_quit_key(&mut self, key: KeyEvent, now: Instant) -> bool {
        let Some(ignore_until) = self.post_editor_quit_key_ignore_until else {
            return false;
        };
        if now >= ignore_until {
            self.post_editor_quit_key_ignore_until = None;
            return false;
        }

        is_quit_key(key) || self.keymap.matches_single(GlobalAction::Quit, key)
    }

    pub(crate) fn set_terminal_area(&mut self, area: Rect) {
        if self.terminal_area != area {
            self.terminal_area = area;
            self.sync_help_menu_visible_rows();
        }
    }

    fn sync_help_menu_visible_rows(&mut self) {
        if !self.help_menu_open {
            return;
        }
        let Some(visible) = help_menu_list_visible_rows(self, self.terminal_area) else {
            return;
        };
        if self.help_menu_visible_rows != visible {
            self.help_menu_visible_rows = visible;
            self.clamp_help_menu_scroll(visible);
        }
    }

    fn help_menu_page_scroll_rows(&self) -> usize {
        help_menu_list_visible_rows(self, self.terminal_area)
            .unwrap_or(self.help_menu_visible_rows)
            .max(1)
    }

    pub(crate) fn toggle_help_menu(&mut self) {
        self.help_menu_open = !self.help_menu_open;
        self.help_menu_input.clear();
        self.help_menu_input_cursor = 0;
        self.help_menu_scroll = 0;
        self.leader_pending = false;
        if self.help_menu_open {
            self.sync_help_menu_visible_rows();
        }
        self.dirty = true;
    }

    pub(crate) fn close_help_menu(&mut self) {
        if self.help_menu_open || !self.help_menu_input.is_empty() || self.help_menu_scroll != 0 {
            self.help_menu_open = false;
            self.help_menu_input.clear();
            self.help_menu_input_cursor = 0;
            self.help_menu_scroll = 0;
            self.leader_pending = false;
            self.dirty = true;
        }
    }

    pub(crate) fn filtered_help_menu_rows(&self) -> Vec<HelpMenuRow> {
        let query = self.help_menu_input.trim().to_ascii_lowercase();
        if query.is_empty() {
            return HELP_MENU_ROWS.to_vec();
        }

        let mut rows = Vec::new();
        let mut index = 0;
        while index < HELP_MENU_ROWS.len() {
            let HelpMenuRow::Section(section) = HELP_MENU_ROWS[index] else {
                index += 1;
                continue;
            };
            index += 1;

            let mut section_rows = Vec::new();
            while index < HELP_MENU_ROWS.len()
                && !matches!(HELP_MENU_ROWS[index], HelpMenuRow::Section(_))
            {
                section_rows.push(HELP_MENU_ROWS[index]);
                index += 1;
            }

            let section_matches = branch_match_score(&query, section).is_some();
            let matching_rows: Vec<_> = section_rows
                .iter()
                .copied()
                .filter(|row| section_matches || self.help_menu_row_matches(&query, *row))
                .collect();

            if section_matches || !matching_rows.is_empty() {
                rows.push(HelpMenuRow::Section(section));
                rows.extend(matching_rows);
            }
        }

        rows
    }

    fn help_menu_row_matches(&self, query: &str, row: HelpMenuRow) -> bool {
        let HelpMenuRow::Binding(key, description) = row else {
            return false;
        };
        let key_label = self.help_menu_key_label(key).to_ascii_lowercase();
        let description = description.to_ascii_lowercase();
        let combined = format!("{key_label} {description}");
        branch_match_score(query, &key_label)
            .or_else(|| branch_match_score(query, &description))
            .or_else(|| branch_match_score(query, &combined))
            .is_some()
    }

    fn help_menu_key_label(&self, key: HelpMenuKey) -> String {
        match key {
            HelpMenuKey::Static(label) => label.to_owned(),
            HelpMenuKey::Leader => self.keymap.leader_label(),
            HelpMenuKey::Global(action) => self.keymap.global_action_label(action),
            HelpMenuKey::GlobalPair(first, second) => format!(
                "{}/{}",
                self.keymap.global_action_label(first),
                self.keymap.global_action_label(second)
            ),
        }
    }

    pub(crate) fn scroll_help_menu(&mut self, delta: isize) {
        let len = self.filtered_help_menu_rows().len();
        if len == 0 || delta == 0 {
            return;
        }
        let visible = self.help_menu_visible_rows.max(1);
        let max_scroll = self.help_menu_max_scroll(visible);
        let next = (self.help_menu_scroll as isize + delta).clamp(0, max_scroll as isize) as usize;
        if self.help_menu_scroll != next {
            self.help_menu_scroll = next;
            self.dirty = true;
        }
    }

    pub(crate) fn set_help_menu_scroll(&mut self, scroll: usize) {
        let next = scroll.min(self.help_menu_max_scroll(self.help_menu_visible_rows.max(1)));
        if self.help_menu_scroll != next {
            self.help_menu_scroll = next;
            self.dirty = true;
        }
    }

    fn help_menu_max_scroll(&self, visible_rows: usize) -> usize {
        self.filtered_help_menu_rows()
            .len()
            .saturating_sub(visible_rows.max(1))
    }

    pub(crate) fn clamp_help_menu_scroll(&mut self, visible_rows: usize) {
        let next = self
            .help_menu_scroll
            .min(self.help_menu_max_scroll(visible_rows));
        if self.help_menu_scroll != next {
            self.help_menu_scroll = next;
            self.dirty = true;
        }
    }

    #[allow(dead_code)]
    pub(crate) fn push_help_menu_input(&mut self, character: char) {
        self.help_menu_input
            .insert(self.help_menu_input_cursor, character);
        self.help_menu_input_cursor += character.len_utf8();
        self.help_menu_scroll = 0;
        self.sync_help_menu_visible_rows();
        self.dirty = true;
    }

    #[allow(dead_code)]
    pub(crate) fn pop_help_menu_input(&mut self) {
        let result = handle_text_input_key(
            &mut self.help_menu_input,
            &mut self.help_menu_input_cursor,
            KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
        );
        if matches!(result, TextInputKeyResult::Edited) {
            self.help_menu_scroll = 0;
            self.sync_help_menu_visible_rows();
            self.dirty = true;
        }
    }

    #[allow(dead_code)]
    pub(crate) fn clear_help_menu_input(&mut self) {
        if !self.help_menu_input.is_empty() || self.help_menu_scroll != 0 {
            self.help_menu_input.clear();
            self.help_menu_input_cursor = 0;
            self.help_menu_scroll = 0;
            self.sync_help_menu_visible_rows();
            self.dirty = true;
        }
    }

    fn apply_help_menu_input_key(&mut self, key: KeyEvent) -> bool {
        match handle_text_input_key(
            &mut self.help_menu_input,
            &mut self.help_menu_input_cursor,
            key,
        ) {
            TextInputKeyResult::Edited => {
                self.help_menu_scroll = 0;
                self.sync_help_menu_visible_rows();
                self.dirty = true;
                true
            }
            TextInputKeyResult::Moved => {
                self.dirty = true;
                true
            }
            TextInputKeyResult::Handled => true,
            TextInputKeyResult::Ignored => false,
        }
    }

    pub(crate) fn set_notice(&mut self, text: impl Into<String>) {
        self.notice = Some(Notice {
            text: text.into(),
            expires_at: Instant::now() + NOTICE_TTL,
        });
        self.dirty = true;
    }

    pub(crate) fn expire_notice(&mut self, now: Instant) {
        let expired = self
            .notice
            .as_ref()
            .is_some_and(|notice| now >= notice.expires_at);
        if expired {
            self.notice = None;
            self.dirty = true;
        }
    }

    pub(crate) fn mark_live_reload_invalidated(&mut self) {
        self.invalidate_diff_cache();
        self.live_reload_invalidated = true;
    }

    pub(crate) fn mark_live_reload_pending(&mut self) {
        self.mark_live_reload_invalidated();
        self.live_reload_pending = true;
        self.dirty = true;
    }

    pub(crate) fn set_error_log(&mut self, text: impl Into<String>) {
        self.error_log = Some(text.into());
        self.error_log_height = ERROR_LOG_DEFAULT_HEIGHT;
        self.dirty = true;
    }

    pub(crate) fn close_error_log(&mut self) -> bool {
        if self.error_log.take().is_some() {
            self.leader_pending = false;
            self.error_log_resizing = false;
            self.rendered_error_log_separator_row = None;
            self.dirty = true;
            true
        } else {
            false
        }
    }

    pub(crate) fn copy_error_log_to_terminal_clipboard(&mut self) {
        let mut stdout = io::stdout().lock();
        self.copy_error_log_to_writer(&mut stdout);
    }

    pub(crate) fn copy_error_log_to_writer<W: Write>(&mut self, writer: &mut W) {
        let Some(error_log) = self.error_log.clone() else {
            self.set_notice("no error log to copy");
            return;
        };

        match write_osc52_clipboard(writer, &error_log) {
            Ok(()) => self.set_notice("error log copied"),
            Err(error) => self.set_error_log(format!("error log copy failed: {error}")),
        }
    }

    pub(crate) fn copy_marks_to_terminal_clipboard(&mut self) {
        let mut stdout = io::stdout().lock();
        self.copy_marks_to_writer(&mut stdout);
    }

    pub(crate) fn copy_marks_to_writer<W: Write>(&mut self, writer: &mut W) {
        let Some(marks) = self.marks_clipboard_json() else {
            self.set_notice("no marks to copy");
            return;
        };

        match write_osc52_clipboard(writer, &marks) {
            Ok(()) => self.set_notice("marks copied"),
            Err(error) => self.set_error_log(format!("marks copy failed: {error}")),
        }
    }

    pub(crate) fn marks_clipboard_json(&self) -> Option<String> {
        let mut marks = self.export_marks();
        if marks.is_empty() {
            return None;
        }
        marks.sort_by(|left, right| {
            (&left.path, left.old_line, left.new_line).cmp(&(
                &right.path,
                right.old_line,
                right.new_line,
            ))
        });

        let mut out = String::from("{\n  \"version\": 1,\n  \"marks\": [\n");
        for (index, mark) in marks.iter().enumerate() {
            if index > 0 {
                out.push_str(",\n");
            }
            out.push_str("    {\n");
            out.push_str("      \"path\": ");
            out.push_str(&json_string(&mark.path));
            if let Some(old_line) = mark.old_line {
                out.push_str(",\n      \"old_line\": ");
                out.push_str(&old_line.to_string());
            }
            if let Some(new_line) = mark.new_line {
                out.push_str(",\n      \"new_line\": ");
                out.push_str(&new_line.to_string());
            }
            out.push_str(",\n      \"body\": ");
            out.push_str(&json_string(&mark.body));
            out.push_str("\n    }");
        }
        out.push_str("\n  ]\n}");
        Some(out)
    }

    fn export_marks(&self) -> Vec<MarkExport> {
        // Copy marks for the current diff, not stale annotations whose path still
        // exists after a reload. Build an unfiltered model so active file/grep
        // filters do not hide otherwise-current marks from export.
        let export_model = UiModel::new(&self.changeset, self.layout, &self.context_expansions);
        let exportable_keys = self.exportable_annotation_keys(&export_model);
        self.annotations
            .iter()
            .filter_map(|(key, body)| {
                if !exportable_keys.contains(key)
                    && !self.collapsed_context_contains_annotation_key(&export_model, key)
                {
                    return None;
                }
                self.export_mark(key, body)
            })
            .collect()
    }

    fn exportable_annotation_keys(&self, model: &UiModel) -> HashSet<AnnotationKey> {
        model
            .rows
            .iter()
            .copied()
            .flat_map(|row| AnnotationKey::candidates_from_ui_row(&self.changeset, row))
            .collect()
    }

    fn collapsed_context_contains_annotation_key(
        &self,
        model: &UiModel,
        key: &AnnotationKey,
    ) -> bool {
        if key.side != AnnotationSide::New {
            return false;
        }

        model.rows.iter().any(|row| {
            let UiRow::Collapsed {
                file,
                hunk,
                new_start,
                lines,
                expanded,
                ..
            } = *row
            else {
                return false;
            };
            let Some(file) = self.changeset.files.get(file) else {
                return false;
            };
            if AnnotationKey::path_for_side(file, AnnotationSide::New) != Some(key.path.as_str()) {
                return false;
            }

            let hidden_start = match context_expansion_direction(hunk) {
                ContextExpansionDirection::Up => new_start,
                ContextExpansionDirection::Down => new_start.saturating_add(expanded),
            };
            key.line >= hidden_start && key.line.saturating_sub(hidden_start) < lines
        })
    }

    fn export_mark(&self, key: &AnnotationKey, body: &str) -> Option<MarkExport> {
        let (old_line, new_line) = self.annotation_key_lines(key)?;
        Some(MarkExport {
            path: key.path.clone(),
            old_line,
            new_line,
            body: body.to_owned(),
        })
    }

    fn annotation_key_lines(&self, key: &AnnotationKey) -> Option<(Option<usize>, Option<usize>)> {
        match key.side {
            AnnotationSide::Old => Some((Some(key.line), None)),
            AnnotationSide::New => {
                Some((self.paired_old_line_for_new_annotation(key), Some(key.line)))
            }
        }
    }

    fn paired_old_line_for_new_annotation(&self, key: &AnnotationKey) -> Option<usize> {
        self.changeset.files.iter().find_map(|file| {
            if AnnotationKey::path_for_side(file, AnnotationSide::New) != Some(key.path.as_str()) {
                return None;
            }

            file.hunks.iter().find_map(|hunk| {
                hunk.lines
                    .iter()
                    .enumerate()
                    .find_map(|(line_index, line)| {
                        if line.new_line == Some(key.line) {
                            paired_old_line_for_addition(&hunk.lines, line_index)
                        } else {
                            None
                        }
                    })
            })
        })
    }

    pub(crate) fn resize_error_log(&mut self, delta: isize) -> bool {
        if self.error_log.is_none() || delta == 0 {
            return false;
        }
        let current = isize::try_from(self.error_log_height).unwrap_or(isize::MAX);
        let next = current
            .saturating_add(delta)
            .clamp(ERROR_LOG_MIN_HEIGHT as isize, ERROR_LOG_MAX_HEIGHT as isize)
            as u16;
        self.set_error_log_height(next)
    }

    pub(crate) fn set_error_log_height(&mut self, height: u16) -> bool {
        if self.error_log.is_none() {
            return false;
        }
        let next = height.clamp(ERROR_LOG_MIN_HEIGHT, ERROR_LOG_MAX_HEIGHT);
        if next == self.error_log_height {
            return false;
        }
        self.error_log_height = next;
        self.dirty = true;
        true
    }

    pub(crate) fn error_log_separator_row(&self) -> Option<u16> {
        self.error_log.as_ref()?;
        self.rendered_error_log_separator_row
    }

    pub(crate) fn set_rendered_error_log_separator_row(&mut self, row: Option<u16>) {
        self.rendered_error_log_separator_row = row.filter(|_| self.error_log.is_some());
    }

    pub(crate) fn set_rendered_diff_area(&mut self, area: Rect) {
        if self.rendered_diff_area != Some(area) {
            self.clear_diff_mouse_hover();
        }
        self.rendered_diff_area = Some(area);
    }

    pub(crate) fn set_rendered_diff_menu_area(&mut self, area: Option<Rect>) {
        self.rendered_diff_menu_area = area.filter(|_| self.diff_menu_open);
    }

    pub(crate) fn set_rendered_branch_menu_area(&mut self, area: Option<Rect>) {
        self.rendered_branch_menu_area = area.filter(|_| self.branch_menu_open.is_some());
    }

    pub(crate) fn set_rendered_commit_menu_area(&mut self, area: Option<Rect>) {
        self.rendered_commit_menu_area = area.filter(|_| self.commit_menu_open);
    }

    pub(crate) fn set_rendered_review_input_area(&mut self, area: Option<Rect>) {
        self.rendered_review_input_area = area.filter(|_| self.review_input_open);
    }

    pub(crate) fn start_error_log_resize(&mut self, row: u16) -> bool {
        if self.error_log_separator_row() != Some(row) {
            return false;
        }
        self.error_log_resizing = true;
        self.dirty = true;
        true
    }

    pub(crate) fn resize_error_log_to_separator_row(&mut self, row: u16) -> bool {
        let Some(separator_row) = self.error_log_separator_row() else {
            return false;
        };
        let delta = i32::from(separator_row).saturating_sub(i32::from(row));
        let current = i32::from(self.error_log_height);
        let next = current.saturating_add(delta).clamp(
            i32::from(ERROR_LOG_MIN_HEIGHT),
            i32::from(ERROR_LOG_MAX_HEIGHT),
        );
        self.set_error_log_height(next as u16)
    }

    pub(crate) fn start_diff_load(
        &mut self,
        options: DiffOptions,
        error_prefix: impl Into<String>,
    ) {
        self.start_diff_load_inner(options, error_prefix, true);
    }

    pub(crate) fn start_uncached_diff_load(
        &mut self,
        options: DiffOptions,
        error_prefix: impl Into<String>,
    ) {
        self.start_diff_load_inner(options, error_prefix, false);
    }

    fn start_diff_load_inner(
        &mut self,
        options: DiffOptions,
        error_prefix: impl Into<String>,
        use_cache: bool,
    ) {
        let error_prefix = error_prefix.into();
        self.pending_review_load = None;

        let use_cache = use_cache && self.diff_cache_invalidator_active();

        if use_cache {
            self.store_current_diff_cache();

            if let Some(cached) = self.take_cached_diff(&options) {
                self.pending_diff_load = None;
                self.replace_cached_diff(options, cached, false);
                return;
            }

            if self
                .pending_diff_load
                .as_ref()
                .is_some_and(|pending| pending.options == options)
            {
                self.dirty = true;
                return;
            }

            self.diff_prefetch_queue
                .retain(|prefetch_options| prefetch_options != &options);
            if let Some(prefetch) = self.take_pending_diff_prefetch(&options) {
                self.pending_diff_load = Some(PendingDiffLoad {
                    options,
                    error_prefix,
                    refresh_branch_metadata: false,
                    rx: prefetch.rx,
                });
                self.dirty = true;
                return;
            }
        } else {
            self.clear_cached_diff_choices();
        }

        let (tx, rx) = oneshot::channel();
        let load_options = options.clone();
        runtime::spawn_detached_blocking(move || {
            let _ = tx.send(mark_diff::load_review_ref(&load_options));
        });

        self.pending_diff_load = Some(PendingDiffLoad {
            options,
            error_prefix,
            refresh_branch_metadata: !use_cache,
            rx,
        });
        self.dirty = true;
    }

    pub(crate) fn drain_pending_diff_load(&mut self) {
        self.drain_pending_review_load();

        let Some(outcome) =
            self.pending_diff_load
                .as_mut()
                .and_then(|pending| match pending.rx.try_recv() {
                    Ok(result) => Some(Some(result)),
                    Err(oneshot::error::TryRecvError::Empty) => None,
                    Err(oneshot::error::TryRecvError::Closed) => Some(None),
                })
        else {
            return;
        };
        let Some(pending) = self.pending_diff_load.take() else {
            return;
        };

        match outcome {
            Some(Ok(changeset)) => {
                if cacheable_diff_options(&pending.options) {
                    let cached = diff_cache_entry(pending.options.clone(), changeset);
                    self.replace_cached_diff(
                        pending.options,
                        cached,
                        pending.refresh_branch_metadata,
                    );
                } else {
                    self.replace_loaded_diff(pending.options, changeset);
                }
            }
            Some(Err(error)) => self.set_error_log(format!("{}: {error}", pending.error_prefix)),
            None => self.set_error_log(format!("{}: worker stopped", pending.error_prefix)),
        }
    }

    pub(crate) fn start_review_load(&mut self, target: String) {
        let (tx, rx) = oneshot::channel();
        let target = target.trim().to_owned();
        let repo = Self::review_load_repo_for_target(&self.changeset.repo, &target);
        let worker_target = target;
        runtime::spawn_detached_blocking(move || {
            let result = mark_command::review_diff_options(repo, &worker_target, false).and_then(
                |options| {
                    mark_diff::load_review_ref(&options).map(|changeset| (options, changeset))
                },
            );
            let _ = tx.send(result);
        });

        self.pending_review_load = Some(PendingReviewLoad {
            error_prefix: "review unavailable".to_owned(),
            rx,
        });
        self.pending_diff_load = None;
        self.dirty = true;
    }

    pub(crate) fn review_load_repo_for_target(repo: &Path, _target: &str) -> Option<PathBuf> {
        // Numeric review IDs are resolved against this repository, while URLs
        // carry their own pull request identity. In both cases, preserve the
        // active repository so the loaded patch can resolve follow-up local
        // actions relative to the same repo the TUI was reviewing.
        if repo.as_os_str().is_empty() {
            return None;
        }

        Some(repo.to_path_buf())
    }

    pub(crate) fn drain_pending_review_load(&mut self) {
        let Some(outcome) =
            self.pending_review_load
                .as_mut()
                .and_then(|pending| match pending.rx.try_recv() {
                    Ok(result) => Some(Some(result)),
                    Err(oneshot::error::TryRecvError::Empty) => None,
                    Err(oneshot::error::TryRecvError::Closed) => Some(None),
                })
        else {
            return;
        };
        let Some(pending) = self.pending_review_load.take() else {
            return;
        };

        match outcome {
            Some(Ok((mut options, changeset))) => {
                options.include_untracked = self.options.include_untracked;
                self.replace_loaded_diff(options, changeset);
            }
            Some(Err(error)) => self.set_error_log(format!("{}: {error}", pending.error_prefix)),
            None => self.set_error_log(format!("{}: worker stopped", pending.error_prefix)),
        }
    }

    pub(crate) fn start_diff_prefetches(&mut self) {
        if !self.diff_cache_invalidator_active() {
            self.clear_cached_diff_choices();
            return;
        }

        if self.diff_prefetch_started {
            self.start_next_diff_prefetch();
            return;
        }

        self.diff_prefetch_started = true;
        self.queue_diff_prefetches();
        self.start_next_diff_prefetch();
    }

    fn queue_diff_prefetches(&mut self) {
        for choice in self.diff_menu_choices() {
            let Some(options) = self.options_for_choice(choice) else {
                continue;
            };
            if options == self.options
                || !cacheable_diff_options(&options)
                || self.diff_cache_contains(&options)
                || self
                    .pending_diff_load
                    .as_ref()
                    .is_some_and(|pending| pending.options == options)
                || self
                    .pending_diff_prefetch
                    .as_ref()
                    .is_some_and(|pending| pending.options == options)
                || self
                    .diff_prefetch_queue
                    .iter()
                    .any(|queued| queued == &options)
            {
                continue;
            }

            self.diff_prefetch_queue.push_back(options);
        }
    }

    fn start_next_diff_prefetch(&mut self) {
        if !self.diff_cache_invalidator_active() {
            self.clear_cached_diff_choices();
            return;
        }

        if self.pending_diff_prefetch.is_some() {
            return;
        }

        while let Some(options) = self.diff_prefetch_queue.pop_front() {
            if options == self.options
                || !cacheable_diff_options(&options)
                || self.diff_cache_contains(&options)
                || self
                    .pending_diff_load
                    .as_ref()
                    .is_some_and(|pending| pending.options == options)
            {
                continue;
            }

            let (tx, rx) = oneshot::channel();
            let load_options = options.clone();
            runtime::spawn_detached_blocking(move || {
                let _ = tx.send(mark_diff::load_review_ref(&load_options));
            });
            self.pending_diff_prefetch = Some(PendingDiffPrefetch { options, rx });
            return;
        }
    }

    pub(crate) fn drain_diff_prefetch(&mut self) {
        let Some(outcome) =
            self.pending_diff_prefetch
                .as_mut()
                .and_then(|pending| match pending.rx.try_recv() {
                    Ok(result) => Some(Some(result)),
                    Err(oneshot::error::TryRecvError::Empty) => None,
                    Err(oneshot::error::TryRecvError::Closed) => Some(None),
                })
        else {
            return;
        };
        let Some(pending) = self.pending_diff_prefetch.take() else {
            return;
        };

        if let Some(Ok(changeset)) = outcome {
            self.cache_loaded_diff(pending.options, changeset);
        }
        self.start_next_diff_prefetch();
    }

    fn take_pending_diff_prefetch(&mut self, options: &DiffOptions) -> Option<PendingDiffPrefetch> {
        if self
            .pending_diff_prefetch
            .as_ref()
            .is_some_and(|pending| pending.options == *options)
        {
            self.pending_diff_prefetch.take()
        } else {
            None
        }
    }

    pub(crate) fn invalidate_diff_cache(&mut self) {
        self.clear_cached_diff_choices();
    }

    pub(crate) fn clear_cached_diff_choices(&mut self) {
        self.diff_cache.clear();
        self.pending_diff_prefetch = None;
        self.diff_prefetch_queue.clear();
        self.diff_prefetch_started = false;
    }

    fn diff_cache_invalidator_active(&self) -> bool {
        self.live_updates_allowed
            && self.live_updates_enabled
            && !self.live_reload_invalidated
            && !self.live_reload_pending
            && live_diff_supported(&self.options)
            && self.live_diff_failed_options.as_ref() != Some(&self.options)
    }

    fn store_current_diff_cache(&mut self) {
        if !self.diff_cache_invalidator_active() || !cacheable_diff_options(&self.options) {
            return;
        }

        let options = self.options.clone();
        let changeset = self.base_changeset.clone();
        self.diff_cache.retain(|entry| entry.options != options);
        let search_index = Arc::clone(&self.search_index);
        let total_stats = self.total_stats.clone();
        let max_line_width = search_index.max_line_width();
        let can_reuse_current_model =
            !self.filters_active() && !self.filter_busy() && self.context_expansions.is_empty();
        let context_expansions = HashMap::new();
        let unified_model = if can_reuse_current_model && self.layout == DiffLayoutMode::Unified {
            self.model.clone()
        } else {
            UiModel::new(&changeset, DiffLayoutMode::Unified, &context_expansions)
        };
        let split_model = if can_reuse_current_model && self.layout == DiffLayoutMode::Split {
            self.model.clone()
        } else {
            UiModel::new(&changeset, DiffLayoutMode::Split, &context_expansions)
        };
        self.diff_cache.insert(
            0,
            DiffCacheEntry {
                options,
                changeset,
                search_index,
                total_stats,
                max_line_width,
                unified_model,
                split_model,
            },
        );
        self.diff_cache.truncate(MAX_DIFF_CACHE_ENTRIES);
    }

    pub(crate) fn cache_loaded_diff(&mut self, options: DiffOptions, changeset: Changeset) {
        if !self.diff_cache_invalidator_active() || !cacheable_diff_options(&options) {
            return;
        }

        self.diff_cache.retain(|entry| entry.options != options);
        self.diff_cache
            .insert(0, diff_cache_entry(options, changeset));
        self.diff_cache.truncate(MAX_DIFF_CACHE_ENTRIES);
    }

    fn take_cached_diff(&mut self, options: &DiffOptions) -> Option<DiffCacheEntry> {
        let index = self
            .diff_cache
            .iter()
            .position(|entry| &entry.options == options)?;
        Some(self.diff_cache.remove(index))
    }

    fn diff_cache_contains(&self, options: &DiffOptions) -> bool {
        self.diff_cache
            .iter()
            .any(|entry| &entry.options == options)
    }

    fn handle_open_menu_mouse_scroll(&mut self, kind: MouseEventKind) -> bool {
        let delta = match kind {
            MouseEventKind::ScrollDown => 1,
            MouseEventKind::ScrollUp => -1,
            _ => return false,
        };

        if self.help_menu_open {
            self.scroll_help_menu(delta);
        } else if self.color_scheme_picker_open {
            self.move_color_scheme_selection(delta);
        } else if self.branch_menu_open.is_some() {
            self.move_branch_selection(delta);
        } else if self.commit_menu_open {
            self.move_commit_selection(delta);
        } else if self.review_input_open {
            // Review input has no scrollable content, but the open modal should
            // still consume wheel events instead of scrolling the diff behind it.
        } else if self.diff_menu_open {
            self.move_diff_menu_selection(delta);
        } else if self.options_menu_open {
            self.move_options_menu_selection(delta);
        } else {
            return false;
        }

        self.mouse_scroll.reset();
        true
    }

    pub(crate) fn handle_mouse(&mut self, mouse: MouseEvent) -> MarkResult<()> {
        if self.handle_open_menu_mouse_scroll(mouse.kind) {
            return Ok(());
        }

        if self.help_menu_open {
            if let MouseEventKind::Down(MouseButton::Left) = mouse.kind {
                self.close_help_menu();
            }
            self.mouse_scroll.reset();
            return Ok(());
        }

        if self.file_sidebar_resizing {
            match mouse.kind {
                MouseEventKind::Drag(MouseButton::Left) | MouseEventKind::Moved => {
                    self.resize_file_sidebar_to_column(mouse.column);
                    return Ok(());
                }
                MouseEventKind::Up(MouseButton::Left) => {
                    self.file_sidebar_resizing = false;
                    self.resize_file_sidebar_to_column(mouse.column);
                    return Ok(());
                }
                _ => {}
            }
        }

        if self.color_scheme_picker_open {
            match mouse.kind {
                MouseEventKind::Moved | MouseEventKind::Drag(MouseButton::Left) => {
                    if let Some(index) = self.color_scheme_index_at(mouse.column, mouse.row) {
                        self.set_color_scheme_selection(index);
                    }
                    return Ok(());
                }
                MouseEventKind::Down(MouseButton::Left) => {
                    if let Some(index) = self.color_scheme_index_at(mouse.column, mouse.row) {
                        self.set_color_scheme_selection(index);
                        self.select_highlighted_color_scheme();
                    } else if self.is_rendered_color_scheme_picker_position(mouse.column, mouse.row)
                    {
                        self.dirty = true;
                    } else {
                        self.close_color_scheme_picker();
                    }
                    return Ok(());
                }
                _ => {}
            }
        }

        if self.options_menu_open {
            if let MouseEventKind::Down(MouseButton::Left) = mouse.kind {
                self.close_options_menu();
                return Ok(());
            }
        }

        if self.error_log_resizing {
            match mouse.kind {
                MouseEventKind::Drag(MouseButton::Left) | MouseEventKind::Moved => {
                    self.resize_error_log_to_separator_row(mouse.row);
                    return Ok(());
                }
                MouseEventKind::Up(MouseButton::Left) => {
                    self.resize_error_log_to_separator_row(mouse.row);
                    self.error_log_resizing = false;
                    self.dirty = true;
                    return Ok(());
                }
                _ => {}
            }
        }

        self.update_diff_mouse_hover(mouse.column, mouse.row);

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if self.start_error_log_resize(mouse.row) {
                    return Ok(());
                }
                if self.start_file_sidebar_resize(mouse.column, mouse.row) {
                    return Ok(());
                }
                self.handle_click(mouse.column, mouse.row);
            }
            MouseEventKind::ScrollDown => {
                if self.is_file_sidebar_position(mouse.column, mouse.row) {
                    self.mouse_scroll.reset();
                    self.scroll_file_sidebar_by(1);
                    return Ok(());
                }
                self.mouse_scroll_or_focus_hunk(MouseScrollDirection::Down);
                self.update_diff_mouse_hover(mouse.column, mouse.row);
            }
            MouseEventKind::ScrollUp => {
                if self.is_file_sidebar_position(mouse.column, mouse.row) {
                    self.mouse_scroll.reset();
                    self.scroll_file_sidebar_by(-1);
                    return Ok(());
                }
                self.mouse_scroll_or_focus_hunk(MouseScrollDirection::Up);
                self.update_diff_mouse_hover(mouse.column, mouse.row);
            }
            MouseEventKind::ScrollLeft => {
                if self.is_file_sidebar_position(mouse.column, mouse.row) {
                    self.mouse_scroll.reset();
                    return Ok(());
                }
                self.scroll_horizontally_by(-(HORIZONTAL_SCROLL_STEP as isize));
                self.update_diff_mouse_hover(mouse.column, mouse.row);
            }
            MouseEventKind::ScrollRight => {
                if self.is_file_sidebar_position(mouse.column, mouse.row) {
                    self.mouse_scroll.reset();
                    return Ok(());
                }
                self.scroll_horizontally_by(HORIZONTAL_SCROLL_STEP as isize);
                self.update_diff_mouse_hover(mouse.column, mouse.row);
            }
            _ => {}
        }
        Ok(())
    }

    pub(crate) fn is_file_sidebar_position(&self, column: u16, row: u16) -> bool {
        self.file_sidebar_open
            && self.file_sidebar_render_width > 0
            && column < self.file_sidebar_render_width
            && row > 0
            && usize::from(row - 1) < self.visible_file_sidebar_rows()
    }

    pub(crate) fn is_file_sidebar_resize_handle(&self, column: u16, row: u16) -> bool {
        self.is_file_sidebar_position(column, row)
            && column.saturating_add(1) == self.file_sidebar_render_width
    }

    pub(crate) fn start_file_sidebar_resize(&mut self, column: u16, row: u16) -> bool {
        if !self.is_file_sidebar_resize_handle(column, row) {
            return false;
        }

        self.file_sidebar_resizing = true;
        self.resize_file_sidebar_to_column(column);
        true
    }

    pub(crate) fn resize_file_sidebar_to_column(&mut self, column: u16) {
        let width = column.saturating_add(1);
        self.set_file_sidebar_width(width);
    }

    pub(crate) fn handle_click(&mut self, column: u16, row: u16) {
        let clicked_selector = row == 0 && column < diff_selector_width(&self.options);
        let clicked_branch_selector = (row == 0)
            .then(|| self.branch_selector_at(column))
            .flatten();
        let clicked_commit_selector = row == 0 && self.commit_selector_at(column);

        if self.review_input_open {
            if self.is_rendered_review_input_position(column, row) {
                self.dirty = true;
                return;
            }

            self.close_review_input();
            if clicked_selector {
                self.toggle_diff_menu();
            }
            return;
        }

        if self.commit_menu_open {
            if let Some(rev) = self.commit_choice_at(column, row) {
                self.close_commit_menu();
                self.select_show_commit(rev);
                return;
            }

            if self.is_rendered_commit_menu_position(column, row) {
                return;
            }

            if clicked_commit_selector {
                self.toggle_commit_menu();
                return;
            }

            self.close_commit_menu();
            if clicked_selector {
                self.toggle_diff_menu();
            }
            return;
        }

        if let Some(menu) = self.branch_menu_open {
            if let Some(branch) = self.branch_choice_at(menu, column, row) {
                self.close_branch_menu();
                self.select_branch(menu, branch);
                return;
            }

            if self.is_rendered_branch_menu_position(column, row) {
                return;
            }

            if let Some(clicked_menu) = clicked_branch_selector {
                self.toggle_branch_menu(clicked_menu);
                return;
            }

            self.close_branch_menu();
            if clicked_selector {
                self.toggle_diff_menu();
            }
            return;
        }

        if self.diff_menu_open {
            if let Some(choice) = self.diff_choice_at(column, row) {
                self.close_diff_menu();
                self.select_diff_choice(choice);
                return;
            }

            if self.is_rendered_diff_menu_position(column, row) {
                return;
            }

            if let Some(menu) = clicked_branch_selector {
                self.close_diff_menu();
                self.toggle_branch_menu(menu);
                return;
            }

            if clicked_selector {
                self.toggle_diff_menu();
                return;
            }

            self.close_diff_menu();
            return;
        }

        if self.color_scheme_picker_open {
            self.close_color_scheme_picker();
            return;
        }

        if self.options_menu_open {
            self.close_options_menu();
            return;
        }

        if clicked_selector {
            self.toggle_diff_menu();
        } else if clicked_commit_selector {
            self.toggle_commit_menu();
        } else if let Some(menu) = clicked_branch_selector {
            self.toggle_branch_menu(menu);
        } else if !self.handle_file_sidebar_click(column, row) {
            self.handle_diff_click(column, row);
        }
    }

    pub(crate) fn handle_file_sidebar_click(&mut self, column: u16, row: u16) -> bool {
        if !self.is_file_sidebar_position(column, row) {
            return false;
        }

        let position = self
            .file_sidebar_scroll
            .saturating_add(usize::from(row - 1));
        let Some(file) = self.model.visible_files().get(position).copied() else {
            return false;
        };

        self.select_file(file);
        true
    }

    pub(crate) fn handle_diff_click(&mut self, column: u16, row: u16) -> bool {
        let Some((diff_column, viewport_row)) = self.diff_viewport_position(column, row) else {
            return false;
        };
        let width = self.viewport_width;
        if annotation_submit_hit_at_column(diff_column, width)
            && self.handle_annotation_submit_click(viewport_row)
        {
            return true;
        }
        if annotation_edit_hit_at_column(diff_column, width)
            && self.handle_annotation_edit_click(viewport_row)
        {
            return true;
        }
        if annotation_close_hit_at_column(diff_column, width)
            && self.handle_annotation_close_click(viewport_row)
        {
            return true;
        }
        if self
            .mouse_hover
            .is_some_and(|(_, hover_row)| hover_row == viewport_row)
            && self.try_open_annotation_draft_at_viewport_row(viewport_row, diff_column)
        {
            return true;
        }

        let Some(model_row) = model_row_for_viewport_row(self, viewport_row) else {
            return false;
        };
        self.handle_context_at_row(model_row)
    }

    fn diff_viewport_position(&self, column: u16, row: u16) -> Option<(u16, u16)> {
        let area = self.rendered_diff_area?;
        if area.width == 0
            || area.height == 0
            || column < area.x
            || row < area.y
            || column >= area.x.saturating_add(area.width)
            || row >= area.y.saturating_add(area.height)
        {
            return None;
        }

        Some((column.saturating_sub(area.x), row.saturating_sub(area.y)))
    }

    pub(crate) fn annotation_anchor_visual_scroll(&self, model_row_index: usize) -> usize {
        if self.line_wrapping {
            let start = self.wrapped_visual_scroll_for_model_row(model_row_index);
            let height = self.wrapped_visual_height_for_model_row(model_row_index);
            start.saturating_add(height.saturating_sub(1))
        } else {
            model_row_index
        }
    }

    pub(crate) fn annotation_label(&self, key: &AnnotationKey) -> Option<String> {
        Some(format!("{} {}{}", key.path, key.side.label(), key.line))
    }

    fn handle_annotation_submit_click(&mut self, viewport_row: u16) -> bool {
        let Some(draft) = self.annotation_draft.as_ref() else {
            return false;
        };
        if compose_block_bottom_viewport_row(self, draft.model_row_index) != Some(viewport_row) {
            return false;
        }
        let draft = self.annotation_draft.take().expect("draft");
        self.commit_annotation_draft(draft);
        true
    }

    fn handle_annotation_edit_click(&mut self, viewport_row: u16) -> bool {
        if self.annotation_draft.is_some() {
            return false;
        }
        let Some((model_row, key)) = annotation_saved_key_at_bottom_border(self, viewport_row)
        else {
            return false;
        };
        self.open_annotation_draft_for_key(key, model_row)
    }

    fn handle_annotation_close_click(&mut self, viewport_row: u16) -> bool {
        if let Some(draft) = self.annotation_draft.as_ref() {
            if compose_block_top_viewport_row(self, draft.model_row_index) == Some(viewport_row) {
                self.annotation_draft = None;
                self.set_scroll_with_grep_sync(
                    self.scroll,
                    false,
                    HunkFocusScrollBehavior::Preserve,
                );
                self.dirty = true;
                return true;
            }
            return false;
        }

        if self.filter_input.is_some() {
            return false;
        }

        let Some((_model_row, key)) = annotation_saved_key_at_top_border(self, viewport_row) else {
            return false;
        };
        if self.annotations.remove(&key).is_some() {
            self.set_scroll_with_grep_sync(self.scroll, false, HunkFocusScrollBehavior::Preserve);
            self.dirty = true;
            return true;
        }
        false
    }

    fn try_open_annotation_draft_at_viewport_row(
        &mut self,
        viewport_row: u16,
        column: u16,
    ) -> bool {
        if self.filter_input.is_some() {
            return false;
        }
        if self.annotation_draft.is_some() {
            return false;
        }
        let Some(visual_row) = visual_scroll_for_viewport_row(self, viewport_row) else {
            return false;
        };
        let row_index = if self.line_wrapping {
            let Some((row_index, _)) = self.model_row_at_scroll(visual_row) else {
                return false;
            };
            row_index
        } else {
            visual_row
        };
        let Some(row) = self.model.row(row_index) else {
            return false;
        };
        if !crate::render::viewport_plan::row_has_diff_code_content(row) {
            return false;
        }
        if self.annotation_anchor_visual_scroll(row_index) != visual_row {
            return false;
        }
        let Some(key) = self.annotation_key_for_add_click(row, column) else {
            return false;
        };
        self.open_annotation_draft_for_key(key, row_index)
    }

    fn annotation_key_for_add_click(&self, row: UiRow, column: u16) -> Option<AnnotationKey> {
        if !annotation_hit_at_column(column, self.viewport_width) {
            return None;
        }
        AnnotationKey::from_ui_row(&self.changeset, row)
    }

    fn open_annotation_draft_for_key(
        &mut self,
        key: AnnotationKey,
        model_row_index: usize,
    ) -> bool {
        if self.filter_input.is_some() {
            return false;
        }
        let existing = self.annotations.get(&key).cloned().unwrap_or_default();
        let cursor = existing.len();
        self.annotation_draft = Some(AnnotationDraft {
            key,
            model_row_index,
            input: existing,
            cursor,
        });
        self.ensure_annotation_draft_visible();
        self.dirty = true;
        true
    }

    fn ensure_annotation_draft_visible(&mut self) {
        let Some((model_row, anchor, desired_scroll)) =
            self.annotation_draft.as_ref().map(|draft| {
                let anchor = self.annotation_anchor_visual_scroll(draft.model_row_index);
                let height = annotation_compose_block_height(draft, self.viewport_width);
                (
                    draft.model_row_index,
                    anchor,
                    annotation_scroll_for_block(anchor, height, self.viewport_rows),
                )
            })
        else {
            return;
        };

        if compose_block_bottom_viewport_row(self, model_row).is_some() {
            return;
        }
        if desired_scroll != self.scroll {
            self.set_scroll_with_grep_sync(
                desired_scroll,
                false,
                HunkFocusScrollBehavior::Preserve,
            );
        }

        // The compose block is emitted only while the annotated row's anchor is still visible.
        // If the draft is too tall for the viewport, the footer can never be shown; do not
        // chase it past the anchor or the editor disappears entirely.
        let max_scroll = self.max_scroll().min(anchor);
        while compose_block_bottom_viewport_row(self, model_row).is_none()
            && self.scroll < max_scroll
        {
            let previous_scroll = self.scroll;
            self.set_scroll_with_grep_sync(
                self.scroll.saturating_add(1),
                false,
                HunkFocusScrollBehavior::Preserve,
            );
            if self.scroll == previous_scroll {
                break;
            }
        }
    }

    fn annotation_model_row(&self, key: &AnnotationKey) -> Option<usize> {
        self.model.rows.iter().enumerate().find_map(|(index, row)| {
            AnnotationKey::candidates_from_ui_row(&self.changeset, *row)
                .into_iter()
                .any(|row_key| row_key == *key)
                .then_some(index)
        })
    }

    fn reanchor_annotation_draft(&mut self) {
        let Some(key) = self
            .annotation_draft
            .as_ref()
            .map(|draft| draft.key.clone())
        else {
            return;
        };
        let Some(model_row_index) = self.annotation_model_row(&key) else {
            self.annotation_draft = None;
            self.dirty = true;
            return;
        };
        if let Some(draft) = self.annotation_draft.as_mut()
            && draft.model_row_index != model_row_index
        {
            draft.model_row_index = model_row_index;
            self.dirty = true;
        }
    }

    pub(crate) fn handle_annotation_input_key(&mut self, key: KeyEvent) -> bool {
        if self.annotation_draft.is_none() {
            return false;
        }
        if self.keymap.matches_single(GlobalAction::CancelMark, key) {
            self.annotation_draft = None;
            self.set_scroll_with_grep_sync(self.scroll, false, HunkFocusScrollBehavior::Preserve);
            self.dirty = true;
            return true;
        }
        if self.keymap.matches_single(GlobalAction::SaveMark, key) {
            let draft = self.annotation_draft.take().expect("draft");
            self.commit_annotation_draft(draft);
            return true;
        }
        let Some(draft) = self.annotation_draft.as_mut() else {
            return false;
        };
        let mut keep_visible = false;
        match key.code {
            KeyCode::Enter => {
                draft.input.insert(draft.cursor, '\n');
                draft.cursor += 1;
                self.dirty = true;
                keep_visible = true;
            }
            _ => match handle_text_input_key(&mut draft.input, &mut draft.cursor, key) {
                TextInputKeyResult::Edited | TextInputKeyResult::Moved => {
                    self.dirty = true;
                    keep_visible = true;
                }
                TextInputKeyResult::Ignored | TextInputKeyResult::Handled => {}
            },
        }
        if keep_visible {
            self.ensure_annotation_draft_visible();
        }
        true
    }

    fn handle_annotation_save_or_cancel_key(&mut self, key: KeyEvent) -> bool {
        if self.annotation_draft.is_none()
            || !(self.keymap.matches_single(GlobalAction::CancelMark, key)
                || self.keymap.matches_single(GlobalAction::SaveMark, key))
        {
            return false;
        }

        self.handle_annotation_input_key(key)
    }

    fn commit_annotation_draft(&mut self, draft: AnnotationDraft) {
        if draft.input.trim().is_empty() {
            self.annotations.remove(&draft.key);
        } else {
            self.annotations.insert(draft.key, draft.input);
        }
        self.set_scroll_with_grep_sync(self.scroll, false, HunkFocusScrollBehavior::Preserve);
        self.dirty = true;
    }

    pub(crate) fn open_annotation_draft_in_editor(&mut self) {
        let Some(draft) = self.annotation_draft.take() else {
            return;
        };
        let Some(editor) = configured_editor() else {
            self.annotation_draft = Some(draft);
            self.set_notice("set $VISUAL, $GIT_EDITOR, or $EDITOR to edit annotation");
            return;
        };
        let scratch = match create_annotation_scratch_file(&draft.input) {
            Ok(scratch) => scratch,
            Err(error) => {
                self.annotation_draft = Some(draft);
                self.set_notice(format!("annotation editor failed: {error}"));
                return;
            }
        };
        self.terminal_clear_requested = true;
        let status_result = open_text_in_editor(&editor, &scratch.path);
        self.post_editor_quit_key_ignore_until = Some(Instant::now() + POST_EDITOR_QUIT_KEY_IGNORE);
        match status_result {
            Ok(status) if status.success() => match fs::read_to_string(&scratch.path) {
                Ok(contents) => {
                    let mut updated = draft;
                    updated.input = normalize_annotation_editor_contents(&contents);
                    updated.cursor = updated.input.len();
                    self.commit_annotation_draft(updated);
                    self.set_notice("annotation saved");
                }
                Err(error) => {
                    self.annotation_draft = Some(draft);
                    self.set_notice(format!("annotation read failed: {error}"));
                }
            },
            Ok(_) => {
                self.annotation_draft = Some(draft);
                self.set_notice("annotation editor closed");
            }
            Err(error) => {
                self.annotation_draft = Some(draft);
                self.set_notice(format!("annotation editor failed: {error}"));
            }
        }
        self.dirty = true;
    }

    pub(crate) fn update_diff_mouse_hover(&mut self, column: u16, row: u16) {
        let next = self.diff_mouse_hover_in_diff_area(column, row);
        if self.mouse_hover != next {
            self.mouse_hover = next;
            self.dirty = true;
        }
    }

    pub(crate) fn clear_diff_mouse_hover(&mut self) {
        if self.mouse_hover.take().is_some() {
            self.dirty = true;
        }
    }

    fn diff_mouse_hover_in_diff_area(&self, column: u16, row: u16) -> Option<(u16, u16)> {
        if self.diff_modal_blocks_mouse_hover() {
            return None;
        }
        let area = self.rendered_diff_area?;
        if area.width == 0 || area.height == 0 {
            return None;
        }
        if column < area.x
            || row < area.y
            || column >= area.x.saturating_add(area.width)
            || row >= area.y.saturating_add(area.height)
        {
            return None;
        }
        Some((column.saturating_sub(area.x), row.saturating_sub(area.y)))
    }

    pub(crate) fn diff_modal_blocks_mouse_hover(&self) -> bool {
        self.help_menu_open
            || self.color_scheme_picker_open
            || self.options_menu_open
            || self.diff_menu_open
            || self.review_input_open
            || self.commit_menu_open
            || self.branch_menu_open.is_some()
            || self.filter_input.is_some()
            || self.annotation_draft.is_some()
    }

    pub(crate) fn diff_mouse_highlight_visual_row(&self) -> Option<usize> {
        let (_, viewport_row) = self.mouse_hover?;
        visual_scroll_for_viewport_row(self, viewport_row)
    }

    pub(crate) fn handle_context_at_row(&mut self, row_index: usize) -> bool {
        match self.model.row(row_index) {
            Some(UiRow::Collapsed { .. }) => self.expand_context_at_row(row_index),
            Some(UiRow::ContextHide { file, hunk, .. }) => self.hide_context(file, hunk),
            _ => false,
        }
    }

    pub(crate) fn expand_context_at_row(&mut self, row_index: usize) -> bool {
        let Some(UiRow::Collapsed {
            file,
            hunk,
            old_start,
            new_start,
            lines,
            expanded,
        }) = self.model.row(row_index)
        else {
            return false;
        };

        let Some((side, source_lines)) = self.ensure_context_lines(file) else {
            self.set_notice("context unavailable for this diff");
            return true;
        };

        let total = lines.saturating_add(expanded);
        let source_start = match side {
            DiffSide::Old => old_start,
            DiffSide::New => new_start,
        };
        let available = available_context_lines(source_start, total, source_lines.len());
        let current = expanded.min(available);
        let remaining = available.saturating_sub(current);
        if remaining == 0 {
            self.set_notice("no more context");
            return true;
        }

        let next = current.saturating_add(self.context_expand_count(remaining));
        self.update_max_line_width_for_expanded_context(
            &source_lines,
            source_start,
            total,
            current,
            next,
            context_expansion_direction(hunk),
        );
        self.context_expansions
            .insert(ContextKey { file, hunk }, next);
        self.rebuild_model_after_context_visibility_change();
        true
    }

    pub(crate) fn hide_context(&mut self, file: usize, hunk: usize) -> bool {
        if self
            .context_expansions
            .remove(&ContextKey { file, hunk })
            .is_none()
        {
            return false;
        }

        self.rebuild_model_after_context_visibility_change();
        true
    }

    fn rebuild_model_after_context_visibility_change(&mut self) {
        let search_result = self.search_index.search_with_grep_match_limit(
            &self.file_filter,
            &self.grep_filter,
            MAX_LIVE_GREP_MATCHES,
        );
        self.replace_model(
            &search_result.visible_files,
            HunkFocusModelBehavior::PreserveIfValid,
        );
        self.grep_matches = grep_match_rows(&self.model, &search_result.grep_matches);
        self.grep_matches_truncated = search_result.grep_matches_truncated;
        self.selected_grep_match = None;
        self.set_scroll_with_grep_sync(self.scroll, true, HunkFocusScrollBehavior::Preserve);
        self.sync_grep_match_selection_to_scroll();
        self.set_horizontal_scroll(self.horizontal_scroll);
        self.dirty = true;
    }

    pub(crate) fn context_expand_count(&self, available: usize) -> usize {
        self.theme.diff.context_expansion.expand_count(available)
    }

    pub(crate) fn ensure_context_lines(
        &mut self,
        file: usize,
    ) -> Option<(DiffSide, Arc<Vec<String>>)> {
        for side in [DiffSide::New, DiffSide::Old] {
            if !self.has_context_source(file, side) {
                continue;
            }
            if let Some(lines) = self.context_lines(file, side) {
                return Some((side, lines));
            }
        }
        None
    }

    pub(crate) fn has_context_source(&self, file: usize, side: DiffSide) -> bool {
        self.changeset
            .files
            .get(file)
            .and_then(|file_diff| {
                full_file_source(&self.changeset.repo, &self.options, file_diff, side)
            })
            .is_some()
    }

    pub(crate) fn context_source_side(&self, file: usize) -> Option<DiffSide> {
        for side in [DiffSide::New, DiffSide::Old] {
            match self.context_cache.get(&ContextSourceKey { file, side }) {
                Some(ContextSourceEntry::Lines(_)) => return Some(side),
                Some(ContextSourceEntry::Unavailable) => continue,
                None if self.has_context_source(file, side) => return Some(side),
                None => {}
            }
        }
        None
    }

    pub(crate) fn context_lines(
        &mut self,
        file: usize,
        side: DiffSide,
    ) -> Option<Arc<Vec<String>>> {
        let key = ContextSourceKey { file, side };
        if !self.context_cache.contains_key(&key) {
            let entry = self
                .load_context_lines(file, side)
                .map(ContextSourceEntry::Lines)
                .unwrap_or(ContextSourceEntry::Unavailable);
            self.context_cache.insert(key, entry);
            self.invalidate_wrapped_visual_layout();
        }

        match self.context_cache.get(&key) {
            Some(ContextSourceEntry::Lines(lines)) => Some(Arc::clone(lines)),
            Some(ContextSourceEntry::Unavailable) | None => None,
        }
    }

    pub(crate) fn load_context_lines(
        &self,
        file: usize,
        side: DiffSide,
    ) -> Option<Arc<Vec<String>>> {
        let file_diff = self.changeset.files.get(file)?;
        let source = full_file_source(&self.changeset.repo, &self.options, file_diff, side)?;
        let text = load_full_file_source(&source).ok()?;
        Some(Arc::new(split_context_source_lines(&text)))
    }

    pub(crate) fn context_line_text(
        &mut self,
        file: usize,
        old_line: usize,
        new_line: usize,
    ) -> String {
        let Some((side, source_lines)) = self.ensure_context_lines(file) else {
            return "context unavailable".to_owned();
        };
        let line_number = match side {
            DiffSide::Old => old_line,
            DiffSide::New => new_line,
        };
        let Some(line_index) = line_number.checked_sub(1) else {
            return String::new();
        };
        source_lines.get(line_index).cloned().unwrap_or_default()
    }

    pub(crate) fn update_max_line_width_for_expanded_context(
        &mut self,
        source_lines: &[String],
        source_start: usize,
        total: usize,
        current: usize,
        next: usize,
        direction: ContextExpansionDirection,
    ) {
        let Some(source_index_start) = source_start.checked_sub(1) else {
            return;
        };
        let (newly_visible_start, newly_visible_end) = match direction {
            ContextExpansionDirection::Up => {
                (total.saturating_sub(next), total.saturating_sub(current))
            }
            ContextExpansionDirection::Down => (current, next),
        };
        for offset in newly_visible_start..newly_visible_end {
            let Some(text) = source_lines.get(source_index_start + offset) else {
                continue;
            };
            self.max_line_width = self.max_line_width.max(text.width());
        }
    }

    pub(crate) fn toggle_diff_menu(&mut self) {
        if self.diff_menu_open {
            self.close_diff_menu();
        } else {
            self.open_diff_menu();
        }
    }

    pub(crate) fn open_diff_menu(&mut self) {
        let choices = self.diff_menu_choices();
        if choices.is_empty() {
            return;
        }
        self.diff_menu_input.clear();
        self.diff_menu_input_cursor = 0;
        self.diff_menu_selected = 0;
        self.diff_menu_open = true;
        self.options_menu_open = false;
        self.close_color_scheme_picker();
        self.branch_menu_open = None;
        self.rendered_branch_menu_area = None;
        self.close_review_input();
        self.close_commit_menu();
        self.dirty = true;
    }

    pub(crate) fn close_diff_menu(&mut self) {
        if self.diff_menu_open
            || !self.diff_menu_input.is_empty()
            || self.rendered_diff_menu_area.is_some()
        {
            self.diff_menu_open = false;
            self.diff_menu_input.clear();
            self.diff_menu_input_cursor = 0;
            self.rendered_diff_menu_area = None;
            self.dirty = true;
        }
    }

    pub(crate) fn open_review_input(&mut self) {
        self.review_input.clear();
        self.review_input_cursor = 0;
        self.review_input_open = true;
        self.diff_menu_open = false;
        self.diff_menu_input.clear();
        self.diff_menu_input_cursor = 0;
        self.rendered_diff_menu_area = None;
        self.options_menu_open = false;
        self.close_color_scheme_picker();
        self.branch_menu_open = None;
        self.rendered_branch_menu_area = None;
        self.close_commit_menu();
        self.dirty = true;
    }

    pub(crate) fn close_review_input(&mut self) {
        if self.review_input_open
            || !self.review_input.is_empty()
            || self.rendered_review_input_area.is_some()
        {
            self.review_input_open = false;
            self.review_input.clear();
            self.review_input_cursor = 0;
            self.rendered_review_input_area = None;
            self.dirty = true;
        }
    }

    pub(crate) fn open_options_menu(&mut self) {
        self.options_menu_draft = OptionsDraft {
            layout: layout_setting_from_override(self.layout_override),
            live_updates_enabled: self.live_updates_enabled,
            context_expansion: self.theme.diff.context_expansion,
            syntax_enabled: self.syntax.is_some(),
            line_wrapping: self.line_wrapping,
            color_scheme: self.color_scheme,
        };
        self.options_menu_selected = self
            .options_menu_selected
            .min(self.options_menu_items().len().saturating_sub(1));
        self.options_menu_input.clear();
        self.options_menu_input_cursor = 0;
        self.options_menu_scroll = 0;
        self.options_menu_open = true;
        self.close_color_scheme_picker();
        self.diff_menu_open = false;
        self.diff_menu_input.clear();
        self.diff_menu_input_cursor = 0;
        self.rendered_diff_menu_area = None;
        self.close_review_input();
        self.branch_menu_open = None;
        self.rendered_branch_menu_area = None;
        self.close_commit_menu();
        self.dirty = true;
    }

    pub(crate) fn close_options_menu(&mut self) {
        if self.options_menu_open
            || !self.options_menu_input.is_empty()
            || self.options_menu_scroll != 0
        {
            self.options_menu_open = false;
            self.options_menu_input.clear();
            self.options_menu_input_cursor = 0;
            self.options_menu_selected = 0;
            self.options_menu_scroll = 0;
            self.close_color_scheme_picker();
            self.dirty = true;
        }
    }

    pub(crate) fn highlighted_option(&self) -> Option<OptionsMenuItem> {
        self.filtered_options_menu_items()
            .get(self.options_menu_selected)
            .copied()
    }

    pub(crate) fn move_options_menu_selection(&mut self, delta: isize) {
        let len = self.filtered_options_menu_items().len();
        if len == 0 {
            return;
        }

        self.options_menu_selected =
            (self.options_menu_selected as isize + delta).rem_euclid(len as isize) as usize;
        self.dirty = true;
    }

    pub(crate) fn set_options_menu_selection(&mut self, selected: usize) {
        let selected = selected.min(self.filtered_options_menu_items().len().saturating_sub(1));
        if self.options_menu_selected != selected {
            self.options_menu_selected = selected;
            self.dirty = true;
        }
    }

    pub(crate) fn ensure_options_menu_selection_visible(&mut self, visible_rows: usize) {
        let len = self.filtered_options_menu_items().len();
        ensure_selector_scroll(
            &mut self.options_menu_scroll,
            self.options_menu_selected,
            len,
            visible_rows,
        );
    }

    fn clamp_options_menu_selection_to_filtered_items(&mut self) {
        let len = self.filtered_options_menu_items().len();
        let previous_selected = self.options_menu_selected;
        let previous_scroll = self.options_menu_scroll;

        if len == 0 {
            self.options_menu_selected = 0;
            self.options_menu_scroll = 0;
        } else {
            self.options_menu_selected = self.options_menu_selected.min(len.saturating_sub(1));
            self.options_menu_scroll = self.options_menu_scroll.min(self.options_menu_selected);
        }

        if self.options_menu_selected != previous_selected
            || self.options_menu_scroll != previous_scroll
        {
            self.dirty = true;
        }
    }

    pub(crate) fn options_menu_items(&self) -> &'static [OptionsMenuItem] {
        COMMON_OPTIONS_MENU_ITEMS
    }

    pub(crate) fn filtered_options_menu_items(&self) -> Vec<OptionsMenuItem> {
        let items = self.options_menu_items();
        let query = self.options_menu_input.trim().to_ascii_lowercase();
        if query.is_empty() {
            return items.to_vec();
        }

        let mut matches: Vec<_> = items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                self.option_match_score(&query, *item)
                    .map(|score| (score, index, *item))
            })
            .collect();
        matches.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
        matches.into_iter().map(|(_, _, item)| item).collect()
    }

    pub(crate) fn option_match_score(
        &self,
        query: &str,
        item: OptionsMenuItem,
    ) -> Option<(usize, usize)> {
        let label = option_label(item).to_ascii_lowercase();
        let value = self.option_value(item).to_ascii_lowercase();
        let search_value = self.option_search_value(item).to_ascii_lowercase();
        let combined = format!("{label} {value} {search_value}");
        branch_match_score(query, &label)
            .or_else(|| branch_match_score(query, &value))
            .or_else(|| branch_match_score(query, &search_value))
            .or_else(|| branch_match_score(query, &combined))
    }

    pub(crate) fn option_search_value(&self, item: OptionsMenuItem) -> String {
        match item {
            OptionsMenuItem::Layout => {
                layout_setting_label(self.options_menu_draft.layout).to_owned()
            }
            OptionsMenuItem::LiveReload if !self.live_updates_allowed => "off disabled".to_owned(),
            OptionsMenuItem::LiveReload => {
                on_off_search(self.options_menu_draft.live_updates_enabled)
            }
            OptionsMenuItem::ContextExpansion => {
                context_expansion_label(self.options_menu_draft.context_expansion)
            }
            OptionsMenuItem::SyntaxHighlighting => {
                on_off_search(self.options_menu_draft.syntax_enabled)
            }
            OptionsMenuItem::LineWrapping => on_off_search(self.options_menu_draft.line_wrapping),
            OptionsMenuItem::ColorScheme => {
                color_scheme_label(self.options_menu_draft.color_scheme).to_owned()
            }
        }
    }

    pub(crate) fn option_value(&self, item: OptionsMenuItem) -> String {
        match item {
            OptionsMenuItem::Layout => {
                format!("[{}]", layout_setting_label(self.options_menu_draft.layout))
            }
            OptionsMenuItem::LiveReload if !self.live_updates_allowed => "[ ] disabled".to_owned(),
            OptionsMenuItem::LiveReload => checkbox(self.options_menu_draft.live_updates_enabled),
            OptionsMenuItem::ContextExpansion => {
                format!(
                    "[{}]",
                    context_expansion_label(self.options_menu_draft.context_expansion)
                )
            }
            OptionsMenuItem::SyntaxHighlighting => checkbox(self.options_menu_draft.syntax_enabled),
            OptionsMenuItem::LineWrapping => checkbox(self.options_menu_draft.line_wrapping),
            OptionsMenuItem::ColorScheme => {
                format!(
                    "[{}]",
                    color_scheme_label(self.options_menu_draft.color_scheme)
                )
            }
        }
    }

    #[allow(dead_code)]
    pub(crate) fn push_options_menu_input(&mut self, character: char) {
        self.options_menu_input
            .insert(self.options_menu_input_cursor, character);
        self.options_menu_input_cursor += character.len_utf8();
        self.options_menu_selected = 0;
        self.options_menu_scroll = 0;
        self.dirty = true;
    }

    #[allow(dead_code)]
    pub(crate) fn pop_options_menu_input(&mut self) {
        let result = handle_text_input_key(
            &mut self.options_menu_input,
            &mut self.options_menu_input_cursor,
            KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
        );
        if matches!(result, TextInputKeyResult::Edited) {
            self.options_menu_selected = 0;
            self.options_menu_scroll = 0;
            self.dirty = true;
        }
    }

    #[allow(dead_code)]
    pub(crate) fn clear_options_menu_input(&mut self) {
        if !self.options_menu_input.is_empty()
            || self.options_menu_selected != 0
            || self.options_menu_scroll != 0
        {
            self.options_menu_input.clear();
            self.options_menu_input_cursor = 0;
            self.options_menu_selected = 0;
            self.options_menu_scroll = 0;
            self.dirty = true;
        }
    }

    fn apply_options_menu_input_key(&mut self, key: KeyEvent) -> bool {
        match handle_text_input_key(
            &mut self.options_menu_input,
            &mut self.options_menu_input_cursor,
            key,
        ) {
            TextInputKeyResult::Edited => {
                self.options_menu_selected = 0;
                self.options_menu_scroll = 0;
                self.dirty = true;
                true
            }
            TextInputKeyResult::Moved => {
                self.dirty = true;
                true
            }
            TextInputKeyResult::Handled => true,
            TextInputKeyResult::Ignored => false,
        }
    }

    pub(crate) fn activate_selected_option(&mut self) {
        match self.highlighted_option() {
            Some(OptionsMenuItem::ColorScheme) => self.open_color_scheme_picker(),
            Some(_) => self.cycle_selected_option(1),
            None => {}
        }
    }

    pub(crate) fn open_color_scheme_picker(&mut self) {
        self.color_scheme_picker_open = true;
        self.color_scheme_preview_original = Some((self.color_scheme, self.theme));
        self.color_scheme_input.clear();
        self.color_scheme_input_cursor = 0;
        self.color_scheme_scroll = 0;
        self.color_scheme_selected = 0;
        self.ensure_color_scheme_selection_visible();
        self.dirty = true;
    }

    pub(crate) fn close_color_scheme_picker(&mut self) {
        if self.color_scheme_picker_open {
            if let Some((color_scheme, theme)) = self.color_scheme_preview_original.take() {
                self.color_scheme = color_scheme;
                self.theme = theme;
            }
            self.color_scheme_picker_open = false;
            self.color_scheme_input.clear();
            self.color_scheme_input_cursor = 0;
            self.color_scheme_scroll = 0;
            self.rendered_color_scheme_picker_area = None;
            self.dirty = true;
        }
    }

    pub(crate) fn selectable_color_schemes(&self) -> Vec<ColorSchemeChoice> {
        COLOR_SCHEME_CHOICES
            .iter()
            .copied()
            .filter(|choice| *choice != self.options_menu_draft.color_scheme)
            .collect()
    }

    pub(crate) fn filtered_color_schemes(&self) -> Vec<ColorSchemeChoice> {
        let choices = self.selectable_color_schemes();
        let query = self.color_scheme_input.trim().to_ascii_lowercase();
        if query.is_empty() {
            return choices;
        }

        let mut matches: Vec<_> = choices
            .iter()
            .enumerate()
            .filter_map(|(index, choice)| {
                let label = color_scheme_label(*choice);
                branch_match_score(&query, label).map(|score| (score, label.len(), index, *choice))
            })
            .collect();
        matches.sort_by(|left, right| {
            left.0
                .cmp(&right.0)
                .then_with(|| left.1.cmp(&right.1))
                .then_with(|| left.2.cmp(&right.2))
        });
        matches
            .into_iter()
            .map(|(_, _, _, choice)| choice)
            .collect()
    }

    pub(crate) fn max_color_scheme_selection(&self) -> usize {
        self.filtered_color_schemes().len().saturating_sub(1)
    }

    fn color_scheme_picker_rows(&self) -> usize {
        color_scheme_picker_list_visible_rows(self, self.terminal_area)
            .unwrap_or(MAX_COLOR_SCHEME_MENU_ROWS)
            .max(1)
    }

    pub(crate) fn ensure_color_scheme_selection_visible(&mut self) {
        let len = self.filtered_color_schemes().len();
        let visible_rows = self.color_scheme_picker_rows();
        ensure_selector_scroll(
            &mut self.color_scheme_scroll,
            self.color_scheme_selected,
            len,
            visible_rows,
        );
    }

    pub(crate) fn set_color_scheme_selection(&mut self, selected: usize) {
        let selected = selected.min(self.max_color_scheme_selection());
        self.color_scheme_selected = selected;
        self.ensure_color_scheme_selection_visible();
        self.preview_highlighted_color_scheme();
        self.dirty = true;
    }

    pub(crate) fn move_color_scheme_selection(&mut self, delta: isize) {
        let len = self.filtered_color_schemes().len();
        if len == 0 {
            return;
        }
        let selected = (self.color_scheme_selected as isize + delta).rem_euclid(len as isize);
        self.set_color_scheme_selection(selected as usize);
    }

    #[allow(dead_code)]
    pub(crate) fn push_color_scheme_input(&mut self, character: char) {
        self.color_scheme_input
            .insert(self.color_scheme_input_cursor, character);
        self.color_scheme_input_cursor += character.len_utf8();
        self.color_scheme_scroll = 0;
        self.color_scheme_selected = 0;
        self.preview_highlighted_color_scheme();
        self.dirty = true;
    }

    #[allow(dead_code)]
    pub(crate) fn pop_color_scheme_input(&mut self) {
        let result = handle_text_input_key(
            &mut self.color_scheme_input,
            &mut self.color_scheme_input_cursor,
            KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
        );
        if matches!(result, TextInputKeyResult::Edited) {
            self.color_scheme_scroll = 0;
            self.color_scheme_selected = 0;
            self.preview_highlighted_color_scheme();
            self.dirty = true;
        }
    }

    #[allow(dead_code)]
    pub(crate) fn clear_color_scheme_input(&mut self) {
        if !self.color_scheme_input.is_empty()
            || self.color_scheme_scroll != 0
            || self.color_scheme_selected != 0
        {
            self.color_scheme_input.clear();
            self.color_scheme_input_cursor = 0;
            self.color_scheme_scroll = 0;
            self.color_scheme_selected = 0;
            self.preview_highlighted_color_scheme();
            self.dirty = true;
        }
    }

    fn apply_color_scheme_input_key(&mut self, key: KeyEvent) -> bool {
        match handle_text_input_key(
            &mut self.color_scheme_input,
            &mut self.color_scheme_input_cursor,
            key,
        ) {
            TextInputKeyResult::Edited => {
                self.color_scheme_scroll = 0;
                self.color_scheme_selected = 0;
                self.preview_highlighted_color_scheme();
                self.dirty = true;
                true
            }
            TextInputKeyResult::Moved => {
                self.dirty = true;
                true
            }
            TextInputKeyResult::Handled => true,
            TextInputKeyResult::Ignored => false,
        }
    }

    pub(crate) fn preview_highlighted_color_scheme(&mut self) {
        let Some(choice) = self
            .filtered_color_schemes()
            .get(self.color_scheme_selected)
            .copied()
        else {
            return;
        };

        self.apply_color_scheme(choice);
    }

    pub(crate) fn select_highlighted_color_scheme(&mut self) {
        let Some(choice) = self
            .filtered_color_schemes()
            .get(self.color_scheme_selected)
            .copied()
        else {
            self.dirty = true;
            return;
        };

        self.options_menu_draft.color_scheme = choice;
        self.color_scheme_picker_open = false;
        self.color_scheme_preview_original = None;
        self.color_scheme_input.clear();
        self.color_scheme_input_cursor = 0;
        self.color_scheme_scroll = 0;
        self.rendered_color_scheme_picker_area = None;
        self.apply_options_menu_draft(OptionsMenuItem::ColorScheme);
    }

    pub(crate) fn cycle_selected_option(&mut self, delta: isize) {
        let Some(changed_item) = self.highlighted_option() else {
            return;
        };

        match changed_item {
            OptionsMenuItem::Layout => {
                self.options_menu_draft.layout =
                    next_layout_setting(self.options_menu_draft.layout, delta);
            }
            OptionsMenuItem::LiveReload => {
                if !self.live_updates_allowed {
                    self.set_error_log("live reload disabled by --no-watch");
                    return;
                }
                self.options_menu_draft.live_updates_enabled =
                    !self.options_menu_draft.live_updates_enabled;
            }
            OptionsMenuItem::ContextExpansion => {
                self.options_menu_draft.context_expansion = if delta < 0 {
                    previous_context_expansion(self.options_menu_draft.context_expansion)
                } else {
                    next_context_expansion(self.options_menu_draft.context_expansion)
                };
            }
            OptionsMenuItem::SyntaxHighlighting => {
                self.options_menu_draft.syntax_enabled = !self.options_menu_draft.syntax_enabled;
            }
            OptionsMenuItem::LineWrapping => {
                self.options_menu_draft.line_wrapping = !self.options_menu_draft.line_wrapping;
            }
            OptionsMenuItem::ColorScheme => {
                let choices = COLOR_SCHEME_CHOICES;
                let current = choices
                    .iter()
                    .position(|choice| *choice == self.options_menu_draft.color_scheme)
                    .unwrap_or_default();
                let next = (current as isize + delta).rem_euclid(choices.len() as isize) as usize;
                self.options_menu_draft.color_scheme = choices[next];
            }
        }

        self.apply_options_menu_draft(changed_item);
    }

    fn apply_options_menu_draft(&mut self, changed_item: OptionsMenuItem) {
        let draft = self.options_menu_draft;
        let live_reload_reenabled = draft.live_updates_enabled && !self.live_updates_enabled;

        if draft.layout != layout_setting_from_override(self.layout_override) {
            self.set_layout_setting(draft.layout);
        }
        if draft.live_updates_enabled != self.live_updates_enabled {
            self.live_updates_enabled = draft.live_updates_enabled;
            self.live_reload_invalidated = false;
            self.live_reload_pending = false;
            self.live_diff_failed_options = None;
            self.dirty = true;
        }
        if draft.context_expansion != self.theme.diff.context_expansion {
            self.theme.diff.context_expansion = draft.context_expansion;
            self.dirty = true;
        }
        if draft.color_scheme != self.color_scheme {
            self.apply_color_scheme(draft.color_scheme);
        }
        if draft.syntax_enabled != self.syntax.is_some() {
            self.set_syntax_enabled(draft.syntax_enabled);
        }
        if draft.line_wrapping != self.line_wrapping {
            let next_scroll = if draft.line_wrapping {
                self.wrapped_visual_scroll_for_model_row(self.scroll)
            } else {
                self.model_row_at_scroll(self.scroll)
                    .map(|(row, _)| row)
                    .unwrap_or_default()
            };
            self.line_wrapping = draft.line_wrapping;
            self.set_scroll(next_scroll);
            self.set_horizontal_scroll(self.horizontal_scroll);
            self.dirty = true;
        }
        self.persist_options_menu_draft(changed_item);

        if live_reload_reenabled {
            self.invalidate_diff_cache();
            self.start_uncached_diff_load(self.options.clone(), "reload failed");
        } else {
            self.dirty = true;
        }
        self.clamp_options_menu_selection_to_filtered_items();
    }

    fn persist_options_menu_draft(&mut self, changed_item: OptionsMenuItem) {
        let draft = self.options_menu_draft;
        #[cfg(test)]
        {
            self.last_persisted_options_menu_draft = Some((draft, changed_item));
        }

        if !self.settings_persistence_enabled {
            return;
        }

        let result = mark_syntax::settings_write_path()
            .and_then(|path| persist_options_menu_draft_to_path(&path, draft, changed_item));
        if let Err(error) = result {
            self.set_error_log(format!("settings not saved: {error}"));
        }
    }

    pub(crate) fn set_syntax_enabled(&mut self, enabled: bool) {
        if enabled == self.syntax.is_some() {
            self.dirty = true;
            return;
        }

        if !enabled {
            self.syntax = None;
            self.options_menu_draft.syntax_enabled = false;
            self.dirty = true;
            return;
        }

        match self.start_syntax_runtime() {
            Ok(Some(mut syntax)) => {
                syntax.clear(self.generation);
                self.syntax = Some(syntax);
                self.options_menu_draft.syntax_enabled = true;
                self.dirty = true;
            }
            Ok(None) => {
                self.options_menu_draft.syntax_enabled = false;
                self.set_error_log("syntax highlighting unavailable: no languages enabled");
            }
            Err(error) => {
                self.options_menu_draft.syntax_enabled = false;
                self.set_error_log(format!("syntax highlighting unavailable: {error}"));
            }
        }
    }

    fn start_syntax_runtime(&self) -> MarkResult<Option<SyntaxRuntime>> {
        match &self.syntax_startup_mode {
            SyntaxStartupMode::Config | SyntaxStartupMode::Disabled => {
                SyntaxRuntime::start(&self.syntax_settings)
            }
            SyntaxStartupMode::Languages(languages) => Ok(SyntaxRuntime::start_with_languages(
                languages.clone(),
                self.syntax_limits,
            )),
        }
    }

    pub(crate) fn apply_color_scheme(&mut self, color_scheme: ColorSchemeChoice) {
        let Some(config) = color_scheme_config(color_scheme) else {
            self.set_error_log("colorscheme custom cannot be reapplied from options");
            return;
        };
        let diff = self.theme.diff;
        match diff_theme_from_config(&config).and_then(|theme| {
            theme
                .with_color_overrides(&self.theme_color_overrides)
                .map(|theme| theme.with_transparent_background(self.theme_transparent_background))
        }) {
            Ok(theme) => {
                self.theme = theme.with_diff_settings(diff);
                self.color_scheme = color_scheme;
                self.dirty = true;
            }
            Err(error) => {
                self.set_error_log(format!("colorscheme ignored: {error}"));
            }
        }
    }

    pub(crate) fn close_branch_menu(&mut self) {
        if self.branch_menu_open.is_some()
            || !self.branch_menu_input.is_empty()
            || self.branch_menu_scroll != 0
            || self.rendered_branch_menu_area.is_some()
        {
            self.branch_menu_open = None;
            self.branch_menu_input.clear();
            self.branch_menu_input_cursor = 0;
            self.branch_menu_scroll = 0;
            self.branch_menu_selected = 0;
            self.rendered_branch_menu_area = None;
            self.dirty = true;
        }
    }

    pub(crate) fn close_commit_menu(&mut self) {
        if self.commit_menu_open
            || !self.commit_menu_input.is_empty()
            || self.commit_menu_scroll != 0
            || self.rendered_commit_menu_area.is_some()
        {
            self.commit_menu_open = false;
            self.commit_menu_input.clear();
            self.commit_menu_input_cursor = 0;
            self.commit_menu_scroll = 0;
            self.commit_menu_selected = 0;
            self.rendered_commit_menu_area = None;
            self.dirty = true;
        }
    }

    pub(crate) fn toggle_commit_menu(&mut self) {
        if self.comparison_commits.is_empty() {
            self.set_notice("commit list unavailable");
            return;
        }
        if self.commit_menu_open {
            self.close_commit_menu();
            return;
        }

        self.commit_menu_open = true;
        self.diff_menu_open = false;
        self.diff_menu_input.clear();
        self.diff_menu_input_cursor = 0;
        self.rendered_diff_menu_area = None;
        self.close_review_input();
        self.branch_menu_open = None;
        self.branch_menu_input.clear();
        self.branch_menu_input_cursor = 0;
        self.rendered_branch_menu_area = None;
        self.options_menu_open = false;
        self.close_color_scheme_picker();
        self.commit_menu_input.clear();
        self.commit_menu_input_cursor = 0;
        self.commit_menu_selected = self
            .selected_commit_menu_choice()
            .and_then(|commit| {
                self.filtered_commits()
                    .iter()
                    .position(|candidate| candidate.sha == commit.sha)
            })
            .unwrap_or_default()
            .min(self.max_commit_menu_selection());
        self.ensure_commit_selection_visible();
        self.dirty = true;
    }

    pub(crate) fn toggle_branch_menu(&mut self, menu: BranchMenu) {
        if self.comparison_branches.is_empty() {
            return;
        }
        if self.branch_menu_open == Some(menu) {
            self.close_branch_menu();
            return;
        }

        self.branch_menu_open = Some(menu);
        self.diff_menu_open = false;
        self.diff_menu_input.clear();
        self.diff_menu_input_cursor = 0;
        self.rendered_diff_menu_area = None;
        self.close_review_input();
        self.options_menu_open = false;
        self.close_color_scheme_picker();
        self.close_commit_menu();
        self.branch_menu_input.clear();
        self.branch_menu_input_cursor = 0;
        self.branch_menu_selected = self
            .branch_ref(menu)
            .and_then(|branch| {
                self.filtered_branches()
                    .iter()
                    .position(|candidate| *candidate == branch)
            })
            .unwrap_or_default()
            .min(self.max_branch_menu_selection());
        self.ensure_branch_selection_visible();
        self.dirty = true;
    }

    pub(crate) fn branch_selector_at(&self, column: u16) -> Option<BranchMenu> {
        [BranchMenu::Head, BranchMenu::Base]
            .into_iter()
            .find(|menu| {
                let Some(start) = self.branch_selector_start(*menu) else {
                    return false;
                };
                let Some(width) = self.branch_selector_width(*menu) else {
                    return false;
                };
                column >= start && column < start.saturating_add(width)
            })
    }

    pub(crate) fn is_rendered_branch_menu_position(&self, column: u16, row: u16) -> bool {
        self.rendered_branch_menu_area
            .is_some_and(|area| rect_contains(area, column, row))
    }

    pub(crate) fn branch_choice_at(
        &self,
        menu: BranchMenu,
        column: u16,
        row: u16,
    ) -> Option<String> {
        if self.branch_menu_open != Some(menu) {
            return None;
        }

        let menu_area = self.rendered_branch_menu_area?;
        let inner = branch_menu_block(self.theme, menu).inner(menu_area);
        if column < inner.x
            || column >= inner.x.saturating_add(inner.width)
            || row < inner.y.saturating_add(2)
            || row >= inner.y.saturating_add(inner.height)
        {
            return None;
        }

        let row_index = usize::from(row.saturating_sub(inner.y).saturating_sub(2));
        let pinned_rows = usize::from(self.selected_branch_menu_choice(menu).is_some());
        if row_index < pinned_rows {
            return None;
        }

        let branch_index = row_index.saturating_sub(pinned_rows);
        let rendered_choices = inner.height.saturating_sub(2 + pinned_rows as u16) as usize;
        if branch_index >= rendered_choices {
            return None;
        }

        self.filtered_branch(branch_index).map(str::to_owned)
    }

    pub(crate) fn filtered_branch(&self, row_index: usize) -> Option<&str> {
        self.filtered_branches()
            .get(self.branch_menu_scroll.saturating_add(row_index))
            .copied()
    }

    pub(crate) fn move_branch_selection(&mut self, delta: isize) {
        let next = if delta < 0 {
            self.branch_menu_selected
                .saturating_sub(delta.unsigned_abs())
        } else {
            self.branch_menu_selected.saturating_add(delta as usize)
        };
        self.set_branch_selection(next);
    }

    pub(crate) fn set_branch_selection(&mut self, selected: usize) {
        let selected = selected.min(self.max_branch_menu_selection());
        if self.branch_menu_selected != selected {
            self.branch_menu_selected = selected;
            self.ensure_branch_selection_visible();
            self.dirty = true;
        }
    }

    pub(crate) fn cycle_branch_completion(&mut self, delta: isize) {
        let len = self.filtered_branches().len();
        if len == 0 {
            return;
        }

        let next = if delta < 0 {
            self.branch_menu_selected
                .checked_sub(1)
                .unwrap_or(len.saturating_sub(1))
        } else {
            (self.branch_menu_selected + 1) % len
        };
        self.set_branch_selection(next);
    }

    pub(crate) fn ensure_branch_selection_visible(&mut self) {
        self.ensure_branch_selection_visible_for_rows(self.branch_menu_rows());
    }

    fn branch_menu_rows(&self) -> usize {
        branch_menu_list_visible_rows(self, self.terminal_area)
            .unwrap_or(MAX_BRANCH_MENU_ROWS)
            .max(1)
    }

    pub(crate) fn ensure_branch_selection_visible_for_rows(&mut self, visible_rows: usize) {
        let len = self.filtered_branches().len();
        ensure_selector_scroll(
            &mut self.branch_menu_scroll,
            self.branch_menu_selected,
            len,
            visible_rows,
        );
    }

    pub(crate) fn max_branch_menu_selection(&self) -> usize {
        self.filtered_branches().len().saturating_sub(1)
    }

    pub(crate) fn max_branch_menu_scroll(&self) -> usize {
        self.max_branch_menu_scroll_for_rows(self.branch_menu_rows())
    }

    pub(crate) fn max_branch_menu_scroll_for_rows(&self, visible_rows: usize) -> usize {
        self.filtered_branches()
            .len()
            .saturating_sub(visible_rows.max(1))
    }

    pub(crate) fn is_show_diff(&self) -> bool {
        matches!(&self.options.source, DiffSource::Show(_))
    }

    pub(crate) fn show_rev_menu_detail(&self) -> String {
        let rev = self.show_rev.as_deref().or(match &self.options.source {
            DiffSource::Show(rev) => Some(rev.as_str()),
            _ => None,
        });
        match rev {
            None | Some("HEAD") => self
                .current_head
                .clone()
                .or_else(|| current_head_label(&self.changeset.repo))
                .unwrap_or_else(|| "HEAD".to_owned()),
            Some(symbolic) => rev_display_label(symbolic).to_owned(),
        }
    }

    pub(crate) fn commit_menu_width(&self) -> u16 {
        let commit_width = commit_menu_width(&self.comparison_commits) as usize;
        let input_width = self.commit_menu_input.width().saturating_add(4);
        commit_width.max(input_width).max(36).saturating_add(4) as u16
    }

    pub(crate) fn max_commit_menu_selection(&self) -> usize {
        self.filtered_commits().len().saturating_sub(1)
    }

    pub(crate) fn max_commit_menu_scroll_for_rows(&self, visible_rows: usize) -> usize {
        self.filtered_commits()
            .len()
            .saturating_sub(visible_rows.max(1))
    }

    pub(crate) fn ensure_commit_selection_visible(&mut self) {
        self.ensure_commit_selection_visible_for_rows(self.commit_menu_rows());
    }

    fn commit_menu_rows(&self) -> usize {
        commit_menu_list_visible_rows(self, self.terminal_area)
            .unwrap_or(MAX_BRANCH_MENU_ROWS)
            .max(1)
    }

    pub(crate) fn ensure_commit_selection_visible_for_rows(&mut self, visible_rows: usize) {
        let len = self.filtered_commits().len();
        ensure_selector_scroll(
            &mut self.commit_menu_scroll,
            self.commit_menu_selected,
            len,
            visible_rows,
        );
    }

    pub(crate) fn move_commit_selection(&mut self, delta: isize) {
        let next = if delta < 0 {
            self.commit_menu_selected
                .saturating_sub(delta.unsigned_abs())
        } else {
            self.commit_menu_selected.saturating_add(delta as usize)
        };
        self.set_commit_selection(next);
    }

    pub(crate) fn set_commit_selection(&mut self, selected: usize) {
        let selected = selected.min(self.max_commit_menu_selection());
        if self.commit_menu_selected != selected {
            self.commit_menu_selected = selected;
            self.ensure_commit_selection_visible();
            self.dirty = true;
        }
    }

    pub(crate) fn cycle_commit_completion(&mut self, delta: isize) {
        let len = self.filtered_commits().len();
        if len == 0 {
            return;
        }

        let next = if delta < 0 {
            self.commit_menu_selected
                .checked_sub(1)
                .unwrap_or(len.saturating_sub(1))
        } else {
            (self.commit_menu_selected + 1) % len
        };
        self.set_commit_selection(next);
    }

    #[allow(dead_code)]
    pub(crate) fn push_commit_input(&mut self, character: char) {
        self.commit_menu_input
            .insert(self.commit_menu_input_cursor, character);
        self.commit_menu_input_cursor += character.len_utf8();
        self.commit_menu_scroll = 0;
        self.commit_menu_selected = 0;
        self.dirty = true;
    }

    #[allow(dead_code)]
    pub(crate) fn pop_commit_input(&mut self) {
        let result = handle_text_input_key(
            &mut self.commit_menu_input,
            &mut self.commit_menu_input_cursor,
            KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
        );
        if matches!(result, TextInputKeyResult::Edited) {
            self.commit_menu_scroll = 0;
            self.commit_menu_selected = 0;
            self.dirty = true;
        }
    }

    #[allow(dead_code)]
    pub(crate) fn clear_commit_input(&mut self) {
        if !self.commit_menu_input.is_empty()
            || self.commit_menu_scroll != 0
            || self.commit_menu_selected != 0
        {
            self.commit_menu_input.clear();
            self.commit_menu_input_cursor = 0;
            self.commit_menu_scroll = 0;
            self.commit_menu_selected = 0;
            self.dirty = true;
        }
    }

    fn apply_commit_input_key(&mut self, key: KeyEvent) -> bool {
        match handle_text_input_key(
            &mut self.commit_menu_input,
            &mut self.commit_menu_input_cursor,
            key,
        ) {
            TextInputKeyResult::Edited => {
                self.commit_menu_scroll = 0;
                self.commit_menu_selected = 0;
                self.dirty = true;
                true
            }
            TextInputKeyResult::Moved => {
                self.dirty = true;
                true
            }
            TextInputKeyResult::Handled => true,
            TextInputKeyResult::Ignored => false,
        }
    }

    pub(crate) fn selected_commit_menu_choice(&self) -> Option<&GitCommit> {
        let rev = self.show_rev.as_deref()?;
        self.comparison_commits.iter().find(|commit| {
            commit.sha == rev
                || commit.sha.starts_with(rev)
                || rev.starts_with(&commit.sha[..commit.sha.len().min(7)])
        })
    }

    pub(crate) fn selectable_commit_count(&self) -> usize {
        let selected = self.selected_commit_menu_choice();
        self.comparison_commits
            .iter()
            .filter(|commit| selected != Some(commit))
            .count()
    }

    pub(crate) fn filtered_commits(&self) -> Vec<&GitCommit> {
        let query = self.commit_menu_input.trim().to_ascii_lowercase();
        let selected = self.selected_commit_menu_choice();
        if query.is_empty() {
            return self
                .comparison_commits
                .iter()
                .filter(|commit| selected != Some(commit))
                .collect();
        }

        let mut matches: Vec<_> = self
            .comparison_commits
            .iter()
            .enumerate()
            .filter(|(_, commit)| selected != Some(commit))
            .filter_map(|(index, commit)| {
                commit_match_score(&query, commit).map(|score| (score, index, commit))
            })
            .collect();
        matches.sort_by(|left, right| {
            left.0
                .cmp(&right.0)
                .then_with(|| left.1.cmp(&right.1))
                .then_with(|| left.2.sha.cmp(&right.2.sha))
        });
        matches.into_iter().map(|(_, _, commit)| commit).collect()
    }

    pub(crate) fn filtered_commit(&self, row_index: usize) -> Option<&GitCommit> {
        self.filtered_commits()
            .get(self.commit_menu_scroll.saturating_add(row_index))
            .copied()
    }

    pub(crate) fn select_highlighted_commit_match(&mut self) {
        let Some(commit) = self
            .filtered_commits()
            .get(self.commit_menu_selected)
            .map(|commit| (*commit).clone())
        else {
            self.set_notice("no matching commit");
            return;
        };
        self.close_commit_menu();
        self.select_show_commit(commit.sha);
    }

    pub(crate) fn select_show_commit(&mut self, rev: String) {
        let mut options = self.options.clone();
        options.source = DiffSource::Show(rev.clone());
        options.scope = DiffScope::All;

        if options == self.options {
            self.show_rev = Some(rev);
            self.dirty = true;
            return;
        }

        self.show_rev = Some(rev);
        self.start_diff_load(options, "show unavailable");
    }

    pub(crate) fn commit_selector_text(&self) -> Option<String> {
        let rev = self.show_rev.as_deref()?;
        let label = self
            .comparison_commits
            .iter()
            .find(|commit| commit.sha == rev || commit.sha.starts_with(rev))
            .map(|commit| {
                let short = commit_short_sha(commit);
                if commit.subject.is_empty() {
                    short.to_owned()
                } else {
                    format!("{short} · {}", commit.subject)
                }
            })
            .unwrap_or_else(|| rev.to_owned());
        Some(format!("{label} ▾"))
    }

    pub(crate) fn commit_selector_width(&self) -> Option<u16> {
        self.commit_selector_text().map(|text| text.width() as u16)
    }

    pub(crate) fn commit_selector_start(&self) -> Option<u16> {
        if !self.is_show_diff() {
            return None;
        }
        let selector_gap = STATUSLINE_SELECTOR_GAP.width() as u16;
        Some(diff_selector_width(&self.options).saturating_add(selector_gap))
    }

    pub(crate) fn commit_selector_at(&self, column: u16) -> bool {
        let Some(start) = self.commit_selector_start() else {
            return false;
        };
        let Some(width) = self.commit_selector_width() else {
            return false;
        };
        column >= start && column < start.saturating_add(width)
    }

    pub(crate) fn is_rendered_commit_menu_position(&self, column: u16, row: u16) -> bool {
        self.rendered_commit_menu_area
            .is_some_and(|area| rect_contains(area, column, row))
    }

    pub(crate) fn is_rendered_review_input_position(&self, column: u16, row: u16) -> bool {
        self.rendered_review_input_area
            .is_some_and(|area| rect_contains(area, column, row))
    }

    pub(crate) fn commit_choice_at(&self, column: u16, row: u16) -> Option<String> {
        if !self.commit_menu_open {
            return None;
        }

        let menu_area = self.rendered_commit_menu_area?;
        let inner = commit_menu_block(self.theme).inner(menu_area);
        if column < inner.x
            || column >= inner.x.saturating_add(inner.width)
            || row < inner.y.saturating_add(2)
            || row >= inner.y.saturating_add(inner.height)
        {
            return None;
        }

        let row_index = usize::from(row.saturating_sub(inner.y).saturating_sub(2));
        let pinned_rows = usize::from(self.selected_commit_menu_choice().is_some());
        if row_index < pinned_rows {
            return None;
        }

        let commit_index = row_index.saturating_sub(pinned_rows);
        let rendered_choices = inner.height.saturating_sub(2 + pinned_rows as u16) as usize;
        if commit_index >= rendered_choices {
            return None;
        }

        self.filtered_commit(commit_index)
            .map(|commit| commit.sha.clone())
    }

    pub(crate) fn filtered_branches(&self) -> Vec<&str> {
        let menu = self.branch_menu_open.unwrap_or(BranchMenu::Base);
        let query = self.branch_menu_input.trim().to_ascii_lowercase();
        let selected = self.selected_branch_menu_choice(menu);
        if query.is_empty() {
            let mut matches: Vec<_> = self
                .comparison_branches
                .iter()
                .enumerate()
                .filter(|(_, branch)| selected != Some(branch.as_str()))
                .map(|(index, branch)| (self.branch_pin_rank(menu, branch), index, branch.as_str()))
                .collect();
            matches.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
            return matches.into_iter().map(|(_, _, branch)| branch).collect();
        }

        let mut matches: Vec<_> = self
            .comparison_branches
            .iter()
            .enumerate()
            .filter(|(_, branch)| selected != Some(branch.as_str()))
            .filter_map(|(index, branch)| {
                branch_match_score(&query, branch).map(|score| {
                    (
                        self.branch_pin_rank(menu, branch),
                        score,
                        branch.len(),
                        index,
                        branch.as_str(),
                    )
                })
            })
            .collect();
        matches.sort_by(|left, right| {
            left.0
                .cmp(&right.0)
                .then_with(|| left.1.cmp(&right.1))
                .then_with(|| left.2.cmp(&right.2))
                .then_with(|| left.3.cmp(&right.3))
                .then_with(|| left.4.cmp(right.4))
        });
        matches
            .into_iter()
            .map(|(_, _, _, _, branch)| branch)
            .collect()
    }

    pub(crate) fn selected_branch_menu_choice(&self, menu: BranchMenu) -> Option<&str> {
        self.branch_ref(menu)
    }

    pub(crate) fn selectable_branch_count(&self, menu: BranchMenu) -> usize {
        let selected = self.selected_branch_menu_choice(menu);
        self.comparison_branches
            .iter()
            .filter(|branch| selected != Some(branch.as_str()))
            .count()
    }

    pub(crate) fn branch_pin_rank(&self, menu: BranchMenu, branch: &str) -> usize {
        let current = self.current_head.as_deref();
        let base = self.branch_base.as_deref();
        match menu {
            BranchMenu::Head => {
                if current == Some(branch) {
                    0
                } else if base == Some(branch) {
                    1
                } else {
                    2
                }
            }
            BranchMenu::Base => {
                if base == Some(branch) {
                    0
                } else if current == Some(branch) {
                    1
                } else {
                    2
                }
            }
        }
    }

    #[allow(dead_code)]
    pub(crate) fn push_branch_input(&mut self, character: char) {
        self.branch_menu_input
            .insert(self.branch_menu_input_cursor, character);
        self.branch_menu_input_cursor += character.len_utf8();
        self.branch_menu_scroll = 0;
        self.branch_menu_selected = 0;
        self.dirty = true;
    }

    #[allow(dead_code)]
    pub(crate) fn pop_branch_input(&mut self) {
        let result = handle_text_input_key(
            &mut self.branch_menu_input,
            &mut self.branch_menu_input_cursor,
            KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
        );
        if matches!(result, TextInputKeyResult::Edited) {
            self.branch_menu_scroll = 0;
            self.branch_menu_selected = 0;
            self.dirty = true;
        }
    }

    #[allow(dead_code)]
    pub(crate) fn clear_branch_input(&mut self) {
        if !self.branch_menu_input.is_empty()
            || self.branch_menu_scroll != 0
            || self.branch_menu_selected != 0
        {
            self.branch_menu_input.clear();
            self.branch_menu_input_cursor = 0;
            self.branch_menu_scroll = 0;
            self.branch_menu_selected = 0;
            self.dirty = true;
        }
    }

    fn apply_branch_input_key(&mut self, key: KeyEvent) -> bool {
        match handle_text_input_key(
            &mut self.branch_menu_input,
            &mut self.branch_menu_input_cursor,
            key,
        ) {
            TextInputKeyResult::Edited => {
                self.branch_menu_scroll = 0;
                self.branch_menu_selected = 0;
                self.dirty = true;
                true
            }
            TextInputKeyResult::Moved => {
                self.dirty = true;
                true
            }
            TextInputKeyResult::Handled => true,
            TextInputKeyResult::Ignored => false,
        }
    }

    pub(crate) fn select_highlighted_branch_match(&mut self) {
        let Some(menu) = self.branch_menu_open else {
            return;
        };
        let Some(branch) = self
            .filtered_branches()
            .get(self.branch_menu_selected)
            .map(|branch| (*branch).to_owned())
        else {
            self.set_notice("no matching branch");
            return;
        };
        self.close_branch_menu();
        self.select_branch(menu, branch);
    }

    pub(crate) fn is_branch_diff(&self) -> bool {
        matches!(
            &self.options.source,
            DiffSource::Base(_) | DiffSource::Branch { .. }
        )
    }

    pub(crate) fn branch_ref(&self, menu: BranchMenu) -> Option<&str> {
        match menu {
            BranchMenu::Head => self.branch_head.as_deref(),
            BranchMenu::Base => self.branch_base.as_deref(),
        }
    }

    pub(crate) fn branch_selector_text(&self, menu: BranchMenu) -> Option<String> {
        let branch = self.branch_ref(menu)?;
        let label = self.branch_label(menu, branch);
        Some(format!("{label} ▾"))
    }

    pub(crate) fn branch_label(&self, menu: BranchMenu, branch: &str) -> String {
        match self.branch_marker(menu, branch) {
            Some(marker) => format!("{marker} {branch}"),
            None => branch.to_owned(),
        }
    }

    pub(crate) fn branch_marker(&self, menu: BranchMenu, branch: &str) -> Option<&'static str> {
        let current = self.current_head.as_deref();
        let base = self.branch_base.as_deref();
        match menu {
            BranchMenu::Head => {
                if current == Some(branch) {
                    Some(CURRENT_BRANCH_MARKER)
                } else if base == Some(branch) {
                    Some(BASE_BRANCH_MARKER)
                } else {
                    None
                }
            }
            BranchMenu::Base => {
                if base == Some(branch) {
                    Some(BASE_BRANCH_MARKER)
                } else if current == Some(branch) {
                    Some(CURRENT_BRANCH_MARKER)
                } else {
                    None
                }
            }
        }
    }

    pub(crate) fn branch_selector_width(&self, menu: BranchMenu) -> Option<u16> {
        self.branch_selector_text(menu)
            .map(|text| text.width() as u16)
    }

    pub(crate) fn branch_menu_width(&self) -> u16 {
        let branch_width = branch_menu_width(&self.comparison_branches) as usize;
        let input_width = self.branch_menu_input.width().saturating_add(4);
        branch_width.max(input_width).max(36).saturating_add(4) as u16
    }

    pub(crate) fn branch_selector_start(&self, menu: BranchMenu) -> Option<u16> {
        if !self.is_branch_diff() {
            return None;
        }

        let head_width = self.branch_selector_width(BranchMenu::Head)?;
        let selector_gap = STATUSLINE_SELECTOR_GAP.width() as u16;
        let head_start = diff_selector_width(&self.options).saturating_add(selector_gap);
        match menu {
            BranchMenu::Head => Some(head_start),
            BranchMenu::Base => Some(
                head_start
                    .saturating_add(head_width)
                    .saturating_add(BRANCH_COMPARISON_SEPARATOR.width() as u16),
            ),
        }
    }

    pub(crate) fn diff_choice_at(&self, column: u16, row: u16) -> Option<DiffChoice> {
        let choices = self.filtered_diff_choices();
        let menu_area = self.rendered_diff_menu_area?;
        let inner = diff_menu_block(self.theme).inner(menu_area);
        if column < inner.x
            || column >= inner.x.saturating_add(inner.width)
            || row < inner.y.saturating_add(2)
            || row >= inner.y.saturating_add(inner.height)
        {
            return None;
        }

        let row_index = usize::from(row.saturating_sub(inner.y).saturating_sub(2));
        let pinned_rows = usize::from(self.selected_diff_menu_choice().is_some());
        if row_index < pinned_rows {
            return None;
        }

        let choice_index = row_index.saturating_sub(pinned_rows);
        let rendered_choices = choices
            .len()
            .min(inner.height.saturating_sub(2 + pinned_rows as u16) as usize);
        if choice_index >= rendered_choices {
            return None;
        }

        choices.get(choice_index).copied()
    }

    pub(crate) fn is_rendered_diff_menu_position(&self, column: u16, row: u16) -> bool {
        self.rendered_diff_menu_area
            .is_some_and(|area| rect_contains(area, column, row))
    }

    pub(crate) fn color_scheme_index_at(&self, column: u16, row: u16) -> Option<usize> {
        let menu_area = self.rendered_color_scheme_picker_area?;
        let inner = color_scheme_picker_block(self.theme).inner(menu_area);
        let choices = self.filtered_color_schemes();
        if column < inner.x
            || column >= inner.x.saturating_add(inner.width)
            || row < inner.y.saturating_add(3)
            || row >= inner.y.saturating_add(inner.height)
        {
            return None;
        }

        let choice_index = self
            .color_scheme_scroll
            .saturating_add(usize::from(row.saturating_sub(inner.y).saturating_sub(3)));
        choices.get(choice_index).map(|_| choice_index)
    }

    pub(crate) fn is_rendered_color_scheme_picker_position(&self, column: u16, row: u16) -> bool {
        self.rendered_color_scheme_picker_area
            .is_some_and(|area| rect_contains(area, column, row))
    }

    pub(crate) fn diff_menu_choices(&self) -> Vec<DiffChoice> {
        if matches!(
            &self.options.source,
            DiffSource::Range { .. } | DiffSource::Difftool { .. }
        ) || (matches!(&self.options.source, DiffSource::Patch(_))
            && !is_review_options(&self.options))
        {
            return Vec::new();
        }

        let mut choices = vec![DiffChoice::All];
        if self.branch_base.is_some() {
            choices.push(DiffChoice::Branch);
        }
        choices.push(DiffChoice::Show);
        choices.extend([DiffChoice::Unstaged, DiffChoice::Staged]);
        choices.push(DiffChoice::Review);
        choices
    }

    pub(crate) fn filtered_diff_choices(&self) -> Vec<DiffChoice> {
        let choices = self.selectable_diff_choices();
        let query = self.diff_menu_input.trim().to_ascii_lowercase();
        if query.is_empty() {
            return choices;
        }

        let mut matches: Vec<_> = choices
            .iter()
            .enumerate()
            .filter_map(|(index, choice)| {
                self.diff_choice_match_score(&query, *choice)
                    .map(|score| (score, index, *choice))
            })
            .collect();
        matches.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
        matches.into_iter().map(|(_, _, choice)| choice).collect()
    }

    pub(crate) fn selectable_diff_choices(&self) -> Vec<DiffChoice> {
        let selected = self.selected_diff_menu_choice();
        self.diff_menu_choices()
            .into_iter()
            .filter(|choice| Some(*choice) != selected)
            .collect()
    }

    pub(crate) fn selected_diff_menu_choice(&self) -> Option<DiffChoice> {
        let selected = self.pending_or_current_diff_choice()?;
        if selected == DiffChoice::Review {
            return None;
        }

        self.diff_menu_choices()
            .contains(&selected)
            .then_some(selected)
    }

    pub(crate) fn diff_choice_match_score(
        &self,
        query: &str,
        choice: DiffChoice,
    ) -> Option<(usize, usize)> {
        let label = choice.label().to_ascii_lowercase();
        let detail = self.diff_choice_detail(choice).to_ascii_lowercase();
        let combined = format!("{label} {detail}");
        branch_match_score(query, &label)
            .or_else(|| branch_match_score(query, &detail))
            .or_else(|| branch_match_score(query, &combined))
    }

    pub(crate) fn diff_choice_detail(&self, choice: DiffChoice) -> String {
        match choice {
            DiffChoice::All => "HEAD → working tree".to_owned(),
            DiffChoice::Unstaged => "index → working tree".to_owned(),
            DiffChoice::Staged => "HEAD → index".to_owned(),
            DiffChoice::Branch => match self.branch_base.as_deref() {
                Some(base) => {
                    let head = self
                        .branch_head
                        .as_deref()
                        .or(self.current_head.as_deref())
                        .unwrap_or("HEAD");
                    format!("{head} → {base}")
                }
                None => "base unavailable".to_owned(),
            },
            DiffChoice::Show => self.show_rev_menu_detail(),
            DiffChoice::Review => "hosted review for this repo".to_owned(),
        }
    }

    pub(crate) fn highlighted_diff_choice(&self) -> Option<DiffChoice> {
        self.filtered_diff_choices()
            .get(self.diff_menu_selected)
            .copied()
    }

    pub(crate) fn move_diff_menu_selection(&mut self, delta: isize) {
        let choices = self.filtered_diff_choices();
        if choices.is_empty() {
            return;
        }

        self.diff_menu_selected =
            (self.diff_menu_selected as isize + delta).rem_euclid(choices.len() as isize) as usize;
        self.dirty = true;
    }

    pub(crate) fn set_diff_menu_selection(&mut self, selected: usize) {
        let selected = selected.min(self.filtered_diff_choices().len().saturating_sub(1));
        if self.diff_menu_selected != selected {
            self.diff_menu_selected = selected;
            self.dirty = true;
        }
    }

    #[allow(dead_code)]
    pub(crate) fn push_diff_menu_input(&mut self, character: char) {
        self.diff_menu_input
            .insert(self.diff_menu_input_cursor, character);
        self.diff_menu_input_cursor += character.len_utf8();
        self.diff_menu_selected = 0;
        self.dirty = true;
    }

    #[allow(dead_code)]
    pub(crate) fn pop_diff_menu_input(&mut self) {
        let result = handle_text_input_key(
            &mut self.diff_menu_input,
            &mut self.diff_menu_input_cursor,
            KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
        );
        if matches!(result, TextInputKeyResult::Edited) {
            self.diff_menu_selected = 0;
            self.dirty = true;
        }
    }

    #[allow(dead_code)]
    pub(crate) fn clear_diff_menu_input(&mut self) {
        if !self.diff_menu_input.is_empty() || self.diff_menu_selected != 0 {
            self.diff_menu_input.clear();
            self.diff_menu_input_cursor = 0;
            self.diff_menu_selected = 0;
            self.dirty = true;
        }
    }

    fn apply_diff_menu_input_key(&mut self, key: KeyEvent) -> bool {
        match handle_text_input_key(
            &mut self.diff_menu_input,
            &mut self.diff_menu_input_cursor,
            key,
        ) {
            TextInputKeyResult::Edited => {
                self.diff_menu_selected = 0;
                self.dirty = true;
                true
            }
            TextInputKeyResult::Moved => {
                self.dirty = true;
                true
            }
            TextInputKeyResult::Handled => true,
            TextInputKeyResult::Ignored => false,
        }
    }

    pub(crate) fn select_highlighted_diff_choice(&mut self) {
        let Some(choice) = self.highlighted_diff_choice() else {
            return;
        };

        self.close_diff_menu();
        self.select_diff_choice(choice);
    }

    pub(crate) fn current_diff_choice(&self) -> Option<DiffChoice> {
        diff_choice_for_options(&self.options)
    }

    pub(crate) fn pending_or_current_diff_choice(&self) -> Option<DiffChoice> {
        if self.pending_review_load.is_some() {
            return Some(DiffChoice::Review);
        }

        self.pending_diff_load
            .as_ref()
            .and_then(|pending| diff_choice_for_options(&pending.options))
            .or_else(|| self.current_diff_choice())
    }

    pub(crate) fn submit_review_input(&mut self) {
        self.submit_review_input_with(Self::start_review_load);
    }

    fn submit_review_input_with(&mut self, start_review_load: impl FnOnce(&mut Self, String)) {
        let target = self.review_input.trim().to_owned();
        if target.is_empty() {
            self.set_error_log("review unavailable: enter a review ID");
            return;
        }

        self.close_review_input();
        start_review_load(self, target);
    }

    #[cfg(test)]
    pub(crate) fn submit_review_input_for_test(
        &mut self,
        start_review_load: impl FnOnce(&mut Self, String),
    ) {
        self.submit_review_input_with(start_review_load);
    }

    pub(crate) fn cycle_diff_choice(&mut self, delta: isize) {
        let choices: Vec<_> = self
            .diff_menu_choices()
            .into_iter()
            .filter(|choice| *choice != DiffChoice::Review)
            .collect();
        if choices.is_empty() || delta == 0 {
            return;
        }

        let current = self
            .pending_or_current_diff_choice()
            .and_then(|choice| choices.iter().position(|candidate| *candidate == choice));
        // Review opens an input modal, so keyboard cycling skips it. If the
        // current choice is absent, anchor just outside the cycle so the first
        // keypress lands on the first/last diff choice, matching the menu.
        let choice_count = choices.len() as isize;
        let next = match current {
            Some(current) => current as isize + delta,
            None if delta > 0 => delta - 1,
            None => delta,
        }
        .rem_euclid(choice_count) as usize;
        self.select_diff_choice(choices[next]);
    }

    pub(crate) fn select_branch(&mut self, menu: BranchMenu, branch: String) {
        let base = match menu {
            BranchMenu::Head => self.branch_base.clone(),
            BranchMenu::Base => Some(branch.clone()),
        };
        let head = match menu {
            BranchMenu::Head => Some(branch.clone()),
            BranchMenu::Base => self
                .branch_head
                .clone()
                .or_else(|| self.current_head.clone())
                .or_else(|| current_head_label(&self.changeset.repo)),
        };
        let Some((base, head)) = base.zip(head) else {
            self.set_error_log("branch diff unavailable");
            return;
        };

        let mut options = self.options.clone();
        options.source = self.branch_source(base, head);
        options.scope = DiffScope::All;

        if options == self.options {
            self.dirty = true;
            return;
        }

        self.start_diff_load(options, "branch diff unavailable");
    }

    pub(crate) fn branch_source(&self, base: String, head: String) -> DiffSource {
        if self.current_head.as_deref() == Some(head.as_str()) {
            DiffSource::Base(base)
        } else {
            DiffSource::Branch { base, head }
        }
    }

    pub(crate) fn select_diff_choice(&mut self, choice: DiffChoice) {
        if !self.diff_menu_choices().contains(&choice) {
            return;
        }

        if choice == DiffChoice::Review {
            self.open_review_input();
            return;
        }

        let Some(options) = self.options_for_choice(choice) else {
            return;
        };

        if options == self.options {
            self.pending_diff_load = None;
            self.pending_review_load = None;
            self.dirty = true;
            return;
        }

        self.start_diff_load(options, "diff unavailable");
    }

    pub(crate) fn options_for_choice(&self, choice: DiffChoice) -> Option<DiffOptions> {
        let mut options = self.options.clone();
        match choice {
            DiffChoice::Branch => {
                let base = self
                    .branch_base
                    .clone()
                    .or_else(|| default_branch_base(&self.options, &self.changeset.repo))?;
                let head = self
                    .branch_head
                    .clone()
                    .or_else(|| self.current_head.clone())
                    .or_else(|| current_head_label(&self.changeset.repo))?;
                options.source = self.branch_source(base, head);
                options.scope = DiffScope::All;
            }
            DiffChoice::All => {
                options.source = DiffSource::Worktree;
                options.scope = DiffScope::All;
            }
            DiffChoice::Unstaged => {
                options.source = DiffSource::Worktree;
                options.scope = DiffScope::Unstaged;
            }
            DiffChoice::Staged => {
                options.source = DiffSource::Worktree;
                options.scope = DiffScope::Staged;
            }
            DiffChoice::Show => {
                let rev = self.show_rev.clone().unwrap_or_else(|| "HEAD".to_owned());
                options.source = DiffSource::Show(rev);
                options.scope = DiffScope::All;
            }
            DiffChoice::Review => return None,
        }

        Some(options)
    }

    pub(crate) fn scroll_by(&mut self, delta: isize) {
        let next = if delta < 0 {
            self.scroll.saturating_sub(delta.unsigned_abs())
        } else {
            self.scroll.saturating_add(delta as usize)
        };
        self.set_scroll(next);
    }

    pub(crate) fn scroll_or_focus_hunk(&mut self, delta: isize) {
        let previous_scroll = self.scroll;
        self.scroll_by(delta);
        if self.scroll == previous_scroll {
            self.move_focused_hunk(delta);
        }
    }

    pub(crate) fn mouse_scroll_or_focus_hunk(&mut self, direction: MouseScrollDirection) {
        let delta = self.mouse_scroll.scroll_delta(direction, Instant::now());
        let previous_scroll = self.scroll;
        self.scroll_by(delta);
        if self.scroll == previous_scroll {
            let hunk_delta = self.mouse_scroll.hunk_focus_delta(direction);
            if hunk_delta != 0 {
                self.move_focused_hunk(hunk_delta);
            }
        } else {
            self.mouse_scroll.reset_hunk_focus_ticks();
        }
    }

    pub(crate) fn scroll_horizontally_by(&mut self, delta: isize) {
        let next = if delta < 0 {
            self.horizontal_scroll.saturating_sub(delta.unsigned_abs())
        } else {
            self.horizontal_scroll.saturating_add(delta as usize)
        };
        self.set_horizontal_scroll(next);
    }

    pub(crate) fn set_horizontal_scroll(&mut self, scroll: usize) {
        let previous_scroll = self.horizontal_scroll;
        self.horizontal_scroll = scroll.min(self.max_horizontal_scroll());
        if self.horizontal_scroll != previous_scroll {
            self.clear_diff_mouse_hover();
            self.dirty = true;
        }
    }

    pub(crate) fn set_scroll(&mut self, scroll: usize) {
        self.set_scroll_with_grep_sync(scroll, true, HunkFocusScrollBehavior::ClearOnScroll);
    }

    fn invalidate_wrapped_visual_layout(&self) {
        self.wrapped_visual_layout.borrow_mut().take();
    }

    fn cached_context_line_text(
        &self,
        file: usize,
        old_line: usize,
        new_line: usize,
    ) -> Option<&str> {
        for side in [DiffSide::New, DiffSide::Old] {
            let key = ContextSourceKey { file, side };
            match self.context_cache.get(&key) {
                Some(ContextSourceEntry::Lines(lines)) => {
                    let line_number = match side {
                        DiffSide::Old => old_line,
                        DiffSide::New => new_line,
                    };
                    let Some(line_index) = line_number.checked_sub(1) else {
                        return Some("");
                    };
                    return Some(lines.get(line_index).map(String::as_str).unwrap_or(""));
                }
                Some(ContextSourceEntry::Unavailable) => continue,
                None if self.has_context_source(file, side) => return None,
                None => {}
            }
        }
        None
    }

    fn wrapped_visual_height_for_text(&self, text: &str) -> usize {
        match self.layout {
            DiffLayoutMode::Unified => {
                wrapped_line_count(text, unified_content_width(self.viewport_width))
            }
            DiffLayoutMode::Split => {
                let left_width = self.viewport_width / 2;
                let right_width = self.viewport_width.saturating_sub(left_width);
                wrapped_line_count(text, split_cell_content_width(left_width)).max(
                    wrapped_line_count(text, split_cell_content_width(right_width)),
                )
            }
        }
    }

    fn wrapped_visual_height_for_row(&self, row: UiRow) -> usize {
        match row {
            UiRow::ContextLine {
                file,
                old_line,
                new_line,
            } => self
                .cached_context_line_text(file, old_line, new_line)
                .map(|text| self.wrapped_visual_height_for_text(text))
                .unwrap_or(1),
            UiRow::UnifiedLine { file, hunk, line } | UiRow::MetaLine { file, hunk, line } => {
                let text = &self.changeset.files[file].hunks[hunk].lines[line].text;
                wrapped_line_count(text, unified_content_width(self.viewport_width))
            }
            UiRow::SplitLine {
                file,
                hunk,
                left,
                right,
            } => {
                let lines = &self.changeset.files[file].hunks[hunk].lines;
                let left_width = self.viewport_width / 2;
                let right_width = self.viewport_width.saturating_sub(left_width);
                let left_content_width = split_cell_content_width(left_width);
                let right_content_width = split_cell_content_width(right_width);
                let left_rows = left
                    .and_then(|index| lines.get(index))
                    .map(|line| wrapped_line_count(&line.text, left_content_width))
                    .unwrap_or(1);
                let right_rows = right
                    .and_then(|index| lines.get(index))
                    .map(|line| wrapped_line_count(&line.text, right_content_width))
                    .unwrap_or(1);
                left_rows.max(right_rows).max(1)
            }
            UiRow::FileSeparator
            | UiRow::FileHeader(_)
            | UiRow::BinaryFile(_)
            | UiRow::Collapsed { .. }
            | UiRow::ContextHide { .. }
            | UiRow::HunkHeader { .. } => 1,
        }
    }

    fn ensure_wrapped_visual_layout(&self) {
        if self
            .wrapped_visual_layout
            .borrow()
            .as_ref()
            .is_some_and(|layout| layout.matches(self))
        {
            return;
        }

        let mut row_starts = Vec::with_capacity(self.model.len().saturating_add(1));
        row_starts.push(0);
        let mut total_rows = 0usize;
        for row_index in 0..self.model.len() {
            let height = self
                .model
                .row(row_index)
                .map(|row| self.wrapped_visual_height_for_row(row))
                .unwrap_or(1)
                .max(1);
            total_rows = total_rows.saturating_add(height);
            row_starts.push(total_rows);
        }

        *self.wrapped_visual_layout.borrow_mut() = Some(WrappedVisualLayout {
            layout: self.layout,
            viewport_width: self.viewport_width,
            model_rows: self.model.len(),
            model_rows_ptr: self.model.rows.as_ptr() as usize,
            row_starts,
            total_rows,
        });
    }

    fn wrapped_visual_row_count(&self) -> usize {
        self.ensure_wrapped_visual_layout();
        self.wrapped_visual_layout
            .borrow()
            .as_ref()
            .map(|layout| layout.total_rows)
            .unwrap_or_default()
    }

    pub(crate) fn wrapped_visual_scroll_for_model_row(&self, row_index: usize) -> usize {
        self.ensure_wrapped_visual_layout();
        self.wrapped_visual_layout
            .borrow()
            .as_ref()
            .and_then(|layout| layout.row_starts.get(row_index.min(layout.model_rows)))
            .copied()
            .unwrap_or_default()
    }

    pub(crate) fn wrapped_visual_height_for_model_row(&self, row_index: usize) -> usize {
        self.ensure_wrapped_visual_layout();
        self.wrapped_visual_layout
            .borrow()
            .as_ref()
            .and_then(|layout| {
                let row_index = row_index.min(layout.model_rows);
                let start = layout.row_starts.get(row_index)?;
                let end = layout.row_starts.get(row_index.saturating_add(1))?;
                Some(end.saturating_sub(*start))
            })
            .unwrap_or(1)
    }

    pub(crate) fn model_row_at_scroll(&self, scroll: usize) -> Option<(usize, usize)> {
        if !self.line_wrapping {
            return self.model.row(scroll).map(|_| (scroll, 0));
        }

        self.ensure_wrapped_visual_layout();
        let layout = self.wrapped_visual_layout.borrow();
        let layout = layout.as_ref()?;
        if scroll >= layout.total_rows {
            return None;
        }

        let row_index = layout
            .row_starts
            .partition_point(|row_start| *row_start <= scroll)
            .saturating_sub(1);
        let row_start = layout
            .row_starts
            .get(row_index)
            .copied()
            .unwrap_or_default();
        Some((row_index, scroll.saturating_sub(row_start)))
    }

    fn scroll_for_model_row(&self, row: usize) -> usize {
        if self.line_wrapping {
            self.wrapped_visual_scroll_for_model_row(row)
        } else {
            row
        }
    }

    fn relative_scroll_from_file_start(&self, file: usize) -> usize {
        self.model
            .file_start_row(file)
            .map(|start| self.scroll.saturating_sub(self.scroll_for_model_row(start)))
            .unwrap_or_default()
    }

    fn visible_model_range_for_viewport(&self, visible_rows: usize) -> Option<Range<usize>> {
        if visible_rows == 0 || self.model.is_empty() {
            return None;
        }

        if !self.line_wrapping {
            let visible_start = self.scroll.min(self.model.len());
            let visible_end = visible_start
                .saturating_add(visible_rows)
                .min(self.model.len());
            return (visible_start < visible_end).then_some(visible_start..visible_end);
        }

        let visible_start = self.model_row_at_scroll(self.scroll).map(|(row, _)| row)?;
        let visible_end = self
            .model_row_at_scroll(self.scroll.saturating_add(visible_rows - 1))
            .map(|(row, _)| row.saturating_add(1))
            .unwrap_or_else(|| self.model.len());

        (visible_start < visible_end).then_some(visible_start..visible_end)
    }

    fn clear_manual_hunk_focus(&mut self) {
        self.manual_hunk_focus = None;
    }

    fn replace_model(
        &mut self,
        visible_files: &[usize],
        hunk_focus_behavior: HunkFocusModelBehavior,
    ) {
        let previous_manual_hunk_focus = self.manual_hunk_focus;
        self.model = UiModel::new_filtered(
            &self.changeset,
            self.layout,
            &self.context_expansions,
            visible_files,
        );
        self.invalidate_wrapped_visual_layout();
        self.manual_hunk_focus = match hunk_focus_behavior {
            HunkFocusModelBehavior::PreserveIfValid => previous_manual_hunk_focus
                .filter(|(file, hunk)| self.model.hunk_start_row(*file, *hunk).is_some()),
            HunkFocusModelBehavior::Clear => None,
        };
        self.reanchor_annotation_draft();
    }

    pub(crate) fn set_scroll_centered_on(&mut self, row: usize) {
        let center_offset = viewport_center_offset(self.viewport_rows);
        let scroll = self.scroll_for_model_row(row).saturating_sub(center_offset);
        let scroll = self.scroll_with_model_row_rendered(scroll, row);
        self.set_scroll_with_grep_sync(scroll, false, HunkFocusScrollBehavior::ClearOnScroll);
    }

    pub(crate) fn set_scroll_focused_on_hunk(&mut self, file: usize, hunk: usize) {
        let Some((range, hunk_start_row)) = hunk_focus_row_range(&self.model, file, hunk) else {
            return;
        };

        let focus_start = self.scroll_for_model_row(range.start);
        let focus_end = self
            .scroll_for_model_row(range.end)
            .max(focus_start.saturating_add(1));
        let hunk_start = self.scroll_for_model_row(hunk_start_row);
        let focus_rows = focus_end.saturating_sub(focus_start).max(1);
        let scroll = if focus_rows > self.viewport_rows {
            // Oversized focus ranges cannot be fully centered. Keep the first
            // useful context row when possible, but never so much context that
            // the hunk header itself falls below the viewport.
            focus_start.max(
                hunk_start
                    .saturating_add(1)
                    .saturating_sub(self.viewport_rows),
            )
        } else {
            let focus_center = focus_start.saturating_add(focus_rows.saturating_sub(1) / 2);
            focus_center.saturating_sub(viewport_center_offset(self.viewport_rows))
        };
        let scroll = self.scroll_with_model_row_rendered(scroll, hunk_start_row);
        self.set_scroll_with_grep_sync(scroll, false, HunkFocusScrollBehavior::Preserve);
    }

    fn scroll_with_model_row_rendered(&self, preferred_scroll: usize, model_row: usize) -> usize {
        let max_scroll = self.max_scroll();
        let preferred_scroll = preferred_scroll.min(max_scroll);
        if self.model_row_rendered_at_scroll(preferred_scroll, self.viewport_rows, model_row) {
            return preferred_scroll;
        }

        let target_scroll = self.scroll_for_model_row(model_row).min(max_scroll);
        if preferred_scroll <= target_scroll {
            for scroll in preferred_scroll.saturating_add(1)..=target_scroll {
                if self.model_row_rendered_at_scroll(scroll, self.viewport_rows, model_row) {
                    return scroll;
                }
            }
        } else {
            for scroll in (target_scroll..preferred_scroll).rev() {
                if self.model_row_rendered_at_scroll(scroll, self.viewport_rows, model_row) {
                    return scroll;
                }
            }
        }

        target_scroll
    }

    fn rendered_diff_rows_for_viewport(&self, visible_rows: usize) -> Vec<RenderedDiffRow> {
        self.rendered_diff_rows_for_viewport_at_scroll(self.scroll, visible_rows)
    }

    fn rendered_diff_rows_for_viewport_at_scroll(
        &self,
        scroll: usize,
        visible_rows: usize,
    ) -> Vec<RenderedDiffRow> {
        plan_diff_viewport_rows_at_scroll(self, scroll, visible_rows)
            .into_iter()
            .enumerate()
            .filter_map(|(viewport_row, slot)| match slot.kind {
                ViewportSlotKind::DiffVisual { model_row, .. } => Some(RenderedDiffRow {
                    viewport_row,
                    model_row,
                }),
                ViewportSlotKind::AnnotationCompose { .. }
                | ViewportSlotKind::AnnotationSaved { .. } => None,
            })
            .collect()
    }

    fn model_row_rendered_at_scroll(
        &self,
        scroll: usize,
        visible_rows: usize,
        model_row: usize,
    ) -> bool {
        self.rendered_diff_rows_for_viewport_at_scroll(scroll, visible_rows)
            .iter()
            .any(|rendered_row| rendered_row.model_row == model_row)
    }

    fn rendered_viewport_focus_row(&self, visible_rows: usize) -> usize {
        let row_count = if self.line_wrapping {
            self.wrapped_visual_row_count()
        } else {
            self.model.len()
        };
        viewport_focus_offset(self.scroll, row_count, visible_rows)
    }

    fn focused_hunk_in_rendered_rows(
        &self,
        rendered_rows: &[RenderedDiffRow],
        search: HunkFocusSearch,
    ) -> Option<(usize, usize)> {
        match search {
            HunkFocusSearch::FirstVisible => {
                for rendered_row in rendered_rows {
                    if let Some(hunk_key) = self
                        .model
                        .row(rendered_row.model_row)
                        .and_then(|row| row.hunk_key())
                    {
                        return Some(hunk_key);
                    }
                }
                None
            }
            HunkFocusSearch::NearestTo(focus_viewport_row) => {
                find_rendered_diff_row_outward(rendered_rows, focus_viewport_row, |rendered_row| {
                    self.model
                        .row(rendered_row.model_row)
                        .and_then(|row| row.hunk_key())
                })
            }
        }
    }

    fn set_scroll_with_grep_sync(
        &mut self,
        scroll: usize,
        sync_grep: bool,
        hunk_focus_behavior: HunkFocusScrollBehavior,
    ) {
        let previous_scroll = self.scroll;
        let previous_file = self.selected_file;
        self.scroll = scroll.min(self.max_scroll());
        if self.scroll != previous_scroll
            && hunk_focus_behavior == HunkFocusScrollBehavior::ClearOnScroll
        {
            self.clear_manual_hunk_focus();
        }
        if let Some(file) = if self.line_wrapping {
            self.model_row_at_scroll(self.scroll)
                .and_then(|(row, _)| self.model.file_at_row(row))
        } else {
            self.model.file_at_row(self.scroll)
        } {
            self.selected_file = file;
        }
        if sync_grep && self.scroll != previous_scroll {
            self.sync_grep_match_selection_to_scroll();
        }
        if self.scroll != previous_scroll || self.selected_file != previous_file {
            if self.scroll != previous_scroll {
                self.clear_diff_mouse_hover();
            }
            self.dirty = true;
        }
    }

    pub(crate) fn max_scroll(&self) -> usize {
        let row_count = if self.line_wrapping {
            self.wrapped_visual_row_count()
        } else {
            self.model.len()
        };
        self.max_scroll_with_annotations(row_count)
    }

    fn max_scroll_with_annotations(&self, row_count: usize) -> usize {
        let mut blocks = Vec::new();
        let draft_key = self.annotation_draft.as_ref().map(|draft| &draft.key);
        for (key, text) in &self.annotations {
            if let Some(model_row) = self.annotation_model_row(key) {
                if draft_key == Some(key) {
                    continue;
                }
                let anchor = self.annotation_anchor_visual_scroll(model_row);
                let height = annotation_saved_block_height(text, self.viewport_width);
                blocks.push((anchor, height));
            }
        }
        if let Some(draft) = self.annotation_draft.as_ref() {
            let anchor = self.annotation_anchor_visual_scroll(draft.model_row_index);
            let height = annotation_compose_block_height(draft, self.viewport_width);
            blocks.push((anchor, height));
        }
        max_scroll_for_annotated_viewport(row_count, self.viewport_rows, blocks)
    }

    pub(crate) fn max_horizontal_scroll(&self) -> usize {
        if self.line_wrapping {
            return 0;
        }

        self.max_line_width
            .saturating_sub(diff_content_width(self.layout, self.viewport_width))
    }

    pub(crate) fn focused_hunk_for_viewport(&self, visible_rows: usize) -> Option<(usize, usize)> {
        let rendered_rows = self.rendered_diff_rows_for_viewport(visible_rows);
        if rendered_rows.is_empty() {
            return None;
        }

        if let Some((file, hunk)) = self.manual_hunk_focus
            && let Some(row) = self.model.hunk_start_row(file, hunk)
            && rendered_rows
                .iter()
                .any(|rendered_row| rendered_row.model_row == row)
        {
            return Some((file, hunk));
        }

        let row_count = if self.line_wrapping {
            self.wrapped_visual_row_count()
        } else {
            self.model.len()
        };
        let search = if max_scroll_for_viewport(row_count, visible_rows) == 0 {
            // When the whole diff fits, start at the first visible hunk; explicit hunk
            // navigation is tracked separately with manual_hunk_focus.
            HunkFocusSearch::FirstVisible
        } else {
            HunkFocusSearch::NearestTo(self.rendered_viewport_focus_row(visible_rows))
        };
        self.focused_hunk_in_rendered_rows(&rendered_rows, search)
    }

    pub(crate) fn focused_hunk_editor_target(&self) -> Option<EditorTarget> {
        if matches!(
            self.options.source,
            DiffSource::Patch(_) | DiffSource::Show(_) | DiffSource::Difftool { .. }
        ) {
            return None;
        }

        let (file, hunk) = self.focused_hunk_for_viewport(self.viewport_rows)?;
        let file_diff = self.changeset.files.get(file)?;
        let hunk_diff = file_diff.hunks.get(hunk)?;
        let path = file_diff.new_path.as_deref()?;
        let line = self
            .focused_hunk_editor_line(file, hunk)
            .unwrap_or_else(|| hunk_diff.new_start.max(1));

        Some(EditorTarget {
            path: repo_file_path(&self.changeset.repo, path),
            line,
        })
    }

    pub(crate) fn focused_hunk_editor_reload_request(&self) -> Option<EditorReloadRequest> {
        if matches!(
            self.options.source,
            DiffSource::Patch(_) | DiffSource::Show(_) | DiffSource::Difftool { .. }
        ) {
            return None;
        }

        let (file, _) = self.focused_hunk_for_viewport(self.viewport_rows)?;
        editor_reload_request_for_file(self.changeset.files.get(file)?)
    }

    pub(crate) fn focused_hunk_editor_line(&self, file: usize, hunk: usize) -> Option<usize> {
        let rendered_rows = self.rendered_diff_rows_for_viewport(self.viewport_rows);
        find_rendered_diff_row_outward(
            &rendered_rows,
            self.rendered_viewport_focus_row(self.viewport_rows),
            |rendered_row| self.editor_line_at_hunk_row(rendered_row.model_row, file, hunk),
        )
    }

    pub(crate) fn editor_line_at_hunk_row(
        &self,
        row_index: usize,
        file: usize,
        hunk: usize,
    ) -> Option<usize> {
        let hunk_diff = self.changeset.files.get(file)?.hunks.get(hunk)?;
        match self.model.row(row_index)? {
            UiRow::UnifiedLine {
                file: row_file,
                hunk: row_hunk,
                line,
            }
            | UiRow::MetaLine {
                file: row_file,
                hunk: row_hunk,
                line,
            } if row_file == file && row_hunk == hunk => {
                hunk_diff.lines.get(line)?.new_line.map(|line| line.max(1))
            }
            UiRow::SplitLine {
                file: row_file,
                hunk: row_hunk,
                left,
                right,
            } if row_file == file && row_hunk == hunk => right
                .or(left)
                .and_then(|line| hunk_diff.lines.get(line))
                .and_then(|line| line.new_line)
                .map(|line| line.max(1)),
            _ => None,
        }
    }

    pub(crate) fn open_focused_hunk_in_editor(&mut self) {
        if let Some(editor) = self.prepare_focused_hunk_editor() {
            self.open_prepared_hunk_in_editor(editor, None);
        }
    }

    fn prepare_focused_hunk_editor(&mut self) -> Option<FocusedEditorLaunch> {
        self.prepare_focused_hunk_editor_with(configured_editor())
    }

    fn prepare_focused_hunk_editor_with(
        &mut self,
        configured_editor: Option<String>,
    ) -> Option<FocusedEditorLaunch> {
        let Some(target) = self.focused_hunk_editor_target() else {
            self.set_notice("no editable focused hunk");
            return None;
        };
        let Some(editor) = configured_editor else {
            self.set_notice("set $VISUAL, $GIT_EDITOR, or $EDITOR to edit focused hunk");
            return None;
        };
        Some(FocusedEditorLaunch { target, editor })
    }

    #[cfg(test)]
    pub(crate) fn prepare_focused_hunk_editor_for_test(
        &mut self,
        configured_editor: Option<String>,
    ) -> bool {
        self.prepare_focused_hunk_editor_with(configured_editor)
            .is_some()
    }

    fn open_prepared_hunk_in_editor(
        &mut self,
        editor: FocusedEditorLaunch,
        mut live_diff: Option<&mut Option<LiveDiff>>,
    ) {
        let FocusedEditorLaunch { target, editor } = editor;
        self.diff_menu_open = false;
        self.diff_menu_input.clear();
        self.diff_menu_input_cursor = 0;
        self.rendered_diff_menu_area = None;
        self.options_menu_open = false;
        self.close_color_scheme_picker();
        self.close_review_input();
        self.close_branch_menu();
        self.terminal_clear_requested = true;
        let mut paused_live_diff = false;
        if matches!(self.options.source, DiffSource::Worktree)
            && let Some(live_diff) = live_diff.as_mut().and_then(|live_diff| live_diff.as_mut())
        {
            live_diff.set_paused(true);
            paused_live_diff = true;
        }
        let scoped_reload = self.focused_hunk_editor_reload_request().or_else(|| {
            repo_relative_path(&self.changeset.repo, &target.path).map(|path| {
                let pathspecs = vec![path.clone()];
                EditorReloadRequest { path, pathspecs }
            })
        });
        let before = FileFingerprint::read(&target.path);
        let status_result = open_editor(&editor, &target);
        self.post_editor_quit_key_ignore_until = Some(Instant::now() + POST_EDITOR_QUIT_KEY_IGNORE);
        if paused_live_diff
            && let Some(live_diff) = live_diff.as_mut().and_then(|live_diff| live_diff.as_mut())
        {
            live_diff.set_paused(false);
        }

        match status_result {
            Ok(status) if status.success() => {
                let changed = file_changed_since(&target.path, before);
                match self.editor_reload_behavior(
                    changed,
                    scoped_reload.as_ref().map(|request| request.path.as_path()),
                ) {
                    EditorReloadBehavior::None => self.set_notice("editor closed"),
                    EditorReloadBehavior::ScopedAsync => {
                        let request = scoped_reload.expect("scoped reload requires a request");
                        self.queue_editor_scoped_reload(request);
                        self.set_notice("editor closed; refreshing edited file");
                    }
                    EditorReloadBehavior::Sync => match self.reload() {
                        Ok(()) => self.set_notice("editor closed; reloading"),
                        Err(error) => {
                            self.set_error_log(format!("editor closed; reload failed: {error}"))
                        }
                    },
                }
            }
            Ok(status) => {
                self.set_notice(format!("editor exited with {status}"));
            }
            Err(error) => self.set_error_log(format!("editor failed: {error}")),
        }
    }

    pub(crate) fn editor_reload_behavior(
        &self,
        target_changed: bool,
        scoped_path: Option<&Path>,
    ) -> EditorReloadBehavior {
        if !target_changed
            || !matches!(
                self.options.source,
                DiffSource::Worktree | DiffSource::Base(_)
            )
        {
            return EditorReloadBehavior::None;
        }

        if scoped_path.is_some() {
            return EditorReloadBehavior::ScopedAsync;
        }

        EditorReloadBehavior::Sync
    }

    pub(crate) fn start_editor_scoped_reload(&mut self, request: EditorReloadRequest) {
        let options = self.options.clone();
        let path = request.path;
        let pathspecs = request.pathspecs;
        let (tx, rx) = oneshot::channel();
        runtime::spawn_detached_blocking(move || {
            let changeset = mark_diff::load_review_ref_paths(&options, &pathspecs);
            let _ = tx.send(EditorScopedReload { path, changeset });
        });
        self.editor_reload = Some(EditorReloadWorker {
            generation: self.generation,
            rx,
        });
    }

    pub(crate) fn queue_editor_scoped_reload(&mut self, request: EditorReloadRequest) {
        self.pending_editor_reload = Some(request);
        self.dirty = true;
    }

    pub(crate) fn start_pending_editor_reload(&mut self) {
        let Some(request) = self.pending_editor_reload.take() else {
            return;
        };

        self.start_editor_scoped_reload(request);
    }

    pub(crate) fn drain_editor_reload(&mut self) -> bool {
        let Some(mut worker) = self.editor_reload.take() else {
            return false;
        };

        match worker.rx.try_recv() {
            Ok(reload) => {
                if worker.generation != self.generation {
                    return false;
                }

                match reload.changeset {
                    Ok(changeset) => {
                        self.replace_path_changeset(&reload.path, changeset);
                        self.set_notice("edited file reloaded");
                    }
                    Err(error) => self.set_error_log(format!("edited file reload failed: {error}")),
                }
                true
            }
            Err(oneshot::error::TryRecvError::Empty) => {
                self.editor_reload = Some(worker);
                false
            }
            Err(oneshot::error::TryRecvError::Closed) => {
                self.set_error_log("edited file reload failed");
                true
            }
        }
    }

    pub(crate) fn viewport_focus_row(&self) -> usize {
        if self.line_wrapping {
            let row_count = self.wrapped_visual_row_count();
            let focus_scroll = self.scroll.saturating_add(viewport_focus_offset(
                self.scroll,
                row_count,
                self.viewport_rows,
            ));
            return self
                .model_row_at_scroll(focus_scroll)
                .map(|(row, _)| row)
                .unwrap_or_else(|| self.model.len().saturating_sub(1));
        }

        self.scroll
            .saturating_add(viewport_focus_offset(
                self.scroll,
                self.model.len(),
                self.viewport_rows,
            ))
            .min(self.model.len().saturating_sub(1))
    }

    pub(crate) fn set_viewport_rows(&mut self, rows: usize) {
        let rows = rows.max(1);
        let previous_rows = self.viewport_rows;
        if previous_rows == rows {
            return;
        }

        let centered_grep_match_row = self.selected_grep_match_row().filter(|row| {
            let previous_centered_scroll = row
                .saturating_sub(viewport_center_offset(previous_rows))
                .min(max_scroll_for_viewport(self.model.len(), previous_rows));
            self.scroll == previous_centered_scroll
        });

        self.viewport_rows = rows;
        if let Some(row) = centered_grep_match_row {
            self.set_scroll_centered_on(row);
        } else {
            self.set_scroll(self.scroll);
        }
        self.clamp_file_sidebar_scroll(self.visible_file_sidebar_rows());
        self.ensure_annotation_draft_visible();
    }

    pub(crate) fn set_viewport_width(&mut self, width: usize) {
        let width = width.max(1);
        if self.viewport_width == width {
            return;
        }

        let wrapped_position = self
            .line_wrapping
            .then(|| self.model_row_at_scroll(self.scroll))
            .flatten();
        self.viewport_width = width;
        self.invalidate_wrapped_visual_layout();
        self.set_horizontal_scroll(self.horizontal_scroll);
        if let Some((row, row_offset)) = wrapped_position {
            let row_scroll = self.wrapped_visual_scroll_for_model_row(row);
            let row_height = self.wrapped_visual_height_for_model_row(row);
            self.set_scroll(
                row_scroll.saturating_add(row_offset.min(row_height.saturating_sub(1))),
            );
        } else {
            self.set_scroll(self.scroll);
        }
        self.ensure_annotation_draft_visible();
    }

    pub(crate) fn scroll_file_sidebar_by(&mut self, delta: isize) {
        let next = if delta < 0 {
            self.file_sidebar_scroll
                .saturating_sub(delta.unsigned_abs())
        } else {
            self.file_sidebar_scroll.saturating_add(delta as usize)
        };
        self.set_file_sidebar_scroll(next);
    }

    pub(crate) fn set_file_sidebar_scroll(&mut self, scroll: usize) {
        let previous_scroll = self.file_sidebar_scroll;
        self.file_sidebar_scroll =
            scroll.min(self.max_file_sidebar_scroll(self.visible_file_sidebar_rows()));
        if self.file_sidebar_scroll != previous_scroll {
            self.dirty = true;
        }
    }

    pub(crate) fn set_file_sidebar_width(&mut self, width: u16) {
        let total_width = self
            .file_sidebar_render_width
            .saturating_add(self.viewport_width.min(usize::from(u16::MAX)) as u16);
        let max_width = max_file_sidebar_width(total_width);
        if max_width == 0 {
            return;
        }

        let width = width.clamp(FILE_SIDEBAR_MIN_WIDTH, max_width);
        if self.file_sidebar_width != Some(width) {
            self.file_sidebar_width = Some(width);
            self.set_horizontal_scroll(self.horizontal_scroll);
            self.dirty = true;
        }
    }

    pub(crate) fn clamp_file_sidebar_scroll(&mut self, visible_rows: usize) {
        self.file_sidebar_scroll = self
            .file_sidebar_scroll
            .min(self.max_file_sidebar_scroll(visible_rows));
    }

    pub(crate) fn prepare_syntax_for_viewport(&mut self, visible_rows: usize) {
        if visible_rows == 0 || self.syntax.is_none() {
            return;
        }
        let mut requested = HashSet::new();
        let mut requested_files = HashSet::new();

        let Some(visible_range) = self.visible_model_range_for_viewport(visible_rows) else {
            return;
        };
        let visible_start = visible_range.start;
        let visible_end = visible_range.end;
        self.prepare_syntax_for_range(
            visible_start,
            visible_end,
            SyntaxPriority::Visible,
            &mut requested,
            &mut requested_files,
        );

        if self.syntax_prefetch_paused() {
            return;
        }

        let prefetch_rows = visible_rows.saturating_mul(self.syntax_limits.prefetch_viewports);
        let ahead_end = visible_end
            .saturating_add(prefetch_rows)
            .min(self.model.len());
        self.prepare_syntax_for_range(
            visible_end,
            ahead_end,
            SyntaxPriority::Prefetch,
            &mut requested,
            &mut requested_files,
        );

        let behind_start = visible_start.saturating_sub(prefetch_rows);
        self.prepare_syntax_for_range(
            behind_start,
            visible_start,
            SyntaxPriority::Prefetch,
            &mut requested,
            &mut requested_files,
        );
    }

    pub(crate) fn prepare_syntax_for_range(
        &mut self,
        start: usize,
        end: usize,
        priority: SyntaxPriority,
        requested: &mut HashSet<SyntaxPosition>,
        requested_files: &mut HashSet<ContextSourceKey>,
    ) {
        for row_index in start..end {
            let Some(row) = self.model.row(row_index) else {
                continue;
            };
            self.prepare_syntax_for_row(row, priority, requested, requested_files);
        }
    }

    pub(crate) fn prepare_syntax_for_row(
        &mut self,
        row: UiRow,
        priority: SyntaxPriority,
        requested: &mut HashSet<SyntaxPosition>,
        requested_files: &mut HashSet<ContextSourceKey>,
    ) {
        match row {
            UiRow::FileSeparator => {}
            UiRow::UnifiedLine { file, hunk, line } => {
                let Some(diff_line) = self
                    .changeset
                    .files
                    .get(file)
                    .and_then(|file_diff| file_diff.hunks.get(hunk))
                    .and_then(|hunk_diff| hunk_diff.lines.get(line))
                else {
                    return;
                };
                if let Some(side) = unified_syntax_side(diff_line.kind) {
                    self.queue_syntax_hunk(file, hunk, side, priority, requested);
                }
            }
            UiRow::SplitLine {
                file,
                hunk,
                left,
                right,
            } => {
                if left.is_some() {
                    self.queue_syntax_hunk(file, hunk, DiffSide::Old, priority, requested);
                }
                if right.is_some() {
                    self.queue_syntax_hunk(file, hunk, DiffSide::New, priority, requested);
                }
            }
            UiRow::ContextLine { file, .. } => {
                if let Some(side) = self.context_source_side(file) {
                    self.queue_syntax_file(file, side, priority, requested_files);
                }
            }
            UiRow::FileHeader(_)
            | UiRow::BinaryFile(_)
            | UiRow::Collapsed { .. }
            | UiRow::ContextHide { .. }
            | UiRow::HunkHeader { .. }
            | UiRow::MetaLine { .. } => {}
        }
    }

    pub(crate) fn queue_syntax_hunk(
        &mut self,
        file: usize,
        hunk: usize,
        side: DiffSide,
        priority: SyntaxPriority,
        requested: &mut HashSet<SyntaxPosition>,
    ) {
        let position = SyntaxPosition {
            generation: self.generation,
            file,
            hunk,
            side,
        };
        if !requested.insert(position) {
            return;
        }
        if let Some(syntax) = self.syntax.as_mut() {
            syntax.queue_hunk(&self.options, &self.changeset, position, priority);
        }
    }

    pub(crate) fn queue_syntax_file(
        &mut self,
        file: usize,
        side: DiffSide,
        priority: SyntaxPriority,
        requested: &mut HashSet<ContextSourceKey>,
    ) {
        if !requested.insert(ContextSourceKey { file, side }) {
            return;
        }
        if let Some(syntax) = self.syntax.as_mut() {
            syntax.queue_full_file(
                &self.options,
                &self.changeset,
                self.generation,
                file,
                side,
                priority,
            );
        }
    }

    pub(crate) fn drain_syntax(&mut self) {
        if let Some(syntax) = self.syntax.as_mut()
            && syntax.drain(self.generation, MAX_SYNTAX_RESULTS_PER_FRAME)
        {
            self.dirty = true;
        }
    }

    pub(crate) fn syntax_stats(&self) -> SyntaxBenchmarkReport {
        self.syntax
            .as_ref()
            .map(SyntaxRuntime::stats)
            .unwrap_or_default()
    }

    pub(crate) fn syntax_prefetch_paused(&self) -> bool {
        self.filter_input.is_some()
    }

    pub(crate) fn open_filter_input(&mut self, kind: DiffFilterKind) {
        self.filter_input = Some(kind);
        self.clear_diff_mouse_hover();
        self.diff_menu_open = false;
        self.diff_menu_input.clear();
        self.diff_menu_input_cursor = 0;
        self.rendered_diff_menu_area = None;
        self.options_menu_open = false;
        self.close_color_scheme_picker();
        self.close_review_input();
        self.close_branch_menu();

        let had_filter =
            !self.filter_query(kind).is_empty() || !self.filter_input_query(kind).is_empty();
        self.filter_query_mut(kind).clear();
        self.filter_input_query_mut(kind).clear();
        *self.filter_input_cursor_mut(kind) = 0;
        if had_filter {
            self.schedule_filter_change(kind, Duration::ZERO);
        } else {
            self.dirty = true;
        }
    }

    pub(crate) fn handle_filter_input_key(&mut self, key: KeyEvent) -> bool {
        let Some(kind) = self.filter_input else {
            return false;
        };

        match key.code {
            KeyCode::Esc => {
                self.clear_all_filters();
                self.filter_input = None;
            }
            KeyCode::Enter => {
                self.commit_filter_input(kind);
                self.filter_input = None;
            }
            _ => match self.apply_filter_input_key(kind, key) {
                TextInputKeyResult::Edited => self.sync_filter_input(kind),
                TextInputKeyResult::Moved => self.dirty = true,
                TextInputKeyResult::Ignored | TextInputKeyResult::Handled => {}
            },
        }

        true
    }

    pub(crate) fn filter_query(&self, kind: DiffFilterKind) -> &str {
        match kind {
            DiffFilterKind::File => &self.file_filter,
            DiffFilterKind::Grep => &self.grep_filter,
        }
    }

    pub(crate) fn filter_query_mut(&mut self, kind: DiffFilterKind) -> &mut String {
        match kind {
            DiffFilterKind::File => &mut self.file_filter,
            DiffFilterKind::Grep => &mut self.grep_filter,
        }
    }

    pub(crate) fn filter_input_query(&self, kind: DiffFilterKind) -> &str {
        match kind {
            DiffFilterKind::File => &self.file_filter_input,
            DiffFilterKind::Grep => &self.grep_filter_input,
        }
    }

    pub(crate) fn filter_input_query_mut(&mut self, kind: DiffFilterKind) -> &mut String {
        match kind {
            DiffFilterKind::File => &mut self.file_filter_input,
            DiffFilterKind::Grep => &mut self.grep_filter_input,
        }
    }

    pub(crate) fn filter_input_cursor(&self, kind: DiffFilterKind) -> usize {
        match kind {
            DiffFilterKind::File => self.file_filter_input_cursor,
            DiffFilterKind::Grep => self.grep_filter_input_cursor,
        }
    }

    pub(crate) fn filter_input_cursor_mut(&mut self, kind: DiffFilterKind) -> &mut usize {
        match kind {
            DiffFilterKind::File => &mut self.file_filter_input_cursor,
            DiffFilterKind::Grep => &mut self.grep_filter_input_cursor,
        }
    }

    fn apply_filter_input_key(
        &mut self,
        kind: DiffFilterKind,
        key: KeyEvent,
    ) -> TextInputKeyResult {
        match kind {
            DiffFilterKind::File => handle_text_input_key(
                &mut self.file_filter_input,
                &mut self.file_filter_input_cursor,
                key,
            ),
            DiffFilterKind::Grep => handle_text_input_key(
                &mut self.grep_filter_input,
                &mut self.grep_filter_input_cursor,
                key,
            ),
        }
    }

    pub(crate) fn commit_filter_input(&mut self, kind: DiffFilterKind) {
        let next = self.filter_input_query(kind).to_owned();
        if self.filter_query(kind) == next {
            if self.pending_filter_apply.is_some() {
                self.schedule_filter_change(kind, Duration::ZERO);
            }
            self.dirty = true;
            return;
        }

        *self.filter_query_mut(kind) = next;
        self.schedule_filter_change(kind, Duration::ZERO);
    }

    pub(crate) fn sync_filter_input(&mut self, kind: DiffFilterKind) {
        let next = self.filter_input_query(kind).to_owned();
        if self.filter_query(kind) == next {
            self.dirty = true;
            return;
        }

        *self.filter_query_mut(kind) = next;
        self.schedule_filter_change(kind, FILTER_DEBOUNCE);
    }

    pub(crate) fn clear_all_filters(&mut self) {
        self.grep_matches.clear();
        self.grep_matches_truncated = false;
        self.selected_grep_match = None;

        if self.file_filter.is_empty() && self.grep_filter.is_empty() {
            self.file_filter_input.clear();
            self.file_filter_input_cursor = 0;
            self.grep_filter_input.clear();
            self.grep_filter_input_cursor = 0;
            self.dirty = true;
            return;
        }

        self.file_filter.clear();
        self.file_filter_input.clear();
        self.file_filter_input_cursor = 0;
        self.grep_filter.clear();
        self.grep_filter_input.clear();
        self.grep_filter_input_cursor = 0;
        self.schedule_filter_apply(Duration::ZERO, false);
    }

    pub(crate) fn apply_filters(&mut self, jump_to_grep: bool) {
        self.pending_filter_apply = None;
        self.filter_worker = None;
        self.filter_searching = false;
        let selected_path = self
            .changeset
            .files
            .get(self.selected_file)
            .map(|file| file.display_path().to_owned());
        let relative_scroll = self.relative_scroll_from_file_start(self.selected_file);

        let search_result = self.search_index.search_with_grep_match_limit(
            &self.file_filter,
            &self.grep_filter,
            MAX_LIVE_GREP_MATCHES,
        );
        self.replace_visible_files(
            search_result,
            selected_path,
            relative_scroll,
            jump_to_grep,
            HunkFocusModelBehavior::PreserveIfValid,
        );
    }

    pub(crate) fn schedule_filter_change(&mut self, kind: DiffFilterKind, debounce: Duration) {
        self.schedule_filter_apply(
            debounce,
            kind == DiffFilterKind::Grep && !self.grep_filter.is_empty(),
        );
    }

    pub(crate) fn schedule_filter_apply(&mut self, debounce: Duration, jump_to_grep: bool) {
        #[cfg(test)]
        {
            let _ = debounce;
            self.apply_filters(jump_to_grep);
        }

        #[cfg(not(test))]
        {
            self.filter_generation = self.filter_generation.wrapping_add(1);
            self.pending_filter_apply = Some(PendingFilterApply {
                generation: self.filter_generation,
                due_at: Instant::now() + debounce,
                jump_to_grep,
            });
            self.filter_worker = None;
            self.filter_searching = true;
            self.dirty = true;
        }
    }

    pub(crate) fn start_due_filter_apply(&mut self) {
        let Some(pending) = self.pending_filter_apply else {
            return;
        };
        if Instant::now() < pending.due_at {
            return;
        }

        self.pending_filter_apply = None;
        let generation = pending.generation;
        let jump_to_grep = pending.jump_to_grep;
        let file_filter = self.file_filter.clone();
        let grep_filter = self.grep_filter.clone();
        let worker_file_filter = file_filter.clone();
        let worker_grep_filter = grep_filter.clone();
        let search_index = Arc::clone(&self.search_index);
        let (tx, rx) = oneshot::channel();
        runtime::spawn_detached_blocking(move || {
            let result = search_index.search_with_grep_match_limit(
                &worker_file_filter,
                &worker_grep_filter,
                MAX_LIVE_GREP_MATCHES,
            );
            let _ = tx.send(result);
        });

        self.filter_worker = Some(FilterWorker {
            generation,
            file_filter,
            grep_filter,
            jump_to_grep,
            rx,
        });
        self.filter_searching = true;
        self.dirty = true;
    }

    pub(crate) fn drain_filter_worker(&mut self) {
        let Some(outcome) =
            self.filter_worker
                .as_mut()
                .and_then(|worker| match worker.rx.try_recv() {
                    Ok(result) => Some(Some(result)),
                    Err(oneshot::error::TryRecvError::Empty) => None,
                    Err(oneshot::error::TryRecvError::Closed) => Some(None),
                })
        else {
            return;
        };

        let Some(worker) = self.filter_worker.take() else {
            return;
        };

        if worker.generation != self.filter_generation
            || worker.file_filter != self.file_filter
            || worker.grep_filter != self.grep_filter
        {
            return;
        }

        self.filter_searching = false;
        match outcome {
            Some(result) => self.apply_filter_result(result, worker.jump_to_grep),
            None => self.set_error_log("filter worker stopped"),
        }
    }

    pub(crate) fn filter_busy(&self) -> bool {
        self.filter_searching || self.pending_filter_apply.is_some() || self.filter_worker.is_some()
    }

    fn apply_filter_result(&mut self, search_result: DiffSearchResult, jump_to_grep: bool) {
        let selected_path = self
            .changeset
            .files
            .get(self.selected_file)
            .map(|file| file.display_path().to_owned());
        let relative_scroll = self.relative_scroll_from_file_start(self.selected_file);

        self.replace_visible_files(
            search_result,
            selected_path,
            relative_scroll,
            jump_to_grep,
            HunkFocusModelBehavior::PreserveIfValid,
        );
    }

    fn replace_visible_files(
        &mut self,
        search_result: DiffSearchResult,
        selected_path: Option<String>,
        relative_scroll: usize,
        jump_to_grep: bool,
        hunk_focus_behavior: HunkFocusModelBehavior,
    ) {
        let DiffSearchResult {
            visible_files,
            grep_matches,
            grep_matches_truncated,
        } = search_result;

        let selected_file = selected_path
            .and_then(|path| {
                self.changeset
                    .files
                    .iter()
                    .position(|file| file.display_path() == path)
            })
            .filter(|file| visible_files.contains(file))
            .or_else(|| visible_files.first().copied())
            .unwrap_or(0);

        self.stats = diff_stats_for_files(&self.changeset, &visible_files);
        self.max_line_width = self.search_index.max_line_width_for_files(&visible_files);
        self.replace_model(&visible_files, hunk_focus_behavior);
        self.selected_file = selected_file;
        self.grep_matches = grep_match_rows(&self.model, &grep_matches);
        self.grep_matches_truncated = grep_matches_truncated;
        self.selected_grep_match = None;

        let scroll = self
            .model
            .file_start_row(self.selected_file)
            .map(|start| {
                self.scroll_for_model_row(start)
                    .saturating_add(relative_scroll)
            })
            .unwrap_or_default();
        let scroll_behavior = match hunk_focus_behavior {
            HunkFocusModelBehavior::PreserveIfValid => HunkFocusScrollBehavior::Preserve,
            HunkFocusModelBehavior::Clear => HunkFocusScrollBehavior::ClearOnScroll,
        };
        self.set_scroll_with_grep_sync(scroll, true, scroll_behavior);
        self.set_horizontal_scroll(self.horizontal_scroll);
        self.ensure_file_sidebar_selection_visible(self.visible_file_sidebar_rows());

        if jump_to_grep && !self.grep_matches.is_empty() {
            self.selected_grep_match = Some(0);
            self.set_scroll_centered_on(self.grep_matches[0]);
        } else {
            self.sync_grep_match_selection_to_scroll();
        }

        self.ensure_annotation_draft_visible();
        self.dirty = true;
    }

    pub(crate) fn filters_active(&self) -> bool {
        !self.file_filter.is_empty() || !self.grep_filter.is_empty()
    }

    #[cfg(test)]
    pub(crate) fn current_grep_match_row(&self) -> Option<usize> {
        self.selected_grep_match_row()
    }

    fn selected_grep_match_row(&self) -> Option<usize> {
        if self.grep_filter.is_empty() {
            return None;
        }

        self.selected_grep_match
            .and_then(|index| self.grep_matches.get(index).copied())
    }

    pub(crate) fn sync_grep_match_selection_to_scroll(&mut self) {
        if self.grep_filter.is_empty() || self.grep_matches.is_empty() {
            self.selected_grep_match = None;
            return;
        }

        self.selected_grep_match = self
            .grep_matches
            .iter()
            .position(|row| self.grep_match_is_visible_or_below_scroll(*row))
            .or_else(|| self.grep_matches.len().checked_sub(1));
    }

    pub(crate) fn move_grep_match(&mut self, delta: isize) {
        if self.grep_filter.is_empty() {
            self.selected_grep_match = None;
            return;
        }

        if self.grep_matches.is_empty() {
            self.selected_grep_match = None;
            self.set_notice("no grep matches");
            return;
        }

        let len = self.grep_matches.len();
        let current = self.selected_grep_match.unwrap_or_else(|| {
            self.grep_matches
                .iter()
                .position(|row| self.grep_match_is_visible_or_below_scroll(*row))
                .unwrap_or(0)
        });
        let next = if delta < 0 {
            current
                .saturating_add(len)
                .saturating_sub(delta.unsigned_abs() % len)
                % len
        } else {
            current.saturating_add(delta as usize) % len
        };

        self.selected_grep_match = Some(next);
        self.set_scroll_for_grep_navigation(self.grep_matches[next]);
        self.dirty = true;
    }

    fn grep_match_is_visible_or_below_scroll(&self, row: usize) -> bool {
        let scroll = self.scroll_for_model_row(row);
        if !self.line_wrapping {
            return scroll >= self.scroll;
        }

        let height = self.wrapped_visual_height_for_model_row(row);
        scroll.saturating_add(height) > self.scroll
    }

    pub(crate) fn set_scroll_for_grep_navigation(&mut self, row: usize) {
        self.set_scroll_centered_on(row);
    }

    pub(crate) fn syntax_line(
        &mut self,
        file: usize,
        hunk: usize,
        line: usize,
        side: DiffSide,
    ) -> Option<HighlightedLine> {
        self.syntax.as_mut().and_then(|syntax| {
            syntax.line(
                SyntaxPosition {
                    generation: self.generation,
                    file,
                    hunk,
                    side,
                },
                line,
            )
        })
    }

    pub(crate) fn syntax_file_line(
        &mut self,
        file: usize,
        side: DiffSide,
        line_number: usize,
    ) -> Option<HighlightedLine> {
        self.syntax
            .as_mut()
            .and_then(|syntax| syntax.full_file_line(self.generation, file, side, line_number))
    }

    pub(crate) fn inline_ranges(
        &mut self,
        file: usize,
        hunk: usize,
        line: usize,
    ) -> Vec<InlineRange> {
        let key = InlineHunkKey {
            generation: self.generation,
            file,
            hunk,
        };
        if !self.inline_cache.contains_key(&key) {
            let cache = self
                .changeset
                .files
                .get(file)
                .and_then(|file_diff| file_diff.hunks.get(hunk))
                .map(|hunk_diff| InlineHunkEmphasisCache::new(&hunk_diff.lines))
                .unwrap_or_else(|| InlineHunkEmphasisCache::new(&[]));
            self.inline_cache.insert(key, cache);
        }

        let Some(lines) = self
            .changeset
            .files
            .get(file)
            .and_then(|file_diff| file_diff.hunks.get(hunk))
            .map(|hunk_diff| hunk_diff.lines.as_slice())
        else {
            return Vec::new();
        };

        self.inline_cache
            .get_mut(&key)
            .map(|hunk_emphasis| hunk_emphasis.ranges_for_line(lines, line))
            .unwrap_or_default()
    }

    pub(crate) fn move_file(&mut self, delta: isize) {
        let visible_files = self.model.visible_files();
        if visible_files.is_empty() {
            return;
        }

        let current = self
            .model
            .visible_file_position(self.selected_file)
            .unwrap_or_default();
        let next = if delta < 0 {
            current.saturating_sub(delta.unsigned_abs())
        } else {
            current.saturating_add(delta as usize)
        }
        .min(visible_files.len() - 1);

        self.select_file(visible_files[next]);
    }

    pub(crate) fn select_file(&mut self, file: usize) {
        if self.model.visible_files().is_empty() {
            return;
        }

        let next = if self.model.file_start_row(file).is_some() {
            file
        } else {
            self.model
                .visible_files()
                .first()
                .copied()
                .unwrap_or_default()
        };

        if next == self.selected_file {
            self.ensure_file_sidebar_selection_visible(self.visible_file_sidebar_rows());
            self.dirty = true;
            return;
        }

        if let Some(row) = self.model.hunk_start_row(next, 0) {
            self.focus_hunk_row(row);
            return;
        }

        self.selected_file = next;
        if let Some(row) = self.model.file_start_row(next) {
            self.set_scroll(self.scroll_for_model_row(row));
        } else {
            self.dirty = true;
        }
        self.ensure_file_sidebar_selection_visible(self.visible_file_sidebar_rows());
    }

    pub(crate) fn toggle_file_sidebar(&mut self) {
        self.file_sidebar_open = !self.file_sidebar_open;
        self.file_sidebar_resizing = false;
        self.diff_menu_open = false;
        self.diff_menu_input.clear();
        self.diff_menu_input_cursor = 0;
        self.rendered_diff_menu_area = None;
        self.options_menu_open = false;
        self.close_color_scheme_picker();
        self.close_review_input();
        self.close_branch_menu();
        self.ensure_file_sidebar_selection_visible(self.visible_file_sidebar_rows());
        self.dirty = true;
    }

    pub(crate) fn visible_file_sidebar_rows(&self) -> usize {
        self.viewport_rows
    }

    pub(crate) fn ensure_file_sidebar_selection_visible(&mut self, visible_rows: usize) {
        let Some(selected_position) = self.model.visible_file_position(self.selected_file) else {
            self.file_sidebar_scroll = 0;
            return;
        };
        if visible_rows == 0 {
            self.file_sidebar_scroll = 0;
            return;
        }

        if selected_position < self.file_sidebar_scroll {
            self.file_sidebar_scroll = selected_position;
        } else if selected_position >= self.file_sidebar_scroll.saturating_add(visible_rows) {
            self.file_sidebar_scroll = self
                .model
                .visible_file_position(self.selected_file)
                .unwrap_or_default()
                .saturating_add(1)
                .saturating_sub(visible_rows);
        }

        self.file_sidebar_scroll = self
            .file_sidebar_scroll
            .min(self.max_file_sidebar_scroll(visible_rows));
    }

    pub(crate) fn max_file_sidebar_scroll(&self, visible_rows: usize) -> usize {
        self.model
            .visible_files()
            .len()
            .saturating_sub(visible_rows.max(1))
    }

    pub(crate) fn next_hunk(&mut self) {
        if let Some(row) = self.model.next_hunk_row(self.hunk_navigation_anchor_row()) {
            self.focus_hunk_row(row);
        }
    }

    pub(crate) fn previous_hunk(&mut self) {
        if let Some(row) = self
            .model
            .previous_hunk_row(self.hunk_navigation_anchor_row())
        {
            self.focus_hunk_row(row);
        }
    }

    pub(crate) fn move_focused_hunk(&mut self, delta: isize) {
        let anchor = self.hunk_navigation_anchor_row();
        let next = if delta < 0 {
            self.model.previous_hunk_row(anchor)
        } else {
            self.model.next_hunk_row(anchor)
        };
        if let Some(row) = next {
            self.focus_hunk_row(row);
        }
    }

    pub(crate) fn hunk_navigation_anchor_row(&self) -> usize {
        if let Some((file, hunk)) = self.focused_hunk_for_viewport(self.viewport_rows)
            && let Some(row) = self.model.hunk_start_row(file, hunk)
        {
            return row;
        }

        self.viewport_focus_row()
    }

    pub(crate) fn focus_hunk_row(&mut self, row: usize) {
        let target_hunk = self.model.row(row).and_then(|row| row.hunk_key());
        let previous_hunk = self.manual_hunk_focus;
        self.clear_manual_hunk_focus();

        let Some((file, hunk)) = target_hunk else {
            self.set_scroll_centered_on(row);
            return;
        };

        self.set_scroll_focused_on_hunk(file, hunk);

        if let Some(row) = self.model.hunk_start_row(file, hunk)
            && self.model_row_rendered_at_scroll(self.scroll, self.viewport_rows, row)
        {
            let previous_file = self.selected_file;
            self.manual_hunk_focus = Some((file, hunk));
            self.selected_file = file;
            self.ensure_file_sidebar_selection_visible(self.visible_file_sidebar_rows());
            if self.manual_hunk_focus != previous_hunk || self.selected_file != previous_file {
                self.dirty = true;
            }
        }
    }

    pub(crate) fn toggle_layout(&mut self) {
        let layout = self.layout.toggled();
        self.set_manual_layout(layout);
    }

    pub(crate) fn set_manual_layout(&mut self, layout: DiffLayoutMode) {
        self.layout_override = Some(layout);
        self.set_layout(layout);
    }

    pub(crate) fn set_layout_setting(&mut self, setting: LayoutSetting) {
        match layout_override_from_setting(setting) {
            Some(layout) => self.set_manual_layout(layout),
            None => {
                self.layout_override = None;
                self.set_layout(default_layout_for_width(
                    self.viewport_width.min(u16::MAX as usize) as u16,
                ));
            }
        }
    }

    pub(crate) fn apply_responsive_layout(&mut self, width: u16) {
        let horizontal_scroll = self.horizontal_scroll;
        self.set_viewport_width(width as usize);
        let responsive_layout = default_layout_for_width(width);
        let layout = self.layout_override.unwrap_or(responsive_layout);
        self.set_layout(layout);
        self.set_horizontal_scroll(horizontal_scroll);
        self.dirty = true;
    }

    pub(crate) fn set_layout(&mut self, layout: DiffLayoutMode) {
        if self.layout == layout {
            return;
        }

        self.layout = layout;
        let search_result = self.search_index.search_with_grep_match_limit(
            &self.file_filter,
            &self.grep_filter,
            MAX_LIVE_GREP_MATCHES,
        );
        self.replace_model(&search_result.visible_files, HunkFocusModelBehavior::Clear);
        self.grep_matches = grep_match_rows(&self.model, &search_result.grep_matches);
        self.grep_matches_truncated = search_result.grep_matches_truncated;
        self.selected_grep_match = None;
        self.set_horizontal_scroll(self.horizontal_scroll);
        let scroll = self
            .model
            .file_start_row(self.selected_file)
            .map(|row| self.scroll_for_model_row(row))
            .unwrap_or_default();
        self.set_scroll(scroll);
        self.sync_grep_match_selection_to_scroll();
        self.ensure_annotation_draft_visible();
        self.dirty = true;
    }

    pub(crate) fn reload(&mut self) -> MarkResult<()> {
        self.invalidate_diff_cache();
        self.start_uncached_diff_load(self.options.clone(), "reload failed");
        Ok(())
    }

    pub(crate) fn replace_changeset(&mut self, changeset: Changeset) {
        self.invalidate_diff_cache();
        self.cache_loaded_diff(self.options.clone(), changeset.clone());
        self.replace_loaded_diff(self.options.clone(), changeset);
    }

    pub(crate) fn replace_path_changeset(&mut self, path: &Path, path_changeset: Changeset) {
        self.invalidate_diff_cache();
        let selected_path = self
            .changeset
            .files
            .get(self.selected_file)
            .map(|file| file.display_path().to_owned());
        let relative_scroll = self.relative_scroll_from_file_start(self.selected_file);

        splice_diff_files_for_path(
            &mut self.changeset.files,
            path,
            path_changeset.files.clone(),
        );
        splice_diff_files_for_path(&mut self.base_changeset.files, path, path_changeset.files);
        self.total_stats = self.changeset.stats();
        self.context_expansions.clear();
        self.context_cache.clear();
        self.generation = self.generation.wrapping_add(1);
        self.inline_cache.clear();
        self.search_index = Arc::new(DiffSearchIndex::new(&self.changeset));
        self.pending_filter_apply = None;
        self.filter_worker = None;
        self.filter_searching = false;
        if let Some(syntax) = self.syntax.as_mut() {
            syntax.clear(self.generation);
        }
        let search_result = self.search_index.search_with_grep_match_limit(
            &self.file_filter,
            &self.grep_filter,
            MAX_LIVE_GREP_MATCHES,
        );
        self.replace_visible_files(
            search_result,
            selected_path,
            relative_scroll,
            false,
            HunkFocusModelBehavior::Clear,
        );
        self.store_current_diff_cache();
        self.dirty = true;
    }

    pub(crate) fn replace_cached_diff(
        &mut self,
        options: DiffOptions,
        cached: DiffCacheEntry,
        refresh_branch_metadata: bool,
    ) {
        let DiffCacheEntry {
            changeset,
            search_index,
            total_stats,
            max_line_width,
            unified_model,
            split_model,
            ..
        } = cached;
        let selected_path = self
            .changeset
            .files
            .get(self.selected_file)
            .map(|file| file.display_path().to_owned());
        let relative_scroll = self.relative_scroll_from_file_start(self.selected_file);

        let previous_branch_base = self.branch_base.clone();
        let previous_branch_head = self.branch_head.clone();
        let previous_repo = self.changeset.repo.clone();
        self.options = options;
        self.live_reload_invalidated = false;
        self.live_reload_pending = false;
        if !refresh_branch_metadata && previous_repo == changeset.repo {
            self.branch_base = branch_base_from_options(&self.options).or(previous_branch_base);
            self.branch_head =
                branch_head_from_options(&self.options, self.current_head.as_deref())
                    .or(previous_branch_head)
                    .or_else(|| self.current_head.clone());
            for branch in [
                self.current_head.clone(),
                self.branch_head.clone(),
                self.branch_base.clone(),
            ]
            .into_iter()
            .flatten()
            {
                if !self
                    .comparison_branches
                    .iter()
                    .any(|candidate| candidate == &branch)
                {
                    self.comparison_branches.push(branch);
                }
            }
        } else {
            self.current_head = current_head_label(&changeset.repo);
            self.branch_base = branch_base_from_options(&self.options)
                .or(previous_branch_base)
                .or_else(|| default_branch_base(&self.options, &changeset.repo));
            self.branch_head =
                branch_head_from_options(&self.options, self.current_head.as_deref())
                    .or(previous_branch_head)
                    .or_else(|| self.current_head.clone());
            self.comparison_branches = comparison_branches(
                &changeset.repo,
                &[
                    self.current_head.as_deref(),
                    self.branch_head.as_deref(),
                    self.branch_base.as_deref(),
                ],
            );
        }
        self.branch_menu_scroll = self.branch_menu_scroll.min(self.max_branch_menu_scroll());
        self.show_rev = show_rev_from_options(&self.options);
        self.comparison_commits =
            comparison_commits(&self.changeset.repo, self.show_rev.as_deref());
        self.commit_menu_scroll = self
            .commit_menu_scroll
            .min(self.max_commit_menu_scroll_for_rows(self.commit_menu_rows()));
        self.total_stats = total_stats;
        self.base_changeset = changeset.clone();
        self.changeset = changeset;
        self.search_index = search_index;
        self.context_expansions.clear();
        self.context_cache.clear();
        self.generation = self.generation.wrapping_add(1);
        self.inline_cache.clear();
        self.pending_filter_apply = None;
        self.filter_worker = None;
        self.filter_searching = false;
        if let Some(syntax) = self.syntax.as_mut() {
            syntax.clear(self.generation);
        }

        if self.filters_active() {
            let search_result = self.search_index.search_with_grep_match_limit(
                &self.file_filter,
                &self.grep_filter,
                MAX_LIVE_GREP_MATCHES,
            );
            self.replace_visible_files(
                search_result,
                selected_path,
                relative_scroll,
                false,
                HunkFocusModelBehavior::Clear,
            );
        } else {
            self.stats = self.total_stats.clone();
            self.max_line_width = max_line_width;
            self.model = match self.layout {
                DiffLayoutMode::Split => split_model,
                DiffLayoutMode::Unified => unified_model,
            };
            self.invalidate_wrapped_visual_layout();
            self.reanchor_annotation_draft();
            self.manual_hunk_focus = None;
            self.selected_file = selected_path
                .and_then(|path| {
                    self.changeset
                        .files
                        .iter()
                        .position(|file| file.display_path() == path)
                })
                .unwrap_or(0);
            self.grep_matches.clear();
            self.grep_matches_truncated = false;
            self.selected_grep_match = None;

            let scroll = self
                .model
                .file_start_row(self.selected_file)
                .map(|start| {
                    self.scroll_for_model_row(start)
                        .saturating_add(relative_scroll)
                })
                .unwrap_or_default();
            self.set_scroll_with_grep_sync(scroll, true, HunkFocusScrollBehavior::ClearOnScroll);
            self.set_horizontal_scroll(self.horizontal_scroll);
            self.ensure_file_sidebar_selection_visible(self.visible_file_sidebar_rows());
            self.ensure_annotation_draft_visible();
            self.dirty = true;
        }
    }

    pub(crate) fn replace_loaded_diff(&mut self, options: DiffOptions, changeset: Changeset) {
        let options_changed = self.options != options;
        if !options_changed && self.base_changeset == changeset {
            if self.live_reload_invalidated || self.live_reload_pending {
                self.live_reload_invalidated = false;
                self.live_reload_pending = false;
            }
            self.dirty = true;
            return;
        }

        let selected_path = self
            .changeset
            .files
            .get(self.selected_file)
            .map(|file| file.display_path().to_owned());
        let relative_scroll = self.relative_scroll_from_file_start(self.selected_file);

        let previous_branch_base = self.branch_base.clone();
        let previous_branch_head = self.branch_head.clone();
        self.options = options;
        self.live_reload_invalidated = false;
        self.live_reload_pending = false;
        self.current_head = current_head_label(&changeset.repo);
        self.branch_base = branch_base_from_options(&self.options)
            .or(previous_branch_base)
            .or_else(|| default_branch_base(&self.options, &changeset.repo));
        self.branch_head = branch_head_from_options(&self.options, self.current_head.as_deref())
            .or(previous_branch_head)
            .or_else(|| self.current_head.clone());
        self.comparison_branches = comparison_branches(
            &changeset.repo,
            &[
                self.current_head.as_deref(),
                self.branch_head.as_deref(),
                self.branch_base.as_deref(),
            ],
        );
        self.branch_menu_scroll = self.branch_menu_scroll.min(self.max_branch_menu_scroll());
        self.show_rev = show_rev_from_options(&self.options);
        self.comparison_commits = comparison_commits(&changeset.repo, self.show_rev.as_deref());
        self.commit_menu_scroll = self
            .commit_menu_scroll
            .min(self.max_commit_menu_scroll_for_rows(self.commit_menu_rows()));
        self.total_stats = changeset.stats();
        self.base_changeset = changeset.clone();
        self.changeset = changeset;
        self.search_index = Arc::new(DiffSearchIndex::new(&self.changeset));
        self.context_expansions.clear();
        self.context_cache.clear();
        self.generation = self.generation.wrapping_add(1);
        self.inline_cache.clear();
        self.pending_filter_apply = None;
        self.filter_worker = None;
        self.filter_searching = false;
        if let Some(syntax) = self.syntax.as_mut() {
            syntax.clear(self.generation);
        }
        let search_result = self.search_index.search_with_grep_match_limit(
            &self.file_filter,
            &self.grep_filter,
            MAX_LIVE_GREP_MATCHES,
        );
        self.replace_visible_files(
            search_result,
            selected_path,
            relative_scroll,
            false,
            HunkFocusModelBehavior::Clear,
        );
        self.dirty = true;
    }
}

pub(crate) fn max_scroll_for_viewport(row_count: usize, viewport_rows: usize) -> usize {
    row_count.saturating_sub(viewport_rows.max(1))
}

fn max_scroll_for_annotated_viewport(
    row_count: usize,
    viewport_rows: usize,
    mut annotation_blocks: Vec<(usize, usize)>,
) -> usize {
    if row_count == 0 {
        return 0;
    }

    annotation_blocks.retain(|(anchor, height)| *anchor < row_count && *height > 0);
    if annotation_blocks.is_empty() {
        return max_scroll_for_viewport(row_count, viewport_rows);
    }

    annotation_blocks.sort_unstable_by_key(|(anchor, _)| *anchor);
    let mut merged_blocks: Vec<(usize, usize)> = Vec::with_capacity(annotation_blocks.len());
    for (anchor, height) in annotation_blocks {
        if let Some((last_anchor, last_height)) = merged_blocks.last_mut()
            && *last_anchor == anchor
        {
            *last_height = last_height.saturating_add(height);
            continue;
        }
        merged_blocks.push((anchor, height));
    }

    let annotation_rows = merged_blocks
        .iter()
        .fold(0usize, |total, (_, height)| total.saturating_add(*height));
    let target_rendered_scroll = row_count
        .saturating_add(annotation_rows)
        .saturating_sub(viewport_rows.max(1));
    if target_rendered_scroll == 0 {
        return 0;
    }

    // `scroll` is expressed in diff visual rows, while annotations add rendered
    // rows after their anchors. Project the last rendered viewport start back to
    // the first diff visual row at or after that rendered position; if that
    // position lands inside an annotation, scrolling to the next diff row reveals
    // rows hidden by the annotation block. If there is no next diff row, fall back
    // to the final anchor so an oversized trailing annotation remains reachable.
    let mut annotation_rows_before = 0usize;
    let mut first_row_in_range = 0usize;
    for (anchor, height) in merged_blocks {
        let candidate = target_rendered_scroll.saturating_sub(annotation_rows_before);
        if candidate <= anchor {
            let projected_scroll = candidate.max(first_row_in_range).min(row_count - 1);
            return projected_scroll;
        }

        annotation_rows_before = annotation_rows_before.saturating_add(height);
        first_row_in_range = anchor.saturating_add(1).min(row_count);
    }

    if first_row_in_range < row_count {
        let projected_scroll = target_rendered_scroll
            .saturating_sub(annotation_rows_before)
            .max(first_row_in_range)
            .min(row_count - 1);
        return projected_scroll;
    }

    row_count - 1
}

fn annotation_scroll_for_block(
    anchor_visual_scroll: usize,
    block_height: usize,
    viewport_rows: usize,
) -> usize {
    anchor_visual_scroll
        .saturating_add(1)
        .saturating_add(block_height)
        .saturating_sub(viewport_rows.max(1))
        .min(anchor_visual_scroll)
}

pub(crate) fn viewport_center_offset(viewport_rows: usize) -> usize {
    viewport_rows.saturating_sub(1) / 2
}

pub(crate) fn viewport_focus_offset(
    scroll: usize,
    row_count: usize,
    viewport_rows: usize,
) -> usize {
    if row_count == 0 {
        return 0;
    }

    let viewport_rows = viewport_rows.max(1);
    let visible_rows = viewport_rows.min(row_count);
    let center = viewport_center_offset(visible_rows);
    if row_count <= viewport_rows {
        return center;
    }

    let bottom = visible_rows.saturating_sub(1);
    let max_scroll = max_scroll_for_viewport(row_count, viewport_rows);
    let scroll = scroll.min(max_scroll);
    let distance_to_end = max_scroll.saturating_sub(scroll);
    let top_ramp = scroll.min(center);
    let bottom_ramp = bottom.saturating_sub(distance_to_end);

    top_ramp.max(bottom_ramp).min(bottom)
}

fn hunk_focus_row_range(
    model: &UiModel,
    file: usize,
    hunk: usize,
) -> Option<(Range<usize>, usize)> {
    let mut range = model.hunk_row_range(file, hunk)?;
    let hunk_start = range.start;

    while range.start > 0
        && model
            .row(range.start - 1)
            .is_some_and(row_extends_hunk_focus_before)
    {
        range.start -= 1;
    }

    while range.end < model.len()
        && model
            .row(range.end)
            .is_some_and(row_extends_hunk_focus_after)
    {
        range.end += 1;
    }

    Some((range, hunk_start))
}

fn row_extends_hunk_focus_before(row: UiRow) -> bool {
    matches!(
        row,
        UiRow::FileHeader(_)
            | UiRow::Collapsed { .. }
            | UiRow::ContextLine { .. }
            | UiRow::ContextHide { .. }
    )
}

fn row_extends_hunk_focus_after(row: UiRow) -> bool {
    matches!(
        row,
        UiRow::Collapsed { .. } | UiRow::ContextLine { .. } | UiRow::ContextHide { .. }
    )
}

fn find_rendered_diff_row_outward<T>(
    rendered_rows: &[RenderedDiffRow],
    focus_viewport_row: usize,
    mut find: impl FnMut(RenderedDiffRow) -> Option<T>,
) -> Option<T> {
    let max_viewport_row = rendered_rows.iter().map(|row| row.viewport_row).max()?;
    let max_distance = focus_viewport_row.max(max_viewport_row.saturating_sub(focus_viewport_row));

    for distance in 0..=max_distance {
        if let Some(viewport_row) = focus_viewport_row.checked_add(distance)
            && viewport_row <= max_viewport_row
            && let Some(rendered_row) = rendered_rows
                .iter()
                .find(|row| row.viewport_row == viewport_row)
            && let Some(found) = find(*rendered_row)
        {
            return Some(found);
        }
        if distance > 0
            && let Some(viewport_row) = focus_viewport_row.checked_sub(distance)
            && let Some(rendered_row) = rendered_rows
                .iter()
                .find(|row| row.viewport_row == viewport_row)
            && let Some(found) = find(*rendered_row)
        {
            return Some(found);
        }
    }

    None
}

pub(crate) fn repo_relative_path(repo: &Path, path: &Path) -> Option<PathBuf> {
    path.strip_prefix(repo).ok().map(Path::to_path_buf)
}

pub(crate) fn editor_reload_request_for_file(
    file: &mark_diff::DiffFile,
) -> Option<EditorReloadRequest> {
    let path = PathBuf::from(file.new_path.as_deref()?);
    let mut pathspecs = Vec::new();
    push_unique_pathspec(&mut pathspecs, file.old_path.as_deref());
    push_unique_pathspec(&mut pathspecs, file.new_path.as_deref());

    Some(EditorReloadRequest { path, pathspecs })
}

fn push_unique_pathspec(pathspecs: &mut Vec<PathBuf>, path: Option<&str>) {
    let Some(path) = path else {
        return;
    };

    let path = PathBuf::from(path);
    if !pathspecs.iter().any(|known| known == &path) {
        pathspecs.push(path);
    }
}

pub(crate) fn splice_diff_files_for_path(
    files: &mut Vec<mark_diff::DiffFile>,
    path: &Path,
    mut replacement: Vec<mark_diff::DiffFile>,
) {
    let mut next = Vec::with_capacity(files.len().saturating_add(replacement.len()));
    let mut inserted = false;

    for file in files.drain(..) {
        if diff_file_matches_path(&file, path) {
            if !inserted {
                next.append(&mut replacement);
                inserted = true;
            }
            continue;
        }

        next.push(file);
    }

    if !inserted {
        next.append(&mut replacement);
    }

    *files = next;
}

pub(crate) fn diff_file_matches_path(file: &mark_diff::DiffFile, path: &Path) -> bool {
    let path = diff_path_string(path);
    file.old_path.as_deref() == Some(path.as_str())
        || file.new_path.as_deref() == Some(path.as_str())
}

pub(crate) fn diff_path_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

pub(crate) fn diff_content_width(layout: DiffLayoutMode, width: usize) -> usize {
    match layout {
        DiffLayoutMode::Unified => unified_content_width(width),
        DiffLayoutMode::Split => {
            let left_width = width / 2;
            let right_width = width.saturating_sub(left_width);
            split_cell_content_width(left_width).min(split_cell_content_width(right_width))
        }
    }
}

pub(crate) fn unified_content_width(width: usize) -> usize {
    let indicator_width = 1.min(width);
    let gutter_width = UNIFIED_GUTTER_WIDTH.min(width.saturating_sub(indicator_width));
    width.saturating_sub(indicator_width + gutter_width)
}

pub(crate) fn split_cell_content_width(width: usize) -> usize {
    let indicator_width = 1.min(width);
    let gutter_width = GUTTER_WIDTH.min(width.saturating_sub(indicator_width));
    width.saturating_sub(indicator_width + gutter_width)
}

pub(crate) fn wrapped_line_count(text: &str, content_width: usize) -> usize {
    let mut count = 1usize;
    for_wrapped_line_start_after_first(text, content_width, |_| {
        count = count.saturating_add(1);
    });
    count
}

pub(crate) fn wrapped_line_start_columns(text: &str, content_width: usize) -> Vec<usize> {
    let mut starts = vec![0];
    for_wrapped_line_start_after_first(text, content_width, |start| starts.push(start));
    starts
}

fn for_wrapped_line_start_after_first(
    text: &str,
    content_width: usize,
    mut visit: impl FnMut(usize),
) {
    if content_width == 0 {
        return;
    }

    let mut line_width = 0usize;
    let mut consumed_width = 0usize;
    for ch in text.chars() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if ch_width == 0 {
            continue;
        }

        if line_width == content_width
            || (line_width > 0 && line_width.saturating_add(ch_width) > content_width)
        {
            visit(consumed_width);
            line_width = 0;
        }

        line_width = line_width.saturating_add(ch_width);
        consumed_width = consumed_width.saturating_add(ch_width);
    }
}

fn normalize_annotation_editor_contents(contents: &str) -> String {
    contents
        .replace("\r\n", "\n")
        .trim_end_matches('\n')
        .to_owned()
}

fn create_annotation_scratch_file(contents: &str) -> MarkResult<AnnotationScratchFile> {
    let prefix = format!("mark-annotations-{}-", process::id());
    let dir = tempfile::Builder::new().prefix(&prefix).tempdir()?;
    #[cfg(unix)]
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700))?;

    let path = dir.path().join("annotation.md");
    write_annotation_scratch_file(&path, contents)?;

    Ok(AnnotationScratchFile { _dir: dir, path })
}

#[cfg(unix)]
fn write_annotation_scratch_file(path: &Path, contents: &str) -> io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(contents.as_bytes())?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn write_annotation_scratch_file(path: &Path, contents: &str) -> io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(contents.as_bytes())
}

#[cfg(test)]
mod annotation_editor_tests {
    use super::*;

    #[test]
    fn annotation_editor_contents_normalize_crlf_line_endings() {
        assert_eq!(
            normalize_annotation_editor_contents("first\r\nsecond\r\n"),
            "first\nsecond"
        );
        assert_eq!(
            normalize_annotation_editor_contents("first\r\nsecond"),
            "first\nsecond"
        );
        assert_eq!(
            normalize_annotation_editor_contents("first\r\n\r\nsecond\r\n"),
            "first\n\nsecond"
        );
        assert_eq!(
            normalize_annotation_editor_contents("trailing spaces  \r\n"),
            "trailing spaces  "
        );
    }
}

#[cfg(all(test, unix))]
mod annotation_scratch_tests {
    use std::os::unix::fs::PermissionsExt as _;

    use super::*;

    #[test]
    fn annotation_scratch_file_is_private_and_removed_on_drop() {
        let scratch = create_annotation_scratch_file("secret").expect("scratch file");
        let dir = scratch.path.parent().expect("scratch dir").to_path_buf();

        assert_eq!(
            fs::metadata(&dir)
                .expect("scratch dir metadata")
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&scratch.path)
                .expect("scratch file metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            fs::read_to_string(&scratch.path).expect("scratch contents"),
            "secret"
        );

        drop(scratch);

        assert!(!dir.exists());
    }
}
