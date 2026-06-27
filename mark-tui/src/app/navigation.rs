use super::*;

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

    pub(super) fn invalidate_wrapped_visual_layout(&self) {
        self.viewport.wrapped_visual_layout.borrow_mut().take();
    }

    pub(super) fn cached_context_line_text(
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

    pub(super) fn wrapped_visual_height_for_text(&self, text: &str) -> usize {
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

    pub(super) fn wrapped_visual_height_for_row(&self, row: UiRow) -> usize {
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

    pub(super) fn ensure_wrapped_visual_layout(&self) {
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

    pub(super) fn wrapped_visual_row_count(&self) -> usize {
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

    pub(super) fn scroll_for_model_row(&self, row: usize) -> usize {
        if self.viewport.line_wrapping {
            self.wrapped_visual_scroll_for_model_row(row)
        } else {
            row
        }
    }

    pub(super) fn relative_scroll_from_file_start(&self, file: usize) -> usize {
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

    pub(super) fn visible_model_range_for_viewport(
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

    pub(super) fn clear_manual_hunk_focus(&mut self) {
        self.viewport.manual_hunk_focus = None;
    }

    pub(super) fn replace_model(
        &mut self,
        visible_files: &[usize],
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
            HunkFocusModelBehavior::PreserveIfValid => previous_manual_hunk_focus
                .filter(|(file, hunk)| self.document.model.hunk_start_row(*file, *hunk).is_some()),
            HunkFocusModelBehavior::Clear => None,
        };
        self.reanchor_annotation_draft();
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

    pub(super) fn scroll_with_model_row_rendered(
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

    pub(super) fn rendered_diff_rows_for_viewport(
        &self,
        visible_rows: usize,
    ) -> Vec<RenderedDiffRow> {
        self.rendered_diff_rows_for_viewport_at_scroll(self.viewport.scroll, visible_rows)
    }

    pub(super) fn rendered_diff_rows_for_viewport_at_scroll(
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

    pub(super) fn model_row_rendered_at_scroll(
        &self,
        scroll: usize,
        visible_rows: usize,
        model_row: usize,
    ) -> bool {
        self.rendered_diff_rows_for_viewport_at_scroll(scroll, visible_rows)
            .iter()
            .any(|rendered_row| rendered_row.model_row == model_row)
    }

    pub(super) fn rendered_viewport_focus_row(&self, visible_rows: usize) -> usize {
        let row_count = if self.viewport.line_wrapping {
            self.wrapped_visual_row_count()
        } else {
            self.document.model.len()
        };
        viewport_focus_offset(self.viewport.scroll, row_count, visible_rows)
    }

    pub(super) fn focused_hunk_in_rendered_rows(
        &self,
        rendered_rows: &[RenderedDiffRow],
        search: HunkFocusSearch,
    ) -> Option<(usize, usize)> {
        match search {
            HunkFocusSearch::FirstVisible => {
                for rendered_row in rendered_rows {
                    if let Some(hunk_key) = self
                        .document
                        .model
                        .row(rendered_row.model_row)
                        .and_then(|row| row.hunk_key())
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
                        .and_then(|row| row.hunk_key())
                })
            }
        }
    }

    pub(super) fn set_scroll_with_grep_sync(
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
            self.sidebar.selected_file = file;
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

    pub(super) fn max_scroll_with_annotations(&self, row_count: usize) -> usize {
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

    pub(crate) fn focused_hunk_for_viewport(&self, visible_rows: usize) -> Option<(usize, usize)> {
        let rendered_rows = self.rendered_diff_rows_for_viewport(visible_rows);
        if rendered_rows.is_empty() {
            return None;
        }

        if let Some((file, hunk)) = self.viewport.manual_hunk_focus
            && rendered_rows.iter().any(|rendered_row| {
                self.document
                    .model
                    .row(rendered_row.model_row)
                    .is_some_and(|row| row.is_hunk_row(file, hunk))
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
