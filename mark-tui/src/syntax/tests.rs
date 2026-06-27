use super::*;
use std::{
    collections::{HashMap, HashSet},
    sync::mpsc as std_mpsc,
    thread,
    time::{Duration, Instant},
};

use mark_syntax::{SyntaxLanguageSet, SyntaxLimits};
use tokio::sync::mpsc;

use crate::theme::{SYNTAX_THEME_ID, SyntaxBenchmarkReport};

#[test]
fn drop_closes_full_result_channel_before_joining_worker() {
    let queue = SyntaxWorkerQueue::new(1, 0);
    let worker_queue = queue.clone();
    let (result_tx, result_rx) = mpsc::channel(1);
    result_tx
        .try_send(SyntaxResult {
            key: syntax_key(0),
            side: Err(SyntaxJobFailure::HighlightError),
        })
        .expect("result channel should be prefilled");
    queue
        .try_push(syntax_job(syntax_key(1)), SyntaxPriority::Visible)
        .expect("syntax job should queue");

    let worker = thread::spawn(move || run_syntax_worker(worker_queue, result_tx));
    wait_until(Duration::from_secs(1), || queue.len() == 0).expect("worker should take queued job");

    let syntax = SyntaxRuntime {
        languages: SyntaxLanguageSet::from_enabled_languages(&[]),
        limits: SyntaxLimits::default(),
        result_rx,
        queue,
        cache: LruCache::new(8),
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
    };
    let (done_tx, done_rx) = std_mpsc::channel();
    let dropper = thread::spawn(move || {
        drop(syntax);
        done_tx.send(()).expect("drop signal should send");
    });

    done_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("syntax runtime drop should not wait on a full result channel");
    dropper.join().expect("dropper thread should finish");
}

fn wait_until(timeout: Duration, condition: impl Fn() -> bool) -> Result<(), ()> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if condition() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(1));
    }
    Err(())
}

fn syntax_key(file: usize) -> SyntaxKey {
    SyntaxKey {
        source: SyntaxSourceId {
            generation: 0,
            file,
            side: DiffSide::New,
            kind: SyntaxSourceKind::HunkSide { hunk: 0 },
        },
        language_hash: 1,
        theme_id: SYNTAX_THEME_ID,
    }
}

fn syntax_job(key: SyntaxKey) -> SyntaxJob {
    SyntaxJob {
        key,
        language: "rust".to_owned(),
        source: SyntaxJobSource::Hunk(HunkSource {
            text: "fn main() {}".to_owned(),
            line_map: vec![Some(0)],
            source_lines: 1,
        }),
        limits: SyntaxLimits::default(),
    }
}
