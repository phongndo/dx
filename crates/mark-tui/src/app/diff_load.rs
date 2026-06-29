mod cache;
mod prefetch;
mod review;

pub(crate) use cache::diff_cache_entry;

use super::{
    AsyncJob, BranchMetadataPolicy, DiffApp, DiffLoadCachePolicy, PendingDiffLoad,
    cacheable_diff_options,
};
use crate::runtime;
use mark_diff::DiffOptions;
use tokio::sync::oneshot;

impl DiffApp {
    pub(crate) fn start_diff_load(
        &mut self,
        options: DiffOptions,
        error_prefix: impl Into<String>,
    ) {
        self.start_diff_load_inner(options, error_prefix, DiffLoadCachePolicy::Use);
    }

    pub(crate) fn start_uncached_diff_load(
        &mut self,
        options: DiffOptions,
        error_prefix: impl Into<String>,
    ) {
        self.start_diff_load_inner(options, error_prefix, DiffLoadCachePolicy::Bypass);
    }

    pub(super) fn start_diff_load_inner(
        &mut self,
        options: DiffOptions,
        error_prefix: impl Into<String>,
        cache_policy: DiffLoadCachePolicy,
    ) {
        let error_prefix = error_prefix.into();
        self.jobs.pending_review_load = None;

        let use_cache = matches!(cache_policy, DiffLoadCachePolicy::Use)
            && self.diff_cache_invalidator_active();

        if use_cache {
            self.store_current_diff_cache();

            if let Some(cached) = self.take_cached_diff(&options) {
                self.jobs.pending_diff_load = None;
                self.replace_cached_diff(options, cached, BranchMetadataPolicy::Preserve);
                return;
            }

            if self
                .jobs
                .pending_diff_load
                .as_ref()
                .is_some_and(|pending| pending.options == options)
            {
                self.runtime.dirty = true;
                return;
            }

            self.jobs
                .diff_prefetch_queue
                .retain(|prefetch_options| prefetch_options != &options);
            if let Some(prefetch) = self.take_pending_diff_prefetch(&options) {
                self.jobs.pending_diff_load = Some(PendingDiffLoad {
                    options,
                    error_prefix,
                    branch_metadata: BranchMetadataPolicy::Preserve,
                    job: prefetch.job,
                });
                self.set_success_notice("reloading");
                self.runtime.dirty = true;
                return;
            }
        } else {
            self.clear_cached_diff_choices();
        }

        let (tx, rx) = oneshot::channel();
        let load_options = options.clone();
        runtime::spawn_detached_blocking(move || {
            let _ = tx.send(mark_diff::load_review_ref(&load_options));
        });

        self.jobs.pending_diff_load = Some(PendingDiffLoad {
            options,
            error_prefix,
            branch_metadata: if use_cache {
                BranchMetadataPolicy::Preserve
            } else {
                BranchMetadataPolicy::Refresh
            },
            job: AsyncJob::new(rx),
        });
        self.set_success_notice("reloading");
        self.runtime.dirty = true;
    }

    pub(crate) fn drain_pending_diff_load(&mut self) {
        self.drain_pending_review_load();

        let Some(outcome) = self
            .jobs
            .pending_diff_load
            .as_mut()
            .and_then(|pending| match pending.job.try_recv() {
                Ok(result) => Some(Some(result)),
                Err(oneshot::error::TryRecvError::Empty) => None,
                Err(oneshot::error::TryRecvError::Closed) => Some(None),
            })
        else {
            return;
        };
        let Some(pending) = self.jobs.pending_diff_load.take() else {
            return;
        };

        match outcome {
            Some(Ok(changeset)) => {
                if cacheable_diff_options(&pending.options) {
                    let cached = diff_cache_entry(pending.options.clone(), changeset);
                    self.replace_cached_diff(pending.options, cached, pending.branch_metadata);
                } else {
                    self.replace_loaded_diff(pending.options, changeset);
                }
            }
            Some(Err(error)) => self.set_error_log(format!("{}: {error}", pending.error_prefix)),
            None => self.set_error_log(format!("{}: worker stopped", pending.error_prefix)),
        }
    }
}
