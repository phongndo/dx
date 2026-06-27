use std::ops::Range;

use super::super::{
    DiffApp, WrappedVisualLayout, split_cell_content_width, unified_content_width,
    wrapped_line_count,
};
use crate::{
    controls::DiffLayoutMode,
    model::{ContextSourceEntry, ContextSourceKey, UiRow},
    syntax::DiffSide,
};

impl DiffApp {
    pub(in crate::app) fn invalidate_wrapped_visual_layout(&self) {
        self.viewport.wrapped_visual_layout.borrow_mut().take();
    }

    pub(in crate::app) fn cached_context_line_text(
        &self,
        file: usize,
        old_line: usize,
        new_line: usize,
    ) -> Option<&str> {
        for side in [DiffSide::New, DiffSide::Old] {
            let key = ContextSourceKey { file, side };
            match self.document.context_cache.get(&key) {
                Some(ContextSourceEntry::Lines(lines)) => {
                    let line_number = match side {
                        DiffSide::Old => old_line,
                        DiffSide::New => new_line,
                    };
                    let Some(line_index) = line_number.checked_sub(1) else {
                        return Some("");
                    };
                    return Some(lines.get(line_index).map(String::as_str).unwrap_or(""));
                }
                Some(ContextSourceEntry::Unavailable) => continue,
                None if self.has_context_source(file, side) => return None,
                None => {}
            }
        }
        None
    }

    pub(in crate::app) fn wrapped_visual_height_for_text(&self, text: &str) -> usize {
        match self.viewport.layout {
            DiffLayoutMode::Unified => {
                wrapped_line_count(text, unified_content_width(self.viewport.viewport_width))
            }
            DiffLayoutMode::Split => {
                let left_width = self.viewport.viewport_width / 2;
                let right_width = self.viewport.viewport_width.saturating_sub(left_width);
                wrapped_line_count(text, split_cell_content_width(left_width)).max(
                    wrapped_line_count(text, split_cell_content_width(right_width)),
                )
            }
        }
    }

    pub(in crate::app) fn wrapped_visual_height_for_row(&self, row: UiRow) -> usize {
        match row {
            UiRow::ContextLine {
                file,
                old_line,
                new_line,
            } => self
                .cached_context_line_text(file, old_line, new_line)
                .map(|text| self.wrapped_visual_height_for_text(text))
                .unwrap_or(1),
            UiRow::UnifiedLine { file, hunk, line } | UiRow::MetaLine { file, hunk, line } => {
                let text = &self.document.changeset.files[file].hunks[hunk].lines[line].text;
                wrapped_line_count(text, unified_content_width(self.viewport.viewport_width))
            }
            UiRow::SplitLine {
                file,
                hunk,
                left,
                right,
            } => {
                let lines = &self.document.changeset.files[file].hunks[hunk].lines;
                let left_width = self.viewport.viewport_width / 2;
                let right_width = self.viewport.viewport_width.saturating_sub(left_width);
                let left_content_width = split_cell_content_width(left_width);
                let right_content_width = split_cell_content_width(right_width);
                let left_rows = left
                    .and_then(|index| lines.get(index))
                    .map(|line| wrapped_line_count(&line.text, left_content_width))
                    .unwrap_or(1);
                let right_rows = right
                    .and_then(|index| lines.get(index))
                    .map(|line| wrapped_line_count(&line.text, right_content_width))
                    .unwrap_or(1);
                left_rows.max(right_rows).max(1)
            }
            UiRow::FileSeparator
            | UiRow::FileHeader(_)
            | UiRow::BinaryFile(_)
            | UiRow::Collapsed { .. }
            | UiRow::ContextHide { .. }
            | UiRow::HunkHeader { .. } => 1,
        }
    }

    pub(in crate::app) fn ensure_wrapped_visual_layout(&self) {
        if self
            .viewport
            .wrapped_visual_layout
            .borrow()
            .as_ref()
            .is_some_and(|layout| layout.matches(self))
        {
            return;
        }

        let mut row_starts = Vec::with_capacity(self.document.model.len().saturating_add(1));
        row_starts.push(0);
        let mut total_rows = 0usize;
        for row_index in 0..self.document.model.len() {
            let height = self
                .document
                .model
                .row(row_index)
                .map(|row| self.wrapped_visual_height_for_row(row))
                .unwrap_or(1)
                .max(1);
            total_rows = total_rows.saturating_add(height);
            row_starts.push(total_rows);
        }

        *self.viewport.wrapped_visual_layout.borrow_mut() = Some(WrappedVisualLayout {
            layout: self.viewport.layout,
            viewport_width: self.viewport.viewport_width,
            model_rows: self.document.model.len(),
            model_rows_ptr: self.document.model.rows.as_ptr() as usize,
            row_starts,
            total_rows,
        });
    }

    pub(in crate::app) fn wrapped_visual_row_count(&self) -> usize {
        self.ensure_wrapped_visual_layout();
        self.viewport
            .wrapped_visual_layout
            .borrow()
            .as_ref()
            .map(|layout| layout.total_rows)
            .unwrap_or_default()
    }

    pub(crate) fn wrapped_visual_scroll_for_model_row(&self, row_index: usize) -> usize {
        self.ensure_wrapped_visual_layout();
        self.viewport
            .wrapped_visual_layout
            .borrow()
            .as_ref()
            .and_then(|layout| layout.row_starts.get(row_index.min(layout.model_rows)))
            .copied()
            .unwrap_or_default()
    }

    pub(crate) fn wrapped_visual_height_for_model_row(&self, row_index: usize) -> usize {
        self.ensure_wrapped_visual_layout();
        self.viewport
            .wrapped_visual_layout
            .borrow()
            .as_ref()
            .and_then(|layout| {
                let row_index = row_index.min(layout.model_rows);
                let start = layout.row_starts.get(row_index)?;
                let end = layout.row_starts.get(row_index.saturating_add(1))?;
                Some(end.saturating_sub(*start))
            })
            .unwrap_or(1)
    }

    pub(crate) fn model_row_at_scroll(&self, scroll: usize) -> Option<(usize, usize)> {
        if !self.viewport.line_wrapping {
            return self.document.model.row(scroll).map(|_| (scroll, 0));
        }

        self.ensure_wrapped_visual_layout();
        let layout = self.viewport.wrapped_visual_layout.borrow();
        let layout = layout.as_ref()?;
        if scroll >= layout.total_rows {
            return None;
        }

        let row_index = layout
            .row_starts
            .partition_point(|row_start| *row_start <= scroll)
            .saturating_sub(1);
        let row_start = layout
            .row_starts
            .get(row_index)
            .copied()
            .unwrap_or_default();
        Some((row_index, scroll.saturating_sub(row_start)))
    }

    pub(in crate::app) fn visible_model_range_for_viewport(
        &self,
        visible_rows: usize,
    ) -> Option<Range<usize>> {
        if visible_rows == 0 || self.document.model.is_empty() {
            return None;
        }

        if !self.viewport.line_wrapping {
            let visible_start = self.viewport.scroll.min(self.document.model.len());
            let visible_end = visible_start
                .saturating_add(visible_rows)
                .min(self.document.model.len());
            return (visible_start < visible_end).then_some(visible_start..visible_end);
        }

        let visible_start = self
            .model_row_at_scroll(self.viewport.scroll)
            .map(|(row, _)| row)?;
        let visible_end = self
            .model_row_at_scroll(self.viewport.scroll.saturating_add(visible_rows - 1))
            .map(|(row, _)| row.saturating_add(1))
            .unwrap_or_else(|| self.document.model.len());

        (visible_start < visible_end).then_some(visible_start..visible_end)
    }
}
