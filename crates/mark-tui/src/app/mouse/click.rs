use super::super::DiffApp;
use crate::render::annotations::{
    annotation_close_hit_at_column, annotation_edit_hit_at_column, annotation_submit_hit_at_column,
};
use crate::render::menus::diff_selector_width;
use crate::render::viewport_plan::model_row_for_viewport_row;

impl DiffApp {
    pub(crate) fn handle_click(&mut self, column: u16, row: u16) {
        let clicked_selector = row == 0 && column < diff_selector_width(&self.document.options);
        let clicked_branch_selector = (row == 0)
            .then(|| self.branch_selector_at(column))
            .flatten();
        let clicked_commit_selector = row == 0 && self.commit_selector_at(column);

        if self.overlays.review_input_open {
            if self.is_rendered_review_input_position(column, row) {
                self.runtime.dirty = true;
                return;
            }

            self.close_review_input();
            if clicked_selector {
                self.toggle_diff_menu();
            }
            return;
        }

        if self.refs.commit_menu_open {
            if let Some(rev) = self.commit_choice_at(column, row) {
                self.close_commit_menu();
                self.select_show_commit(rev);
                return;
            }

            if self.is_rendered_commit_menu_position(column, row) {
                return;
            }

            if clicked_commit_selector {
                self.toggle_commit_menu();
                return;
            }

            self.close_commit_menu();
            if clicked_selector {
                self.toggle_diff_menu();
            }
            return;
        }

        if let Some(menu) = self.refs.branch_menu_open {
            if let Some(branch) = self.branch_choice_at(menu, column, row) {
                self.close_branch_menu();
                self.select_branch(menu, branch);
                return;
            }

            if self.is_rendered_branch_menu_position(column, row) {
                return;
            }

            if let Some(clicked_menu) = clicked_branch_selector {
                self.toggle_branch_menu(clicked_menu);
                return;
            }

            self.close_branch_menu();
            if clicked_selector {
                self.toggle_diff_menu();
            }
            return;
        }

        if self.overlays.diff_menu_open {
            if let Some(choice) = self.diff_choice_at(column, row) {
                self.close_diff_menu();
                self.select_diff_choice(choice);
                return;
            }

            if self.is_rendered_diff_menu_position(column, row) {
                return;
            }

            if let Some(menu) = clicked_branch_selector {
                self.close_diff_menu();
                self.toggle_branch_menu(menu);
                return;
            }

            if clicked_selector {
                self.toggle_diff_menu();
                return;
            }

            self.close_diff_menu();
            return;
        }

        if self.overlays.color_scheme_picker_open {
            self.close_color_scheme_picker();
            return;
        }

        if self.overlays.options_menu_open {
            self.close_options_menu();
            return;
        }

        if clicked_selector {
            self.toggle_diff_menu();
        } else if clicked_commit_selector {
            self.toggle_commit_menu();
        } else if let Some(menu) = clicked_branch_selector {
            self.toggle_branch_menu(menu);
        } else if !self.handle_file_sidebar_click(column, row) {
            self.handle_diff_click(column, row);
        }
    }

    pub(crate) fn handle_file_sidebar_click(&mut self, column: u16, row: u16) -> bool {
        if !self.is_file_sidebar_position(column, row) {
            return false;
        }

        let position = self
            .sidebar
            .file_sidebar_scroll
            .saturating_add(usize::from(row - 1));
        let Some(file) = self.document.model.visible_files().get(position).copied() else {
            return false;
        };

        self.select_file(file);
        true
    }

    pub(crate) fn handle_diff_click(&mut self, column: u16, row: u16) -> bool {
        let Some((diff_column, viewport_row)) = self.diff_viewport_position(column, row) else {
            return false;
        };
        let width = self.viewport.viewport_width;
        if annotation_submit_hit_at_column(diff_column, width)
            && self.handle_annotation_submit_click(viewport_row)
        {
            return true;
        }
        if annotation_edit_hit_at_column(diff_column, width)
            && self.handle_annotation_edit_click(viewport_row)
        {
            return true;
        }
        if annotation_close_hit_at_column(diff_column, width)
            && self.handle_annotation_close_click(viewport_row)
        {
            return true;
        }
        if self
            .viewport
            .mouse_hover
            .is_some_and(|(_, hover_row)| hover_row == viewport_row)
            && self.try_open_annotation_draft_at_viewport_row(viewport_row, diff_column)
        {
            return true;
        }

        let Some(model_row) = model_row_for_viewport_row(self, viewport_row) else {
            return false;
        };
        self.handle_context_at_row(model_row)
    }
}
