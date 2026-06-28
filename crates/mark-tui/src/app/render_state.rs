use ratatui::layout::Rect;

use super::DiffApp;
use crate::render::snapshot::{HitMap, RenderPlan, RenderStatePlan};

impl DiffApp {
    pub(crate) fn set_terminal_area(&mut self, area: Rect) {
        if self.viewport.set_terminal_area(area) {
            self.sync_help_menu_visible_rows();
        }
    }

    #[cfg(test)]
    pub(crate) fn set_rendered_diff_area(&mut self, area: Rect) {
        if self.viewport.set_rendered_diff_area(Some(area)) {
            self.clear_diff_mouse_hover();
        }
        self.runtime.hit_map.diff_area = Some(area);
    }

    pub(crate) fn set_rendered_diff_menu_area(&mut self, area: Option<Rect>) {
        self.overlays.rendered_diff_menu_area = area.filter(|_| self.overlays.diff_menu_open);
        self.runtime.hit_map.diff_menu_area = self.overlays.rendered_diff_menu_area;
    }

    pub(crate) fn set_rendered_branch_menu_area(&mut self, area: Option<Rect>) {
        self.overlays.rendered_branch_menu_area =
            area.filter(|_| self.refs.branch_menu_open.is_some());
        self.runtime.hit_map.branch_menu_area = self.overlays.rendered_branch_menu_area;
    }

    pub(crate) fn set_rendered_color_scheme_picker_area(&mut self, area: Option<Rect>) {
        self.overlays.rendered_color_scheme_picker_area =
            area.filter(|_| self.overlays.color_scheme_picker_open);
        self.runtime.hit_map.color_scheme_picker_area =
            self.overlays.rendered_color_scheme_picker_area;
    }

    pub(crate) fn apply_render_hit_map(&mut self, mut hit_map: HitMap) {
        hit_map.diff_menu_area = hit_map
            .diff_menu_area
            .filter(|_| self.overlays.diff_menu_open);
        hit_map.branch_menu_area = hit_map
            .branch_menu_area
            .filter(|_| self.refs.branch_menu_open.is_some());
        hit_map.commit_menu_area = hit_map
            .commit_menu_area
            .filter(|_| self.refs.commit_menu_open);
        hit_map.options_menu_area = hit_map
            .options_menu_area
            .filter(|_| self.overlays.options_menu_open);
        hit_map.annotation_menu_area = hit_map
            .annotation_menu_area
            .filter(|_| self.overlays.annotation_menu_open);
        hit_map.review_input_area = hit_map
            .review_input_area
            .filter(|_| self.overlays.review_input_open);
        hit_map.color_scheme_picker_area = hit_map
            .color_scheme_picker_area
            .filter(|_| self.overlays.color_scheme_picker_open);
        hit_map.error_log_separator_row = hit_map
            .error_log_separator_row
            .filter(|_| self.notifications.error_log.is_some());

        if self.viewport.set_rendered_diff_area(hit_map.diff_area) {
            self.clear_diff_mouse_hover();
        }
        self.overlays.rendered_diff_menu_area = hit_map.diff_menu_area;
        self.overlays.rendered_branch_menu_area = hit_map.branch_menu_area;
        self.overlays.rendered_commit_menu_area = hit_map.commit_menu_area;
        self.overlays.rendered_annotation_menu_area = hit_map.annotation_menu_area;
        self.overlays.rendered_review_input_area = hit_map.review_input_area;
        self.overlays.rendered_color_scheme_picker_area = hit_map.color_scheme_picker_area;
        self.notifications.rendered_error_log_separator_row = hit_map.error_log_separator_row;
        self.runtime.hit_map = hit_map;
    }

    pub(crate) fn apply_render_plan(&mut self, plan: RenderPlan) {
        self.apply_render_state_plan(plan.state);
        self.apply_render_hit_map(plan.hit_map);
    }

    fn apply_render_state_plan(&mut self, state: RenderStatePlan) {
        self.set_terminal_area(state.terminal_area);
        self.sidebar.file_sidebar_render_width = state.file_sidebar_render_width;
        if let Some(rows) = state.file_sidebar_visible_rows {
            self.clamp_file_sidebar_scroll(rows);
        }
        self.set_viewport_rows(state.viewport_rows);
        self.set_viewport_width(state.viewport_width);

        if let Some(rows) = state.options_menu_visible_rows {
            self.ensure_options_menu_selection_visible(rows);
        }
        if let Some(rows) = state.annotation_menu_visible_rows {
            self.ensure_annotation_menu_selection_visible(rows);
        }
        if let Some(rows) = state.color_scheme_picker_visible_rows
            && self.overlays.color_scheme_picker_open
        {
            let len = self.filtered_color_schemes().len();
            self.overlays
                .color_scheme_picker
                .ensure_selected_visible(len, rows);
        }
        if let Some(rows) = state.branch_menu_visible_rows {
            self.ensure_branch_selection_visible_for_rows(rows);
        }
        if let Some(rows) = state.commit_menu_visible_rows {
            self.ensure_commit_selection_visible_for_rows(rows);
        }
        if let Some(rows) = state.help_menu_visible_rows
            && self.overlays.help_menu_open
        {
            self.overlays.help_menu_visible_rows = rows;
            self.clamp_help_menu_scroll(rows);
        }
    }
}
