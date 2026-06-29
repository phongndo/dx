use std::time::Instant;

use super::super::{
    DiffApp, HunkFocusScrollBehavior, MouseScrollDirection, diff_content_width,
    hunk_focus_row_range, max_scroll_for_annotated_viewport, viewport_center_offset,
};
use crate::model::FileIndex;
use crate::render::annotations::{annotation_compose_block_height, annotation_saved_block_height};

impl DiffApp {
    pub(crate) fn scroll_by(&mut self, delta: isize) {
        let next = if delta < 0 {
            self.viewport.scroll.saturating_sub(delta.unsigned_abs())
        } else {
            self.viewport.scroll.saturating_add(delta as usize)
        };
        self.set_scroll(next);
    }

    pub(crate) fn scroll_or_focus_hunk(&mut self, delta: isize) {
        let previous_scroll = self.viewport.scroll;
        self.scroll_by(delta);
        if self.viewport.scroll == previous_scroll {
            self.move_focused_hunk(delta);
        }
    }

    pub(crate) fn mouse_scroll_or_focus_hunk(&mut self, direction: MouseScrollDirection) {
        let delta = self
            .input
            .mouse_scroll
            .scroll_delta(direction, Instant::now());
        let previous_scroll = self.viewport.scroll;
        self.scroll_by(delta);
        if self.viewport.scroll == previous_scroll {
            let hunk_delta = self.input.mouse_scroll.hunk_focus_delta(direction);
            if hunk_delta != 0 {
                self.move_focused_hunk(hunk_delta);
            }
        } else {
            self.input.mouse_scroll.reset_hunk_focus_ticks();
        }
    }

    pub(crate) fn scroll_horizontally_by(&mut self, delta: isize) {
        let next = if delta < 0 {
            self.viewport
                .horizontal_scroll
                .saturating_sub(delta.unsigned_abs())
        } else {
            self.viewport
                .horizontal_scroll
                .saturating_add(delta as usize)
        };
        self.set_horizontal_scroll(next);
    }

    pub(crate) fn set_horizontal_scroll(&mut self, scroll: usize) {
        let previous_scroll = self.viewport.horizontal_scroll;
        self.viewport.horizontal_scroll = scroll.min(self.max_horizontal_scroll());
        if self.viewport.horizontal_scroll != previous_scroll {
            self.clear_diff_mouse_hover();
            self.runtime.dirty = true;
        }
    }

    pub(crate) fn set_scroll(&mut self, scroll: usize) {
        self.set_scroll_with_grep_sync(scroll, true, HunkFocusScrollBehavior::ClearOnScroll);
    }

    pub(in crate::app) fn scroll_for_model_row(&self, row: usize) -> usize {
        if self.viewport.line_wrapping {
            self.wrapped_visual_scroll_for_model_row(row)
        } else {
            row
        }
    }

    pub(in crate::app) fn relative_scroll_from_file_start(&self, file: usize) -> usize {
        self.document
            .model
            .file_start_row(file)
            .map(|start| {
                self.viewport
                    .scroll
                    .saturating_sub(self.scroll_for_model_row(start))
            })
            .unwrap_or_default()
    }

    pub(crate) fn set_scroll_centered_on(&mut self, row: usize) {
        let center_offset = viewport_center_offset(self.viewport.viewport_rows);
        let scroll = self.scroll_for_model_row(row).saturating_sub(center_offset);
        let scroll = self.scroll_with_model_row_rendered(scroll, row);
        self.set_scroll_with_grep_sync(scroll, false, HunkFocusScrollBehavior::ClearOnScroll);
    }

    pub(crate) fn set_scroll_focused_on_hunk(&mut self, file: usize, hunk: usize) {
        let Some((range, hunk_start_row)) = hunk_focus_row_range(&self.document.model, file, hunk)
        else {
            return;
        };

        let focus_start = self.scroll_for_model_row(range.start);
        let focus_end = self
            .scroll_for_model_row(range.end)
            .max(focus_start.saturating_add(1));
        let hunk_start = self.scroll_for_model_row(hunk_start_row);
        let focus_rows = focus_end.saturating_sub(focus_start).max(1);
        let scroll = if focus_rows > self.viewport.viewport_rows {
            // Oversized focus ranges cannot be fully centered. Keep the first
            // useful context row when possible, but never so much context that
            // the hunk header itself falls below the viewport.
            focus_start.max(
                hunk_start
                    .saturating_add(1)
                    .saturating_sub(self.viewport.viewport_rows),
            )
        } else {
            let focus_center = focus_start.saturating_add(focus_rows.saturating_sub(1) / 2);
            focus_center.saturating_sub(viewport_center_offset(self.viewport.viewport_rows))
        };
        let scroll = self.scroll_with_model_row_rendered(scroll, hunk_start_row);
        self.set_scroll_with_grep_sync(scroll, false, HunkFocusScrollBehavior::Preserve);
    }

    pub(in crate::app) fn set_scroll_with_grep_sync(
        &mut self,
        scroll: usize,
        sync_grep: bool,
        hunk_focus_behavior: HunkFocusScrollBehavior,
    ) {
        let previous_scroll = self.viewport.scroll;
        let previous_file = self.sidebar.selected_file;
        self.viewport.scroll = scroll.min(self.max_scroll());
        if self.viewport.scroll != previous_scroll
            && hunk_focus_behavior == HunkFocusScrollBehavior::ClearOnScroll
        {
            self.clear_manual_hunk_focus();
        }
        if let Some(file) = if self.viewport.line_wrapping {
            self.model_row_at_scroll(self.viewport.scroll)
                .and_then(|(row, _)| self.document.model.file_at_row(row))
        } else {
            self.document.model.file_at_row(self.viewport.scroll)
        } {
            self.sidebar.selected_file = FileIndex::new(file);
        }
        if sync_grep && self.viewport.scroll != previous_scroll {
            self.sync_grep_match_selection_to_scroll();
        }
        if self.viewport.scroll != previous_scroll || self.sidebar.selected_file != previous_file {
            if self.viewport.scroll != previous_scroll {
                self.clear_diff_mouse_hover();
            }
            self.runtime.dirty = true;
        }
    }

    pub(crate) fn max_scroll(&self) -> usize {
        let row_count = if self.viewport.line_wrapping {
            self.wrapped_visual_row_count()
        } else {
            self.document.model.len()
        };
        self.max_scroll_with_annotations(row_count)
    }

    pub(in crate::app) fn max_scroll_with_annotations(&self, row_count: usize) -> usize {
        let mut blocks = Vec::new();
        let draft_key = self
            .annotations_state
            .annotation_draft
            .as_ref()
            .map(|draft| &draft.key);
        for (key, text) in &self.annotations_state.annotations {
            if let Some(model_row) = self.annotation_model_row(key) {
                if draft_key == Some(key) {
                    continue;
                }
                let anchor = self.annotation_anchor_visual_scroll(model_row);
                let height = annotation_saved_block_height(text, self.viewport.viewport_width);
                blocks.push((anchor, height));
            }
        }
        if let Some(draft) = self.annotations_state.annotation_draft.as_ref() {
            let anchor = self.annotation_anchor_visual_scroll(draft.model_row_index);
            let height = annotation_compose_block_height(draft, self.viewport.viewport_width);
            blocks.push((anchor, height));
        }
        max_scroll_for_annotated_viewport(row_count, self.viewport.viewport_rows, blocks)
    }

    pub(crate) fn max_horizontal_scroll(&self) -> usize {
        if self.viewport.line_wrapping {
            return 0;
        }

        self.document
            .max_line_width
            .saturating_sub(diff_content_width(
                self.viewport.layout,
                self.viewport.viewport_width,
            ))
    }
}
