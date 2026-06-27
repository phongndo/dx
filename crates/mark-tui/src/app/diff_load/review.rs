use std::path::{Path, PathBuf};

use tokio::sync::oneshot;

use super::super::{DiffApp, PendingReviewLoad};
use crate::runtime;

impl DiffApp {
    pub(crate) fn start_review_load(&mut self, target: String) {
        let (tx, rx) = oneshot::channel();
        let target = target.trim().to_owned();
        let repo = Self::review_load_repo_for_target(&self.document.changeset.repo, &target);
        let worker_target = target;
        runtime::spawn_detached_blocking(move || {
            let result = mark_command::review_diff_options(repo, &worker_target, false).and_then(
                |options| {
                    mark_diff::load_review_ref(&options).map(|changeset| (options, changeset))
                },
            );
            let _ = tx.send(result);
        });

        self.jobs.pending_review_load = Some(PendingReviewLoad {
            error_prefix: "review unavailable".to_owned(),
            rx,
        });
        self.jobs.pending_diff_load = None;
        self.runtime.dirty = true;
    }

    pub(crate) fn review_load_repo_for_target(repo: &Path, _target: &str) -> Option<PathBuf> {
        // Numeric review IDs are resolved against this repository, while URLs
        // carry their own pull request identity. In both cases, preserve the
        // active repository so the loaded patch can resolve follow-up local
        // actions relative to the same repo the TUI was reviewing.
        if repo.as_os_str().is_empty() {
            return None;
        }

        Some(repo.to_path_buf())
    }

    pub(crate) fn drain_pending_review_load(&mut self) {
        let Some(outcome) =
            self.jobs.pending_review_load.as_mut().and_then(|pending| {
                match pending.rx.try_recv() {
                    Ok(result) => Some(Some(result)),
                    Err(oneshot::error::TryRecvError::Empty) => None,
                    Err(oneshot::error::TryRecvError::Closed) => Some(None),
                }
            })
        else {
            return;
        };
        let Some(pending) = self.jobs.pending_review_load.take() else {
            return;
        };

        match outcome {
            Some(Ok((mut options, changeset))) => {
                options.include_untracked = self.document.options.include_untracked;
                self.replace_loaded_diff(options, changeset);
            }
            Some(Err(error)) => self.set_error_log(format!("{}: {error}", pending.error_prefix)),
            None => self.set_error_log(format!("{}: worker stopped", pending.error_prefix)),
        }
    }
}
