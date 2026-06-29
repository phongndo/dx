use super::DiffApp;

impl DiffApp {
    pub(crate) fn toggle_diff_menu(&mut self) {
        if self.overlays.diff_menu_is_open() {
            self.close_diff_menu();
        } else {
            self.open_diff_menu();
        }
    }

    pub(crate) fn open_diff_menu(&mut self) {
        let choices = self.diff_menu_choices();
        if choices.is_empty() {
            return;
        }
        self.close_color_scheme_picker();
        self.overlays.diff_menu.reset();
        self.overlays.open_diff_menu();
        self.close_branch_menu();
        self.close_review_input();
        self.close_commit_menu();
        self.runtime.dirty = true;
    }

    pub(crate) fn close_diff_menu(&mut self) {
        if self.overlays.close_diff_menu() {
            self.runtime.hit_map.diff_menu_area = None;
            self.runtime.dirty = true;
        }
    }

    pub(crate) fn open_review_input(&mut self) {
        self.close_color_scheme_picker();
        self.overlays.open_review_input();
        self.overlays.diff_menu.reset_input();
        self.set_rendered_diff_menu_area(None);
        self.close_branch_menu();
        self.close_commit_menu();
        self.runtime.dirty = true;
    }

    pub(crate) fn close_review_input(&mut self) {
        if self.overlays.close_review_input() {
            self.runtime.hit_map.review_input_area = None;
            self.runtime.dirty = true;
        }
    }
}
