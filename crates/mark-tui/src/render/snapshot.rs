use ratatui::layout::Rect;

use crate::{
    app::{DiffApp, ERROR_LOG_MAX_HEIGHT, ERROR_LOG_MIN_HEIGHT},
    controls::DiffLayoutMode,
    model::FileIndex,
    theme::DiffTheme,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OverlayLayer {
    DiffMenu,
    ReviewInput,
    OptionsMenu,
    AnnotationMenu,
    ColorSchemePicker,
    BranchMenu,
    CommitMenu,
    HelpMenu,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DiffViewportSnapshot {
    pub(crate) layout: DiffLayoutMode,
    pub(crate) scroll: usize,
    pub(crate) horizontal_scroll: usize,
    pub(crate) line_wrapping: bool,
    pub(crate) viewport_rows: usize,
    pub(crate) viewport_width: usize,
    pub(crate) selected_file: FileIndex,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RenderSnapshot {
    pub(crate) theme: DiffTheme,
    pub(crate) diff: DiffViewportSnapshot,
    pub(crate) filter_bar_visible: bool,
    pub(crate) error_log_visible: bool,
    pub(crate) requested_error_log_height: u16,
    pub(crate) file_sidebar_open: bool,
    pub(crate) overlay_layers: Vec<OverlayLayer>,
}

impl RenderSnapshot {
    pub(crate) fn from_app(app: &DiffApp) -> Self {
        let mut overlay_layers = Vec::new();
        if app.overlays.diff_menu_is_open() {
            overlay_layers.push(OverlayLayer::DiffMenu);
        }
        if app.overlays.review_input_is_open() {
            overlay_layers.push(OverlayLayer::ReviewInput);
        }
        if app.overlays.options_menu_is_open() {
            overlay_layers.push(OverlayLayer::OptionsMenu);
        }
        if app.overlays.annotation_menu_is_open() {
            overlay_layers.push(OverlayLayer::AnnotationMenu);
        }
        if app.overlays.color_scheme_picker_is_open() {
            overlay_layers.push(OverlayLayer::ColorSchemePicker);
        }
        if app.refs.branch_menu_is_open() {
            overlay_layers.push(OverlayLayer::BranchMenu);
        }
        if app.refs.commit_menu_is_open() {
            overlay_layers.push(OverlayLayer::CommitMenu);
        }
        if app.overlays.help_menu_is_open() {
            overlay_layers.push(OverlayLayer::HelpMenu);
        }

        Self {
            theme: app.config.theme,
            diff: DiffViewportSnapshot {
                layout: app.viewport.layout,
                scroll: app.viewport.scroll,
                horizontal_scroll: app.viewport.horizontal_scroll,
                line_wrapping: app.viewport.line_wrapping,
                viewport_rows: app.viewport.viewport_rows,
                viewport_width: app.viewport.viewport_width,
                selected_file: app.sidebar.selected_file,
            },
            filter_bar_visible: app.filters.input_open() || app.filters.active(),
            error_log_visible: app.notifications.error_log.is_some(),
            requested_error_log_height: app.notifications.error_log_height,
            file_sidebar_open: app.sidebar.file_sidebar_open,
            overlay_layers,
        }
    }

    pub(crate) fn error_log_height(&self, available_height: u16) -> u16 {
        if !self.error_log_visible || available_height == 0 {
            return 0;
        }

        self.requested_error_log_height
            .clamp(ERROR_LOG_MIN_HEIGHT, ERROR_LOG_MAX_HEIGHT)
            .min(available_height)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RenderLayoutSnapshot {
    pub(crate) root: Rect,
    pub(crate) diff: Rect,
    pub(crate) body: Rect,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct HitMap {
    pub(crate) diff_area: Option<Rect>,
    pub(crate) diff_menu_area: Option<Rect>,
    pub(crate) branch_menu_area: Option<Rect>,
    pub(crate) commit_menu_area: Option<Rect>,
    pub(crate) options_menu_area: Option<Rect>,
    pub(crate) annotation_menu_area: Option<Rect>,
    pub(crate) review_input_area: Option<Rect>,
    pub(crate) color_scheme_picker_area: Option<Rect>,
    pub(crate) error_log_separator_row: Option<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RenderPlan {
    pub(crate) layout: RenderLayoutSnapshot,
    pub(crate) hit_map: HitMap,
    pub(crate) state: RenderStatePlan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RenderStatePlan {
    pub(crate) terminal_area: Rect,
    pub(crate) file_sidebar_render_width: u16,
    pub(crate) file_sidebar_visible_rows: Option<usize>,
    pub(crate) viewport_rows: usize,
    pub(crate) viewport_width: usize,
    pub(crate) options_menu_visible_rows: Option<usize>,
    pub(crate) annotation_menu_visible_rows: Option<usize>,
    pub(crate) color_scheme_picker_visible_rows: Option<usize>,
    pub(crate) branch_menu_visible_rows: Option<usize>,
    pub(crate) commit_menu_visible_rows: Option<usize>,
    pub(crate) help_menu_visible_rows: Option<usize>,
}
