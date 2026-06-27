use super::*;

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
    pub(crate) manual_hunk_focus: Option<(usize, usize)>,
    pub(crate) terminal_area: Rect,
    pub(crate) rendered_diff_area: Option<Rect>,
    pub(crate) mouse_hover: Option<(u16, u16)>,
}

#[derive(Debug)]
pub(crate) struct FileSidebarState {
    pub(crate) selected_file: usize,
    pub(crate) file_sidebar_open: bool,
    pub(crate) file_sidebar_scroll: usize,
    pub(crate) file_sidebar_width: Option<u16>,
    pub(crate) file_sidebar_render_width: u16,
    pub(crate) file_sidebar_resizing: bool,
}

#[derive(Debug)]
pub(crate) struct AnnotationState {
    pub(crate) annotations: AnnotationStore,
    pub(crate) annotation_draft: Option<AnnotationDraft>,
}

#[derive(Debug)]
pub(crate) struct OverlayState {
    pub(crate) help_menu_open: bool,
    pub(crate) help_menu_input: String,
    pub(crate) help_menu_input_cursor: usize,
    pub(crate) help_menu_scroll: usize,
    pub(crate) help_menu_visible_rows: usize,
    pub(crate) diff_menu_open: bool,
    pub(crate) diff_menu: SelectorState,
    pub(crate) review_input_open: bool,
    pub(crate) review_input: String,
    pub(crate) review_input_cursor: usize,
    pub(crate) options_menu_open: bool,
    pub(crate) options_menu: SelectorState,
    pub(crate) options_menu_draft: OptionsDraft,
    pub(crate) color_scheme_picker_open: bool,
    pub(crate) color_scheme_picker: SelectorState,
    pub(crate) color_scheme_preview_original: Option<(ColorSchemeChoice, DiffTheme)>,
    pub(crate) rendered_diff_menu_area: Option<Rect>,
    pub(crate) rendered_branch_menu_area: Option<Rect>,
    pub(crate) rendered_commit_menu_area: Option<Rect>,
    pub(crate) rendered_review_input_area: Option<Rect>,
    pub(crate) rendered_color_scheme_picker_area: Option<Rect>,
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
    pub(crate) grep_matches: Vec<usize>,
    pub(crate) grep_matches_truncated: bool,
    pub(crate) selected_grep_match: Option<usize>,
}

#[derive(Debug)]
pub(crate) struct ReferenceState {
    pub(crate) branch_menu_open: Option<BranchMenu>,
    pub(crate) branch_menu: SelectorState,
    pub(crate) branch_base: Option<String>,
    pub(crate) branch_head: Option<String>,
    pub(crate) current_head: Option<String>,
    pub(crate) comparison_branches: Vec<String>,
    pub(crate) commit_menu_open: bool,
    pub(crate) commit_menu: SelectorState,
    pub(crate) show_rev: Option<String>,
    pub(crate) comparison_commits: Vec<GitCommit>,
}

#[derive(Debug)]
pub(crate) struct JobState {
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
}

#[derive(Debug)]
pub(crate) struct NotificationState {
    pub(crate) error_log: Option<String>,
    pub(crate) error_log_height: u16,
    pub(crate) error_log_resizing: bool,
    pub(crate) rendered_error_log_separator_row: Option<u16>,
    pub(crate) toasts: Toasts,
}

#[derive(Debug)]
pub(crate) struct InputState {
    pub(crate) key_prefix_pending: Option<KeyPress>,
    pub(crate) mouse_scroll: MouseScroll,
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
}
