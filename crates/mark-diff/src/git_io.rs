use std::{
    fs,
    io::{self, ErrorKind, Read, Write},
    path::{Path, PathBuf},
    process::{self, Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use mark_core::{MarkError, MarkResult};

use crate::{
    copy_to_writer,
    stats::{PatchFileStat, PatchStats},
};

pub(super) fn git_diff_bytes(repo: &Path, args: &[String]) -> MarkResult<Vec<u8>> {
    git_diff_bytes_with_index(repo, args, None)
}

fn git_diff_bytes_with_index(
    repo: &Path,
    args: &[String],
    index: Option<&Path>,
) -> MarkResult<Vec<u8>> {
    let mut command = Command::new("git");
    command.arg("-C").arg(repo).args(args);
    if let Some(index) = index {
        command.env("GIT_INDEX_FILE", index);
    }

    let output = command.output()?;
    if !output.status.success() {
        return Err(git_error("failed to render git diff", &output));
    }
    Ok(output.stdout)
}

pub(super) fn git_diff_bytes_with_untracked(repo: &Path, args: &[String]) -> MarkResult<Vec<u8>> {
    let untracked = untracked_paths(repo)?;
    git_diff_bytes_with_untracked_paths(repo, args, untracked)
}

pub(super) fn git_diff_bytes_with_untracked_pathspecs(
    repo: &Path,
    args: &[String],
    pathspecs: &[PathBuf],
) -> MarkResult<Vec<u8>> {
    let untracked = untracked_paths_for(repo, pathspecs)?;
    git_diff_bytes_with_untracked_paths(repo, args, untracked)
}

fn git_diff_bytes_with_untracked_paths(
    repo: &Path,
    args: &[String],
    untracked: Vec<PathBuf>,
) -> MarkResult<Vec<u8>> {
    if untracked.is_empty() {
        return git_diff_bytes(repo, args);
    }

    let temp_index = create_temp_index(repo)?;
    add_intent_to_add(repo, temp_index.path(), &untracked)?;
    git_diff_bytes_with_index(repo, args, Some(temp_index.path()))
}

pub(super) fn git_diff_to_writer(
    repo: &Path,
    args: &[String],
    writer: impl Write,
) -> MarkResult<()> {
    git_diff_to_writer_with_index(repo, args, None, writer)
}

fn git_diff_to_writer_with_index(
    repo: &Path,
    args: &[String],
    index: Option<&Path>,
    mut writer: impl Write,
) -> MarkResult<()> {
    let mut command = Command::new("git");
    let stderr = StderrCapture::new()?;
    command
        .arg("-C")
        .arg(repo)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(stderr.stdio()?);
    if let Some(index) = index {
        command.env("GIT_INDEX_FILE", index);
    }

    let mut child = command.spawn()?;
    if let Some(mut stdout) = child.stdout.take() {
        if let Err(error) = copy_to_writer(&mut stdout, &mut writer) {
            abort_git_child(child, stderr);
            return Err(error.into());
        }
    }
    wait_for_git_child(child, stderr, "failed to render git diff")
}

pub(super) fn git_diff_to_writer_with_untracked(
    repo: &Path,
    args: &[String],
    writer: impl Write,
) -> MarkResult<()> {
    let untracked = untracked_paths(repo)?;
    if untracked.is_empty() {
        return git_diff_to_writer(repo, args, writer);
    }

    let temp_index = create_temp_index(repo)?;
    add_intent_to_add(repo, temp_index.path(), &untracked)?;
    git_diff_to_writer_with_index(repo, args, Some(temp_index.path()), writer)
}

pub(super) fn git_numstat_stats(repo: &Path, args: &[String]) -> MarkResult<PatchStats> {
    git_numstat_stats_with_index(repo, args, None)
}

fn git_numstat_stats_with_index(
    repo: &Path,
    args: &[String],
    index: Option<&Path>,
) -> MarkResult<PatchStats> {
    let mut command = Command::new("git");
    let stderr = StderrCapture::new()?;
    command
        .arg("-C")
        .arg(repo)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(stderr.stdio()?);
    if let Some(index) = index {
        command.env("GIT_INDEX_FILE", index);
    }

    let mut child = command.spawn()?;
    let stats = match if let Some(stdout) = child.stdout.take() {
        parse_numstat(stdout)
    } else {
        Ok(PatchStats::default())
    } {
        Ok(stats) => stats,
        Err(error) => {
            abort_git_child(child, stderr);
            return Err(error.into());
        }
    };
    wait_for_git_child(child, stderr, "failed to render git diff")?;
    Ok(stats)
}

pub(super) struct StderrCapture {
    pub(super) path: PathBuf,
    file: Option<fs::File>,
}

impl StderrCapture {
    pub(super) fn new() -> io::Result<Self> {
        for attempt in 0..1000u32 {
            let path = std::env::temp_dir().join(format!(
                "mark-git-stderr-{}-{}-{attempt}.tmp",
                process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(io::Error::other)?
                    .as_nanos()
            ));
            match create_private_temp_file(&path) {
                Ok(file) => {
                    return Ok(Self {
                        path,
                        file: Some(file),
                    });
                }
                Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }

        Err(io::Error::new(
            ErrorKind::AlreadyExists,
            "failed to create git stderr temp file",
        ))
    }

    fn stdio(&self) -> io::Result<Stdio> {
        let file = self
            .file
            .as_ref()
            .ok_or_else(|| io::Error::other("git stderr temp file was already closed"))?;
        Ok(Stdio::from(file.try_clone()?))
    }

    fn read(mut self) -> io::Result<Vec<u8>> {
        drop(self.file.take());
        fs::read(&self.path)
    }

    pub(super) fn discard(mut self) {
        drop(self.file.take());
    }
}

impl Drop for StderrCapture {
    fn drop(&mut self) {
        drop(self.file.take());
        let _ = fs::remove_file(&self.path);
    }
}

fn wait_for_git_child(
    mut child: process::Child,
    stderr: StderrCapture,
    message: &str,
) -> MarkResult<()> {
    let status = child.wait()?;
    let stderr = stderr.read()?;
    let output = process::Output {
        status,
        stdout: Vec::new(),
        stderr,
    };
    if !output.status.success() {
        return Err(git_error(message, &output));
    }
    Ok(())
}

fn abort_git_child(mut child: process::Child, stderr: StderrCapture) {
    let _ = child.kill();
    let _ = child.wait();
    stderr.discard();
}

pub(super) fn git_numstat_stats_with_untracked(
    repo: &Path,
    args: &[String],
) -> MarkResult<PatchStats> {
    let untracked = untracked_paths(repo)?;
    if untracked.is_empty() {
        return git_numstat_stats(repo, args);
    }

    let temp_index = create_temp_index(repo)?;
    add_intent_to_add(repo, temp_index.path(), &untracked)?;
    git_numstat_stats_with_index(repo, args, Some(temp_index.path()))
}

pub(super) fn parse_numstat(mut reader: impl Read) -> io::Result<PatchStats> {
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes)?;

    let records = bytes
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
        .collect::<Vec<_>>();
    let mut stats = PatchStats::default();
    let mut index = 0usize;

    while let Some(record) = records.get(index).copied() {
        let mut fields = record.splitn(3, |byte| *byte == b'\t');
        let additions = fields.next().unwrap_or_default();
        let deletions = fields.next().unwrap_or_default();
        let path = fields.next().unwrap_or_default();
        let (display_path, next_index) = if path.is_empty() && index + 2 < records.len() {
            (records[index + 2], index + 3)
        } else {
            (path, index + 1)
        };

        let is_binary = additions == b"-" || deletions == b"-";
        let additions = parse_numstat_count(additions).unwrap_or_default();
        let deletions = parse_numstat_count(deletions).unwrap_or_default();
        let file = PatchFileStat {
            old_path: None,
            new_path: Some(String::from_utf8_lossy(display_path).into_owned()),
            additions,
            deletions,
            is_binary,
        };

        stats.totals.files += 1;
        stats.totals.additions += additions;
        stats.totals.deletions += deletions;
        if is_binary {
            stats.totals.binary_files += 1;
        }
        stats.files.push(file);
        index = next_index;
    }

    Ok(stats)
}

fn parse_numstat_count(bytes: &[u8]) -> Option<usize> {
    std::str::from_utf8(bytes).ok()?.parse().ok()
}

fn add_intent_to_add(repo: &Path, index: &Path, paths: &[PathBuf]) -> MarkResult<()> {
    for chunk in paths.chunks(128) {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo)
            .env("GIT_INDEX_FILE", index)
            .args(["add", "-N", "--"])
            .args(chunk)
            .output()?;
        if !output.status.success() {
            return Err(git_error(
                "failed to prepare untracked files for diff",
                &output,
            ));
        }
    }
    Ok(())
}

#[derive(Debug)]
struct TempIndex {
    path: PathBuf,
}

impl TempIndex {
    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempIndex {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn create_temp_index(repo: &Path) -> MarkResult<TempIndex> {
    let source = git_path(repo, "index")?;
    for attempt in 0..16 {
        let path = temp_index_path(&source, attempt)?;
        let mut temp = match create_private_temp_file(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        };

        let copy_result = (|| -> MarkResult<()> {
            if source.exists() {
                let mut source_file = fs::File::open(&source)?;
                std::io::copy(&mut source_file, &mut temp)?;
                temp.flush()?;
            } else {
                temp.flush()?;
                initialize_empty_index(repo, &path)?;
            }
            Ok(())
        })();

        if let Err(error) = copy_result {
            let _ = fs::remove_file(&path);
            return Err(error);
        }

        return Ok(TempIndex { path });
    }

    Err(MarkError::Usage(
        "failed to create a unique temporary git index".to_owned(),
    ))
}

fn initialize_empty_index(repo: &Path, index: &Path) -> MarkResult<()> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .env("GIT_INDEX_FILE", index)
        .args(["read-tree", "--empty"])
        .output()?;
    if !output.status.success() {
        return Err(git_error(
            "failed to initialize temporary git index",
            &output,
        ));
    }
    Ok(())
}

fn git_path(repo: &Path, path: &str) -> MarkResult<PathBuf> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "--git-path", path])
        .output()?;
    if !output.status.success() {
        return Err(git_error("failed to resolve git path", &output));
    }

    let path = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if path.is_empty() {
        return Err(MarkError::Usage("git path was empty".to_owned()));
    }

    let path = PathBuf::from(path);
    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(repo.join(path))
    }
}

fn create_private_temp_file(path: &Path) -> std::io::Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

pub(super) fn temp_index_path(index_path: &Path, attempt: u32) -> MarkResult<PathBuf> {
    let parent = index_path.parent().ok_or_else(|| {
        MarkError::Usage(format!(
            "git index path has no parent: {}",
            index_path.display()
        ))
    })?;
    Ok(parent.join(format!(
        ".mark-diff-index-{}-{}-{}.tmp",
        process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| MarkError::Usage(format!("system time before unix epoch: {error}")))?
            .as_nanos(),
        attempt
    )))
}

fn untracked_paths(repo: &Path) -> MarkResult<Vec<PathBuf>> {
    untracked_paths_for(repo, &[])
}

fn untracked_paths_for(repo: &Path, pathspecs: &[PathBuf]) -> MarkResult<Vec<PathBuf>> {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(repo)
        .args(["ls-files", "--others", "--exclude-standard", "-z"]);
    if !pathspecs.is_empty() {
        command.arg("--").args(pathspecs);
    }

    let output = command.output()?;

    if !output.status.success() {
        return Err(git_error("failed to list untracked files", &output));
    }

    Ok(output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(path_from_git_bytes)
        .collect())
}

#[cfg(unix)]
fn path_from_git_bytes(path: &[u8]) -> PathBuf {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};

    PathBuf::from(OsString::from_vec(path.to_vec()))
}

#[cfg(not(unix))]
fn path_from_git_bytes(path: &[u8]) -> PathBuf {
    PathBuf::from(String::from_utf8_lossy(path).into_owned())
}

pub(super) fn git_error(message: &str, output: &std::process::Output) -> MarkError {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if stderr.is_empty() {
        MarkError::Usage(message.to_owned())
    } else {
        MarkError::Usage(format!("{message}: {stderr}"))
    }
}
