use super::*;
use crate::{
    difftool::rewrite_difftool_patch_paths,
    git_io::{StderrCapture, parse_numstat, temp_index_path},
};
use std::{
    env, fs,
    io::{BufReader, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

mod branch_review;
mod difftool;
mod parser;
mod patch_sources;
mod range;
mod repo_infra;
mod show;
mod worktree;

fn temp_test_dir(name: &str) -> PathBuf {
    env::temp_dir().join(format!(
        "mark-diff-{name}-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos()
    ))
}

fn init_repo(repo: &Path) {
    fs::create_dir_all(repo).expect("repo directory should be created");
    git(["init", "-q"], repo);
    git(["config", "user.email", "test@example.com"], repo);
    git(["config", "user.name", "Test"], repo);
    fs::write(repo.join("base.txt"), "base\n").expect("base file should be written");
    git(["add", "base.txt"], repo);
    git(["commit", "-q", "-m", "init"], repo);
}

fn git<const N: usize>(args: [&str; N], cwd: &Path) {
    let output = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("git should run");
    assert!(
        output.status.success(),
        "git failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git_apply(repo: &Path, patch: &[u8]) {
    let mut child = Command::new("git")
        .current_dir(repo)
        .args(["apply", "--binary"])
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("git apply should start");
    child
        .stdin
        .as_mut()
        .expect("stdin should be open")
        .write_all(patch)
        .expect("patch should be written");
    let output = child.wait_with_output().expect("git apply should finish");
    assert!(
        output.status.success(),
        "git apply failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git_output<const N: usize>(args: [&str; N], cwd: &Path) -> String {
    let output = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("git should run");
    assert!(
        output.status.success(),
        "git failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}
