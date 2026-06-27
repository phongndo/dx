use std::{collections::HashSet, env, path::Path, process::Command};

use super::refs::GitCommit;

pub(crate) fn git_log_commits(repo: &Path) -> Vec<GitCommit> {
    if repo.as_os_str().is_empty() || !repo.exists() {
        return Vec::new();
    }

    let output = match Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["log", "--format=%H%x09%s", "-n", "500"])
        .output()
    {
        Ok(output) if output.status.success() => output,
        _ => return Vec::new(),
    };

    let mut commits = Vec::new();
    let mut seen = HashSet::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let Some((sha, subject)) = line.split_once('\t') else {
            continue;
        };
        let sha = sha.trim();
        if sha.is_empty() || !seen.insert(sha.to_owned()) {
            continue;
        }
        commits.push(GitCommit {
            sha: sha.to_owned(),
            subject: subject.trim().to_owned(),
        });
    }
    commits
}

pub(crate) fn git_commit_subject(repo: &Path, rev: &str) -> Option<String> {
    git_output(repo, ["log", "-1", "--format=%s", rev])
}

pub(crate) fn git_branches(repo: &Path) -> Vec<String> {
    if repo.as_os_str().is_empty() || !repo.exists() {
        return Vec::new();
    }

    let output = match Command::new("git")
        .arg("-C")
        .arg(repo)
        .args([
            "for-each-ref",
            "--sort=-committerdate",
            "--format=%(committerdate:unix)%09%(refname:short)",
            "refs/heads",
            "refs/remotes",
        ])
        .output()
    {
        Ok(output) if output.status.success() => output,
        _ => return Vec::new(),
    };

    let mut branches = Vec::new();
    let mut seen = HashSet::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let branch = line
            .split_once('\t')
            .map(|(_, branch)| branch)
            .unwrap_or(line)
            .trim();
        if branch.is_empty() || branch.ends_with("/HEAD") || !seen.insert(branch.to_owned()) {
            continue;
        }
        branches.push(branch.to_owned());
    }
    branches
}

pub(crate) fn env_branch_base() -> Option<String> {
    env::var("MARK_BASE_BRANCH")
        .ok()
        .map(|base| base.trim().to_owned())
        .filter(|base| !base.is_empty())
}

pub(crate) fn git_remote_head_branch(repo: &Path) -> Option<String> {
    git_output(
        repo,
        [
            "symbolic-ref",
            "--quiet",
            "--short",
            "refs/remotes/origin/HEAD",
        ],
    )
}

pub(crate) fn git_local_branch_candidate(repo: &Path) -> Option<String> {
    if !repo.exists() {
        return None;
    }

    ["main", "master"].into_iter().find_map(|branch| {
        mark_git::branch_exists(repo, branch)
            .ok()
            .filter(|exists| *exists)
            .map(|_| branch.to_owned())
    })
}

pub(crate) fn git_output<const N: usize>(repo: &Path, args: [&str; N]) -> Option<String> {
    if repo.as_os_str().is_empty() || !repo.exists() {
        return None;
    }

    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!value.is_empty()).then_some(value)
}
