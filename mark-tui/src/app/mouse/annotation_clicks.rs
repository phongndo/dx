use super::super::{DiffApp, HunkFocusScrollBehavior, annotation_scroll_for_block};
use crate::annotation::{AnnotationDraft, AnnotationKey};
use crate::model::UiRow;
use crate::render::annotations::{annotation_compose_block_height, annotation_hit_at_column};
use crate::render::viewport_plan::{
    annotation_saved_key_at_bottom_border, annotation_saved_key_at_top_border,
    compose_block_bottom_viewport_row, compose_block_top_viewport_row,
    visual_scroll_for_viewport_row,
};

impl DiffApp {
    pub(super) fn diff_viewport_position(&self, column: u16, row: u16) -> Option<(u16, u16)> {
        let area = self.viewport.rendered_diff_area?;
        if area.width == 0
            || area.height == 0
            || column < area.x
            || row < area.y
            || column >= area.x.saturating_add(area.width)
            || row >= area.y.saturating_add(area.height)
        {
            return None;
        }

        Some((column.saturating_sub(area.x), row.saturating_sub(area.y)))
    }

    pub(crate) fn annotation_anchor_visual_scroll(&self, model_row_index: usize) -> usize {
        if self.viewport.line_wrapping {
            let start = self.wrapped_visual_scroll_for_model_row(model_row_index);
            let height = self.wrapped_visual_height_for_model_row(model_row_index);
            start.saturating_add(height.saturating_sub(1))
        } else {
            model_row_index
        }
    }

    pub(crate) fn annotation_label(&self, key: &AnnotationKey) -> Option<String> {
        Some(format!("{} {}{}", key.path, key.side.label(), key.line))
    }

    pub(super) fn handle_annotation_submit_click(&mut self, viewport_row: u16) -> bool {
        let Some(draft) = self.annotations_state.annotation_draft.as_ref() else {
            return false;
        };
        if compose_block_bottom_viewport_row(self, draft.model_row_index) != Some(viewport_row) {
            return false;
        }
        let draft = self
            .annotations_state
            .annotation_draft
            .take()
            .expect("draft");
        self.commit_annotation_draft(draft);
        true
    }

    pub(super) fn handle_annotation_edit_click(&mut self, viewport_row: u16) -> bool {
        if self.annotations_state.annotation_draft.is_some() {
            return false;
        }
        let Some((model_row, key)) = annotation_saved_key_at_bottom_border(self, viewport_row)
        else {
            return false;
        };
        self.open_annotation_draft_for_key(key, model_row)
    }

    pub(super) fn handle_annotation_close_click(&mut self, viewport_row: u16) -> bool {
        if let Some(draft) = self.annotations_state.annotation_draft.as_ref() {
            if compose_block_top_viewport_row(self, draft.model_row_index) == Some(viewport_row) {
                self.annotations_state.annotation_draft = None;
                self.set_scroll_with_grep_sync(
                    self.viewport.scroll,
                    false,
                    HunkFocusScrollBehavior::Preserve,
                );
                self.runtime.dirty = true;
                return true;
            }
            return false;
        }

        if self.filters.filter_input.is_some() {
            return false;
        }

        let Some((_model_row, key)) = annotation_saved_key_at_top_border(self, viewport_row) else {
            return false;
        };
        if self.annotations_state.annotations.remove(&key).is_some() {
            self.set_scroll_with_grep_sync(
                self.viewport.scroll,
                false,
                HunkFocusScrollBehavior::Preserve,
            );
            self.runtime.dirty = true;
            return true;
        }
        false
    }

    pub(super) fn try_open_annotation_draft_at_viewport_row(
        &mut self,
        viewport_row: u16,
        column: u16,
    ) -> bool {
        if self.filters.filter_input.is_some() {
            return false;
        }
        if self.annotations_state.annotation_draft.is_some() {
            return false;
        }
        let Some(visual_row) = visual_scroll_for_viewport_row(self, viewport_row) else {
            return false;
        };
        let row_index = if self.viewport.line_wrapping {
            let Some((row_index, _)) = self.model_row_at_scroll(visual_row) else {
                return false;
            };
            row_index
        } else {
            visual_row
        };
        let Some(row) = self.document.model.row(row_index) else {
            return false;
        };
        if !crate::render::viewport_plan::row_has_diff_code_content(row) {
            return false;
        }
        if self.annotation_anchor_visual_scroll(row_index) != visual_row {
            return false;
        }
        let Some(key) = self.annotation_key_for_add_click(row, column) else {
            return false;
        };
        self.open_annotation_draft_for_key(key, row_index)
    }

    pub(super) fn annotation_key_for_add_click(
        &self,
        row: UiRow,
        column: u16,
    ) -> Option<AnnotationKey> {
        if !annotation_hit_at_column(column, self.viewport.viewport_width) {
            return None;
        }
        AnnotationKey::from_ui_row(&self.document.changeset, row)
    }

    pub(super) fn open_annotation_draft_for_key(
        &mut self,
        key: AnnotationKey,
        model_row_index: usize,
    ) -> bool {
        if self.filters.filter_input.is_some() {
            return false;
        }
        let existing = self
            .annotations_state
            .annotations
            .get(&key)
            .cloned()
            .unwrap_or_default();
        let cursor = existing.len();
        self.annotations_state.annotation_draft = Some(AnnotationDraft {
            key,
            model_row_index,
            input: existing,
            cursor,
        });
        self.ensure_annotation_draft_visible();
        self.runtime.dirty = true;
        true
    }

    pub(in crate::app) fn ensure_annotation_draft_visible(&mut self) {
        let Some((model_row, anchor, desired_scroll)) = self
            .annotations_state
            .annotation_draft
            .as_ref()
            .map(|draft| {
                let anchor = self.annotation_anchor_visual_scroll(draft.model_row_index);
                let height = annotation_compose_block_height(draft, self.viewport.viewport_width);
                (
                    draft.model_row_index,
                    anchor,
                    annotation_scroll_for_block(anchor, height, self.viewport.viewport_rows),
                )
            })
        else {
            return;
        };

        if compose_block_bottom_viewport_row(self, model_row).is_some() {
            return;
        }
        if desired_scroll != self.viewport.scroll {
            self.set_scroll_with_grep_sync(
                desired_scroll,
                false,
                HunkFocusScrollBehavior::Preserve,
            );
        }

        // The compose block is emitted only while the annotated row's anchor is still visible.
        // If the draft is too tall for the viewport, the footer can never be shown; do not
        // chase it past the anchor or the editor disappears entirely.
        let max_scroll = self.max_scroll().min(anchor);
        while compose_block_bottom_viewport_row(self, model_row).is_none()
            && self.viewport.scroll < max_scroll
        {
            let previous_scroll = self.viewport.scroll;
            self.set_scroll_with_grep_sync(
                self.viewport.scroll.saturating_add(1),
                false,
                HunkFocusScrollBehavior::Preserve,
            );
            if self.viewport.scroll == previous_scroll {
                break;
            }
        }
    }
}
