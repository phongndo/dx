use super::DiffApp;
use crate::model::FileIndex;
use crate::render::sidebar::max_file_sidebar_width;
use crate::theme::FILE_SIDEBAR_MIN_WIDTH;

impl DiffApp {
    pub(crate) fn scroll_file_sidebar_by(&mut self, delta: isize) {
        let next = if delta < 0 {
            self.sidebar
                .file_sidebar_scroll
                .saturating_sub(delta.unsigned_abs())
        } else {
            self.sidebar
                .file_sidebar_scroll
                .saturating_add(delta as usize)
        };
        self.set_file_sidebar_scroll(next);
    }

    pub(crate) fn set_file_sidebar_scroll(&mut self, scroll: usize) {
        let previous_scroll = self.sidebar.file_sidebar_scroll;
        self.sidebar.file_sidebar_scroll =
            scroll.min(self.max_file_sidebar_scroll(self.visible_file_sidebar_rows()));
        if self.sidebar.file_sidebar_scroll != previous_scroll {
            self.runtime.dirty = true;
        }
    }

    pub(crate) fn set_file_sidebar_width(&mut self, width: u16) {
        let total_width = self
            .sidebar
            .file_sidebar_render_width
            .saturating_add(self.viewport.viewport_width.min(usize::from(u16::MAX)) as u16);
        let max_width = max_file_sidebar_width(total_width);
        if max_width == 0 {
            return;
        }

        let width = width.clamp(FILE_SIDEBAR_MIN_WIDTH, max_width);
        if self.sidebar.file_sidebar_width != Some(width) {
            self.sidebar.file_sidebar_width = Some(width);
            self.set_horizontal_scroll(self.viewport.horizontal_scroll);
            self.runtime.dirty = true;
        }
    }

    pub(crate) fn clamp_file_sidebar_scroll(&mut self, visible_rows: usize) {
        self.sidebar.file_sidebar_scroll = self
            .sidebar
            .file_sidebar_scroll
            .min(self.max_file_sidebar_scroll(visible_rows));
    }

    pub(crate) fn move_file(&mut self, delta: isize) {
        let visible_files = self.document.model.visible_files();
        if visible_files.is_empty() {
            return;
        }

        let current = self
            .document
            .model
            .visible_file_position(self.sidebar.selected_file.get())
            .unwrap_or_default();
        let next = if delta < 0 {
            current.saturating_sub(delta.unsigned_abs())
        } else {
            current.saturating_add(delta as usize)
        }
        .min(visible_files.len() - 1);

        self.select_file(visible_files[next].get());
    }

    pub(crate) fn select_file(&mut self, file: usize) {
        if self.document.model.visible_files().is_empty() {
            return;
        }

        let next = if self.document.model.file_start_row(file).is_some() {
            file
        } else {
            self.document
                .model
                .visible_files()
                .first()
                .copied()
                .map(|file| file.get())
                .unwrap_or_default()
        };

        if FileIndex::new(next) == self.sidebar.selected_file {
            self.ensure_file_sidebar_selection_visible(self.visible_file_sidebar_rows());
            self.runtime.dirty = true;
            return;
        }

        if let Some(row) = self.document.model.hunk_start_row(next, 0) {
            self.focus_hunk_row(row);
            return;
        }

        self.sidebar.selected_file = FileIndex::new(next);
        if let Some(row) = self.document.model.file_start_row(next) {
            self.set_scroll(self.scroll_for_model_row(row));
        } else {
            self.runtime.dirty = true;
        }
        self.ensure_file_sidebar_selection_visible(self.visible_file_sidebar_rows());
    }

    pub(crate) fn toggle_file_sidebar(&mut self) {
        self.sidebar.file_sidebar_open = !self.sidebar.file_sidebar_open;
        self.sidebar.finish_resize();
        self.close_color_scheme_picker();
        self.overlays.hide_diff_menu();
        self.overlays.hide_options_menu();
        self.close_review_input();
        self.close_branch_menu();
        self.ensure_file_sidebar_selection_visible(self.visible_file_sidebar_rows());
        self.runtime.dirty = true;
    }

    pub(crate) fn visible_file_sidebar_rows(&self) -> usize {
        self.viewport.viewport_rows
    }

    pub(crate) fn ensure_file_sidebar_selection_visible(&mut self, visible_rows: usize) {
        let Some(selected_position) = self
            .document
            .model
            .visible_file_position(self.sidebar.selected_file.get())
        else {
            self.sidebar.file_sidebar_scroll = 0;
            return;
        };
        if visible_rows == 0 {
            self.sidebar.file_sidebar_scroll = 0;
            return;
        }

        if selected_position < self.sidebar.file_sidebar_scroll {
            self.sidebar.file_sidebar_scroll = selected_position;
        } else if selected_position
            >= self
                .sidebar
                .file_sidebar_scroll
                .saturating_add(visible_rows)
        {
            self.sidebar.file_sidebar_scroll = self
                .document
                .model
                .visible_file_position(self.sidebar.selected_file.get())
                .unwrap_or_default()
                .saturating_add(1)
                .saturating_sub(visible_rows);
        }

        self.sidebar.file_sidebar_scroll = self
            .sidebar
            .file_sidebar_scroll
            .min(self.max_file_sidebar_scroll(visible_rows));
    }

    pub(crate) fn max_file_sidebar_scroll(&self, visible_rows: usize) -> usize {
        self.document
            .model
            .visible_files()
            .len()
            .saturating_sub(visible_rows.max(1))
    }
}
