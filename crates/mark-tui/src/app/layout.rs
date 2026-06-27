use mark_syntax::LayoutSetting;

use super::{
    DiffApp, HunkFocusModelBehavior, MAX_LIVE_GREP_MATCHES, layout_override_from_setting,
    max_scroll_for_viewport, viewport_center_offset, viewport_focus_offset,
};
use crate::{
    controls::{DiffLayoutMode, default_layout_for_width},
    search::grep_match_rows,
};

impl DiffApp {
    pub(crate) fn viewport_focus_row(&self) -> usize {
        if self.viewport.line_wrapping {
            let row_count = self.wrapped_visual_row_count();
            let focus_scroll = self.viewport.scroll.saturating_add(viewport_focus_offset(
                self.viewport.scroll,
                row_count,
                self.viewport.viewport_rows,
            ));
            return self
                .model_row_at_scroll(focus_scroll)
                .map(|(row, _)| row)
                .unwrap_or_else(|| self.document.model.len().saturating_sub(1));
        }

        self.viewport
            .scroll
            .saturating_add(viewport_focus_offset(
                self.viewport.scroll,
                self.document.model.len(),
                self.viewport.viewport_rows,
            ))
            .min(self.document.model.len().saturating_sub(1))
    }

    pub(crate) fn set_viewport_rows(&mut self, rows: usize) {
        let rows = rows.max(1);
        let previous_rows = self.viewport.viewport_rows;
        if previous_rows == rows {
            return;
        }

        let centered_grep_match_row = self.selected_grep_match_row().filter(|row| {
            let previous_centered_scroll = row
                .saturating_sub(viewport_center_offset(previous_rows))
                .min(max_scroll_for_viewport(
                    self.document.model.len(),
                    previous_rows,
                ));
            self.viewport.scroll == previous_centered_scroll
        });

        self.viewport.viewport_rows = rows;
        if let Some(row) = centered_grep_match_row {
            self.set_scroll_centered_on(row);
        } else {
            self.set_scroll(self.viewport.scroll);
        }
        self.clamp_file_sidebar_scroll(self.visible_file_sidebar_rows());
        self.ensure_annotation_draft_visible();
    }

    pub(crate) fn set_viewport_width(&mut self, width: usize) {
        let width = width.max(1);
        if self.viewport.viewport_width == width {
            return;
        }

        let wrapped_position = self
            .viewport
            .line_wrapping
            .then(|| self.model_row_at_scroll(self.viewport.scroll))
            .flatten();
        self.viewport.viewport_width = width;
        self.invalidate_wrapped_visual_layout();
        self.set_horizontal_scroll(self.viewport.horizontal_scroll);
        if let Some((row, row_offset)) = wrapped_position {
            let row_scroll = self.wrapped_visual_scroll_for_model_row(row);
            let row_height = self.wrapped_visual_height_for_model_row(row);
            self.set_scroll(
                row_scroll.saturating_add(row_offset.min(row_height.saturating_sub(1))),
            );
        } else {
            self.set_scroll(self.viewport.scroll);
        }
        self.ensure_annotation_draft_visible();
    }

    pub(crate) fn toggle_layout(&mut self) {
        let layout = self.viewport.layout.toggled();
        self.set_manual_layout(layout);
    }

    pub(crate) fn set_manual_layout(&mut self, layout: DiffLayoutMode) {
        self.viewport.layout_override = Some(layout);
        self.set_layout(layout);
    }

    pub(crate) fn set_layout_setting(&mut self, setting: LayoutSetting) {
        match layout_override_from_setting(setting) {
            Some(layout) => self.set_manual_layout(layout),
            None => {
                self.viewport.layout_override = None;
                self.set_layout(default_layout_for_width(
                    self.viewport.viewport_width.min(u16::MAX as usize) as u16,
                ));
            }
        }
    }

    pub(crate) fn apply_responsive_layout(&mut self, width: u16) {
        let horizontal_scroll = self.viewport.horizontal_scroll;
        self.set_viewport_width(width as usize);
        let responsive_layout = default_layout_for_width(width);
        let layout = self.viewport.layout_override.unwrap_or(responsive_layout);
        self.set_layout(layout);
        self.set_horizontal_scroll(horizontal_scroll);
        self.runtime.dirty = true;
    }

    pub(crate) fn set_layout(&mut self, layout: DiffLayoutMode) {
        if self.viewport.layout == layout {
            return;
        }

        self.viewport.layout = layout;
        let search_result = self.document.search_index.search_with_grep_match_limit(
            &self.filters.file_filter,
            &self.filters.grep_filter,
            MAX_LIVE_GREP_MATCHES,
        );
        self.replace_model(&search_result.visible_files, HunkFocusModelBehavior::Clear);
        self.filters.grep_matches =
            grep_match_rows(&self.document.model, &search_result.grep_matches);
        self.filters.grep_matches_truncated = search_result.grep_matches_truncated;
        self.filters.selected_grep_match = None;
        self.set_horizontal_scroll(self.viewport.horizontal_scroll);
        let scroll = self
            .document
            .model
            .file_start_row(self.sidebar.selected_file)
            .map(|row| self.scroll_for_model_row(row))
            .unwrap_or_default();
        self.set_scroll(scroll);
        self.sync_grep_match_selection_to_scroll();
        self.ensure_annotation_draft_visible();
        self.runtime.dirty = true;
    }
}
