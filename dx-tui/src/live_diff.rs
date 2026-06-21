use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use dx_core::{DxError, DxResult};
use dx_diff::{Changeset, DiffOptions, DiffSource};
use notify::{RecursiveMode, Watcher};
use tokio::sync::mpsc::{self, Receiver, Sender};

use crate::runtime;
use crate::theme::LIVE_RELOAD_DEBOUNCE;

const LIVE_DIFF_CONTROL_CHANNEL_CAPACITY: usize = 64;
const LIVE_DIFF_RELOAD_CHANNEL_CAPACITY: usize = 4;

pub(crate) struct LiveDiff {
    pub(crate) options: DiffOptions,
    pub(crate) _watcher: notify::RecommendedWatcher,
    pub(crate) _worker: tokio::task::JoinHandle<()>,
    pub(crate) control_tx: Sender<LiveDiffCommand>,
    pub(crate) reload_rx: Receiver<LiveDiffReload>,
    paused: Arc<AtomicBool>,
    pending_while_paused: Arc<AtomicBool>,
    invalidated: Arc<AtomicBool>,
}

impl LiveDiff {
    pub(crate) fn start(options: DiffOptions, repo: &Path) -> DxResult<Self> {
        let watch_spec = live_diff_watch_spec_for_options(repo, &options)?;
        let filter = watch_spec.filter.clone();
        let (control_tx, control_rx) = mpsc::channel(LIVE_DIFF_CONTROL_CHANNEL_CAPACITY);
        let (reload_tx, reload_rx) = mpsc::channel(LIVE_DIFF_RELOAD_CHANNEL_CAPACITY);
        let watcher_tx = control_tx.clone();
        let paused = Arc::new(AtomicBool::new(false));
        let watcher_paused = Arc::clone(&paused);
        let pending_while_paused = Arc::new(AtomicBool::new(false));
        let watcher_pending_while_paused = Arc::clone(&pending_while_paused);
        let invalidated = Arc::new(AtomicBool::new(false));
        let watcher_invalidated = Arc::clone(&invalidated);

        let mut watcher =
            notify::recommended_watcher(move |result: Result<notify::Event, notify::Error>| {
                match result {
                    Ok(event) if filter.is_relevant_event(&event) => {
                        queue_changed_or_record_pending(
                            &watcher_paused,
                            &watcher_pending_while_paused,
                            &watcher_invalidated,
                            &watcher_tx,
                        );
                    }
                    Ok(_) | Err(_) => {}
                }
            })
            .map_err(|error| watcher_error("failed to start live diff watcher", error))?;

        for watch_path in &watch_spec.watch_paths {
            if !watch_path.path.exists() {
                continue;
            }
            watcher
                .watch(&watch_path.path, watch_path.recursive_mode())
                .map_err(|error| {
                    watcher_error(
                        &format!("failed to watch {}", watch_path.path.display()),
                        error,
                    )
                })?;
        }

        let worker = spawn_live_diff_worker(
            options.clone(),
            control_rx,
            reload_tx,
            Arc::clone(&paused),
            Arc::clone(&pending_while_paused),
        );

        Ok(Self {
            options,
            _watcher: watcher,
            _worker: worker,
            control_tx,
            reload_rx,
            paused,
            pending_while_paused,
            invalidated,
        })
    }

    pub(crate) fn take_invalidated(&self) -> bool {
        self.invalidated.swap(false, Ordering::AcqRel)
    }

    pub(crate) fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Release);
        if !paused {
            flush_pending_paused_reload(&self.pending_while_paused, &self.control_tx);
        }
    }
}

impl Drop for LiveDiff {
    fn drop(&mut self) {
        let _ = self.control_tx.try_send(LiveDiffCommand::Stop);
        self._worker.abort();
    }
}

#[derive(Debug)]
pub(crate) enum LiveDiffCommand {
    Changed,
    Stop,
}

#[derive(Debug)]
pub(crate) enum LiveDiffReload {
    Started,
    Loaded(DxResult<Changeset>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LiveDiffWatchPath {
    pub(crate) path: PathBuf,
    pub(crate) recursive: bool,
}

impl LiveDiffWatchPath {
    pub(crate) fn recursive_mode(&self) -> RecursiveMode {
        if self.recursive {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct LiveDiffWatchSpec {
    pub(crate) watch_paths: Vec<LiveDiffWatchPath>,
    pub(crate) filter: LiveDiffFilter,
}

impl LiveDiffWatchSpec {
    pub(crate) fn new(repo: &Path) -> Self {
        let mut spec = Self {
            watch_paths: Vec::new(),
            filter: LiveDiffFilter {
                repo: repo.to_path_buf(),
                git_state_paths: Vec::new(),
                exact_paths: Vec::new(),
            },
        };
        spec.add_watch_path(repo.to_path_buf(), true);
        spec
    }

    pub(crate) fn exact_paths(repo: &Path) -> Self {
        Self {
            watch_paths: Vec::new(),
            filter: LiveDiffFilter {
                repo: repo.to_path_buf(),
                git_state_paths: Vec::new(),
                exact_paths: Vec::new(),
            },
        }
    }

    pub(crate) fn add_git_state_path(&mut self, path: PathBuf) {
        if !self
            .filter
            .git_state_paths
            .iter()
            .any(|known| known == &path)
        {
            self.filter.git_state_paths.push(path);
        }
    }

    pub(crate) fn add_exact_path(&mut self, path: PathBuf) {
        if !self.filter.exact_paths.iter().any(|known| known == &path) {
            self.filter.exact_paths.push(path);
        }
    }

    pub(crate) fn add_watch_path(&mut self, path: PathBuf, recursive: bool) {
        if let Some(existing) = self
            .watch_paths
            .iter_mut()
            .find(|watch_path| watch_path.path == path)
        {
            existing.recursive |= recursive;
            return;
        }

        self.watch_paths.push(LiveDiffWatchPath { path, recursive });
    }

    pub(crate) fn add_git_state(&mut self, path: PathBuf) {
        self.add_git_state_path(path.clone());
        if path.is_dir() {
            self.add_watch_path(path, true);
        } else if let Some(parent) = path.parent() {
            self.add_watch_path(parent.to_path_buf(), false);
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct LiveDiffFilter {
    pub(crate) repo: PathBuf,
    pub(crate) git_state_paths: Vec<PathBuf>,
    pub(crate) exact_paths: Vec<PathBuf>,
}

impl LiveDiffFilter {
    pub(crate) fn is_relevant_event(&self, event: &notify::Event) -> bool {
        if matches!(event.kind, notify::EventKind::Access(_)) {
            return false;
        }

        if event.paths.is_empty() {
            return true;
        }

        event.paths.iter().any(|path| self.is_relevant_path(path))
    }

    pub(crate) fn is_relevant_path(&self, path: &Path) -> bool {
        let joined;
        let path = if path.is_absolute() || path.starts_with(&self.repo) {
            path
        } else {
            joined = self.repo.join(path);
            &joined
        };

        if self.is_git_state_path(path) {
            return true;
        }

        if self.is_exact_path(path) {
            return true;
        }

        if self.is_inside_repo_dot_git(path) {
            return false;
        }

        if !self.exact_paths.is_empty() {
            return false;
        }

        path.starts_with(&self.repo)
    }

    pub(crate) fn is_git_state_path(&self, path: &Path) -> bool {
        self.git_state_paths.iter().any(|state_path| {
            path == state_path
                || path.starts_with(state_path)
                || state_path.parent().is_some_and(|parent| path == parent)
        })
    }

    pub(crate) fn is_exact_path(&self, path: &Path) -> bool {
        self.exact_paths.iter().any(|exact_path| {
            path == exact_path || exact_path.parent().is_some_and(|parent| path == parent)
        })
    }

    pub(crate) fn is_inside_repo_dot_git(&self, path: &Path) -> bool {
        let Ok(relative) = path.strip_prefix(&self.repo) else {
            return false;
        };

        relative
            .components()
            .next()
            .is_some_and(|component| component.as_os_str() == OsStr::new(".git"))
    }
}

pub(crate) fn live_diff_supported(options: &DiffOptions) -> bool {
    matches!(
        options.source,
        DiffSource::Worktree | DiffSource::Base(_) | DiffSource::Difftool { .. }
    )
}

pub(crate) fn live_diff_watch_spec_for_options(
    repo: &Path,
    options: &DiffOptions,
) -> DxResult<LiveDiffWatchSpec> {
    match &options.source {
        DiffSource::Difftool { left, right, .. } => {
            Ok(live_diff_difftool_watch_spec(repo, left, right))
        }
        _ => live_diff_watch_spec(repo),
    }
}

pub(crate) fn live_diff_watch_spec(repo: &Path) -> DxResult<LiveDiffWatchSpec> {
    let mut spec = LiveDiffWatchSpec::new(repo);
    for git_path in [
        "index",
        "index.lock",
        "HEAD",
        "HEAD.lock",
        "refs",
        "packed-refs",
        "packed-refs.lock",
        "info/exclude",
        "config",
    ] {
        spec.add_git_state(dx_git::git_path(repo, git_path)?);
    }
    Ok(spec)
}

pub(crate) fn live_diff_difftool_watch_spec(
    repo: &Path,
    left: &Path,
    right: &Path,
) -> LiveDiffWatchSpec {
    let mut spec = LiveDiffWatchSpec::exact_paths(repo);
    add_difftool_watch_path(&mut spec, repo, left);
    add_difftool_watch_path(&mut spec, repo, right);
    spec
}

fn add_difftool_watch_path(spec: &mut LiveDiffWatchSpec, repo: &Path, path: &Path) {
    if is_null_path(path) {
        return;
    }

    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        repo.join(path)
    };
    spec.add_exact_path(path.clone());

    let watch_path = if path.is_dir() {
        path
    } else if let Some(parent) = path.parent() {
        parent.to_path_buf()
    } else {
        return;
    };
    spec.add_watch_path(watch_path, false);
}

fn is_null_path(path: &Path) -> bool {
    path == Path::new("/dev/null") || path == Path::new("NUL")
}

pub(crate) fn spawn_live_diff_worker(
    options: DiffOptions,
    control_rx: Receiver<LiveDiffCommand>,
    reload_tx: Sender<LiveDiffReload>,
    paused: Arc<AtomicBool>,
    pending_while_paused: Arc<AtomicBool>,
) -> tokio::task::JoinHandle<()> {
    runtime::spawn(async move {
        let mut control_rx = control_rx;
        while let Some(LiveDiffCommand::Changed) = control_rx.recv().await {
            if !wait_for_live_diff_quiet_period(&mut control_rx).await {
                break;
            }
            if reload_should_wait_for_unpause(&paused, &pending_while_paused) {
                continue;
            }

            if !send_live_reload(&reload_tx, LiveDiffReload::Started).await {
                break;
            }
            let load_options = options.clone();
            let changeset = match runtime::run_detached_blocking(move || {
                dx_diff::load_review_ref(&load_options)
            })
            .await
            {
                Ok(changeset) => changeset,
                Err(error) => Err(DxError::Io(std::io::Error::other(format!(
                    "live diff worker stopped: {error}"
                )))),
            };
            if reload_should_wait_for_unpause(&paused, &pending_while_paused) {
                continue;
            }
            if !send_live_reload(&reload_tx, LiveDiffReload::Loaded(changeset)).await {
                break;
            }
        }
    })
}

async fn send_live_reload(sender: &Sender<LiveDiffReload>, reload: LiveDiffReload) -> bool {
    sender.send(reload).await.is_ok()
}

fn queue_changed_or_record_pending(
    paused: &AtomicBool,
    pending_while_paused: &AtomicBool,
    invalidated: &AtomicBool,
    control_tx: &Sender<LiveDiffCommand>,
) {
    invalidated.store(true, Ordering::Release);

    if paused.load(Ordering::Acquire) {
        pending_while_paused.store(true, Ordering::Release);
        if paused.load(Ordering::Acquire) {
            return;
        }
        if !pending_while_paused.swap(false, Ordering::AcqRel) {
            return;
        }
    }

    let _ = runtime::send_with_timeout(control_tx, LiveDiffCommand::Changed);
}

fn flush_pending_paused_reload(
    pending_while_paused: &AtomicBool,
    control_tx: &Sender<LiveDiffCommand>,
) {
    if pending_while_paused.swap(false, Ordering::AcqRel) {
        let _ = runtime::send_with_timeout(control_tx, LiveDiffCommand::Changed);
    }
}

fn reload_should_wait_for_unpause(paused: &AtomicBool, pending_while_paused: &AtomicBool) -> bool {
    if !paused.load(Ordering::Acquire) {
        return false;
    }

    pending_while_paused.store(true, Ordering::Release);
    if paused.load(Ordering::Acquire) {
        return true;
    }

    pending_while_paused.swap(false, Ordering::AcqRel);
    false
}

pub(crate) async fn wait_for_live_diff_quiet_period(
    control_rx: &mut Receiver<LiveDiffCommand>,
) -> bool {
    loop {
        match tokio::time::timeout(LIVE_RELOAD_DEBOUNCE, control_rx.recv()).await {
            Ok(Some(LiveDiffCommand::Changed)) => continue,
            Ok(Some(LiveDiffCommand::Stop)) | Ok(None) => return false,
            Err(_) => return true,
        }
    }
}

pub(crate) fn watcher_error(context: &str, error: notify::Error) -> DxError {
    DxError::Usage(format!("{context}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paused_live_diff_records_and_flushes_pending_reload() {
        let paused = AtomicBool::new(true);
        let pending = AtomicBool::new(false);
        let invalidated = AtomicBool::new(false);
        let (tx, mut rx) = mpsc::channel(2);

        queue_changed_or_record_pending(&paused, &pending, &invalidated, &tx);

        assert!(pending.load(Ordering::Acquire));
        assert!(invalidated.load(Ordering::Acquire));
        assert!(matches!(
            rx.try_recv(),
            Err(tokio::sync::mpsc::error::TryRecvError::Empty)
        ));

        paused.store(false, Ordering::Release);
        flush_pending_paused_reload(&pending, &tx);

        assert!(!pending.load(Ordering::Acquire));
        assert!(matches!(rx.try_recv(), Ok(LiveDiffCommand::Changed)));
    }

    #[test]
    fn changed_live_diff_marks_invalidated_before_reload_starts() {
        let paused = AtomicBool::new(false);
        let pending = AtomicBool::new(false);
        let invalidated = AtomicBool::new(false);
        let (tx, mut rx) = mpsc::channel(2);

        queue_changed_or_record_pending(&paused, &pending, &invalidated, &tx);

        assert!(invalidated.load(Ordering::Acquire));
        assert!(!pending.load(Ordering::Acquire));
        assert!(matches!(rx.try_recv(), Ok(LiveDiffCommand::Changed)));
    }

    #[test]
    fn difftool_watch_spec_tracks_only_pair_paths() {
        let repo = PathBuf::from("/repo");
        let spec = live_diff_difftool_watch_spec(
            &repo,
            Path::new("left.tmp"),
            Path::new("/tmp/right.tmp"),
        );

        assert_eq!(
            spec.filter.exact_paths,
            vec![
                PathBuf::from("/repo/left.tmp"),
                PathBuf::from("/tmp/right.tmp")
            ]
        );
        assert!(spec.filter.is_relevant_path(Path::new("/repo/left.tmp")));
        assert!(spec.filter.is_relevant_path(Path::new("/tmp/right.tmp")));
        assert!(!spec.filter.is_relevant_path(Path::new("/repo/other.tmp")));
    }

    #[test]
    fn difftool_live_diff_is_supported() {
        assert!(live_diff_supported(&DiffOptions {
            source: DiffSource::Difftool {
                left: PathBuf::from("left.tmp"),
                right: PathBuf::from("right.tmp"),
                path: None,
            },
            ..DiffOptions::default()
        }));
    }

    #[test]
    fn worker_pause_check_marks_pending_reload() {
        let paused = AtomicBool::new(true);
        let pending = AtomicBool::new(false);

        assert!(reload_should_wait_for_unpause(&paused, &pending));
        assert!(pending.load(Ordering::Acquire));
    }

    #[test]
    fn live_reload_send_waits_for_receiver_capacity() {
        let runtime = crate::runtime::build_runtime().expect("runtime should start");
        runtime.block_on(async {
            let (tx, mut rx) = mpsc::channel(1);
            tx.try_send(LiveDiffReload::Started)
                .expect("initial reload should send");
            let send_task = tokio::spawn({
                let send_tx = tx.clone();
                async move { send_live_reload(&send_tx, LiveDiffReload::Started).await }
            });

            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            assert!(!send_task.is_finished());
            assert!(matches!(rx.try_recv(), Ok(LiveDiffReload::Started)));

            assert!(
                tokio::time::timeout(std::time::Duration::from_secs(1), send_task)
                    .await
                    .expect("send task should finish")
                    .expect("send task should not panic")
            );
            assert!(matches!(rx.try_recv(), Ok(LiveDiffReload::Started)));
        });
    }
}
