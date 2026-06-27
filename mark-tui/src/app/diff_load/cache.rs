use std::{collections::HashMap, sync::Arc};

use mark_diff::{Changeset, DiffOptions};

use super::super::{DiffApp, DiffCacheEntry, MAX_DIFF_CACHE_ENTRIES, cacheable_diff_options};
use crate::{controls::DiffLayoutMode, model::UiModel, search::DiffSearchIndex};

pub(crate) fn diff_cache_entry(options: DiffOptions, changeset: Changeset) -> DiffCacheEntry {
    let search_index = Arc::new(DiffSearchIndex::new(&changeset));
    let max_line_width = search_index.max_line_width();
    let total_stats = changeset.stats();
    let context_expansions = HashMap::new();
    let unified_model = UiModel::new(&changeset, DiffLayoutMode::Unified, &context_expansions);
    let split_model = UiModel::new(&changeset, DiffLayoutMode::Split, &context_expansions);
    DiffCacheEntry {
        options,
        changeset,
        search_index,
        total_stats,
        max_line_width,
        unified_model,
        split_model,
    }
}

impl DiffApp {
    pub(crate) fn invalidate_diff_cache(&mut self) {
        self.clear_cached_diff_choices();
    }

    pub(crate) fn clear_cached_diff_choices(&mut self) {
        self.jobs.clear_cached_diff_choices();
    }

    pub(in crate::app) fn diff_cache_invalidator_active(&self) -> bool {
        self.jobs
            .diff_cache_invalidator_active(&self.document.options)
    }

    pub(in crate::app) fn store_current_diff_cache(&mut self) {
        if !self.diff_cache_invalidator_active() || !cacheable_diff_options(&self.document.options)
        {
            return;
        }

        let options = self.document.options.clone();
        let changeset = self.document.base_changeset.clone();
        self.jobs
            .diff_cache
            .retain(|entry| entry.options != options);
        let search_index = Arc::clone(&self.document.search_index);
        let total_stats = self.document.total_stats.clone();
        let max_line_width = search_index.max_line_width();
        let can_reuse_current_model = !self.filters.active()
            && !self.filter_busy()
            && self.document.context_expansions.is_empty();
        let context_expansions = HashMap::new();
        let unified_model =
            if can_reuse_current_model && self.viewport.layout == DiffLayoutMode::Unified {
                self.document.model.clone()
            } else {
                UiModel::new(&changeset, DiffLayoutMode::Unified, &context_expansions)
            };
        let split_model =
            if can_reuse_current_model && self.viewport.layout == DiffLayoutMode::Split {
                self.document.model.clone()
            } else {
                UiModel::new(&changeset, DiffLayoutMode::Split, &context_expansions)
            };
        self.jobs.diff_cache.insert(
            0,
            DiffCacheEntry {
                options,
                changeset,
                search_index,
                total_stats,
                max_line_width,
                unified_model,
                split_model,
            },
        );
        self.jobs.diff_cache.truncate(MAX_DIFF_CACHE_ENTRIES);
    }

    pub(crate) fn cache_loaded_diff(&mut self, options: DiffOptions, changeset: Changeset) {
        if !self.diff_cache_invalidator_active() || !cacheable_diff_options(&options) {
            return;
        }

        self.jobs
            .diff_cache
            .retain(|entry| entry.options != options);
        self.jobs
            .diff_cache
            .insert(0, diff_cache_entry(options, changeset));
        self.jobs.diff_cache.truncate(MAX_DIFF_CACHE_ENTRIES);
    }

    pub(in crate::app) fn take_cached_diff(
        &mut self,
        options: &DiffOptions,
    ) -> Option<DiffCacheEntry> {
        let index = self
            .jobs
            .diff_cache
            .iter()
            .position(|entry| &entry.options == options)?;
        Some(self.jobs.diff_cache.remove(index))
    }

    pub(in crate::app) fn diff_cache_contains(&self, options: &DiffOptions) -> bool {
        self.jobs
            .diff_cache
            .iter()
            .any(|entry| &entry.options == options)
    }
}
