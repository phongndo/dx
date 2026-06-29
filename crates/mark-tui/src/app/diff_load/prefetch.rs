use mark_diff::DiffOptions;
use tokio::sync::oneshot;

use super::super::{AsyncJob, DiffApp, PendingDiffPrefetch, cacheable_diff_options};
use crate::runtime;

impl DiffApp {
    pub(crate) fn start_diff_prefetches(&mut self) {
        if !self.diff_cache_invalidator_active() {
            self.clear_cached_diff_choices();
            return;
        }

        if self.jobs.diff_prefetch_started {
            self.start_next_diff_prefetch();
            return;
        }

        self.jobs.diff_prefetch_started = true;
        self.queue_diff_prefetches();
        self.start_next_diff_prefetch();
    }

    pub(in crate::app) fn queue_diff_prefetches(&mut self) {
        for choice in self.diff_menu_choices() {
            let Some(options) = self.options_for_choice(choice) else {
                continue;
            };
            if options == self.document.options
                || !cacheable_diff_options(&options)
                || self.diff_cache_contains(&options)
                || self
                    .jobs
                    .pending_diff_load
                    .as_ref()
                    .is_some_and(|pending| pending.options == options)
                || self
                    .jobs
                    .pending_diff_prefetch
                    .as_ref()
                    .is_some_and(|pending| pending.options == options)
                || self
                    .jobs
                    .diff_prefetch_queue
                    .iter()
                    .any(|queued| queued == &options)
            {
                continue;
            }

            self.jobs.diff_prefetch_queue.push_back(options);
        }
    }

    pub(in crate::app) fn start_next_diff_prefetch(&mut self) {
        if !self.diff_cache_invalidator_active() {
            self.clear_cached_diff_choices();
            return;
        }

        if self.jobs.pending_diff_prefetch.is_some() {
            return;
        }

        while let Some(options) = self.jobs.diff_prefetch_queue.pop_front() {
            if options == self.document.options
                || !cacheable_diff_options(&options)
                || self.diff_cache_contains(&options)
                || self
                    .jobs
                    .pending_diff_load
                    .as_ref()
                    .is_some_and(|pending| pending.options == options)
            {
                continue;
            }

            let (tx, rx) = oneshot::channel();
            let load_options = options.clone();
            runtime::spawn_detached_blocking(move || {
                let _ = tx.send(mark_diff::load_review_ref(&load_options));
            });
            self.jobs.pending_diff_prefetch = Some(PendingDiffPrefetch {
                options,
                job: AsyncJob::new(rx),
            });
            return;
        }
    }

    pub(crate) fn drain_diff_prefetch(&mut self) {
        let Some(outcome) = self
            .jobs
            .pending_diff_prefetch
            .as_mut()
            .and_then(|pending| match pending.job.try_recv() {
                Ok(result) => Some(Some(result)),
                Err(oneshot::error::TryRecvError::Empty) => None,
                Err(oneshot::error::TryRecvError::Closed) => Some(None),
            })
        else {
            return;
        };
        let Some(pending) = self.jobs.pending_diff_prefetch.take() else {
            return;
        };

        if let Some(Ok(changeset)) = outcome {
            self.cache_loaded_diff(pending.options, changeset);
        }
        self.start_next_diff_prefetch();
    }

    pub(in crate::app) fn take_pending_diff_prefetch(
        &mut self,
        options: &DiffOptions,
    ) -> Option<PendingDiffPrefetch> {
        if self
            .jobs
            .pending_diff_prefetch
            .as_ref()
            .is_some_and(|pending| pending.options == *options)
        {
            self.jobs.pending_diff_prefetch.take()
        } else {
            None
        }
    }
}
