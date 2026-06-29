use super::super::DiffApp;
use crate::render::viewport_plan::visual_scroll_for_viewport_row;

impl DiffApp {
    pub(crate) fn update_diff_mouse_hover(&mut self, column: u16, row: u16) {
        let next = self.diff_mouse_hover_in_diff_area(column, row);
        if self.viewport.mouse_hover != next {
            self.viewport.mouse_hover = next;
            self.runtime.dirty = true;
        }
    }

    pub(crate) fn clear_diff_mouse_hover(&mut self) {
        if self.viewport.mouse_hover.take().is_some() {
            self.runtime.dirty = true;
        }
    }

    fn diff_mouse_hover_in_diff_area(&self, column: u16, row: u16) -> Option<(u16, u16)> {
        if self.diff_modal_blocks_mouse_hover() {
            return None;
        }
        let area = self.viewport.rendered_diff_area?;
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
        self.overlays.help_menu_is_open()
            || self.overlays.color_scheme_picker_is_open()
            || self.overlays.options_menu_is_open()
            || self.overlays.diff_menu_is_open()
            || self.overlays.review_input_is_open()
            || self.refs.commit_menu_is_open()
            || self.refs.branch_menu_is_open()
            || self.filters.filter_input.is_some()
            || self.annotations_state.annotation_draft.is_some()
    }

    pub(crate) fn diff_mouse_highlight_visual_row(&self) -> Option<usize> {
        let (_, viewport_row) = self.viewport.mouse_hover?;
        visual_scroll_for_viewport_row(self, viewport_row)
    }
}
