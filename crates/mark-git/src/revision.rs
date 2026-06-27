use std::{
    path::Path,
    process::{Command, Output, Stdio},
};

use mark_core::{MarkError, MarkResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevisionStatus {
    Exists,
    Missing,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevisionKind {
    Commit,
    Object,
}

pub fn revision_status(repo: Option<&Path>, rev: &str, kind: RevisionKind) -> RevisionStatus {
    match kind {
        RevisionKind::Commit => commit_revision_status(repo, rev),
        RevisionKind::Object => match revision_expression_exists_optional(repo, rev) {
            Some(true) => RevisionStatus::Exists,
            Some(false) => missing_revision_status(repo),
            None => RevisionStatus::Unknown,
        },
    }
}

fn commit_revision_status(repo: Option<&Path>, rev: &str) -> RevisionStatus {
    let Some(object) = resolve_revision_optional(repo, rev) else {
        return missing_revision_status(repo);
    };

    match revision_object_matches_optional(repo, &object, "commit") {
        Some(true) => RevisionStatus::Exists,
        Some(false) => RevisionStatus::Missing,
        None => RevisionStatus::Unknown,
    }
}

fn missing_revision_status(repo: Option<&Path>) -> RevisionStatus {
    if git_repository_available(repo) {
        RevisionStatus::Missing
    } else {
        RevisionStatus::Unknown
    }
}

pub fn show_target(repo: &Path, rev: &str) -> MarkResult<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["cat-file", "-t", "--end-of-options"])
        .arg(rev)
        .output()?;

    if !output.status.success() {
        return Ok(rev.to_owned());
    }

    if String::from_utf8_lossy(&output.stdout).trim() == "tag" {
        Ok(format!("{rev}^{{}}"))
    } else {
        Ok(rev.to_owned())
    }
}

pub fn existing_commitish_revision(repo: &Path, rev: &str, kind: &str) -> MarkResult<String> {
    existing_revision(repo, rev, kind, "commit")
}

pub fn existing_object_revision(repo: &Path, rev: &str, kind: &str) -> MarkResult<String> {
    if revision_expression_exists(repo, rev)? {
        return Ok(rev.to_owned());
    }

    let label = if kind.is_empty() {
        "revision".to_owned()
    } else {
        format!("{kind} revision")
    };
    Err(MarkError::Usage(format!("unknown {label} `{rev}`")))
}

pub fn revision_expression_exists(repo: &Path, rev: &str) -> MarkResult<bool> {
    let output = rev_parse_verify(repo, rev)?;
    // `rev-parse --verify` exits non-zero for expressions that expand to
    // multiple objects, but still writes the resolved objects. `git diff`
    // accepts those expressions as range operands.
    if output.status.success() || !output_stdout_is_empty(&output) {
        return Ok(true);
    }

    multi_revision_expression_exists(repo, rev)
}

pub fn range_right_operand_is_pathspec(repo: &Path, left: &str, right: &str) -> MarkResult<bool> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        // Keep the right operand ambiguous here so Git decides whether it is a
        // pathspec in the same way it would for `git diff <rev> <path>`.
        .args(["diff", "--quiet", "--no-ext-diff", "--end-of-options"])
        .arg(left)
        .arg(right)
        .output()?;

    Ok(matches!(output.status.code(), Some(0) | Some(1)))
}

pub fn worktree_base_revision(repo: &Path) -> MarkResult<String> {
    if has_head(repo)? {
        Ok("HEAD".to_owned())
    } else {
        empty_tree_revision(repo)
    }
}

pub fn merge_base_revision(repo: &Path, base: &str) -> MarkResult<String> {
    if !commitish_exists(repo, base)? {
        return Err(MarkError::Usage(format!("unknown base revision `{base}`")));
    }

    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["merge-base", "--end-of-options", base, "HEAD"])
        .output()?;
    if !output.status.success() {
        return Err(crate::git_error(
            "failed to derive branch merge base",
            &output,
        ));
    }

    let revision = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if revision.is_empty() {
        return Err(MarkError::Usage(
            "git returned an empty merge base revision".to_owned(),
        ));
    }
    Ok(revision)
}

fn existing_revision(repo: &Path, rev: &str, kind: &str, object_kind: &str) -> MarkResult<String> {
    if revision_exists(repo, rev, object_kind)? {
        return Ok(rev.to_owned());
    }

    let label = if kind.is_empty() {
        "revision".to_owned()
    } else {
        format!("{kind} revision")
    };
    Err(MarkError::Usage(format!("unknown {label} `{rev}`")))
}

fn commitish_exists(repo: &Path, rev: &str) -> MarkResult<bool> {
    revision_exists(repo, rev, "commit")
}

fn revision_exists(repo: &Path, rev: &str, object_kind: &str) -> MarkResult<bool> {
    let Some(object) = resolve_revision(repo, rev)? else {
        return Ok(false);
    };

    revision_object_matches(repo, &object, object_kind)
}

fn resolve_revision(repo: &Path, rev: &str) -> MarkResult<Option<String>> {
    let output = rev_parse_verify(repo, rev)?;
    if !output.status.success() {
        return Ok(None);
    }

    let revision = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if revision.is_empty() {
        Ok(None)
    } else {
        Ok(Some(revision))
    }
}

fn resolve_revision_optional(repo: Option<&Path>, rev: &str) -> Option<String> {
    let output = rev_parse_verify_optional(repo, rev).ok()?;
    if !output.status.success() {
        return None;
    }

    let revision = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if revision.is_empty() {
        None
    } else {
        Some(revision)
    }
}

fn revision_expression_exists_optional(repo: Option<&Path>, rev: &str) -> Option<bool> {
    let output = rev_parse_verify_optional(repo, rev).ok()?;
    // `rev-parse --verify` exits non-zero for expressions that expand to
    // multiple objects, but still writes the resolved objects. `git diff`
    // accepts those expressions as range operands.
    if output.status.success() || !output_stdout_is_empty(&output) {
        return Some(true);
    }

    multi_revision_expression_exists_optional(repo, rev)
}

fn rev_parse_verify(repo: &Path, rev: &str) -> MarkResult<Output> {
    Ok(rev_parse_verify_optional(Some(repo), rev)?)
}

fn rev_parse_verify_optional(repo: Option<&Path>, rev: &str) -> std::io::Result<Output> {
    let mut command = git_command(repo);
    command
        .args(["rev-parse", "--verify", "--quiet", "--end-of-options"])
        .arg(rev);
    command.output()
}

fn multi_revision_expression_exists(repo: &Path, rev: &str) -> MarkResult<bool> {
    let output = multi_revision_expression_output(Some(repo), rev)?;
    Ok(output.status.success())
}

fn multi_revision_expression_exists_optional(repo: Option<&Path>, rev: &str) -> Option<bool> {
    multi_revision_expression_output(repo, rev)
        .ok()
        .map(|output| output.status.success())
}

fn multi_revision_expression_output(repo: Option<&Path>, rev: &str) -> std::io::Result<Output> {
    let mut command = git_command(repo);
    command
        .args(["rev-list", "--no-walk", "--quiet", "--end-of-options"])
        .arg(rev);
    command.output()
}

fn output_stdout_is_empty(output: &Output) -> bool {
    String::from_utf8_lossy(&output.stdout).trim().is_empty()
}

fn revision_object_matches(repo: &Path, object: &str, object_kind: &str) -> MarkResult<bool> {
    let output = revision_object_matches_output(Some(repo), object, object_kind)?;
    Ok(output.status.success())
}

fn revision_object_matches_optional(
    repo: Option<&Path>,
    object: &str,
    object_kind: &str,
) -> Option<bool> {
    revision_object_matches_output(repo, object, object_kind)
        .ok()
        .map(|output| output.status.success())
}

fn revision_object_matches_output(
    repo: Option<&Path>,
    object: &str,
    object_kind: &str,
) -> std::io::Result<Output> {
    let mut command = git_command(repo);
    command
        .args(["rev-parse", "--verify", "--quiet", "--end-of-options"])
        .arg(format!("{object}^{{{object_kind}}}"));
    command.output()
}

fn empty_tree_revision(repo: &Path) -> MarkResult<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["hash-object", "-t", "tree", "--stdin"])
        .stdin(Stdio::null())
        .output()?;
    if !output.status.success() {
        return Err(crate::git_error(
            "failed to derive empty tree revision",
            &output,
        ));
    }

    let revision = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if revision.is_empty() {
        return Err(MarkError::Usage(
            "git returned an empty tree revision with no object id".to_owned(),
        ));
    }
    Ok(revision)
}

fn has_head(repo: &Path) -> MarkResult<bool> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "--verify", "--quiet", "HEAD"])
        .output()?;
    Ok(output.status.success())
}

fn git_repository_available(repo: Option<&Path>) -> bool {
    let mut command = git_command(repo);
    command.args(["rev-parse", "--show-toplevel"]);

    command.output().is_ok_and(|output| output.status.success())
}

fn git_command(repo: Option<&Path>) -> Command {
    let mut command = Command::new("git");
    if let Some(repo) = repo {
        command.arg("-C").arg(repo);
    }
    command
}
