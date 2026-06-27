use mark_core::{MarkError, MarkResult};
use std::{
    borrow::Cow,
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

const STREAM_BUFFER_BYTES: usize = 1024 * 1024;

mod difftool;
mod git_args;
mod git_io;
mod parser;
mod stats;
mod types;

use difftool::{difftool_patch_bytes, difftool_workdir};
use git_args::{
    append_pathspecs, diff_title, git_diff_args, should_include_untracked, validate_options,
};
use git_io::{
    git_diff_bytes, git_diff_bytes_with_untracked, git_diff_bytes_with_untracked_pathspecs,
    git_diff_to_writer, git_diff_to_writer_with_untracked,
};
pub use parser::parse_patch;
#[cfg(test)]
use stats::parse_patch_stats;
use stats::{patch_stats, render_patch_stats, terminal_safe_text};
pub use types::{
    Changeset, DiffFile, DiffHunk, DiffLine, DiffLineKind, DiffOptions, DiffRowRef, DiffScope,
    DiffSource, DiffStats, DiffViewModel, FileStatus, PatchSource,
};

pub fn load(options: DiffOptions) -> MarkResult<Changeset> {
    load_changeset(&options, true)
}

pub fn load_review(options: DiffOptions) -> MarkResult<Changeset> {
    load_review_ref(&options)
}

pub fn load_review_ref(options: &DiffOptions) -> MarkResult<Changeset> {
    load_changeset(options, false)
}

pub fn load_review_ref_path(options: &DiffOptions, path: &Path) -> MarkResult<Changeset> {
    load_changeset_paths(options, &[path.to_path_buf()], false)
}

pub fn load_review_ref_paths(options: &DiffOptions, paths: &[PathBuf]) -> MarkResult<Changeset> {
    load_changeset_paths(options, paths, false)
}

fn load_changeset(options: &DiffOptions, keep_raw_patch: bool) -> MarkResult<Changeset> {
    let title = diff_title(options);
    let (repo, patch) = diff_patch_bytes(options)?;
    changeset_from_patch(repo, title, patch, keep_raw_patch)
}

fn load_changeset_paths(
    options: &DiffOptions,
    paths: &[PathBuf],
    keep_raw_patch: bool,
) -> MarkResult<Changeset> {
    let title = diff_title(options);
    let (repo, patch) = diff_patch_bytes_paths(options, paths)?;
    changeset_from_patch(repo, title, Cow::Owned(patch), keep_raw_patch)
}

fn changeset_from_patch(
    repo: PathBuf,
    title: String,
    patch: Cow<'_, [u8]>,
    keep_raw_patch: bool,
) -> MarkResult<Changeset> {
    let files = {
        // The parsed model is text-only for stats/TUI display. Keep raw_patch
        // as bytes and only decode lossily at this display/parsing boundary.
        let patch_text = String::from_utf8_lossy(patch.as_ref());
        parse_patch(&patch_text)
    };
    let raw_patch = if keep_raw_patch {
        patch.into_owned()
    } else {
        Vec::new()
    };

    Ok(Changeset {
        repo,
        title,
        files,
        raw_patch,
    })
}

fn diff_patch_bytes_paths(
    options: &DiffOptions,
    paths: &[PathBuf],
) -> MarkResult<(PathBuf, Vec<u8>)> {
    if matches!(
        options.source,
        DiffSource::Patch(_) | DiffSource::Difftool { .. }
    ) {
        return Err(MarkError::Usage(
            "path-scoped reload does not apply to patch or difftool input".to_owned(),
        ));
    }
    if paths.is_empty() {
        return Err(MarkError::Usage(
            "path-scoped reload requires at least one path".to_owned(),
        ));
    }

    let repo = mark_git::repository_root(options.repo.as_deref())?;
    validate_options(options)?;
    let mut args = git_diff_args(options, &repo)?;
    append_pathspecs(&mut args, paths);
    let patch = if should_include_untracked(options) {
        git_diff_bytes_with_untracked_pathspecs(&repo, &args, paths)?
    } else {
        git_diff_bytes(&repo, &args)?
    };

    Ok((repo, patch))
}

fn diff_patch_bytes(options: &DiffOptions) -> MarkResult<(PathBuf, Cow<'_, [u8]>)> {
    if let DiffSource::Patch(source) = &options.source {
        validate_options(options)?;
        let repo = options.repo.clone().unwrap_or_default();
        return Ok((repo, patch_source_bytes(source)?));
    }

    if let DiffSource::Difftool { left, right, path } = &options.source {
        validate_options(options)?;
        let repo = difftool_workdir(options)?;
        let patch = difftool_patch_bytes(&repo, left, right, path.as_deref())?;
        return Ok((repo, Cow::Owned(patch)));
    }

    let repo = mark_git::repository_root(options.repo.as_deref())?;
    validate_options(options)?;
    let args = git_diff_args(options, &repo)?;
    let patch = if should_include_untracked(options) {
        git_diff_bytes_with_untracked(&repo, &args)?
    } else {
        git_diff_bytes(&repo, &args)?
    };

    Ok((repo, Cow::Owned(patch)))
}

pub fn render(options: DiffOptions) -> MarkResult<String> {
    let bytes = render_bytes(options)?;
    String::from_utf8(bytes).map_err(|_| {
        MarkError::Usage("diff output is not valid UTF-8; use byte-preserving output".to_owned())
    })
}

pub fn render_bytes(options: DiffOptions) -> MarkResult<Vec<u8>> {
    if options.stat {
        return render_stat_bytes(&options);
    }
    let (_, patch) = diff_patch_bytes(&options)?;
    Ok(patch.into_owned())
}

pub fn render_to_writer(options: DiffOptions, writer: impl Write) -> MarkResult<()> {
    render_to_writer_ref(&options, writer)
}

pub fn render_to_writer_ref(options: &DiffOptions, mut writer: impl Write) -> MarkResult<()> {
    if options.stat {
        writer.write_all(&render_stat_bytes(options)?)?;
        return Ok(());
    }

    if let DiffSource::Patch(source) = &options.source {
        validate_options(options)?;
        write_patch_source(source, writer)?;
        return Ok(());
    }

    if let DiffSource::Difftool { left, right, path } = &options.source {
        validate_options(options)?;
        let repo = difftool_workdir(options)?;
        writer.write_all(&difftool_patch_bytes(&repo, left, right, path.as_deref())?)?;
        return Ok(());
    }

    let repo = mark_git::repository_root(options.repo.as_deref())?;
    validate_options(options)?;
    let args = git_diff_args(options, &repo)?;
    if should_include_untracked(options) {
        git_diff_to_writer_with_untracked(&repo, &args, writer)
    } else {
        git_diff_to_writer(&repo, &args, writer)
    }
}

fn render_stat_bytes(options: &DiffOptions) -> MarkResult<Vec<u8>> {
    let stats = patch_stats(options)?;
    Ok(render_patch_stats(&stats).into_bytes())
}

fn write_patch_source(source: &PatchSource, mut writer: impl Write) -> MarkResult<()> {
    match source {
        PatchSource::File(path) => {
            let mut file = fs::File::open(path)?;
            copy_to_writer(&mut file, &mut writer)?;
        }
        PatchSource::Stdin(patch) => writer.write_all(patch.as_ref())?,
        PatchSource::Text { patch, .. } => writer.write_all(patch.as_ref())?,
    }
    Ok(())
}

fn copy_to_writer(mut reader: impl Read, mut writer: impl Write) -> io::Result<u64> {
    let mut total = 0u64;
    let mut buffer = vec![0; STREAM_BUFFER_BYTES];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        writer.write_all(&buffer[..read])?;
        total = total.saturating_add(read as u64);
    }
    Ok(total)
}

fn patch_source_bytes(source: &PatchSource) -> MarkResult<Cow<'_, [u8]>> {
    match source {
        PatchSource::File(path) => Ok(Cow::Owned(fs::read(path)?)),
        PatchSource::Stdin(patch) => Ok(Cow::Borrowed(patch.as_ref())),
        PatchSource::Text { patch, .. } => Ok(Cow::Borrowed(patch.as_ref())),
    }
}

pub fn render_stat(changeset: &Changeset) -> String {
    let mut output = String::new();
    for file in &changeset.files {
        output.push_str(&format!(
            "{:>6} {:>6} {}\n",
            file.additions,
            file.deletions,
            terminal_safe_text(file.display_path())
        ));
    }
    let stats = changeset.stats();
    output.push_str(&format!(
        "\n{} files changed, {} insertions(+), {} deletions(-)",
        stats.files, stats.additions, stats.deletions
    ));
    if stats.binary_files > 0 {
        output.push_str(&format!(", {} binary", stats.binary_files));
    }
    output.push('\n');
    output
}

#[cfg(test)]
mod tests;
