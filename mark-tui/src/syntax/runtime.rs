use std::{
    collections::{HashMap, HashSet},
    thread,
};

use mark_core::MarkResult;
use mark_syntax::{SyntaxLanguageSet, SyntaxLimits, SyntaxSettings};
use tokio::sync::mpsc::{self, Receiver};

use crate::theme::SyntaxBenchmarkReport;

use super::source::{SyntaxResult, run_syntax_worker};
use super::{LruCache, SyntaxWorkerQueue};

use super::types::{HighlightedSide, SyntaxKey, SyntaxPosition, SyntaxSkipReason, SyntaxSourceId};

#[derive(Debug)]
pub(crate) struct SyntaxRuntime {
    pub(crate) languages: SyntaxLanguageSet,
    pub(crate) limits: SyntaxLimits,
    pub(crate) result_rx: Receiver<SyntaxResult>,
    pub(crate) queue: SyntaxWorkerQueue,
    pub(crate) cache: LruCache<SyntaxKey, HighlightedSide>,
    pub(crate) pending: HashSet<SyntaxKey>,
    pub(crate) source_keys: HashMap<SyntaxSourceId, SyntaxKey>,
    pub(crate) position_keys: HashMap<SyntaxPosition, SyntaxKey>,
    pub(crate) line_maps: HashMap<SyntaxPosition, Vec<Option<usize>>>,
    pub(crate) skipped: HashMap<SyntaxPosition, SyntaxSkipReason>,
    pub(crate) skipped_sources: HashSet<SyntaxSourceId>,
    pub(crate) unavailable_full_files: HashSet<SyntaxKey>,
    pub(crate) failed: HashSet<SyntaxKey>,
    pub(crate) stats: SyntaxBenchmarkReport,
    pub(crate) worker: Option<thread::JoinHandle<()>>,
}

impl SyntaxRuntime {
    pub(crate) fn start(settings: &SyntaxSettings) -> MarkResult<Option<Self>> {
        let languages = SyntaxLanguageSet::load_with_mode(settings.mode)?;
        Ok(Self::start_with_language_set(languages, settings.limits))
    }

    pub(crate) fn start_with_language_set(
        languages: SyntaxLanguageSet,
        limits: SyntaxLimits,
    ) -> Option<Self> {
        if languages.is_empty() {
            return None;
        }

        let (result_tx, result_rx) = mpsc::channel(limits.queue_entries.max(1));
        let queue = SyntaxWorkerQueue::new(limits.queue_entries, 0);
        let worker_queue = queue.clone();
        let worker = thread::spawn(move || run_syntax_worker(worker_queue, result_tx));

        Some(Self {
            languages,
            limits,
            result_rx,
            queue,
            cache: LruCache::new(limits.cache_entries),
            pending: HashSet::new(),
            source_keys: HashMap::new(),
            position_keys: HashMap::new(),
            line_maps: HashMap::new(),
            skipped: HashMap::new(),
            skipped_sources: HashSet::new(),
            unavailable_full_files: HashSet::new(),
            failed: HashSet::new(),
            stats: SyntaxBenchmarkReport::default(),
            worker: Some(worker),
        })
    }

    pub(crate) fn start_with_languages(
        languages: Vec<String>,
        limits: SyntaxLimits,
    ) -> Option<Self> {
        let languages = SyntaxLanguageSet::from_enabled_languages(&languages);
        Self::start_with_language_set(languages, limits)
    }

    pub(crate) fn clear(&mut self, generation: u64) {
        self.cache.clear();
        self.pending.clear();
        self.source_keys.clear();
        self.position_keys.clear();
        self.line_maps.clear();
        self.skipped.clear();
        self.skipped_sources.clear();
        self.unavailable_full_files.clear();
        self.failed.clear();
        self.queue.set_generation(generation);
    }

    pub(crate) fn is_idle(&self) -> bool {
        self.pending.is_empty() && self.queue.len() == 0
    }

    pub(crate) fn stats(&self) -> SyntaxBenchmarkReport {
        self.stats.clone()
    }

    pub(crate) fn estimated_memory_bytes(&self) -> usize {
        self.cache.values().map(HighlightedSide::memory_bytes).sum()
    }
}

impl Drop for SyntaxRuntime {
    fn drop(&mut self) {
        self.result_rx.close();
        while self.result_rx.try_recv().is_ok() {}
        self.queue.close();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
