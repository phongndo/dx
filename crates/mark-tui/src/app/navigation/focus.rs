use super::super::{
    DiffApp, HunkFocusModelBehavior, HunkFocusSearch, RenderedDiffRow,
    find_rendered_diff_row_outward, max_scroll_for_viewport, viewport_focus_offset,
};
use crate::{
    model::{FileIndex, HunkIndex, UiModel, UiRow},
    render::viewport_plan::{ViewportSlotKind, plan_diff_viewport_rows_at_scroll},
};

impl DiffApp {
    pub(in crate::app) fn clear_manual_hunk_focus(&mut self) {
        self.viewport.manual_hunk_focus = None;
    }

    pub(in crate::app) fn replace_model(
        &mut self,
        visible_files: &[FileIndex],
        hunk_focus_behavior: HunkFocusModelBehavior,
    ) {
        let previous_manual_hunk_focus = self.viewport.manual_hunk_focus;
        self.document.model = UiModel::new_filtered(
            &self.document.changeset,
            self.viewport.layout,
            &self.document.context_expansions,
            visible_files,
        );
        self.invalidate_wrapped_visual_layout();
        self.viewport.manual_hunk_focus = match hunk_focus_behavior {
            HunkFocusModelBehavior::PreserveIfValid => {
                previous_manual_hunk_focus.filter(|(file, hunk)| {
                    self.document
                        .model
                        .hunk_start_row(file.get(), hunk.get())
                        .is_some()
                })
            }
            HunkFocusModelBehavior::Clear => None,
        };
        self.reanchor_annotation_draft();
    }

    pub(in crate::app) fn scroll_with_model_row_rendered(
        &self,
        preferred_scroll: usize,
        model_row: usize,
    ) -> usize {
        let max_scroll = self.max_scroll();
        let preferred_scroll = preferred_scroll.min(max_scroll);
        if self.model_row_rendered_at_scroll(
            preferred_scroll,
            self.viewport.viewport_rows,
            model_row,
        ) {
            return preferred_scroll;
        }

        let target_scroll = self.scroll_for_model_row(model_row).min(max_scroll);
        if preferred_scroll <= target_scroll {
            for scroll in preferred_scroll.saturating_add(1)..=target_scroll {
                if self.model_row_rendered_at_scroll(scroll, self.viewport.viewport_rows, model_row)
                {
                    return scroll;
                }
            }
        } else {
            for scroll in (target_scroll..preferred_scroll).rev() {
                if self.model_row_rendered_at_scroll(scroll, self.viewport.viewport_rows, model_row)
                {
                    return scroll;
                }
            }
        }

        target_scroll
    }

    pub(in crate::app) fn rendered_diff_rows_for_viewport(
        &self,
        visible_rows: usize,
    ) -> Vec<RenderedDiffRow> {
        self.rendered_diff_rows_for_viewport_at_scroll(self.viewport.scroll, visible_rows)
    }

    pub(in crate::app) fn rendered_diff_rows_for_viewport_at_scroll(
        &self,
        scroll: usize,
        visible_rows: usize,
    ) -> Vec<RenderedDiffRow> {
        plan_diff_viewport_rows_at_scroll(self, scroll, visible_rows)
            .into_iter()
            .enumerate()
            .filter_map(|(viewport_row, slot)| match slot.kind {
                ViewportSlotKind::DiffVisual { model_row, .. } => Some(RenderedDiffRow {
                    viewport_row,
                    model_row,
                }),
                ViewportSlotKind::AnnotationCompose { .. }
                | ViewportSlotKind::AnnotationSaved { .. } => None,
            })
            .collect()
    }

    pub(in crate::app) fn model_row_rendered_at_scroll(
        &self,
        scroll: usize,
        visible_rows: usize,
        model_row: usize,
    ) -> bool {
        self.rendered_diff_rows_for_viewport_at_scroll(scroll, visible_rows)
            .iter()
            .any(|rendered_row| rendered_row.model_row == model_row)
    }

    pub(in crate::app) fn rendered_viewport_focus_row(&self, visible_rows: usize) -> usize {
        let row_count = if self.viewport.line_wrapping {
            self.wrapped_visual_row_count()
        } else {
            self.document.model.len()
        };
        viewport_focus_offset(self.viewport.scroll, row_count, visible_rows)
    }

    pub(in crate::app) fn focused_hunk_in_rendered_rows(
        &self,
        rendered_rows: &[RenderedDiffRow],
        search: HunkFocusSearch,
    ) -> Option<(FileIndex, HunkIndex)> {
        match search {
            HunkFocusSearch::FirstVisible => {
                for rendered_row in rendered_rows {
                    if let Some(hunk_key) = self
                        .document
                        .model
                        .row(rendered_row.model_row)
                        .and_then(UiRow::typed_hunk_key)
                    {
                        return Some(hunk_key);
                    }
                }
                None
            }
            HunkFocusSearch::NearestTo(focus_viewport_row) => {
                find_rendered_diff_row_outward(rendered_rows, focus_viewport_row, |rendered_row| {
                    self.document
                        .model
                        .row(rendered_row.model_row)
                        .and_then(UiRow::typed_hunk_key)
                })
            }
        }
    }

    pub(crate) fn focused_hunk_for_viewport(
        &self,
        visible_rows: usize,
    ) -> Option<(FileIndex, HunkIndex)> {
        let rendered_rows = self.rendered_diff_rows_for_viewport(visible_rows);
        if rendered_rows.is_empty() {
            return None;
        }

        if let Some((file, hunk)) = self.viewport.manual_hunk_focus
            && rendered_rows.iter().any(|rendered_row| {
                self.document
                    .model
                    .row(rendered_row.model_row)
                    .is_some_and(|row| row.is_hunk_row(file.get(), hunk.get()))
            })
        {
            return Some((file, hunk));
        }

        let row_count = if self.viewport.line_wrapping {
            self.wrapped_visual_row_count()
        } else {
            self.document.model.len()
        };
        let search = if max_scroll_for_viewport(row_count, visible_rows) == 0 {
            // When the whole diff fits, start at the first visible hunk; explicit hunk
            // navigation is tracked separately with manual_hunk_focus.
            HunkFocusSearch::FirstVisible
        } else {
            HunkFocusSearch::NearestTo(self.rendered_viewport_focus_row(visible_rows))
        };
        self.focused_hunk_in_rendered_rows(&rendered_rows, search)
    }
}
