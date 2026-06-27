use super::{DiffApp, HunkFocusModelBehavior, HunkFocusScrollBehavior, MAX_LIVE_GREP_MATCHES};
use crate::model::{ContextKey, ContextSourceEntry, ContextSourceKey, UiRow, context_expands_up};
use crate::search::grep_match_rows;
use crate::syntax::{
    DiffSide, available_context_lines, full_file_source, load_full_file_source,
    split_context_source_lines,
};
use std::sync::Arc;
use unicode_width::UnicodeWidthStr;

impl DiffApp {
    pub(super) fn context_source_line_count(&self, file: usize) -> Option<(DiffSide, usize)> {
        for side in [DiffSide::New, DiffSide::Old] {
            let key = ContextSourceKey { file, side };
            match self.document.context_cache.get(&key) {
                Some(ContextSourceEntry::Lines(lines)) => return Some((side, lines.len())),
                Some(ContextSourceEntry::Unavailable) => continue,
                None => {
                    if let Some(lines) = self.load_context_lines(file, side) {
                        return Some((side, lines.len()));
                    }
                }
            }
        }
        None
    }

    pub(crate) fn handle_context_at_row(&mut self, row_index: usize) -> bool {
        match self.document.model.row(row_index) {
            Some(UiRow::Collapsed { .. }) => self.expand_context_at_row(row_index),
            Some(UiRow::ContextHide { file, hunk, .. }) => self.hide_context(file, hunk),
            _ => false,
        }
    }

    pub(crate) fn expand_context_around_focused_hunk(&mut self, direction: isize) -> bool {
        let Some((file, hunk)) = self.focused_hunk_for_viewport(self.viewport.viewport_rows) else {
            return false;
        };

        let target_hunk = if direction < 0 {
            hunk
        } else {
            let Some(next_hunk) = hunk.checked_add(1) else {
                return false;
            };
            let Some(hunk_count) = self
                .document
                .changeset
                .files
                .get(file)
                .map(|file_diff| file_diff.hunks.len())
            else {
                return false;
            };
            if next_hunk == hunk_count {
                return self.expand_trailing_context_for_key(file, next_hunk);
            }
            if next_hunk > hunk_count {
                return false;
            }
            next_hunk
        };

        self.expand_context_for_key(file, target_hunk)
    }

    pub(crate) fn expand_trailing_context_for_key(&mut self, file: usize, hunk: usize) -> bool {
        let Some(file_diff) = self.document.changeset.files.get(file) else {
            return false;
        };
        if hunk != file_diff.hunks.len() {
            return false;
        }
        let Some(last_hunk) = hunk
            .checked_sub(1)
            .and_then(|hunk| file_diff.hunks.get(hunk))
        else {
            return false;
        };
        let old_start = last_hunk.old_start.saturating_add(last_hunk.old_count);
        let new_start = last_hunk.new_start.saturating_add(last_hunk.new_count);

        let Some((side, source_lines)) = self.ensure_context_lines(file) else {
            self.set_notice("context unavailable for this diff");
            return true;
        };

        let source_start = match side {
            DiffSide::Old => old_start,
            DiffSide::New => new_start,
        };
        let available = available_context_lines(source_start, usize::MAX, source_lines.len());
        let current = self
            .document
            .context_expansions
            .get(&ContextKey { file, hunk })
            .copied()
            .unwrap_or_default()
            .min(available);
        if current == available {
            self.set_notice("no more context");
            return true;
        }

        self.update_max_line_width_for_expanded_context(
            &source_lines,
            source_start,
            current..available,
        );
        self.document
            .context_expansions
            .insert(ContextKey { file, hunk }, available);
        self.rebuild_model_after_context_visibility_change();
        true
    }

    pub(crate) fn expand_context_for_key(&mut self, file: usize, hunk: usize) -> bool {
        let Some(row_index) = self.document.model.rows.iter().position(|row| {
            matches!(
                row,
                UiRow::Collapsed {
                    file: row_file,
                    hunk: row_hunk,
                    ..
                } if *row_file == file && *row_hunk == hunk
            )
        }) else {
            return false;
        };

        self.expand_context_at_row(row_index)
    }

    pub(crate) fn expand_context_at_row(&mut self, row_index: usize) -> bool {
        let Some(UiRow::Collapsed {
            file,
            hunk,
            old_start,
            new_start,
            lines,
            expanded,
        }) = self.document.model.row(row_index)
        else {
            return false;
        };

        let Some((side, source_lines)) = self.ensure_context_lines(file) else {
            self.set_warning_notice("context unavailable for this diff");
            return true;
        };

        let total = lines.saturating_add(expanded);
        let expands_up = context_expands_up(hunk);
        let source_start = match (side, expands_up) {
            (DiffSide::Old, true) => old_start,
            (DiffSide::Old, false) => old_start.saturating_sub(expanded),
            (DiffSide::New, true) => new_start,
            (DiffSide::New, false) => new_start.saturating_sub(expanded),
        };
        let available = available_context_lines(source_start, total, source_lines.len());
        let current = expanded.min(available);
        let remaining = available.saturating_sub(current);
        if remaining == 0 {
            self.set_warning_notice("no more context");
            return true;
        }

        let next = available;
        let newly_visible = if expands_up {
            total.saturating_sub(next)..total.saturating_sub(current)
        } else {
            current..next
        };
        self.update_max_line_width_for_expanded_context(&source_lines, source_start, newly_visible);
        self.document
            .context_expansions
            .insert(ContextKey { file, hunk }, next);
        self.rebuild_model_after_context_visibility_change();
        true
    }

    pub(crate) fn hide_context(&mut self, file: usize, hunk: usize) -> bool {
        if self
            .document
            .context_expansions
            .remove(&ContextKey { file, hunk })
            .is_none()
        {
            return false;
        }

        self.rebuild_model_after_context_visibility_change();
        true
    }

    pub(crate) fn collapse_all_context(&mut self) -> bool {
        if self.document.context_expansions.is_empty() {
            return false;
        }

        self.document.context_expansions.clear();
        self.rebuild_model_after_context_visibility_change();
        true
    }

    pub(super) fn rebuild_model_after_context_visibility_change(&mut self) {
        let search_result = self.document.search_index.search_with_grep_match_limit(
            &self.filters.file_filter,
            &self.filters.grep_filter,
            MAX_LIVE_GREP_MATCHES,
        );
        self.replace_model(
            &search_result.visible_files,
            HunkFocusModelBehavior::PreserveIfValid,
        );
        self.filters.grep_matches =
            grep_match_rows(&self.document.model, &search_result.grep_matches);
        self.filters.grep_matches_truncated = search_result.grep_matches_truncated;
        self.filters.selected_grep_match = None;
        self.set_scroll_with_grep_sync(
            self.viewport.scroll,
            true,
            HunkFocusScrollBehavior::Preserve,
        );
        self.sync_grep_match_selection_to_scroll();
        self.set_horizontal_scroll(self.viewport.horizontal_scroll);
        self.runtime.dirty = true;
    }

    pub(crate) fn ensure_context_lines(
        &mut self,
        file: usize,
    ) -> Option<(DiffSide, Arc<Vec<String>>)> {
        for side in [DiffSide::New, DiffSide::Old] {
            if !self.has_context_source(file, side) {
                continue;
            }
            if let Some(lines) = self.context_lines(file, side) {
                return Some((side, lines));
            }
        }
        None
    }

    pub(crate) fn has_context_source(&self, file: usize, side: DiffSide) -> bool {
        self.document
            .changeset
            .files
            .get(file)
            .and_then(|file_diff| {
                full_file_source(
                    &self.document.changeset.repo,
                    &self.document.options,
                    file_diff,
                    side,
                )
            })
            .is_some()
    }

    pub(crate) fn context_source_side(&self, file: usize) -> Option<DiffSide> {
        for side in [DiffSide::New, DiffSide::Old] {
            match self
                .document
                .context_cache
                .get(&ContextSourceKey { file, side })
            {
                Some(ContextSourceEntry::Lines(_)) => return Some(side),
                Some(ContextSourceEntry::Unavailable) => continue,
                None if self.has_context_source(file, side) => return Some(side),
                None => {}
            }
        }
        None
    }

    pub(crate) fn context_lines(
        &mut self,
        file: usize,
        side: DiffSide,
    ) -> Option<Arc<Vec<String>>> {
        let key = ContextSourceKey { file, side };
        if !self.document.context_cache.contains_key(&key) {
            let entry = self
                .load_context_lines(file, side)
                .map(ContextSourceEntry::Lines)
                .unwrap_or(ContextSourceEntry::Unavailable);
            self.document.context_cache.insert(key, entry);
            self.invalidate_wrapped_visual_layout();
        }

        match self.document.context_cache.get(&key) {
            Some(ContextSourceEntry::Lines(lines)) => Some(Arc::clone(lines)),
            Some(ContextSourceEntry::Unavailable) | None => None,
        }
    }

    pub(crate) fn load_context_lines(
        &self,
        file: usize,
        side: DiffSide,
    ) -> Option<Arc<Vec<String>>> {
        let file_diff = self.document.changeset.files.get(file)?;
        let source = full_file_source(
            &self.document.changeset.repo,
            &self.document.options,
            file_diff,
            side,
        )?;
        let text = load_full_file_source(&source).ok()?;
        Some(Arc::new(split_context_source_lines(&text)))
    }

    pub(crate) fn context_line_text(
        &mut self,
        file: usize,
        old_line: usize,
        new_line: usize,
    ) -> String {
        let Some((side, source_lines)) = self.ensure_context_lines(file) else {
            return "context unavailable".to_owned();
        };
        let line_number = match side {
            DiffSide::Old => old_line,
            DiffSide::New => new_line,
        };
        let Some(line_index) = line_number.checked_sub(1) else {
            return String::new();
        };
        source_lines.get(line_index).cloned().unwrap_or_default()
    }

    pub(crate) fn update_max_line_width_for_expanded_context(
        &mut self,
        source_lines: &[String],
        source_start: usize,
        offsets: std::ops::Range<usize>,
    ) {
        let Some(source_index_start) = source_start.checked_sub(1) else {
            return;
        };
        for offset in offsets {
            let Some(text) = source_lines.get(source_index_start + offset) else {
                continue;
            };
            self.document.max_line_width = self.document.max_line_width.max(text.width());
        }
    }
}
