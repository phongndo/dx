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
    compositor::{Compositor, RectComponent},
    diff::draw_diff,
    menus::{
        draw_branch_menu, draw_color_scheme_picker, draw_commit_menu, draw_diff_menu,
        draw_help_menu, draw_options_menu, draw_review_input,
    },
    screen_layout::ScreenLayout,
    sidebar::draw_file_sidebar,
    snapshot::{OverlayLayer, RenderSnapshot},
    statusline::{draw_error_log, draw_filter_bar, draw_header},
    toast::draw_toasts,
};

type LayerRenderer = fn(&mut Frame<'_>, &mut DiffApp, Rect);

fn overlay_renderer(layer: OverlayLayer) -> LayerRenderer {
    match layer {
        OverlayLayer::DiffMenu => draw_diff_menu,
        OverlayLayer::ReviewInput => draw_review_input,
        OverlayLayer::OptionsMenu => draw_options_menu,
        OverlayLayer::ColorSchemePicker => draw_color_scheme_picker,
        OverlayLayer::BranchMenu => draw_branch_menu,
        OverlayLayer::CommitMenu => draw_commit_menu,
        OverlayLayer::HelpMenu => draw_help_menu,
    }
}

pub(crate) fn draw(frame: &mut Frame<'_>, app: &mut DiffApp) {
    let area = frame.area();
    app.set_terminal_area(area);
    if area.height == 0 {
        return;
    }

    let snapshot = RenderSnapshot::from_app(app);
    let layout = ScreenLayout::build(app, &snapshot, area);
    let layout_snapshot = layout.snapshot();
    layout.apply_to_app(app);

    let mut compositor = Compositor::new();
    compositor.push(RectComponent::new(layout.header, draw_header_component));
    if let Some(sidebar_area) = layout.sidebar {
        compositor.push(RectComponent::new(sidebar_area, draw_file_sidebar));
    }
    compositor.push(RectComponent::new(layout.diff, draw_diff));
    if let Some(filter_bar_area) = layout.filter_bar {
        compositor.push(RectComponent::new(
            filter_bar_area,
            draw_filter_bar_component,
        ));
    }
    if let Some(error_log_area) = layout.error_log {
        compositor.push(RectComponent::new(error_log_area, draw_error_log_component));
    }
    compositor.push(RectComponent::new(
        layout_snapshot.body,
        draw_toasts_component,
    ));
    for layer in snapshot.overlay_layers {
        compositor.push(RectComponent::new(
            layout_snapshot.root,
            overlay_renderer(layer),
        ));
    }
    compositor.render(frame, app);
}

fn draw_header_component(frame: &mut Frame<'_>, app: &mut DiffApp, area: Rect) {
    draw_header(frame, app, area);
}

fn draw_filter_bar_component(frame: &mut Frame<'_>, app: &mut DiffApp, area: Rect) {
    draw_filter_bar(frame, app, area);
}

fn draw_error_log_component(frame: &mut Frame<'_>, app: &mut DiffApp, area: Rect) {
    draw_error_log(frame, app, area);
}

fn draw_toasts_component(frame: &mut Frame<'_>, app: &mut DiffApp, area: Rect) {
    draw_toasts(frame, app, area);
}
