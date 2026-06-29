mod action;
mod annotation_editor;
mod annotations;
mod choices;
mod clipboard;
mod context;
mod controllers;
mod core;
mod diff_files;
mod diff_load;
mod diff_replace;
mod editor_reload;
mod effect;
mod error_log;
mod file_sidebar;
mod filters;
mod help;
mod hunk_focus;
mod init;
mod inline_ranges;
mod input;
mod layout;
mod lifecycle;
mod marks;
mod menu_options;
mod menu_refs;
mod menus;
mod mouse;
mod navigation;
mod notices;
mod options;
mod render_state;
mod runner;
mod state;
mod syntax;
mod viewport;

pub(crate) use action::AppAction;
pub(crate) use annotation_editor::{
    create_annotation_scratch_file, normalize_annotation_editor_contents,
};
pub(crate) use annotations::AnnotationMenuItem;
#[cfg(test)]
pub(crate) use clipboard::osc52_clipboard_sequence;
pub(crate) use clipboard::{json_string, write_osc52_clipboard};
pub(crate) use core::{
    AnnotationScratchFile, AsyncJob, BranchMetadataPolicy, DIFF_PREFETCH_POLL, DiffApp,
    DiffCacheEntry, DiffLoadCachePolicy, EDITOR_RELOAD_POLL, ERROR_LOG_DEFAULT_HEIGHT,
    ERROR_LOG_MAX_HEIGHT, ERROR_LOG_MIN_HEIGHT, EditorReloadBehavior, EditorReloadRequest,
    EditorReloadWorker, EditorScopedReload, FILTER_DEBOUNCE, FILTER_WORKER_POLL, FilterWorker,
    FocusedEditorLaunch, HunkFocusModelBehavior, HunkFocusScrollBehavior, HunkFocusSearch,
    MAX_COLOR_SCHEME_MENU_ROWS, MAX_DIFF_CACHE_ENTRIES, MAX_LIVE_GREP_MATCHES,
    MOUSE_HUNK_FOCUS_SCROLL_TICKS, MarkExport, NORMAL_GLOBAL_ACTIONS, POST_EDITOR_QUIT_KEY_IGNORE,
    PendingDiffLoad, PendingDiffPrefetch, PendingFilterApply, PendingReviewLoad,
    PostFilterNavigation, RenderedDiffRow, SyntaxStartupMode, WrappedVisualLayout,
    cacheable_diff_options, diff_choice_for_options, is_plain_char_key, next_context_expansion,
    previous_context_expansion, rect_contains, show_rev_from_options,
};
pub(crate) use diff_files::{
    diff_content_width, editor_reload_request_for_file, repo_relative_path,
    splice_diff_files_for_path, split_cell_content_width, unified_content_width,
    wrapped_line_count, wrapped_line_start_columns,
};
#[cfg(test)]
pub(crate) use diff_load::diff_cache_entry;
#[cfg(test)]
pub(crate) use editor_reload::{FileFingerprint, file_changed_since};
pub(crate) use effect::{ActionOutcome, AppEffect};
#[cfg(test)]
pub(crate) use init::{
    layout_override_from_settings, syntax_runtime_for_diff, syntax_settings_for_diff,
};
pub(crate) use mouse::{MouseScroll, MouseScrollDirection};
pub(crate) use options::{
    COLOR_SCHEME_CHOICES, COMMON_OPTIONS_MENU_ITEMS, ColorSchemeChoice, OptionsDraft,
    OptionsMenuItem, checkbox, color_scheme_config, color_scheme_from_config, color_scheme_label,
    context_expansion_label, layout_override_from_setting, layout_setting_from_override,
    layout_setting_label, next_layout_setting, next_notification_mode, next_toast_corner,
    next_toast_max_visible, next_toast_timeout_ms, notification_mode_label, on_off_search,
    option_label, persist_options_menu_draft_to_path, toast_corner_label, toast_timeout_label,
};
#[cfg(test)]
pub(crate) use runner::{drain_live_reloads, handle_event};
pub(crate) use runner::{is_quit_key, run_loop, sync_live_diff};
pub(crate) use state::{
    ActiveOverlay, ActiveReferenceMenu, AnnotationState, AppConfigState, DocumentState,
    FileSidebarState, FilterState, InputState, JobState, LiveReloadStatus, LiveUpdatesState,
    NotificationState, OverlayState, ReferenceState, RuntimeState, ViewportState,
};
pub(crate) use viewport::{
    annotation_scroll_for_block, find_rendered_diff_row_outward, hunk_focus_row_range,
    max_scroll_for_annotated_viewport, max_scroll_for_viewport, viewport_center_offset,
    viewport_focus_offset,
};
