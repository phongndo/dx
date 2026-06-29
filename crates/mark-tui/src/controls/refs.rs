use std::path::Path;

use mark_diff::{BranchName, CommitSha, DiffOptions, DiffSource};
use unicode_width::UnicodeWidthStr;

use super::{
    git_queries::{
        env_branch_base, git_branches, git_commit_subject, git_local_branch_candidate,
        git_log_commits, git_output, git_remote_head_branch,
    },
    matcher::branch_match_score,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GitCommit {
    pub(crate) sha: CommitSha,
    pub(crate) subject: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BranchMenu {
    Head,
    Base,
}

pub(crate) fn default_branch_base(options: &DiffOptions, repo: &Path) -> Option<String> {
    branch_base_from_options(options)
        .or_else(env_branch_base)
        .or_else(|| git_remote_head_branch(repo))
        .or_else(|| git_local_branch_candidate(repo))
}

pub(crate) fn comparison_branches(repo: &Path, selected_refs: &[Option<&str>]) -> Vec<BranchName> {
    let mut branches: Vec<_> = git_branches(repo)
        .into_iter()
        .map(BranchName::from)
        .collect();
    for selected in selected_refs
        .iter()
        .filter_map(|selected| selected.filter(|reference| !reference.is_empty()))
    {
        if !branches.iter().any(|branch| branch.as_str() == selected) {
            branches.push(selected.into());
        }
    }
    branches
}

pub(crate) fn commit_match_score(query: &str, commit: &GitCommit) -> Option<(usize, usize)> {
    let sha_lower = commit.sha.as_str().to_ascii_lowercase();
    let short = commit.sha.get(..7).unwrap_or(commit.sha.as_str());
    let short_lower = short.to_ascii_lowercase();
    let subject_lower = commit.subject.to_ascii_lowercase();
    let combined = format!("{short_lower} {subject_lower}");

    branch_match_score(query, &sha_lower)
        .or_else(|| branch_match_score(query, &short_lower))
        .or_else(|| branch_match_score(query, &subject_lower))
        .or_else(|| branch_match_score(query, &combined))
}

pub(crate) fn comparison_commits(repo: &Path, selected_rev: Option<&str>) -> Vec<GitCommit> {
    let mut commits = git_log_commits(repo);
    if let Some(rev) = selected_rev.filter(|rev| !rev.is_empty()) {
        if !commits
            .iter()
            .any(|commit| commit.sha.as_str() == rev || commit.sha.starts_with(rev))
        {
            let subject = git_commit_subject(repo, rev).unwrap_or_default();
            commits.insert(
                0,
                GitCommit {
                    sha: rev.into(),
                    subject,
                },
            );
        }
    }
    commits
}

pub(crate) fn commit_short_sha(commit: &GitCommit) -> &str {
    commit.sha.get(..7).unwrap_or(commit.sha.as_str())
}

pub(crate) fn rev_display_label(rev: &str) -> &str {
    if rev.len() > 7 && rev.chars().all(|c| c.is_ascii_hexdigit()) {
        rev.get(..7).unwrap_or(rev)
    } else {
        rev
    }
}

pub(crate) fn commit_menu_label(commit: &GitCommit) -> String {
    let short = commit_short_sha(commit);
    if commit.subject.is_empty() {
        short.to_owned()
    } else {
        format!("{short} {subject}", subject = commit.subject)
    }
}

pub(crate) fn commit_menu_width(commits: &[GitCommit]) -> u16 {
    commits
        .iter()
        .map(|commit| commit_menu_label(commit).width() + 8)
        .max()
        .unwrap_or_default() as u16
}

pub(crate) fn branch_base_from_options(options: &DiffOptions) -> Option<String> {
    match &options.source {
        DiffSource::Base(base) if !base.is_empty() => Some(base.to_string()),
        DiffSource::Branch { base, .. } if !base.is_empty() => Some(base.to_string()),
        _ => None,
    }
}

pub(crate) fn branch_head_from_options(
    options: &DiffOptions,
    current_head: Option<&str>,
) -> Option<String> {
    match &options.source {
        DiffSource::Base(_) => current_head.map(str::to_owned),
        DiffSource::Branch { head, .. } if !head.is_empty() => Some(head.to_string()),
        _ => None,
    }
}

pub(crate) fn current_head_label(repo: &Path) -> Option<String> {
    mark_git::current_branch(repo)
        .ok()
        .flatten()
        .or_else(|| git_output(repo, ["rev-parse", "--short", "HEAD"]))
}
