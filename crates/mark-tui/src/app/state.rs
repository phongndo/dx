use super::{
    AppEffect, ColorSchemeChoice, DIFF_PREFETCH_POLL, DiffCacheEntry, EDITOR_RELOAD_POLL,
    EditorReloadRequest, EditorReloadWorker, FILTER_WORKER_POLL, FilterWorker, MouseScroll,
    OptionsDraft, PendingDiffLoad, PendingDiffPrefetch, PendingFilterApply, PendingReviewLoad,
    SyntaxStartupMode, WrappedVisualLayout,
};
use crate::annotation::{AnnotationDraft, AnnotationStore};
use crate::controls::{BranchMenu, DiffFilterKind, DiffLayoutMode, GitCommit};
use crate::keymap::{KeyPress, Keymap};
use crate::live_diff::live_diff_supported;
use crate::model::{
    ContextKey, ContextSourceEntry, ContextSourceKey, FileIndex, HunkIndex, ModelRow, UiModel,
};
use crate::render::snapshot::HitMap;
use crate::search::{DiffSearchIndex, SearchMatchIndex};
use crate::selector::SelectorState;
use crate::syntax::{InlineHunkEmphasisCache, InlineHunkKey, LruCache, SyntaxRuntime};
use crate::text_input::{TextInputKeyResult, handle_text_input_key};
use crate::theme::{DiffTheme, EVENT_POLL};
use crate::toast::{ToastLevel, Toasts};
use crossterm::event::KeyEvent;
use mark_diff::{BranchName, Changeset, DiffOptions, DiffStats};
use mark_syntax::{ColorOverrides, SyntaxLimits, SyntaxSettings};
use ratatui::layout::Rect;
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[cfg(test)]
use super::OptionsMenuItem;

#[derive(Debug)]
pub(crate) struct DocumentState {
    pub(crate) options: DiffOptions,
    pub(crate) base_changeset: Changeset,
    pub(crate) changeset: Changeset,
    pub(crate) search_index: Arc<DiffSearchIndex>,
    pub(crate) total_stats: DiffStats,
    pub(crate) stats: DiffStats,
    pub(crate) model: UiModel,
    pub(crate) max_line_width: usize,
    pub(crate) context_expansions: HashMap<ContextKey, usize>,
    pub(crate) context_cache: HashMap<ContextSourceKey, ContextSourceEntry>,
    pub(crate) inline_cache: LruCache<InlineHunkKey, InlineHunkEmphasisCache>,
    pub(crate) generation: u64,
}

#[derive(Debug)]
pub(crate) struct ViewportState {
    pub(crate) layout: DiffLayoutMode,
    pub(crate) layout_override: Option<DiffLayoutMode>,
    pub(crate) scroll: usize,
    pub(crate) horizontal_scroll: usize,
    pub(crate) line_wrapping: bool,
    pub(crate) viewport_rows: usize,
    pub(crate) viewport_width: usize,
    pub(crate) wrapped_visual_layout: RefCell<Option<WrappedVisualLayout>>,
    pub(crate) manual_hunk_focus: Option<(FileIndex, HunkIndex)>,
    pub(crate) terminal_area: Rect,
    pub(crate) rendered_diff_area: Option<Rect>,
    pub(crate) mouse_hover: Option<(u16, u16)>,
}

impl ViewportState {
    pub(crate) fn set_terminal_area(&mut self, area: Rect) -> bool {
        if self.terminal_area == area {
            return false;
        }

        self.terminal_area = area;
        true
    }

    pub(crate) fn set_rendered_diff_area(&mut self, area: Option<Rect>) -> bool {
        if self.rendered_diff_area == area {
            return false;
        }

        self.rendered_diff_area = area;
        true
    }
}

#[derive(Debug)]
pub(crate) struct FileSidebarState {
    pub(crate) selected_file: FileIndex,
    pub(crate) file_sidebar_open: bool,
    pub(crate) file_sidebar_scroll: usize,
    pub(crate) file_sidebar_width: Option<u16>,
    pub(crate) file_sidebar_render_width: u16,
    pub(crate) file_sidebar_resizing: bool,
}

impl FileSidebarState {
    pub(crate) fn is_position(&self, column: u16, row: u16, visible_rows: usize) -> bool {
        self.file_sidebar_open
            && self.file_sidebar_render_width > 0
            && column < self.file_sidebar_render_width
            && row > 0
            && usize::from(row - 1) < visible_rows
    }

    pub(crate) fn is_resize_handle(&self, column: u16, row: u16, visible_rows: usize) -> bool {
        self.is_position(column, row, visible_rows)
            && column.saturating_add(1) == self.file_sidebar_render_width
    }

    pub(crate) fn start_resize(&mut self) {
        self.file_sidebar_resizing = true;
    }

    pub(crate) fn finish_resize(&mut self) {
        self.file_sidebar_resizing = false;
    }
}

#[derive(Debug)]
pub(crate) struct AnnotationState {
    pub(crate) annotations: AnnotationStore,
    pub(crate) annotation_draft: Option<AnnotationDraft>,
}

#[derive(Debug)]
pub(crate) struct OverlayState {
    pub(crate) active_overlay: ActiveOverlay,
    pub(crate) help_menu_input: String,
    pub(crate) help_menu_input_cursor: usize,
    pub(crate) help_menu_scroll: usize,
    pub(crate) help_menu_visible_rows: usize,
    pub(crate) diff_menu: SelectorState,
    pub(crate) review_input: String,
    pub(crate) review_input_cursor: usize,
    pub(crate) options_menu: SelectorState,
    pub(crate) annotation_menu: SelectorState,
    pub(crate) options_menu_draft: OptionsDraft,
    pub(crate) color_scheme_picker: SelectorState,
    pub(crate) color_scheme_preview_original: Option<(ColorSchemeChoice, DiffTheme)>,
    pub(crate) rendered_diff_menu_area: Option<Rect>,
    pub(crate) rendered_branch_menu_area: Option<Rect>,
    pub(crate) rendered_commit_menu_area: Option<Rect>,
    pub(crate) rendered_annotation_menu_area: Option<Rect>,
    pub(crate) rendered_review_input_area: Option<Rect>,
    pub(crate) rendered_color_scheme_picker_area: Option<Rect>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum ActiveOverlay {
    #[default]
    None,
    HelpMenu,
    DiffMenu,
    ReviewInput,
    OptionsMenu(OptionsOverlayMode),
    AnnotationMenu,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OptionsOverlayMode {
    Menu,
    ColorSchemePicker,
}

impl OverlayState {
    pub(crate) fn help_menu_is_open(&self) -> bool {
        self.active_overlay == ActiveOverlay::HelpMenu
    }

    pub(crate) fn diff_menu_is_open(&self) -> bool {
        self.active_overlay == ActiveOverlay::DiffMenu
    }

    pub(crate) fn review_input_is_open(&self) -> bool {
        self.active_overlay == ActiveOverlay::ReviewInput
    }

    pub(crate) fn options_menu_is_open(&self) -> bool {
        matches!(self.active_overlay, ActiveOverlay::OptionsMenu(_))
    }

    pub(crate) fn annotation_menu_is_open(&self) -> bool {
        self.active_overlay == ActiveOverlay::AnnotationMenu
    }

    pub(crate) fn color_scheme_picker_is_open(&self) -> bool {
        self.active_overlay == ActiveOverlay::OptionsMenu(OptionsOverlayMode::ColorSchemePicker)
    }

    pub(crate) fn open_help_menu(&mut self) {
        self.active_overlay = ActiveOverlay::HelpMenu;
    }

    pub(crate) fn open_diff_menu(&mut self) {
        self.active_overlay = ActiveOverlay::DiffMenu;
    }

    pub(crate) fn open_options_menu(&mut self) {
        self.active_overlay = ActiveOverlay::OptionsMenu(OptionsOverlayMode::Menu);
    }

    pub(crate) fn open_annotation_menu(&mut self) {
        self.active_overlay = ActiveOverlay::AnnotationMenu;
    }

    pub(crate) fn hide_diff_menu(&mut self) {
        if self.diff_menu_is_open() {
            self.active_overlay = ActiveOverlay::None;
        }
        self.diff_menu.reset_input();
        self.rendered_diff_menu_area = None;
    }

    pub(crate) fn hide_options_menu(&mut self) {
        if self.options_menu_is_open() {
            self.active_overlay = ActiveOverlay::None;
        }
    }

    pub(crate) fn close_help_menu(&mut self) {
        if self.help_menu_is_open() {
            self.active_overlay = ActiveOverlay::None;
        }
        self.help_menu_input.clear();
        self.help_menu_input_cursor = 0;
        self.help_menu_scroll = 0;
    }

    pub(crate) fn close_diff_menu(&mut self) -> bool {
        if !self.diff_menu_is_open()
            && self.diff_menu.input.is_empty()
            && self.rendered_diff_menu_area.is_none()
        {
            return false;
        }

        if self.diff_menu_is_open() {
            self.active_overlay = ActiveOverlay::None;
        }
        self.diff_menu.reset_input();
        self.rendered_diff_menu_area = None;
        true
    }

    pub(crate) fn close_options_menu(&mut self) -> bool {
        if !self.options_menu_is_open()
            && self.options_menu.input.is_empty()
            && self.options_menu.scroll == 0
        {
            return false;
        }

        if self.options_menu_is_open() {
            self.active_overlay = ActiveOverlay::None;
        }
        self.options_menu.reset();
        self.color_scheme_picker.reset_input_and_scroll();
        self.color_scheme_preview_original = None;
        self.rendered_color_scheme_picker_area = None;
        true
    }

    pub(crate) fn close_annotation_menu(&mut self) -> bool {
        if !self.annotation_menu_is_open()
            && self.annotation_menu.input.is_empty()
            && self.annotation_menu.scroll == 0
            && self.rendered_annotation_menu_area.is_none()
        {
            return false;
        }

        if self.annotation_menu_is_open() {
            self.active_overlay = ActiveOverlay::None;
        }
        self.annotation_menu.reset();
        self.rendered_annotation_menu_area = None;
        true
    }

    pub(crate) fn open_review_input(&mut self) {
        self.review_input.clear();
        self.review_input_cursor = 0;
        self.active_overlay = ActiveOverlay::ReviewInput;
    }

    pub(crate) fn close_review_input(&mut self) -> bool {
        if !self.review_input_is_open()
            && self.review_input.is_empty()
            && self.rendered_review_input_area.is_none()
        {
            return false;
        }

        if self.review_input_is_open() {
            self.active_overlay = ActiveOverlay::None;
        }
        self.review_input.clear();
        self.review_input_cursor = 0;
        self.rendered_review_input_area = None;
        true
    }

    pub(crate) fn open_color_scheme_picker(&mut self) {
        self.active_overlay = ActiveOverlay::OptionsMenu(OptionsOverlayMode::ColorSchemePicker);
    }

    pub(crate) fn close_color_scheme_picker(
        &mut self,
    ) -> (bool, Option<(ColorSchemeChoice, DiffTheme)>) {
        if !self.color_scheme_picker_is_open() {
            return (false, None);
        }

        self.active_overlay = ActiveOverlay::OptionsMenu(OptionsOverlayMode::Menu);
        self.color_scheme_picker.reset_input_and_scroll();
        self.rendered_color_scheme_picker_area = None;
        (true, self.color_scheme_preview_original.take())
    }

    pub(crate) fn accept_color_scheme_picker(&mut self) {
        if self.color_scheme_picker_is_open() {
            self.active_overlay = ActiveOverlay::OptionsMenu(OptionsOverlayMode::Menu);
        }
        self.color_scheme_preview_original = None;
        self.color_scheme_picker.reset_input_and_scroll();
        self.rendered_color_scheme_picker_area = None;
    }
}

#[derive(Debug)]
pub(crate) struct FilterState {
    pub(crate) filter_input: Option<DiffFilterKind>,
    pub(crate) file_filter: String,
    pub(crate) file_filter_input: String,
    pub(crate) file_filter_input_cursor: usize,
    pub(crate) grep_filter: String,
    pub(crate) grep_filter_input: String,
    pub(crate) grep_filter_input_cursor: usize,
    pub(crate) grep_matches: Vec<ModelRow>,
    pub(crate) grep_matches_truncated: bool,
    pub(crate) selected_grep_match: Option<SearchMatchIndex>,
}

impl FilterState {
    pub(crate) fn input_open(&self) -> bool {
        self.filter_input.is_some()
    }

    pub(crate) fn grep_active(&self) -> bool {
        !self.grep_filter.is_empty()
    }

    pub(crate) fn active(&self) -> bool {
        !self.file_filter.is_empty() || !self.grep_filter.is_empty()
    }

    pub(crate) fn query(&self, kind: DiffFilterKind) -> &str {
        match kind {
            DiffFilterKind::File => &self.file_filter,
            DiffFilterKind::Grep => &self.grep_filter,
        }
    }

    pub(crate) fn query_mut(&mut self, kind: DiffFilterKind) -> &mut String {
        match kind {
            DiffFilterKind::File => &mut self.file_filter,
            DiffFilterKind::Grep => &mut self.grep_filter,
        }
    }

    pub(crate) fn input_query(&self, kind: DiffFilterKind) -> &str {
        match kind {
            DiffFilterKind::File => &self.file_filter_input,
            DiffFilterKind::Grep => &self.grep_filter_input,
        }
    }

    pub(crate) fn input_query_mut(&mut self, kind: DiffFilterKind) -> &mut String {
        match kind {
            DiffFilterKind::File => &mut self.file_filter_input,
            DiffFilterKind::Grep => &mut self.grep_filter_input,
        }
    }

    pub(crate) fn input_cursor(&self, kind: DiffFilterKind) -> usize {
        match kind {
            DiffFilterKind::File => self.file_filter_input_cursor,
            DiffFilterKind::Grep => self.grep_filter_input_cursor,
        }
    }

    pub(crate) fn input_cursor_mut(&mut self, kind: DiffFilterKind) -> &mut usize {
        match kind {
            DiffFilterKind::File => &mut self.file_filter_input_cursor,
            DiffFilterKind::Grep => &mut self.grep_filter_input_cursor,
        }
    }

    pub(crate) fn apply_input_key(
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

    pub(crate) fn clear_all(&mut self) -> bool {
        self.grep_matches.clear();
        self.grep_matches_truncated = false;
        self.selected_grep_match = None;

        let had_active_filter = !self.file_filter.is_empty() || !self.grep_filter.is_empty();
        self.file_filter.clear();
        self.file_filter_input.clear();
        self.file_filter_input_cursor = 0;
        self.grep_filter.clear();
        self.grep_filter_input.clear();
        self.grep_filter_input_cursor = 0;
        had_active_filter
    }
}

#[derive(Debug)]
pub(crate) struct ReferenceState {
    pub(crate) active_menu: ActiveReferenceMenu,
    pub(crate) branch_menu: SelectorState,
    pub(crate) branch_base: Option<String>,
    pub(crate) branch_head: Option<String>,
    pub(crate) current_head: Option<String>,
    pub(crate) comparison_branches: Vec<BranchName>,
    pub(crate) commit_menu: SelectorState,
    pub(crate) show_rev: Option<String>,
    pub(crate) comparison_commits: Vec<GitCommit>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum ActiveReferenceMenu {
    #[default]
    None,
    Branch(BranchMenu),
    Commit,
}

impl ReferenceState {
    pub(crate) fn branch_menu_open(&self) -> Option<BranchMenu> {
        match self.active_menu {
            ActiveReferenceMenu::Branch(menu) => Some(menu),
            ActiveReferenceMenu::None | ActiveReferenceMenu::Commit => None,
        }
    }

    pub(crate) fn branch_menu_is_open(&self) -> bool {
        self.branch_menu_open().is_some()
    }

    pub(crate) fn commit_menu_is_open(&self) -> bool {
        self.active_menu == ActiveReferenceMenu::Commit
    }

    pub(crate) fn open_branch_menu(&mut self, menu: BranchMenu) {
        self.active_menu = ActiveReferenceMenu::Branch(menu);
    }

    pub(crate) fn open_commit_menu(&mut self) {
        self.active_menu = ActiveReferenceMenu::Commit;
    }

    pub(crate) fn close_branch_menu(&mut self, overlays: &mut OverlayState) -> bool {
        if !self.branch_menu_is_open()
            && self.branch_menu.input.is_empty()
            && self.branch_menu.scroll == 0
            && overlays.rendered_branch_menu_area.is_none()
        {
            return false;
        }

        if self.branch_menu_is_open() {
            self.active_menu = ActiveReferenceMenu::None;
        }
        self.branch_menu.reset();
        overlays.rendered_branch_menu_area = None;
        true
    }

    pub(crate) fn close_commit_menu(&mut self, overlays: &mut OverlayState) -> bool {
        if !self.commit_menu_is_open()
            && self.commit_menu.input.is_empty()
            && self.commit_menu.scroll == 0
            && overlays.rendered_commit_menu_area.is_none()
        {
            return false;
        }

        if self.commit_menu_is_open() {
            self.active_menu = ActiveReferenceMenu::None;
        }
        self.commit_menu.reset();
        overlays.rendered_commit_menu_area = None;
        true
    }
}

#[derive(Debug)]
pub(crate) struct JobState {
    pub(crate) live_diff_failed_options: Option<DiffOptions>,
    pub(crate) editor_reload: Option<EditorReloadWorker>,
    pub(crate) pending_editor_reload: Option<EditorReloadRequest>,
    pub(crate) post_editor_quit_key_ignore_until: Option<Instant>,
    pub(crate) live_updates: LiveUpdatesState,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LiveUpdatesState {
    DisabledByCli,
    DisabledByUser,
    Enabled(LiveReloadStatus),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LiveReloadStatus {
    Idle,
    Invalidated,
    Pending,
}

impl LiveUpdatesState {
    pub(crate) fn from_allowed_and_enabled(allowed: bool, enabled: bool) -> Self {
        if !allowed {
            Self::DisabledByCli
        } else if enabled {
            Self::Enabled(LiveReloadStatus::Idle)
        } else {
            Self::DisabledByUser
        }
    }

    pub(crate) fn allowed(self) -> bool {
        !matches!(self, Self::DisabledByCli)
    }

    pub(crate) fn enabled(self) -> bool {
        matches!(self, Self::Enabled(_))
    }

    pub(crate) fn status(self) -> Option<LiveReloadStatus> {
        match self {
            Self::Enabled(status) => Some(status),
            Self::DisabledByCli | Self::DisabledByUser => None,
        }
    }

    pub(crate) fn set_user_enabled(&mut self, enabled: bool) {
        if !self.allowed() {
            return;
        }
        *self = if enabled {
            Self::Enabled(LiveReloadStatus::Idle)
        } else {
            Self::DisabledByUser
        };
    }

    pub(crate) fn reset_reload(&mut self) {
        if self.enabled() {
            *self = Self::Enabled(LiveReloadStatus::Idle);
        }
    }

    pub(crate) fn mark_invalidated(&mut self) {
        match self {
            Self::Enabled(LiveReloadStatus::Pending) => {}
            Self::Enabled(status) => *status = LiveReloadStatus::Invalidated,
            Self::DisabledByCli | Self::DisabledByUser => {}
        }
    }

    pub(crate) fn mark_pending(&mut self) {
        if self.enabled() {
            *self = Self::Enabled(LiveReloadStatus::Pending);
        }
    }
}

impl JobState {
    pub(crate) fn diff_cache_invalidator_active(&self, options: &DiffOptions) -> bool {
        self.live_updates.status() == Some(LiveReloadStatus::Idle)
            && live_diff_supported(options)
            && self.live_diff_failed_options.as_ref() != Some(options)
    }

    pub(crate) fn clear_cached_diff_choices(&mut self) {
        self.diff_cache.clear();
        self.pending_diff_prefetch = None;
        self.diff_prefetch_queue.clear();
        self.diff_prefetch_started = false;
    }

    pub(crate) fn event_poll(&self, now: Instant) -> Duration {
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

    pub(crate) fn mark_live_reload_invalidated(&mut self) {
        self.live_updates.mark_invalidated();
    }

    pub(crate) fn mark_live_reload_pending(&mut self) {
        self.live_updates.mark_pending();
    }
}

#[derive(Debug)]
pub(crate) struct NotificationState {
    pub(crate) error_log: Option<String>,
    pub(crate) error_log_height: u16,
    pub(crate) error_log_resizing: bool,
    pub(crate) rendered_error_log_separator_row: Option<u16>,
    pub(crate) toasts: Toasts,
}

impl NotificationState {
    pub(crate) fn push_toast(&mut self, level: ToastLevel, text: impl Into<String>) -> bool {
        self.toasts.push(level, text)
    }

    pub(crate) fn expire_toasts(&mut self, now: Instant) -> bool {
        self.toasts.expire(now)
    }
}

#[derive(Debug)]
pub(crate) struct InputState {
    pub(crate) key_prefix_pending: Option<KeyPress>,
    pub(crate) mouse_scroll: MouseScroll,
}

impl InputState {
    pub(crate) fn begin_key_prefix(&mut self, prefix: KeyPress) {
        self.key_prefix_pending = Some(prefix);
    }

    pub(crate) fn clear_key_prefix(&mut self) {
        self.key_prefix_pending = None;
    }

    pub(crate) fn take_key_prefix(&mut self) -> Option<KeyPress> {
        self.key_prefix_pending.take()
    }

    pub(crate) fn reset_mouse_scroll(&mut self) {
        self.mouse_scroll.reset();
    }
}

#[derive(Debug)]
pub(crate) struct AppConfigState {
    pub(crate) keymap: Keymap,
    pub(crate) theme: DiffTheme,
    pub(crate) color_scheme: ColorSchemeChoice,
    pub(crate) theme_color_overrides: ColorOverrides,
    pub(crate) theme_transparent_background: bool,
    pub(crate) settings_persistence_enabled: bool,
    #[cfg(test)]
    pub(crate) last_persisted_options_menu_draft: Option<(OptionsDraft, OptionsMenuItem)>,
    pub(crate) syntax_settings: SyntaxSettings,
    pub(crate) syntax_startup_mode: SyntaxStartupMode,
    pub(crate) syntax_limits: SyntaxLimits,
    pub(crate) syntax: Option<SyntaxRuntime>,
}

#[derive(Debug)]
pub(crate) struct RuntimeState {
    pub(crate) terminal_clear_requested: bool,
    pub(crate) dirty: bool,
    pub(crate) hit_map: HitMap,
    pub(crate) pending_effects: Vec<AppEffect>,
}

impl RuntimeState {
    pub(crate) fn push_effect(&mut self, effect: AppEffect) {
        self.pending_effects.push(effect);
    }

    pub(crate) fn take_effects(&mut self) -> Vec<AppEffect> {
        std::mem::take(&mut self.pending_effects)
    }

    pub(crate) fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    pub(crate) fn request_terminal_clear(&mut self) {
        self.terminal_clear_requested = true;
        self.mark_dirty();
    }
}
