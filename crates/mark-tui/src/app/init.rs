use super::{
    AnnotationState, AppConfigState, ColorSchemeChoice, DiffApp, DocumentState,
    ERROR_LOG_DEFAULT_HEIGHT, FileSidebarState, FilterState, InputState, JobState, MouseScroll,
    NotificationState, OptionsDraft, OverlayState, ReferenceState, RuntimeState, SyntaxStartupMode,
    ViewportState, color_scheme_from_config, layout_override_from_setting,
    layout_setting_from_override, show_rev_from_options,
};
use crate::annotation::AnnotationStore;
use crate::controls::{
    DiffLayoutMode, branch_head_from_options, comparison_branches, comparison_commits,
    current_head_label, default_branch_base,
};
use crate::keymap::Keymap;
use crate::model::{UiModel, UiRow};
use crate::render::snapshot::HitMap;
use crate::search::DiffSearchIndex;
use crate::selector::SelectorState;
use crate::syntax::{LruCache, SyntaxRuntime};
use crate::theme::{DiffTheme, MAX_INLINE_DIFF_CACHE_ENTRIES, diff_theme_from_config};
use crate::toast::Toasts;
use mark_core::MarkResult;
use mark_diff::{Changeset, DiffOptions};
use mark_syntax::SyntaxSettings;
use ratatui::layout::Rect;
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

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
        app.viewport.layout_override = Some(layout);
        app.overlays.options_menu_draft.layout =
            layout_setting_from_override(app.viewport.layout_override);
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
            document: DocumentState {
                options,
                base_changeset: changeset.clone(),
                changeset,
                search_index,
                total_stats,
                stats,
                model,
                max_line_width,
                context_expansions,
                context_cache,
                inline_cache: LruCache::new(MAX_INLINE_DIFF_CACHE_ENTRIES),
                generation: 0,
            },
            viewport: ViewportState {
                layout,
                layout_override,
                scroll: 0,
                horizontal_scroll: 0,
                line_wrapping: settings.line_wrapping,
                viewport_rows: 1,
                viewport_width: 1,
                wrapped_visual_layout: RefCell::new(None),
                manual_hunk_focus,
                terminal_area: Rect::default(),
                rendered_diff_area: None,
                mouse_hover: None,
            },
            sidebar: FileSidebarState {
                selected_file: 0,
                file_sidebar_open: false,
                file_sidebar_scroll: 0,
                file_sidebar_width: None,
                file_sidebar_render_width: 0,
                file_sidebar_resizing: false,
            },
            annotations_state: AnnotationState {
                annotations: AnnotationStore::default(),
                annotation_draft: None,
            },
            overlays: OverlayState {
                help_menu_open: false,
                help_menu_input: String::new(),
                help_menu_input_cursor: 0,
                help_menu_scroll: 0,
                help_menu_visible_rows: 1,
                diff_menu_open: false,
                diff_menu: SelectorState::default(),
                review_input_open: false,
                review_input: String::new(),
                review_input_cursor: 0,
                options_menu_open: false,
                options_menu: SelectorState::default(),
                options_menu_draft: OptionsDraft {
                    layout: layout_setting_from_override(layout_override),
                    live_updates_enabled: settings.live_reload,
                    context_expansion,
                    syntax_enabled: syntax.is_some(),
                    line_wrapping: settings.line_wrapping,
                    color_scheme,
                    notification_mode: settings.notifications.mode,
                    toast_corner: settings.notifications.corner,
                    toast_timeout_ms: settings.notifications.timeout_ms,
                    toast_max_visible: settings.notifications.max_visible,
                },
                color_scheme_picker_open: false,
                color_scheme_picker: SelectorState::default(),
                color_scheme_preview_original: None,
                rendered_diff_menu_area: None,
                rendered_branch_menu_area: None,
                rendered_commit_menu_area: None,
                rendered_review_input_area: None,
                rendered_color_scheme_picker_area: None,
            },
            filters: FilterState {
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
            },
            refs: ReferenceState {
                branch_menu_open: None,
                branch_menu: SelectorState::default(),
                branch_base,
                branch_head,
                current_head,
                comparison_branches,
                commit_menu_open: false,
                commit_menu: SelectorState::default(),
                show_rev,
                comparison_commits,
            },
            jobs: JobState {
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
            },
            notifications: NotificationState {
                error_log: startup_error_log,
                error_log_height: ERROR_LOG_DEFAULT_HEIGHT,
                error_log_resizing: false,
                rendered_error_log_separator_row: None,
                toasts: Toasts::new(settings.notifications),
            },
            input: InputState {
                key_prefix_pending: None,
                mouse_scroll: MouseScroll::default(),
            },
            config: AppConfigState {
                keymap,
                theme,
                color_scheme,
                theme_color_overrides,
                theme_transparent_background,
                settings_persistence_enabled: !cfg!(test),
                #[cfg(test)]
                last_persisted_options_menu_draft: None,
                syntax_settings: settings,
                syntax_startup_mode: syntax_mode,
                syntax_limits,
                syntax,
            },
            runtime: RuntimeState {
                terminal_clear_requested: false,
                dirty: true,
                hit_map: HitMap::default(),
                pending_effects: Vec::new(),
            },
        }
    }
}
