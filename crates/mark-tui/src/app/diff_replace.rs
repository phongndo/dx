use std::{path::Path, sync::Arc};

use mark_core::MarkResult;
use mark_diff::{Changeset, DiffOptions};

use super::{
    DiffApp, DiffCacheEntry, HunkFocusModelBehavior, HunkFocusScrollBehavior,
    MAX_LIVE_GREP_MATCHES, show_rev_from_options, splice_diff_files_for_path,
};
use crate::{
    controls::{
        DiffLayoutMode, branch_base_from_options, branch_head_from_options, comparison_branches,
        comparison_commits, current_head_label, default_branch_base,
    },
    search::DiffSearchIndex,
};

impl DiffApp {
    pub(crate) fn reload(&mut self) -> MarkResult<()> {
        self.invalidate_diff_cache();
        self.start_uncached_diff_load(self.document.options.clone(), "reload failed");
        Ok(())
    }

    pub(crate) fn replace_changeset(&mut self, changeset: Changeset) {
        self.invalidate_diff_cache();
        self.cache_loaded_diff(self.document.options.clone(), changeset.clone());
        self.replace_loaded_diff(self.document.options.clone(), changeset);
    }

    pub(crate) fn replace_path_changeset(&mut self, path: &Path, path_changeset: Changeset) {
        self.invalidate_diff_cache();
        let selected_path = self
            .document
            .changeset
            .files
            .get(self.sidebar.selected_file)
            .map(|file| file.display_path().to_owned());
        let relative_scroll = self.relative_scroll_from_file_start(self.sidebar.selected_file);

        splice_diff_files_for_path(
            &mut self.document.changeset.files,
            path,
            path_changeset.files.clone(),
        );
        splice_diff_files_for_path(
            &mut self.document.base_changeset.files,
            path,
            path_changeset.files,
        );
        self.document.total_stats = self.document.changeset.stats();
        self.document.context_expansions.clear();
        self.document.context_cache.clear();
        self.document.generation = self.document.generation.wrapping_add(1);
        self.document.inline_cache.clear();
        self.document.search_index = Arc::new(DiffSearchIndex::new(&self.document.changeset));
        self.jobs.pending_filter_apply = None;
        self.jobs.filter_worker = None;
        self.jobs.filter_searching = false;
        if let Some(syntax) = self.config.syntax.as_mut() {
            syntax.clear(self.document.generation);
        }
        let search_result = self.document.search_index.search_with_grep_match_limit(
            &self.filters.file_filter,
            &self.filters.grep_filter,
            MAX_LIVE_GREP_MATCHES,
        );
        self.replace_visible_files(
            search_result,
            selected_path,
            relative_scroll,
            false,
            HunkFocusModelBehavior::Clear,
        );
        self.store_current_diff_cache();
        self.runtime.dirty = true;
    }

    pub(crate) fn replace_cached_diff(
        &mut self,
        options: DiffOptions,
        cached: DiffCacheEntry,
        refresh_branch_metadata: bool,
    ) {
        let DiffCacheEntry {
            changeset,
            search_index,
            total_stats,
            max_line_width,
            unified_model,
            split_model,
            ..
        } = cached;
        let selected_path = self
            .document
            .changeset
            .files
            .get(self.sidebar.selected_file)
            .map(|file| file.display_path().to_owned());
        let relative_scroll = self.relative_scroll_from_file_start(self.sidebar.selected_file);

        let previous_branch_base = self.refs.branch_base.clone();
        let previous_branch_head = self.refs.branch_head.clone();
        let previous_repo = self.document.changeset.repo.clone();
        self.document.options = options;
        self.jobs.live_reload_invalidated = false;
        self.jobs.live_reload_pending = false;
        if !refresh_branch_metadata && previous_repo == changeset.repo {
            self.refs.branch_base =
                branch_base_from_options(&self.document.options).or(previous_branch_base);
            self.refs.branch_head =
                branch_head_from_options(&self.document.options, self.refs.current_head.as_deref())
                    .or(previous_branch_head)
                    .or_else(|| self.refs.current_head.clone());
            for branch in [
                self.refs.current_head.clone(),
                self.refs.branch_head.clone(),
                self.refs.branch_base.clone(),
            ]
            .into_iter()
            .flatten()
            {
                if !self
                    .refs
                    .comparison_branches
                    .iter()
                    .any(|candidate| candidate == &branch)
                {
                    self.refs.comparison_branches.push(branch);
                }
            }
        } else {
            self.refs.current_head = current_head_label(&changeset.repo);
            self.refs.branch_base = branch_base_from_options(&self.document.options)
                .or(previous_branch_base)
                .or_else(|| default_branch_base(&self.document.options, &changeset.repo));
            self.refs.branch_head =
                branch_head_from_options(&self.document.options, self.refs.current_head.as_deref())
                    .or(previous_branch_head)
                    .or_else(|| self.refs.current_head.clone());
            self.refs.comparison_branches = comparison_branches(
                &changeset.repo,
                &[
                    self.refs.current_head.as_deref(),
                    self.refs.branch_head.as_deref(),
                    self.refs.branch_base.as_deref(),
                ],
            );
        }
        self.refs.branch_menu.scroll = self
            .refs
            .branch_menu
            .scroll
            .min(self.max_branch_menu_scroll());
        self.refs.show_rev = show_rev_from_options(&self.document.options);
        self.refs.comparison_commits =
            comparison_commits(&self.document.changeset.repo, self.refs.show_rev.as_deref());
        self.refs.commit_menu.scroll = self
            .refs
            .commit_menu
            .scroll
            .min(self.max_commit_menu_scroll_for_rows(self.commit_menu_rows()));
        self.document.total_stats = total_stats;
        self.document.base_changeset = changeset.clone();
        self.document.changeset = changeset;
        self.document.search_index = search_index;
        self.document.context_expansions.clear();
        self.document.context_cache.clear();
        self.document.generation = self.document.generation.wrapping_add(1);
        self.document.inline_cache.clear();
        self.jobs.pending_filter_apply = None;
        self.jobs.filter_worker = None;
        self.jobs.filter_searching = false;
        if let Some(syntax) = self.config.syntax.as_mut() {
            syntax.clear(self.document.generation);
        }

        if self.filters.active() {
            let search_result = self.document.search_index.search_with_grep_match_limit(
                &self.filters.file_filter,
                &self.filters.grep_filter,
                MAX_LIVE_GREP_MATCHES,
            );
            self.replace_visible_files(
                search_result,
                selected_path,
                relative_scroll,
                false,
                HunkFocusModelBehavior::Clear,
            );
        } else {
            self.document.stats = self.document.total_stats.clone();
            self.document.max_line_width = max_line_width;
            self.document.model = match self.viewport.layout {
                DiffLayoutMode::Split => split_model,
                DiffLayoutMode::Unified => unified_model,
            };
            self.invalidate_wrapped_visual_layout();
            self.reanchor_annotation_draft();
            self.viewport.manual_hunk_focus = None;
            self.sidebar.selected_file = selected_path
                .and_then(|path| {
                    self.document
                        .changeset
                        .files
                        .iter()
                        .position(|file| file.display_path() == path)
                })
                .unwrap_or(0);
            self.filters.grep_matches.clear();
            self.filters.grep_matches_truncated = false;
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
            self.set_scroll_with_grep_sync(scroll, true, HunkFocusScrollBehavior::ClearOnScroll);
            self.set_horizontal_scroll(self.viewport.horizontal_scroll);
            self.ensure_file_sidebar_selection_visible(self.visible_file_sidebar_rows());
            self.ensure_annotation_draft_visible();
            self.runtime.dirty = true;
        }
    }

    pub(crate) fn replace_loaded_diff(&mut self, options: DiffOptions, changeset: Changeset) {
        let options_changed = self.document.options != options;
        if !options_changed && self.document.base_changeset == changeset {
            if self.jobs.live_reload_invalidated || self.jobs.live_reload_pending {
                self.jobs.live_reload_invalidated = false;
                self.jobs.live_reload_pending = false;
            }
            self.runtime.dirty = true;
            return;
        }

        let selected_path = self
            .document
            .changeset
            .files
            .get(self.sidebar.selected_file)
            .map(|file| file.display_path().to_owned());
        let relative_scroll = self.relative_scroll_from_file_start(self.sidebar.selected_file);

        let previous_branch_base = self.refs.branch_base.clone();
        let previous_branch_head = self.refs.branch_head.clone();
        self.document.options = options;
        self.jobs.live_reload_invalidated = false;
        self.jobs.live_reload_pending = false;
        self.refs.current_head = current_head_label(&changeset.repo);
        self.refs.branch_base = branch_base_from_options(&self.document.options)
            .or(previous_branch_base)
            .or_else(|| default_branch_base(&self.document.options, &changeset.repo));
        self.refs.branch_head =
            branch_head_from_options(&self.document.options, self.refs.current_head.as_deref())
                .or(previous_branch_head)
                .or_else(|| self.refs.current_head.clone());
        self.refs.comparison_branches = comparison_branches(
            &changeset.repo,
            &[
                self.refs.current_head.as_deref(),
                self.refs.branch_head.as_deref(),
                self.refs.branch_base.as_deref(),
            ],
        );
        self.refs.branch_menu.scroll = self
            .refs
            .branch_menu
            .scroll
            .min(self.max_branch_menu_scroll());
        self.refs.show_rev = show_rev_from_options(&self.document.options);
        self.refs.comparison_commits =
            comparison_commits(&changeset.repo, self.refs.show_rev.as_deref());
        self.refs.commit_menu.scroll = self
            .refs
            .commit_menu
            .scroll
            .min(self.max_commit_menu_scroll_for_rows(self.commit_menu_rows()));
        self.document.total_stats = changeset.stats();
        self.document.base_changeset = changeset.clone();
        self.document.changeset = changeset;
        self.document.search_index = Arc::new(DiffSearchIndex::new(&self.document.changeset));
        self.document.context_expansions.clear();
        self.document.context_cache.clear();
        self.document.generation = self.document.generation.wrapping_add(1);
        self.document.inline_cache.clear();
        self.jobs.pending_filter_apply = None;
        self.jobs.filter_worker = None;
        self.jobs.filter_searching = false;
        if let Some(syntax) = self.config.syntax.as_mut() {
            syntax.clear(self.document.generation);
        }
        let search_result = self.document.search_index.search_with_grep_match_limit(
            &self.filters.file_filter,
            &self.filters.grep_filter,
            MAX_LIVE_GREP_MATCHES,
        );
        self.replace_visible_files(
            search_result,
            selected_path,
            relative_scroll,
            false,
            HunkFocusModelBehavior::Clear,
        );
        self.runtime.dirty = true;
    }
}
