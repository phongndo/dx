use std::path::{Path, PathBuf};

use mark_core::MarkResult;
use mark_git::{
    existing_commitish_revision, existing_object_revision, merge_base_revision,
    range_right_operand_is_pathspec, revision_expression_exists, show_target,
    worktree_base_revision,
};

use crate::{DiffOptions, DiffSource, PatchSource, difftool::difftool_display_path};

pub(super) fn validate_options(_options: &DiffOptions) -> MarkResult<()> {
    Ok(())
}

pub(super) fn git_diff_args(options: &DiffOptions, repo: &Path) -> MarkResult<Vec<String>> {
    if let DiffSource::Show(rev) = &options.source {
        return git_show_args(repo, rev);
    }

    let mut args = vec![
        "diff".to_owned(),
        "--binary".to_owned(),
        "--no-ext-diff".to_owned(),
        "--no-color".to_owned(),
        "--find-renames".to_owned(),
    ];

    match &options.source {
        DiffSource::Worktree => {
            args.push("--end-of-options".to_owned());
            args.push(worktree_base_revision(repo)?);
        }
        DiffSource::Base(base) => {
            args.push("--end-of-options".to_owned());
            args.push(merge_base_revision(repo, base)?);
        }
        DiffSource::Branch { base, head } => {
            args.push("--end-of-options".to_owned());
            let base = existing_commitish_revision(repo, base, "base")?;
            let head = existing_commitish_revision(repo, head, "head")?;
            args.push(format!("{base}...{head}"));
        }
        DiffSource::Range { left, right } => {
            append_range_args(&mut args, repo, left, right)?;
        }
        DiffSource::Show(_) => {}
        DiffSource::Difftool { .. } => {}
        DiffSource::Patch(_) => {}
    }

    Ok(args)
}

fn git_show_args(repo: &Path, rev: &str) -> MarkResult<Vec<String>> {
    Ok(vec![
        "show".to_owned(),
        "--format=".to_owned(),
        "--binary".to_owned(),
        "--no-ext-diff".to_owned(),
        "--no-color".to_owned(),
        "--find-renames".to_owned(),
        "-m".to_owned(),
        "--end-of-options".to_owned(),
        show_target(repo, rev)?,
    ])
}

pub(super) fn append_pathspecs(args: &mut Vec<String>, paths: &[PathBuf]) {
    args.push("--".to_owned());
    args.extend(paths.iter().map(|path| path.to_string_lossy().into_owned()));
}

pub(super) fn git_diff_numstat_args(options: &DiffOptions, repo: &Path) -> MarkResult<Vec<String>> {
    if let DiffSource::Show(rev) = &options.source {
        return git_show_numstat_args(repo, rev);
    }

    let mut args = vec![
        "diff".to_owned(),
        "--numstat".to_owned(),
        "-z".to_owned(),
        "--no-ext-diff".to_owned(),
        "--no-color".to_owned(),
        "--find-renames".to_owned(),
    ];

    match &options.source {
        DiffSource::Worktree => {
            args.push("--end-of-options".to_owned());
            args.push(worktree_base_revision(repo)?);
        }
        DiffSource::Base(base) => {
            args.push("--end-of-options".to_owned());
            args.push(merge_base_revision(repo, base)?);
        }
        DiffSource::Branch { base, head } => {
            args.push("--end-of-options".to_owned());
            let base = existing_commitish_revision(repo, base, "base")?;
            let head = existing_commitish_revision(repo, head, "head")?;
            args.push(format!("{base}...{head}"));
        }
        DiffSource::Range { left, right } => {
            append_range_args(&mut args, repo, left, right)?;
        }
        DiffSource::Show(_) => {}
        DiffSource::Difftool { .. } => {}
        DiffSource::Patch(_) => {}
    }

    Ok(args)
}

fn append_range_args(
    args: &mut Vec<String>,
    repo: &Path,
    left: &str,
    right: &str,
) -> MarkResult<()> {
    args.push("--end-of-options".to_owned());
    let left = existing_object_revision(repo, left, "")?;

    if revision_expression_exists(repo, right)? {
        args.push(left);
        args.push(right.to_owned());
    } else if range_right_operand_is_pathspec(repo, &left, right)? {
        args.push(left);
        args.push("--".to_owned());
        args.push(right.to_owned());
    } else {
        args.push(left);
        args.push(existing_object_revision(repo, right, "")?);
    }

    Ok(())
}

fn git_show_numstat_args(repo: &Path, rev: &str) -> MarkResult<Vec<String>> {
    Ok(vec![
        "show".to_owned(),
        "--format=".to_owned(),
        "--numstat".to_owned(),
        "-z".to_owned(),
        "--no-ext-diff".to_owned(),
        "--no-color".to_owned(),
        "--find-renames".to_owned(),
        "-m".to_owned(),
        "--end-of-options".to_owned(),
        show_target(repo, rev)?,
    ])
}

pub(super) fn should_include_untracked(options: &DiffOptions) -> bool {
    options.include_untracked()
        && matches!(options.source, DiffSource::Worktree | DiffSource::Base(_))
}

pub(super) fn diff_title(options: &DiffOptions) -> String {
    match &options.source {
        DiffSource::Worktree => "working tree vs HEAD".to_owned(),
        DiffSource::Show(rev) => format!("show {rev}"),
        DiffSource::Base(base) => format!("{base}...HEAD"),
        DiffSource::Branch { base, head } => format!("{base}...{head}"),
        DiffSource::Range { left, right } => format!("{left}..{right}"),
        DiffSource::Difftool {
            left, right, path, ..
        } => {
            format!(
                "git difftool: {}",
                difftool_display_path(left, right, path.as_deref())
            )
        }
        DiffSource::Patch(PatchSource::File(path)) => format!("patch {}", path.display()),
        DiffSource::Patch(PatchSource::Stdin(_)) => "patch stdin".to_owned(),
        DiffSource::Patch(PatchSource::Text { label, .. })
        | DiffSource::Patch(PatchSource::Review { label, .. }) => label.to_string(),
    }
}
