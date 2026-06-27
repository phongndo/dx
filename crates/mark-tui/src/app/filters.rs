use super::{
    DiffApp, FILTER_DEBOUNCE, FilterWorker, HunkFocusModelBehavior, HunkFocusScrollBehavior,
    MAX_LIVE_GREP_MATCHES,
};
use crate::controls::{DiffFilterKind, diff_stats_for_files};
use crate::runtime;
use crate::search::{DiffSearchResult, grep_match_rows};
use crate::text_input::TextInputKeyResult;
use crossterm::event::{KeyCode, KeyEvent};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::oneshot;

#[cfg(not(test))]
use super::PendingFilterApply;

impl DiffApp {
    pub(crate) fn open_filter_input(&mut self, kind: DiffFilterKind) {
        self.filters.filter_input = Some(kind);
        self.clear_diff_mouse_hover();
        self.overlays.diff_menu_open = false;
        self.overlays.diff_menu.reset_input();
        self.set_rendered_diff_menu_area(None);
        self.overlays.options_menu_open = false;
        self.close_color_scheme_picker();
        self.close_review_input();
        self.close_branch_menu();

        let had_filter =
            !self.filters.query(kind).is_empty() || !self.filters.input_query(kind).is_empty();
        self.filters.query_mut(kind).clear();
        self.filters.input_query_mut(kind).clear();
        *self.filters.input_cursor_mut(kind) = 0;
        if had_filter {
            self.schedule_filter_change(kind, Duration::ZERO);
        } else {
            self.runtime.dirty = true;
        }
    }

    pub(crate) fn handle_filter_input_key(&mut self, key: KeyEvent) -> bool {
        let Some(kind) = self.filters.filter_input else {
            return false;
        };

        match key.code {
            KeyCode::Esc => {
                self.clear_all_filters();
                self.filters.filter_input = None;
            }
            KeyCode::Enter => {
                self.commit_filter_input(kind);
                self.filters.filter_input = None;
            }
            _ => match self.filters.apply_input_key(kind, key) {
                TextInputKeyResult::Edited => self.sync_filter_input(kind),
                TextInputKeyResult::Moved => self.runtime.dirty = true,
                TextInputKeyResult::Ignored | TextInputKeyResult::Handled => {}
            },
        }

        true
    }

    pub(crate) fn commit_filter_input(&mut self, kind: DiffFilterKind) {
        let next = self.filters.input_query(kind).to_owned();
        if self.filters.query(kind) == next {
            if self.jobs.pending_filter_apply.is_some() {
                self.schedule_filter_change(kind, Duration::ZERO);
            }
            self.runtime.dirty = true;
            return;
        }

        *self.filters.query_mut(kind) = next;
        self.schedule_filter_change(kind, Duration::ZERO);
    }

    pub(crate) fn sync_filter_input(&mut self, kind: DiffFilterKind) {
        let next = self.filters.input_query(kind).to_owned();
        if self.filters.query(kind) == next {
            self.runtime.dirty = true;
            return;
        }

        *self.filters.query_mut(kind) = next;
        self.schedule_filter_change(kind, FILTER_DEBOUNCE);
    }

    pub(crate) fn clear_all_filters(&mut self) {
        if !self.filters.clear_all() {
            self.runtime.dirty = true;
            return;
        }
        self.schedule_filter_apply(Duration::ZERO, false);
    }

    pub(crate) fn apply_filters(&mut self, jump_to_grep: bool) {
        self.jobs.pending_filter_apply = None;
        self.jobs.filter_worker = None;
        self.jobs.filter_searching = false;
        let selected_path = self
            .document
            .changeset
            .files
            .get(self.sidebar.selected_file)
            .map(|file| file.display_path().to_owned());
        let relative_scroll = self.relative_scroll_from_file_start(self.sidebar.selected_file);

        let search_result = self.document.search_index.search_with_grep_match_limit(
            &self.filters.file_filter,
            &self.filters.grep_filter,
            MAX_LIVE_GREP_MATCHES,
        );
        self.replace_visible_files(
            search_result,
            selected_path,
            relative_scroll,
            jump_to_grep,
            HunkFocusModelBehavior::PreserveIfValid,
        );
    }

    pub(crate) fn schedule_filter_change(&mut self, kind: DiffFilterKind, debounce: Duration) {
        self.schedule_filter_apply(
            debounce,
            kind == DiffFilterKind::Grep && !self.filters.grep_filter.is_empty(),
        );
    }

    pub(crate) fn schedule_filter_apply(&mut self, debounce: Duration, jump_to_grep: bool) {
        #[cfg(test)]
        {
            let _ = debounce;
            self.apply_filters(jump_to_grep);
        }

        #[cfg(not(test))]
        {
            self.jobs.filter_generation = self.jobs.filter_generation.wrapping_add(1);
            self.jobs.pending_filter_apply = Some(PendingFilterApply {
                generation: self.jobs.filter_generation,
                due_at: Instant::now() + debounce,
                jump_to_grep,
            });
            self.jobs.filter_worker = None;
            self.jobs.filter_searching = true;
            self.runtime.dirty = true;
        }
    }

    pub(crate) fn start_due_filter_apply(&mut self) {
        let Some(pending) = self.jobs.pending_filter_apply else {
            return;
        };
        if Instant::now() < pending.due_at {
            return;
        }

        self.jobs.pending_filter_apply = None;
        let generation = pending.generation;
        let jump_to_grep = pending.jump_to_grep;
        let file_filter = self.filters.file_filter.clone();
        let grep_filter = self.filters.grep_filter.clone();
        let worker_file_filter = file_filter.clone();
        let worker_grep_filter = grep_filter.clone();
        let search_index = Arc::clone(&self.document.search_index);
        let (tx, rx) = oneshot::channel();
        runtime::spawn_detached_blocking(move || {
            let result = search_index.search_with_grep_match_limit(
                &worker_file_filter,
                &worker_grep_filter,
                MAX_LIVE_GREP_MATCHES,
            );
            let _ = tx.send(result);
        });

        self.jobs.filter_worker = Some(FilterWorker {
            generation,
            file_filter,
            grep_filter,
            jump_to_grep,
            rx,
        });
        self.jobs.filter_searching = true;
        self.runtime.dirty = true;
    }

    pub(crate) fn drain_filter_worker(&mut self) {
        let Some(outcome) =
            self.jobs
                .filter_worker
                .as_mut()
                .and_then(|worker| match worker.rx.try_recv() {
                    Ok(result) => Some(Some(result)),
                    Err(oneshot::error::TryRecvError::Empty) => None,
                    Err(oneshot::error::TryRecvError::Closed) => Some(None),
                })
        else {
            return;
        };

        let Some(worker) = self.jobs.filter_worker.take() else {
            return;
        };

        if worker.generation != self.jobs.filter_generation
            || worker.file_filter != self.filters.file_filter
            || worker.grep_filter != self.filters.grep_filter
        {
            return;
        }

        self.jobs.filter_searching = false;
        match outcome {
            Some(result) => self.apply_filter_result(result, worker.jump_to_grep),
            None => self.set_error_log("filter worker stopped"),
        }
    }

    pub(crate) fn filter_busy(&self) -> bool {
        self.jobs.filter_searching
            || self.jobs.pending_filter_apply.is_some()
            || self.jobs.filter_worker.is_some()
    }

    pub(super) fn apply_filter_result(
        &mut self,
        search_result: DiffSearchResult,
        jump_to_grep: bool,
    ) {
        let selected_path = self
            .document
            .changeset
            .files
            .get(self.sidebar.selected_file)
            .map(|file| file.display_path().to_owned());
        let relative_scroll = self.relative_scroll_from_file_start(self.sidebar.selected_file);

        self.replace_visible_files(
            search_result,
            selected_path,
            relative_scroll,
            jump_to_grep,
            HunkFocusModelBehavior::PreserveIfValid,
        );
    }

    pub(super) fn replace_visible_files(
        &mut self,
        search_result: DiffSearchResult,
        selected_path: Option<String>,
        relative_scroll: usize,
        jump_to_grep: bool,
        hunk_focus_behavior: HunkFocusModelBehavior,
    ) {
        let DiffSearchResult {
            visible_files,
            grep_matches,
            grep_matches_truncated,
        } = search_result;

        let selected_file = selected_path
            .and_then(|path| {
                self.document
                    .changeset
                    .files
                    .iter()
                    .position(|file| file.display_path() == path)
            })
            .filter(|file| visible_files.contains(file))
            .or_else(|| visible_files.first().copied())
            .unwrap_or(0);

        self.document.stats = diff_stats_for_files(&self.document.changeset, &visible_files);
        self.document.max_line_width = self
            .document
            .search_index
            .max_line_width_for_files(&visible_files);
        self.replace_model(&visible_files, hunk_focus_behavior);
        self.sidebar.selected_file = selected_file;
        self.filters.grep_matches = grep_match_rows(&self.document.model, &grep_matches);
        self.filters.grep_matches_truncated = grep_matches_truncated;
        self.filters.selected_grep_match = None;

        let scroll = self
            .document
            .model
            .file_start_row(self.sidebar.selected_file)
            .map(|start| {
                self.scroll_for_model_row(start)
                    .saturating_add(relative_scroll)
            })
            .unwrap_or_default();
        let scroll_behavior = match hunk_focus_behavior {
            HunkFocusModelBehavior::PreserveIfValid => HunkFocusScrollBehavior::Preserve,
            HunkFocusModelBehavior::Clear => HunkFocusScrollBehavior::ClearOnScroll,
        };
        self.set_scroll_with_grep_sync(scroll, true, scroll_behavior);
        self.set_horizontal_scroll(self.viewport.horizontal_scroll);
        self.ensure_file_sidebar_selection_visible(self.visible_file_sidebar_rows());

        if jump_to_grep && !self.filters.grep_matches.is_empty() {
            self.filters.selected_grep_match = Some(0);
            self.set_scroll_centered_on(self.filters.grep_matches[0]);
        } else {
            self.sync_grep_match_selection_to_scroll();
        }

        self.ensure_annotation_draft_visible();
        self.runtime.dirty = true;
    }

    #[cfg(test)]
    pub(crate) fn current_grep_match_row(&self) -> Option<usize> {
        self.selected_grep_match_row()
    }

    pub(super) fn selected_grep_match_row(&self) -> Option<usize> {
        if self.filters.grep_filter.is_empty() {
            return None;
        }

        self.filters
            .selected_grep_match
            .and_then(|index| self.filters.grep_matches.get(index).copied())
    }

    pub(crate) fn sync_grep_match_selection_to_scroll(&mut self) {
        if self.filters.grep_filter.is_empty() || self.filters.grep_matches.is_empty() {
            self.filters.selected_grep_match = None;
            return;
        }

        self.filters.selected_grep_match = self
            .filters
            .grep_matches
            .iter()
            .position(|row| self.grep_match_is_visible_or_below_scroll(*row))
            .or_else(|| self.filters.grep_matches.len().checked_sub(1));
    }

    pub(crate) fn move_grep_match(&mut self, delta: isize) {
        if self.filters.grep_filter.is_empty() {
            self.filters.selected_grep_match = None;
            return;
        }

        if self.filters.grep_matches.is_empty() {
            self.filters.selected_grep_match = None;
            self.set_warning_notice("no grep matches");
            return;
        }

        let len = self.filters.grep_matches.len();
        let current = self.filters.selected_grep_match.unwrap_or_else(|| {
            self.filters
                .grep_matches
                .iter()
                .position(|row| self.grep_match_is_visible_or_below_scroll(*row))
                .unwrap_or(0)
        });
        let next = if delta < 0 {
            current
                .saturating_add(len)
                .saturating_sub(delta.unsigned_abs() % len)
                % len
        } else {
            current.saturating_add(delta as usize) % len
        };

        self.filters.selected_grep_match = Some(next);
        self.set_scroll_for_grep_navigation(self.filters.grep_matches[next]);
        self.runtime.dirty = true;
    }

    pub(super) fn grep_match_is_visible_or_below_scroll(&self, row: usize) -> bool {
        let scroll = self.scroll_for_model_row(row);
        if !self.viewport.line_wrapping {
            return scroll >= self.viewport.scroll;
        }

        let height = self.wrapped_visual_height_for_model_row(row);
        scroll.saturating_add(height) > self.viewport.scroll
    }

    pub(crate) fn set_scroll_for_grep_navigation(&mut self, row: usize) {
        self.set_scroll_centered_on(row);
    }
}
