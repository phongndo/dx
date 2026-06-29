use mark_diff::{Changeset, DiffOptions};

use crate::theme::SYNTAX_THEME_ID;

use super::{
    SyntaxPriority, SyntaxQueueError,
    runtime::SyntaxRuntime,
    source::{
        SyntaxJob, SyntaxJobSource, build_full_file_line_map, build_hunk_source, full_file_source,
        hash_text, syntax_path,
    },
    types::{
        DiffSide, SyntaxKey, SyntaxPosition, SyntaxSkipReason, SyntaxSourceId, SyntaxSourceKind,
    },
};

impl SyntaxRuntime {
    pub(crate) fn queue_hunk(
        &mut self,
        options: &DiffOptions,
        changeset: &Changeset,
        position: SyntaxPosition,
        priority: SyntaxPriority,
    ) {
        let SyntaxPosition {
            generation,
            file,
            hunk,
            side,
        } = position;
        self.stats.queue_requests = self.stats.queue_requests.saturating_add(1);
        if let Some(key) = self.position_keys.get(&position).copied() {
            if self.cache.contains_key(&key) {
                return;
            }
            if self.pending.contains(&key) {
                if priority == SyntaxPriority::Visible {
                    self.queue.promote(key);
                }
                return;
            }
        }
        if self.skipped.contains_key(&position) {
            return;
        }

        let Some(file_diff) = changeset.files.get(file) else {
            self.skip(position, SyntaxSkipReason::InvalidPosition);
            return;
        };
        let Some(path) = syntax_path(file_diff, side) else {
            self.skip(position, SyntaxSkipReason::NoPath);
            return;
        };
        let Some(language) = self.languages.language_for_path(path) else {
            self.skip(position, SyntaxSkipReason::NoLanguage);
            return;
        };
        let Some(hunk_diff) = file_diff.hunks().get(hunk) else {
            self.skip(position, SyntaxSkipReason::InvalidPosition);
            return;
        };

        if let Some(source) = full_file_source(&changeset.repo, options, file_diff, side) {
            let key = SyntaxKey {
                source: SyntaxSourceId {
                    generation,
                    file,
                    side,
                    kind: SyntaxSourceKind::FullFile,
                },
                language_hash: hash_text(&language),
                theme_id: SYNTAX_THEME_ID,
            };

            if !self.unavailable_full_files.contains(&key) {
                if self.failed.contains(&key) {
                    self.skip(position, SyntaxSkipReason::HighlightError);
                    return;
                }

                let line_map = match build_full_file_line_map(&hunk_diff.lines, side) {
                    Ok(line_map) => line_map,
                    Err(reason) => {
                        self.skip(position, reason);
                        return;
                    }
                };

                self.source_keys.insert(key.source, key);
                self.position_keys.insert(position, key);
                self.line_maps.insert(position, line_map);
                if self.queue_job(
                    key,
                    language,
                    SyntaxJobSource::FullFile(source),
                    priority,
                    Some(position),
                ) {
                    return;
                }
                return;
            }
        }

        let source = match build_hunk_source(&hunk_diff.lines, side, self.limits) {
            Ok(source) => source,
            Err(reason) => {
                self.skip(position, reason);
                return;
            }
        };

        let key = SyntaxKey {
            source: SyntaxSourceId {
                generation,
                file,
                side,
                kind: SyntaxSourceKind::HunkSide { hunk },
            },
            language_hash: hash_text(&language),
            theme_id: SYNTAX_THEME_ID,
        };
        self.source_keys.insert(key.source, key);
        self.position_keys.insert(position, key);
        self.line_maps.insert(position, source.line_map.clone());
        if self.failed.contains(&key) {
            self.skip(position, SyntaxSkipReason::HighlightError);
            return;
        }

        self.queue_job(
            key,
            language,
            SyntaxJobSource::Hunk(source),
            priority,
            Some(position),
        );
    }

    pub(crate) fn queue_full_file(
        &mut self,
        options: &DiffOptions,
        changeset: &Changeset,
        generation: u64,
        file: usize,
        side: DiffSide,
        priority: SyntaxPriority,
    ) {
        self.stats.queue_requests = self.stats.queue_requests.saturating_add(1);
        let source_id = SyntaxSourceId {
            generation,
            file,
            side,
            kind: SyntaxSourceKind::FullFile,
        };
        if let Some(key) = self.source_keys.get(&source_id).copied() {
            if self.cache.contains_key(&key) {
                return;
            }
            if self.pending.contains(&key) {
                if priority == SyntaxPriority::Visible {
                    self.queue.promote(key);
                }
                return;
            }
        }
        if self.skipped_sources.contains(&source_id) {
            return;
        }

        let Some(file_diff) = changeset.files.get(file) else {
            self.skip_source(source_id);
            return;
        };
        let Some(path) = syntax_path(file_diff, side) else {
            self.skip_source(source_id);
            return;
        };
        let Some(language) = self.languages.language_for_path(path) else {
            self.skip_source(source_id);
            return;
        };
        let Some(source) = full_file_source(&changeset.repo, options, file_diff, side) else {
            self.skip_source(source_id);
            return;
        };

        let key = SyntaxKey {
            source: source_id,
            language_hash: hash_text(&language),
            theme_id: SYNTAX_THEME_ID,
        };
        self.source_keys.insert(source_id, key);
        if self.unavailable_full_files.contains(&key) || self.failed.contains(&key) {
            self.skip_source(source_id);
            return;
        }

        self.queue_job(
            key,
            language,
            SyntaxJobSource::FullFile(source),
            priority,
            None,
        );
    }

    pub(crate) fn queue_job(
        &mut self,
        key: SyntaxKey,
        language: String,
        source: SyntaxJobSource,
        priority: SyntaxPriority,
        position: Option<SyntaxPosition>,
    ) -> bool {
        if self.cache.contains_key(&key) {
            return true;
        }
        if self.pending.contains(&key) {
            if priority == SyntaxPriority::Visible {
                self.queue.promote(key);
            }
            return true;
        }

        let source_bytes = source.known_bytes();
        let source_lines = source.known_lines();

        let job = SyntaxJob {
            key,
            language,
            source,
            limits: self.limits,
        };

        match self.queue.try_push(job, priority) {
            Ok(push) => {
                if let Some(dropped) = push.dropped {
                    self.pending.remove(&dropped);
                    self.stats.jobs_evicted = self.stats.jobs_evicted.saturating_add(1);
                }
                self.stats.jobs_queued = self.stats.jobs_queued.saturating_add(1);
                self.stats.queue_depth_peak = self.stats.queue_depth_peak.max(push.depth);
                if let Some(source_bytes) = source_bytes {
                    self.stats.source_bytes_queued =
                        self.stats.source_bytes_queued.saturating_add(source_bytes);
                }
                if let Some(source_lines) = source_lines {
                    self.stats.source_lines_queued =
                        self.stats.source_lines_queued.saturating_add(source_lines);
                }
                self.pending.insert(key);
                true
            }
            Err(SyntaxQueueError::Full | SyntaxQueueError::Stale) => {
                self.stats.jobs_rejected = self.stats.jobs_rejected.saturating_add(1);
                false
            }
            Err(SyntaxQueueError::Closed) => {
                if let Some(position) = position {
                    self.skip(position, SyntaxSkipReason::QueueClosed);
                } else {
                    self.skip_source(key.source);
                }
                false
            }
        }
    }

    pub(crate) fn skip(&mut self, position: SyntaxPosition, reason: SyntaxSkipReason) {
        if self.skipped.insert(position, reason).is_none() {
            self.stats.jobs_skipped = self.stats.jobs_skipped.saturating_add(1);
        }
    }

    pub(crate) fn skip_source(&mut self, source_id: SyntaxSourceId) {
        if self.skipped_sources.insert(source_id) {
            self.stats.jobs_skipped = self.stats.jobs_skipped.saturating_add(1);
        }
    }
}
