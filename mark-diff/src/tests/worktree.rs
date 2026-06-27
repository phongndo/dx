use super::*;

#[test]
fn render_bytes_stat_matches_full_changeset_stat_for_repo_source() {
    let test_dir = temp_test_dir("repo-stat-equivalence");
    let repo = test_dir.join("repo");
    fs::create_dir_all(&test_dir).expect("test directory should be created");
    init_repo(&repo);
    fs::write(repo.join("rename.txt"), "same\n").expect("renamed file should be written");
    fs::write(repo.join("binary.bin"), b"\0base\n").expect("binary file should be written");
    git(["add", "rename.txt", "binary.bin"], &repo);
    git(["commit", "-q", "-m", "fixtures"], &repo);

    fs::write(repo.join("base.txt"), "base\nnext\n").expect("tracked file should change");
    fs::write(repo.join("binary.bin"), b"\0changed\n").expect("binary file should change");
    fs::write(repo.join("untracked.txt"), "new\n").expect("untracked file should be written");
    git(["mv", "rename.txt", "renamed.txt"], &repo);
    let options = DiffOptions {
        repo: Some(repo.clone()),
        stat: true,
        ..DiffOptions::default()
    };

    let streamed = String::from_utf8(render_bytes(options.clone()).unwrap()).unwrap();
    let full = render_stat(&load_review_ref(&options).unwrap());

    assert_eq!(streamed, full);
    fs::remove_dir_all(test_dir).expect("test directory should be removed");
}

#[test]
fn render_untracked_empty_and_noeol_files_as_applyable_patch() {
    let test_dir = temp_test_dir("untracked-exact");
    let repo = test_dir.join("repo");
    let destination = test_dir.join("destination");
    fs::create_dir_all(&test_dir).expect("test directory should be created");
    init_repo(&repo);

    fs::write(repo.join("empty.txt"), "").expect("empty file should be written");
    fs::write(repo.join("noeol.txt"), "no newline").expect("noeol file should be written");

    git(
        [
            "clone",
            "-q",
            repo.to_str().unwrap(),
            destination.to_str().unwrap(),
        ],
        &test_dir,
    );
    let patch = render(DiffOptions {
        repo: Some(repo.clone()),
        ..DiffOptions::default()
    })
    .expect("diff should render");

    git_apply(&destination, patch.as_bytes());
    assert_eq!(fs::read(destination.join("empty.txt")).unwrap(), b"");
    assert_eq!(
        fs::read(destination.join("noeol.txt")).unwrap(),
        b"no newline"
    );

    fs::remove_dir_all(test_dir).expect("test directory should be removed");
}

#[test]
fn render_unborn_head_worktree_diff_against_empty_tree() {
    let test_dir = temp_test_dir("unborn-head-diff");
    let repo = test_dir.join("repo");
    fs::create_dir_all(&test_dir).expect("test directory should be created");
    fs::create_dir_all(&repo).expect("repo directory should be created");
    git(["init", "-q"], &repo);
    git(["config", "user.email", "test@example.com"], &repo);
    git(["config", "user.name", "Test"], &repo);
    fs::write(repo.join("new.txt"), "new\n").expect("new file should be written");

    let output = render(DiffOptions {
        repo: Some(repo.clone()),
        ..DiffOptions::default()
    })
    .expect("unborn HEAD diff should render");

    assert!(output.contains("diff --git a/new.txt b/new.txt"));
    assert!(output.contains("new file mode"));
    assert!(output.contains("+new"));
    fs::remove_dir_all(test_dir).expect("test directory should be removed");
}

#[test]
fn render_unborn_sha256_head_worktree_diff_against_empty_tree() {
    let test_dir = temp_test_dir("unborn-sha256-head-diff");
    let repo = test_dir.join("repo");
    fs::create_dir_all(&test_dir).expect("test directory should be created");
    fs::create_dir_all(&repo).expect("repo directory should be created");
    let init = Command::new("git")
        .current_dir(&repo)
        .args(["init", "-q", "--object-format=sha256"])
        .output()
        .expect("git init should run");
    if !init.status.success() {
        fs::remove_dir_all(test_dir).expect("test directory should be removed");
        return;
    }

    fs::write(repo.join("new.txt"), "new\n").expect("new file should be written");

    let output = render(DiffOptions {
        repo: Some(repo.clone()),
        ..DiffOptions::default()
    })
    .expect("unborn SHA-256 HEAD diff should render");

    assert!(output.contains("diff --git a/new.txt b/new.txt"));
    assert!(output.contains("new file mode"));
    assert!(output.contains("+new"));
    fs::remove_dir_all(test_dir).expect("test directory should be removed");
}

#[test]
fn render_untracked_symlink_as_symlink_without_reading_target() {
    let test_dir = temp_test_dir("untracked-symlink");
    let repo = test_dir.join("repo");
    let destination = test_dir.join("destination");
    fs::create_dir_all(&test_dir).expect("test directory should be created");
    init_repo(&repo);

    fs::write(test_dir.join("secret.txt"), "outside secret\n")
        .expect("target file should be written");
    std::os::unix::fs::symlink("../secret.txt", repo.join("link.txt"))
        .expect("symlink should be created");

    git(
        [
            "clone",
            "-q",
            repo.to_str().unwrap(),
            destination.to_str().unwrap(),
        ],
        &test_dir,
    );
    let patch = render(DiffOptions {
        repo: Some(repo.clone()),
        ..DiffOptions::default()
    })
    .expect("diff should render");

    assert!(patch.contains("new file mode 120000"));
    assert!(patch.contains("+../secret.txt"));
    assert!(!patch.contains("outside secret"));

    git_apply(&destination, patch.as_bytes());
    let target = fs::read_link(destination.join("link.txt")).unwrap();
    assert_eq!(target, PathBuf::from("../secret.txt"));

    fs::remove_dir_all(test_dir).expect("test directory should be removed");
}
