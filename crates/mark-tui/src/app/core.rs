use super::{
    AnnotationState, AppConfigState, DocumentState, FileSidebarState, FilterState, InputState,
    JobState, NotificationState, OverlayState, ReferenceState, RuntimeState, ViewportState,
};
use crate::controls::{DiffChoice, DiffLayoutMode, is_review_options};
use crate::editor::EditorTarget;
use crate::keymap::GlobalAction;
use crate::model::UiModel;
use crate::search::{DiffSearchIndex, DiffSearchResult};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mark_core::MarkResult;
use mark_diff::{Changeset, DiffOptions, DiffSource, DiffStats};
use mark_syntax::DiffContextExpansion;
use ratatui::layout::Rect;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tokio::sync::oneshot;

pub(crate) const MOUSE_HUNK_FOCUS_SCROLL_TICKS: isize = 3;
pub(crate) const EDITOR_RELOAD_POLL: Duration = Duration::from_millis(8);
pub(crate) const FILTER_DEBOUNCE: Duration = Duration::from_millis(120);
pub(crate) const DIFF_PREFETCH_POLL: Duration = Duration::from_millis(8);
pub(crate) const FILTER_WORKER_POLL: Duration = Duration::from_millis(8);
pub(crate) const MAX_LIVE_GREP_MATCHES: usize = 10_000;
pub(crate) const MAX_DIFF_CACHE_ENTRIES: usize = 4;
pub(crate) const MAX_COLOR_SCHEME_MENU_ROWS: usize = 9;
pub(crate) const ERROR_LOG_DEFAULT_HEIGHT: u16 = 6;
pub(crate) const ERROR_LOG_MIN_HEIGHT: u16 = 3;
pub(crate) const ERROR_LOG_MAX_HEIGHT: u16 = 40;
pub(crate) const POST_EDITOR_QUIT_KEY_IGNORE: Duration = Duration::from_millis(250);
pub(crate) const NORMAL_GLOBAL_ACTIONS: &[GlobalAction] = &[
    GlobalAction::Quit,
    GlobalAction::Help,
    GlobalAction::Reload,
    GlobalAction::FileFilter,
    GlobalAction::Grep,
    GlobalAction::DiffMenu,
    GlobalAction::HeadBranch,
    GlobalAction::BaseBranch,
    GlobalAction::CommitPicker,
    GlobalAction::OptionsMenu,
    GlobalAction::AnnotationMenu,
    GlobalAction::FileBrowser,
    GlobalAction::PreviousFile,
    GlobalAction::NextFile,
    GlobalAction::PreviousHunk,
    GlobalAction::NextHunk,
    GlobalAction::ExpandContextUp,
    GlobalAction::ExpandContextDown,
    GlobalAction::CollapseContextAll,
    GlobalAction::Layout,
    GlobalAction::EditHunk,
    GlobalAction::CopyMarks,
    GlobalAction::CopyErrorLog,
    GlobalAction::ClearFilters,
    GlobalAction::NextDiffType,
    GlobalAction::PreviousDiffType,
    GlobalAction::NextAnnotation,
    GlobalAction::PreviousAnnotation,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HunkFocusScrollBehavior {
    Preserve,
    ClearOnScroll,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HunkFocusModelBehavior {
    PreserveIfValid,
    Clear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditorReloadBehavior {
    None,
    ScopedAsync,
    Sync,
}

pub(crate) struct FocusedEditorLaunch {
    pub(crate) target: EditorTarget,
    pub(crate) editor: String,
}

pub(crate) struct AsyncJob<T> {
    rx: oneshot::Receiver<T>,
}

impl<T> AsyncJob<T> {
    pub(crate) fn new(rx: oneshot::Receiver<T>) -> Self {
        Self { rx }
    }

    pub(crate) fn try_recv(&mut self) -> Result<T, oneshot::error::TryRecvError> {
        self.rx.try_recv()
    }
}

impl<T> std::fmt::Debug for AsyncJob<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("AsyncJob").finish()
    }
}

pub(crate) struct EditorReloadWorker {
    pub(crate) generation: u64,
    pub(crate) job: AsyncJob<EditorScopedReload>,
}

impl std::fmt::Debug for EditorReloadWorker {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("EditorReloadWorker").finish()
    }
}

pub(crate) struct EditorScopedReload {
    pub(crate) path: PathBuf,
    pub(crate) changeset: MarkResult<Changeset>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PostFilterNavigation {
    Preserve,
    JumpToGrep,
}

impl PostFilterNavigation {
    pub(crate) fn jumps_to_grep(self) -> bool {
        matches!(self, Self::JumpToGrep)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PendingFilterApply {
    pub(crate) generation: u64,
    pub(crate) due_at: Instant,
    pub(crate) navigation: PostFilterNavigation,
}

pub(crate) struct FilterWorker {
    pub(crate) generation: u64,
    pub(crate) file_filter: String,
    pub(crate) grep_filter: String,
    pub(crate) navigation: PostFilterNavigation,
    pub(crate) job: AsyncJob<DiffSearchResult>,
}

impl std::fmt::Debug for FilterWorker {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FilterWorker")
            .field("generation", &self.generation)
            .field("file_filter", &self.file_filter)
            .field("grep_filter", &self.grep_filter)
            .field("navigation", &self.navigation)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MarkExport {
    pub(crate) path: String,
    pub(crate) old_line: Option<usize>,
    pub(crate) new_line: Option<usize>,
    pub(crate) body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EditorReloadRequest {
    pub(crate) path: PathBuf,
    pub(crate) pathspecs: Vec<PathBuf>,
}

pub(crate) fn is_plain_char_key(key: KeyEvent, character: char) -> bool {
    key.code == KeyCode::Char(character)
        && !key.modifiers.contains(KeyModifiers::CONTROL)
        && !key.modifiers.contains(KeyModifiers::ALT)
}

pub(crate) fn rect_contains(area: Rect, column: u16, row: u16) -> bool {
    column >= area.x
        && column < area.x.saturating_add(area.width)
        && row >= area.y
        && row < area.y.saturating_add(area.height)
}

pub(crate) fn show_rev_from_options(options: &DiffOptions) -> Option<String> {
    match &options.source {
        DiffSource::Show(rev) if !rev.is_empty() => Some(rev.to_string()),
        _ => None,
    }
}

pub(crate) fn diff_choice_for_options(options: &DiffOptions) -> Option<DiffChoice> {
    if is_review_options(options) {
        return Some(DiffChoice::Review);
    }

    match &options.source {
        DiffSource::Worktree => Some(DiffChoice::All),
        DiffSource::Base(_) | DiffSource::Branch { .. } => Some(DiffChoice::Branch),
        DiffSource::Show(_) => Some(DiffChoice::Show),
        _ => None,
    }
}

pub(crate) fn cacheable_diff_options(options: &DiffOptions) -> bool {
    !options.is_stat()
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DiffLoadCachePolicy {
    Use,
    Bypass,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BranchMetadataPolicy {
    Preserve,
    Refresh,
}

#[derive(Debug)]
pub(crate) struct PendingDiffLoad {
    pub(crate) options: DiffOptions,
    pub(crate) error_prefix: String,
    pub(crate) branch_metadata: BranchMetadataPolicy,
    pub(crate) job: AsyncJob<MarkResult<Changeset>>,
}

#[derive(Debug)]
pub(crate) struct PendingReviewLoad {
    pub(crate) error_prefix: String,
    pub(crate) job: AsyncJob<MarkResult<(DiffOptions, Changeset)>>,
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
    pub(crate) job: AsyncJob<MarkResult<Changeset>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SyntaxStartupMode {
    Config,
    Disabled,
    Languages(Vec<String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HunkFocusSearch {
    FirstVisible,
    NearestTo(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RenderedDiffRow {
    pub(crate) viewport_row: usize,
    pub(crate) model_row: usize,
}

#[derive(Debug)]
pub(crate) struct AnnotationScratchFile {
    pub(crate) _dir: TempDir,
    pub(crate) path: PathBuf,
}

#[derive(Debug)]
pub(crate) struct WrappedVisualLayout {
    pub(crate) layout: DiffLayoutMode,
    pub(crate) viewport_width: usize,
    pub(crate) model_rows: usize,
    pub(crate) model_rows_ptr: usize,
    pub(crate) row_starts: Vec<usize>,
    pub(crate) total_rows: usize,
}

impl WrappedVisualLayout {
    pub(crate) fn matches(&self, app: &DiffApp) -> bool {
        self.layout == app.viewport.layout
            && self.viewport_width == app.viewport.viewport_width
            && self.model_rows == app.document.model.len()
            && self.model_rows_ptr == app.document.model.rows.as_ptr() as usize
    }
}

#[derive(Debug)]
pub(crate) struct DiffApp {
    pub(crate) document: DocumentState,
    pub(crate) viewport: ViewportState,
    pub(crate) sidebar: FileSidebarState,
    pub(crate) annotations_state: AnnotationState,
    pub(crate) overlays: OverlayState,
    pub(crate) filters: FilterState,
    pub(crate) refs: ReferenceState,
    pub(crate) jobs: JobState,
    pub(crate) notifications: NotificationState,
    pub(crate) input: InputState,
    pub(crate) config: AppConfigState,
    pub(crate) runtime: RuntimeState,
}
