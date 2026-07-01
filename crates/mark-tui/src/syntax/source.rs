use std::{
    collections::hash_map::DefaultHasher,
    fs,
    hash::{Hash, Hasher},
    panic::{self, AssertUnwindSafe},
    path::{Component, Path, PathBuf},
    process::Command,
};

use mark_diff::{
    DiffLine, DiffLineKind, DiffOptions, DiffSource, RepoRelativePath, RepoRoot, RevSpec,
};
use mark_syntax::{SyntaxHighlighter, SyntaxLimits};
use tokio::sync::mpsc::Sender;

use super::{DiffSide, HighlightedSide, SyntaxKey, SyntaxSkipReason, SyntaxWorkerQueue};

#[derive(Debug)]
pub(crate) struct SyntaxJob {
    pub(crate) key: SyntaxKey,
    pub(crate) language: String,
    pub(crate) source: SyntaxJobSource,
    pub(crate) limits: SyntaxLimits,
}

#[derive(Debug)]
pub(crate) struct SyntaxResult {
    pub(crate) key: SyntaxKey,
    pub(crate) side: Result<SyntaxSuccess, SyntaxJobFailure>,
}

#[derive(Debug)]
pub(crate) struct SyntaxSuccess {
    pub(crate) side: HighlightedSide,
    pub(crate) source_bytes: Option<u64>,
    pub(crate) source_lines: Option<u64>,
}

#[derive(Debug)]
pub(crate) enum SyntaxJobFailure {
    Unavailable,
    HighlightError,
}

#[derive(Debug)]
pub(crate) struct HunkSource {
    pub(crate) text: String,
    pub(crate) line_map: Vec<Option<usize>>,
    pub(crate) source_lines: usize,
}

#[derive(Debug)]
pub(crate) enum SyntaxJobSource {
    Hunk(HunkSource),
    FullFile(FullFileSource),
}

impl SyntaxJobSource {
    pub(crate) fn known_bytes(&self) -> Option<u64> {
        match self {
            Self::Hunk(source) => Some(source.text.len() as u64),
            Self::FullFile(_) => None,
        }
    }

    pub(crate) fn known_lines(&self) -> Option<u64> {
        match self {
            Self::Hunk(source) => Some(source.source_lines as u64),
            Self::FullFile(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FullFileSource {
    pub(crate) repo: RepoRoot,
    pub(crate) kind: FullFileSourceKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FullFileSourceKind {
    Worktree {
        path: RepoRelativePath,
    },
    GitRevision {
        rev: RevSpec,
        path: RepoRelativePath,
    },
    GitMergeBase {
        base: RevSpec,
        head: RevSpec,
        path: RepoRelativePath,
    },
}

pub(crate) fn run_syntax_worker(queue: SyntaxWorkerQueue, result_tx: Sender<SyntaxResult>) {
    let mut highlighter = SyntaxHighlighter::new();
    while let Some(job) = queue.pop() {
        let side = panic::catch_unwind(AssertUnwindSafe(|| {
            let (source, source_bytes, source_lines) = load_job_source(job.source, job.limits)?;
            highlighter
                .highlight(&job.language, &source)
                .map(|highlighted| SyntaxSuccess {
                    side: HighlightedSide {
                        lines: highlighted.lines,
                    },
                    source_bytes,
                    source_lines,
                })
                .map_err(|_| SyntaxJobFailure::HighlightError)
        }))
        .unwrap_or(Err(SyntaxJobFailure::HighlightError));
        if result_tx
            .blocking_send(SyntaxResult { key: job.key, side })
            .is_err()
        {
            break;
        }
    }
}

pub(crate) fn load_job_source(
    source: SyntaxJobSource,
    limits: SyntaxLimits,
) -> Result<(String, Option<u64>, Option<u64>), SyntaxJobFailure> {
    match source {
        SyntaxJobSource::Hunk(source) => Ok((source.text, None, None)),
        SyntaxJobSource::FullFile(source) => {
            let text = load_full_file_source(&source).map_err(|_| SyntaxJobFailure::Unavailable)?;
            validate_highlight_source(&text, limits).map_err(|_| SyntaxJobFailure::Unavailable)?;
            let source_bytes = text.len() as u64;
            let source_lines = source_line_count(&text) as u64;
            Ok((text, Some(source_bytes), Some(source_lines)))
        }
    }
}

pub(crate) fn syntax_path(file: &mark_diff::DiffFile, side: DiffSide) -> Option<&str> {
    match side {
        DiffSide::Old => file.old_path().or(file.new_path()),
        DiffSide::New => file.new_path().or(file.old_path()),
    }
}

pub(crate) fn build_hunk_source(
    lines: &[DiffLine],
    side: DiffSide,
    limits: SyntaxLimits,
) -> Result<HunkSource, SyntaxSkipReason> {
    let mut text = String::new();
    let mut line_map = vec![None; lines.len()];
    let mut source_lines = 0;

    for (index, line) in lines.iter().enumerate() {
        if !line_belongs_to_side(line.kind(), side) {
            continue;
        }
        if line.text().len() > limits.max_line_bytes {
            return Err(SyntaxSkipReason::TooLarge);
        }
        if source_lines > 0 {
            text.push('\n');
        }
        text.push_str(line.text());
        if text.len() > limits.max_source_bytes {
            return Err(SyntaxSkipReason::TooLarge);
        }
        line_map[index] = Some(source_lines);
        source_lines += 1;
    }

    if source_lines == 0 {
        return Err(SyntaxSkipReason::NoSource);
    }

    Ok(HunkSource {
        text,
        line_map,
        source_lines,
    })
}

pub(crate) fn build_full_file_line_map(
    lines: &[DiffLine],
    side: DiffSide,
) -> Result<Vec<Option<usize>>, SyntaxSkipReason> {
    let mut line_map = vec![None; lines.len()];
    let mut source_lines = 0;

    for (index, line) in lines.iter().enumerate() {
        if !line_belongs_to_side(line.kind(), side) {
            continue;
        }

        let Some(line_number) = diff_line_number(line, side) else {
            continue;
        };
        let Some(source_line) = line_number.checked_sub(1) else {
            continue;
        };
        line_map[index] = Some(source_line);
        source_lines += 1;
    }

    if source_lines == 0 {
        return Err(SyntaxSkipReason::NoSource);
    }

    Ok(line_map)
}

pub(crate) fn diff_line_number(line: &DiffLine, side: DiffSide) -> Option<usize> {
    match side {
        DiffSide::Old => line.old_line(),
        DiffSide::New => line.new_line(),
    }
}

pub(crate) fn full_file_source(
    repo: &Path,
    options: &DiffOptions,
    file: &mark_diff::DiffFile,
    side: DiffSide,
) -> Option<FullFileSource> {
    if matches!(
        options.source,
        DiffSource::Patch(_) | DiffSource::Show(_) | DiffSource::Difftool { .. }
    ) {
        return None;
    }
    if !repo.is_dir() {
        return None;
    }

    let path: RepoRelativePath = file_path_for_side(file, side)?.into();
    let kind = match (&options.source, side) {
        (DiffSource::Patch(_) | DiffSource::Show(_) | DiffSource::Difftool { .. }, _) => {
            return None;
        }
        (DiffSource::Worktree, DiffSide::Old) => FullFileSourceKind::GitRevision {
            rev: "HEAD".into(),
            path,
        },
        (DiffSource::Worktree, DiffSide::New) => FullFileSourceKind::Worktree { path },
        (DiffSource::Base(base), DiffSide::Old) => FullFileSourceKind::GitMergeBase {
            base: base.clone(),
            head: "HEAD".into(),
            path,
        },
        (DiffSource::Base(_), DiffSide::New) => FullFileSourceKind::Worktree { path },
        (DiffSource::Branch { base, head }, DiffSide::Old) => FullFileSourceKind::GitMergeBase {
            base: base.clone(),
            head: head.clone(),
            path,
        },
        (DiffSource::Branch { head, .. }, DiffSide::New) => FullFileSourceKind::GitRevision {
            rev: head.clone(),
            path,
        },
        (DiffSource::Range { left, .. }, DiffSide::Old) => FullFileSourceKind::GitRevision {
            rev: left.clone(),
            path,
        },
        (DiffSource::Range { right, .. }, DiffSide::New) => FullFileSourceKind::GitRevision {
            rev: right.clone(),
            path,
        },
    };

    Some(FullFileSource {
        repo: repo.to_owned().into(),
        kind,
    })
}

pub(crate) fn file_path_for_side(file: &mark_diff::DiffFile, side: DiffSide) -> Option<&str> {
    match side {
        DiffSide::Old => file.old_path(),
        DiffSide::New => file.new_path(),
    }
}

pub(crate) fn load_full_file_source(source: &FullFileSource) -> Result<String, SyntaxSkipReason> {
    let bytes = match &source.kind {
        FullFileSourceKind::Worktree { path } => read_worktree_file(&source.repo, path)?,
        FullFileSourceKind::GitRevision { rev, path } => {
            git_blob(&source.repo, &format!("{rev}:{path}"))?
        }
        FullFileSourceKind::GitMergeBase { base, head, path } => {
            let rev = git_merge_base(&source.repo, base, head)?;
            git_blob(&source.repo, &format!("{rev}:{path}"))?
        }
    };

    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

pub(crate) fn read_worktree_file(repo: &Path, path: &str) -> Result<Vec<u8>, SyntaxSkipReason> {
    let path = safe_repo_join(repo, path).ok_or(SyntaxSkipReason::NoPath)?;
    let metadata = fs::symlink_metadata(&path).map_err(|_| SyntaxSkipReason::NoSource)?;
    if !metadata.file_type().is_file() {
        return Err(SyntaxSkipReason::NoSource);
    }
    fs::read(path).map_err(|_| SyntaxSkipReason::NoSource)
}

pub(crate) fn safe_repo_join(repo: &Path, path: &str) -> Option<PathBuf> {
    let path = Path::new(path);
    if path.is_absolute() {
        return None;
    }

    let mut joined = repo.to_owned();
    for component in path.components() {
        match component {
            Component::Normal(part) => joined.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    Some(joined)
}

pub(crate) fn git_blob(repo: &Path, object: &str) -> Result<Vec<u8>, SyntaxSkipReason> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args([
            "show",
            "--no-ext-diff",
            "--no-color",
            "--end-of-options",
            object,
        ])
        .output()
        .map_err(|_| SyntaxSkipReason::NoSource)?;
    if !output.status.success() {
        return Err(SyntaxSkipReason::NoSource);
    }
    Ok(output.stdout)
}

pub(crate) fn git_merge_base(
    repo: &Path,
    base: &str,
    head: &str,
) -> Result<String, SyntaxSkipReason> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["merge-base", "--end-of-options", base, head])
        .output()
        .map_err(|_| SyntaxSkipReason::NoSource)?;
    if !output.status.success() {
        return Err(SyntaxSkipReason::NoSource);
    }

    let rev = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if rev.is_empty() {
        return Err(SyntaxSkipReason::NoSource);
    }
    Ok(rev)
}

pub(crate) fn validate_highlight_source(
    source: &str,
    limits: SyntaxLimits,
) -> Result<(), SyntaxSkipReason> {
    if source.len() > limits.max_source_bytes {
        return Err(SyntaxSkipReason::TooLarge);
    }
    if source
        .lines()
        .any(|line| line.len() > limits.max_line_bytes)
    {
        return Err(SyntaxSkipReason::TooLarge);
    }
    Ok(())
}

pub(crate) fn source_line_count(source: &str) -> usize {
    source.lines().count().max(1)
}

pub(crate) fn split_context_source_lines(source: &str) -> Vec<String> {
    source.lines().map(str::to_owned).collect()
}

pub(crate) fn available_context_lines(
    source_start: usize,
    total: usize,
    source_line_count: usize,
) -> usize {
    let Some(source_index_start) = source_start.checked_sub(1) else {
        return 0;
    };
    source_line_count
        .saturating_sub(source_index_start)
        .min(total)
}

pub(crate) fn hash_text(text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

pub(crate) fn line_belongs_to_side(kind: DiffLineKind, side: DiffSide) -> bool {
    matches!(
        (side, kind),
        (
            DiffSide::Old,
            DiffLineKind::Context | DiffLineKind::Deletion
        ) | (
            DiffSide::New,
            DiffLineKind::Context | DiffLineKind::Addition
        )
    )
}

pub(crate) fn unified_syntax_side(kind: DiffLineKind) -> Option<DiffSide> {
    match kind {
        DiffLineKind::Deletion => Some(DiffSide::Old),
        DiffLineKind::Addition | DiffLineKind::Context => Some(DiffSide::New),
        DiffLineKind::Meta => None,
    }
}
