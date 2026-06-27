use super::*;

#[test]
fn base_branch_diff_includes_committed_staged_and_untracked_changes() {
    let test_dir = temp_test_dir("base-branch-all-changes");
    let repo = test_dir.join("repo");
    fs::create_dir_all(&test_dir).expect("test directory should be created");
    init_repo(&repo);
    git(["branch", "-M", "main"], &repo);
    git(["checkout", "-q", "-b", "feature"], &repo);

    fs::write(repo.join("committed.txt"), "committed\n").expect("committed file should be written");
    git(["add", "committed.txt"], &repo);
    git(["commit", "-q", "-m", "committed"], &repo);
    fs::write(repo.join("staged.txt"), "staged\n").expect("staged file should be written");
    git(["add", "staged.txt"], &repo);
    fs::write(repo.join("untracked.txt"), "untracked\n").expect("untracked file should be written");

    let changeset = load_review_ref(&DiffOptions {
        repo: Some(repo.clone()),
        source: DiffSource::Base("main".to_owned()),
        ..DiffOptions::default()
    })
    .expect("base branch diff should load");
    let paths = changeset
        .files
        .iter()
        .map(DiffFile::display_path)
        .collect::<Vec<_>>();

    assert!(paths.contains(&"committed.txt"));
    assert!(paths.contains(&"staged.txt"));
    assert!(paths.contains(&"untracked.txt"));
    fs::remove_dir_all(test_dir).expect("test directory should be removed");
}

#[test]
fn base_branch_diff_reports_unknown_base_revision() {
    let test_dir = temp_test_dir("unknown-base-revision");
    let repo = test_dir.join("repo");
    fs::create_dir_all(&test_dir).expect("test directory should be created");
    init_repo(&repo);

    let error = render(DiffOptions {
        repo: Some(repo.clone()),
        source: DiffSource::Base("missing-branch".to_owned()),
        ..DiffOptions::default()
    })
    .expect_err("missing base should fail before git merge-base");

    assert_eq!(error.to_string(), "unknown base revision `missing-branch`");
    fs::remove_dir_all(test_dir).expect("test directory should be removed");
}

#[test]
fn base_branch_diff_keeps_commitish_validation() {
    let test_dir = temp_test_dir("base-treeish-revision");
    let repo = test_dir.join("repo");
    fs::create_dir_all(&test_dir).expect("test directory should be created");
    init_repo(&repo);

    let error = render(DiffOptions {
        repo: Some(repo.clone()),
        source: DiffSource::Base("HEAD^{tree}".to_owned()),
        ..DiffOptions::default()
    })
    .expect_err("merge-base diffs should still require commit-ish base revisions");

    assert_eq!(error.to_string(), "unknown base revision `HEAD^{tree}`");
    fs::remove_dir_all(test_dir).expect("test directory should be removed");
}

#[test]
fn load_review_ref_path_limits_tracked_and_untracked_files() {
    let test_dir = temp_test_dir("path-scoped-review");
    let repo = test_dir.join("repo");
    fs::create_dir_all(&test_dir).expect("test directory should be created");
    init_repo(&repo);
    fs::write(repo.join("other.txt"), "other\n").expect("other file should be written");
    git(["add", "other.txt"], &repo);
    git(["commit", "-q", "-m", "other"], &repo);

    fs::write(repo.join("base.txt"), "base changed\n").expect("base file should change");
    fs::write(repo.join("other.txt"), "other changed\n").expect("other file should change");
    fs::write(repo.join("new.txt"), "new\n").expect("untracked file should be written");
    let options = DiffOptions {
        repo: Some(repo.clone()),
        ..DiffOptions::default()
    };

    let tracked = load_review_ref_path(&options, Path::new("base.txt")).unwrap();
    assert_eq!(tracked.files.len(), 1);
    assert_eq!(tracked.files[0].display_path(), "base.txt");

    let untracked = load_review_ref_path(&options, Path::new("new.txt")).unwrap();
    assert_eq!(untracked.files.len(), 1);
    assert_eq!(untracked.files[0].display_path(), "new.txt");

    fs::remove_dir_all(test_dir).expect("test directory should be removed");
}

#[test]
fn load_review_ref_paths_preserves_scoped_rename_metadata() {
    let test_dir = temp_test_dir("path-scoped-rename");
    let repo = test_dir.join("repo");
    fs::create_dir_all(&test_dir).expect("test directory should be created");
    init_repo(&repo);
    let base = (1..=20)
        .map(|line| format!("line {line}\n"))
        .collect::<String>();
    fs::write(repo.join("old.txt"), base).expect("old file should be written");
    git(["add", "old.txt"], &repo);
    git(["commit", "-q", "-m", "old"], &repo);

    git(["mv", "old.txt", "new.txt"], &repo);
    let changed = (1..=20)
        .map(|line| {
            if line == 20 {
                "line changed\n".to_owned()
            } else {
                format!("line {line}\n")
            }
        })
        .collect::<String>();
    fs::write(repo.join("new.txt"), changed).expect("new file should be changed");
    let options = DiffOptions {
        repo: Some(repo.clone()),
        ..DiffOptions::default()
    };

    let new_only = load_review_ref_path(&options, Path::new("new.txt")).unwrap();
    assert_eq!(new_only.files[0].status, FileStatus::Added);

    let paired = load_review_ref_paths(
        &options,
        &[PathBuf::from("old.txt"), PathBuf::from("new.txt")],
    )
    .unwrap();

    assert_eq!(paired.files.len(), 1);
    assert_eq!(paired.files[0].status, FileStatus::Renamed);
    assert_eq!(paired.files[0].old_path.as_deref(), Some("old.txt"));
    assert_eq!(paired.files[0].new_path.as_deref(), Some("new.txt"));
    assert_eq!(paired.files[0].additions, 1);
    assert_eq!(paired.files[0].deletions, 1);

    fs::remove_dir_all(test_dir).expect("test directory should be removed");
}
