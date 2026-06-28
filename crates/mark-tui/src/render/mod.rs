pub(crate) mod annotations;
pub(crate) mod compositor;
pub(crate) mod diff;
pub(crate) mod grep;
pub(crate) mod headers;
pub(crate) mod menus;
pub(crate) mod screen_layout;
pub(crate) mod selector_menu;
pub(crate) mod sidebar;
pub(crate) mod snapshot;
pub(crate) mod statusline;
pub(crate) mod style;
pub(crate) mod text;
pub(crate) mod toast;
pub(crate) mod viewport_plan;

use crate::app::DiffApp;
use ratatui::{Frame, layout::Rect};

use self::{
    compositor::{ComponentId, Compositor, RectComponent, RenderContext},
    diff::draw_diff,
    menus::{
        annotation_menu_area, annotation_menu_list_visible_rows, branch_menu_area,
        branch_menu_list_visible_rows, color_scheme_picker_area,
        color_scheme_picker_list_visible_rows, commit_menu_area, commit_menu_list_visible_rows,
        diff_menu_area, draw_annotation_menu, draw_branch_menu, draw_color_scheme_picker,
        draw_commit_menu, draw_diff_menu, draw_help_menu, draw_options_menu, draw_review_input,
        help_menu_list_visible_rows, options_menu_area, options_menu_block, review_input_area,
    },
    screen_layout::ScreenLayout,
    selector_menu::selector_menu_list_rows,
    sidebar::draw_file_sidebar,
    snapshot::{HitMap, OverlayLayer, RenderPlan, RenderSnapshot, RenderStatePlan},
    statusline::{draw_error_log, draw_filter_bar, draw_header},
    toast::draw_toasts,
};

fn overlay_component_id(layer: OverlayLayer) -> ComponentId {
    match layer {
        OverlayLayer::DiffMenu => ComponentId::DiffMenu,
        OverlayLayer::ReviewInput => ComponentId::ReviewInput,
        OverlayLayer::OptionsMenu => ComponentId::OptionsMenu,
        OverlayLayer::AnnotationMenu => ComponentId::AnnotationMenu,
        OverlayLayer::ColorSchemePicker => ComponentId::ColorSchemePicker,
        OverlayLayer::BranchMenu => ComponentId::BranchMenu,
        OverlayLayer::CommitMenu => ComponentId::CommitMenu,
        OverlayLayer::HelpMenu => ComponentId::HelpMenu,
    }
}

pub(crate) fn draw(frame: &mut Frame<'_>, app: &mut DiffApp) {
    let area = frame.area();
    if area.height == 0 {
        return;
    }

    let snapshot = RenderSnapshot::from_app(app);
    let layout = ScreenLayout::build(app, &snapshot, area);
    let layout_snapshot = layout.snapshot();
    let render_plan = RenderPlan {
        layout: layout_snapshot,
        hit_map: build_hit_map(app, &layout, area),
        state: build_render_state_plan(app, &layout, area),
    };
    app.apply_render_plan(render_plan);

    let mut compositor = Compositor::<AppRenderCtx<'_>>::new();
    compositor.push(RectComponent::new(ComponentId::Header, layout.header));
    if let Some(sidebar_area) = layout.sidebar {
        compositor.push(RectComponent::new(ComponentId::FileSidebar, sidebar_area));
    }
    compositor.push(RectComponent::new(ComponentId::DiffView, layout.diff));
    if let Some(filter_bar_area) = layout.filter_bar {
        compositor.push(RectComponent::new(ComponentId::FilterBar, filter_bar_area));
    }
    if let Some(error_log_area) = layout.error_log {
        compositor.push(RectComponent::new(
            ComponentId::ErrorLogPanel,
            error_log_area,
        ));
    }
    compositor.push(RectComponent::new(
        ComponentId::Toasts,
        render_plan.layout.body,
    ));
    for layer in snapshot.overlay_layers {
        compositor.push(RectComponent::new(
            overlay_component_id(layer),
            render_plan.layout.root,
        ));
    }
    let mut ctx = AppRenderCtx { app };
    compositor.render(frame, &mut ctx);
}

struct AppRenderCtx<'a> {
    app: &'a mut DiffApp,
}

impl RenderContext for AppRenderCtx<'_> {
    fn render_rect_component(&mut self, frame: &mut Frame<'_>, id: ComponentId, area: Rect) {
        match id {
            ComponentId::Header => draw_header_component(frame, self.app, area),
            ComponentId::FileSidebar => draw_file_sidebar(frame, self.app, area),
            ComponentId::DiffView => draw_diff(frame, self.app, area),
            ComponentId::FilterBar => draw_filter_bar_component(frame, self.app, area),
            ComponentId::ErrorLogPanel => draw_error_log_component(frame, self.app, area),
            ComponentId::Toasts => draw_toasts_component(frame, self.app, area),
            ComponentId::DiffMenu => draw_diff_menu(frame, self.app, area),
            ComponentId::ReviewInput => draw_review_input(frame, self.app, area),
            ComponentId::OptionsMenu => draw_options_menu(frame, self.app, area),
            ComponentId::AnnotationMenu => draw_annotation_menu(frame, self.app, area),
            ComponentId::ColorSchemePicker => draw_color_scheme_picker(frame, self.app, area),
            ComponentId::BranchMenu => draw_branch_menu(frame, self.app, area),
            ComponentId::CommitMenu => draw_commit_menu(frame, self.app, area),
            ComponentId::HelpMenu => draw_help_menu(frame, self.app, area),
            ComponentId::AnnotationDraftBindings
            | ComponentId::QuitKey
            | ComponentId::EditorShortcut
            | ComponentId::MouseScrollReset
            | ComponentId::FilterInput
            | ComponentId::AnnotationInput
            | ComponentId::ErrorLog
            | ComponentId::Prefix
            | ComponentId::GlobalAction
            | ComponentId::OpenMenuKey
            | ComponentId::ErrorLogResize
            | ComponentId::Navigation
            | ComponentId::OpenMenuScroll
            | ComponentId::FileSidebarResize => {}
        }
    }
}

fn build_render_state_plan(app: &DiffApp, layout: &ScreenLayout, root: Rect) -> RenderStatePlan {
    let option_items = app.filtered_options_menu_items();
    let options_menu_visible_rows = options_menu_area(app, root, &option_items).map(|area| {
        let inner = options_menu_block(app.config.theme).inner(area);
        selector_menu_list_rows(inner.height, 0)
    });
    let annotation_menu_visible_rows = annotation_menu_list_visible_rows(app, root);

    RenderStatePlan {
        terminal_area: root,
        file_sidebar_render_width: layout.sidebar_width,
        file_sidebar_visible_rows: layout.sidebar.map(|area| area.height as usize),
        viewport_rows: layout.diff.height as usize,
        viewport_width: layout.diff.width as usize,
        options_menu_visible_rows,
        annotation_menu_visible_rows,
        color_scheme_picker_visible_rows: color_scheme_picker_list_visible_rows(app, root),
        branch_menu_visible_rows: branch_menu_list_visible_rows(app, root),
        commit_menu_visible_rows: commit_menu_list_visible_rows(app, root),
        help_menu_visible_rows: help_menu_list_visible_rows(app, root),
    }
}

fn build_hit_map(app: &DiffApp, layout: &ScreenLayout, root: Rect) -> HitMap {
    let diff_choices = app.filtered_diff_choices();
    let option_items = app.filtered_options_menu_items();
    let annotation_items = app.filtered_annotation_menu_items();
    HitMap {
        diff_area: Some(layout.diff),
        diff_menu_area: diff_menu_area(app, root, &diff_choices),
        branch_menu_area: branch_menu_area(app, root),
        commit_menu_area: commit_menu_area(app, root),
        options_menu_area: options_menu_area(app, root, &option_items),
        annotation_menu_area: annotation_menu_area(app, root, &annotation_items),
        review_input_area: review_input_area(app, root),
        color_scheme_picker_area: color_scheme_picker_area(app, root),
        error_log_separator_row: layout.error_log.map(|area| area.y),
    }
}

fn draw_header_component(frame: &mut Frame<'_>, app: &DiffApp, area: Rect) {
    draw_header(frame, app, area);
}

fn draw_filter_bar_component(frame: &mut Frame<'_>, app: &DiffApp, area: Rect) {
    draw_filter_bar(frame, app, area);
}

fn draw_error_log_component(frame: &mut Frame<'_>, app: &DiffApp, area: Rect) {
    draw_error_log(frame, app, area);
}

fn draw_toasts_component(frame: &mut Frame<'_>, app: &DiffApp, area: Rect) {
    draw_toasts(frame, app, area);
}
