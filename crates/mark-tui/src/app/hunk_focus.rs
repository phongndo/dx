use super::DiffApp;
use crate::model::{FileIndex, HunkIndex};

impl DiffApp {
    pub(crate) fn next_hunk(&mut self) {
        if let Some(row) = self
            .document
            .model
            .next_hunk_row(self.hunk_navigation_anchor_row())
        {
            self.focus_hunk_row(row);
        }
    }

    pub(crate) fn previous_hunk(&mut self) {
        if let Some(row) = self
            .document
            .model
            .previous_hunk_row(self.hunk_navigation_anchor_row())
        {
            self.focus_hunk_row(row);
        }
    }

    pub(crate) fn move_focused_hunk(&mut self, delta: isize) {
        let anchor = self.hunk_navigation_anchor_row();
        let next = if delta < 0 {
            self.document.model.previous_hunk_row(anchor)
        } else {
            self.document.model.next_hunk_row(anchor)
        };
        if let Some(row) = next {
            self.focus_hunk_row(row);
        }
    }

    pub(crate) fn hunk_navigation_anchor_row(&self) -> usize {
        if let Some((file, hunk)) = self.focused_hunk_for_viewport(self.viewport.viewport_rows)
            && let Some(row) = self.document.model.hunk_start_row(file.get(), hunk.get())
        {
            return row;
        }

        self.viewport_focus_row()
    }

    pub(crate) fn focus_hunk_row(&mut self, row: usize) {
        let target_hunk = self.document.model.row(row).and_then(|row| row.hunk_key());
        let previous_hunk = self.viewport.manual_hunk_focus;
        self.clear_manual_hunk_focus();

        let Some((file, hunk)) = target_hunk else {
            self.set_scroll_centered_on(row);
            return;
        };

        self.set_scroll_focused_on_hunk(file, hunk);

        if let Some(row) = self.document.model.hunk_start_row(file, hunk)
            && self.model_row_rendered_at_scroll(
                self.viewport.scroll,
                self.viewport.viewport_rows,
                row,
            )
        {
            let previous_file = self.sidebar.selected_file;
            self.viewport.manual_hunk_focus = Some((FileIndex::new(file), HunkIndex::new(hunk)));
            self.sidebar.selected_file = FileIndex::new(file);
            self.ensure_file_sidebar_selection_visible(self.visible_file_sidebar_rows());
            if self.viewport.manual_hunk_focus != previous_hunk
                || self.sidebar.selected_file != previous_file
            {
                self.runtime.dirty = true;
            }
        }
    }
}
