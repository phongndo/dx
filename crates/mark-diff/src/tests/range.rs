use super::*;

#[test]
fn range_diff_reports_unknown_revision() {
    let test_dir = temp_test_dir("unknown-range-revision");
    let repo = test_dir.join("repo");
    fs::create_dir_all(&test_dir).expect("test directory should be created");
    init_repo(&repo);

    let error = render(DiffOptions {
        repo: Some(repo.clone().into()),
        source: DiffSource::Range {
            left: "HEAD".into(),
            right: "missing-branch".into(),
        },
        local_untracked: crate::UntrackedMode::Exclude,
        output: crate::DiffOutput::Patch,
    })
    .expect_err("missing range side should fail before git diff");

    assert_eq!(error.to_string(), "unknown revision `missing-branch`");
    fs::remove_dir_all(test_dir).expect("test directory should be removed");
}

#[test]
fn range_diff_accepts_pathspec_right_operand() {
    let test_dir = temp_test_dir("range-pathspec-revision");
    let repo = test_dir.join("repo");
    fs::create_dir_all(&test_dir).expect("test directory should be created");
    init_repo(&repo);
    fs::create_dir_all(repo.join("src")).expect("source directory should be created");
    fs::write(repo.join("src/lib.rs"), "one\n").expect("lib file should be written");
    fs::write(repo.join("src/other.rs"), "one\n").expect("other file should be written");
    git(["add", "src/lib.rs", "src/other.rs"], &repo);
    git(["commit", "-q", "-m", "add sources"], &repo);
    fs::write(repo.join("src/lib.rs"), "two\n").expect("lib file should change");
    fs::write(repo.join("src/other.rs"), "two\n").expect("other file should change");

    let options = DiffOptions {
        repo: Some(repo.clone().into()),
        source: DiffSource::Range {
            left: "HEAD".into(),
            right: "src/lib.rs".into(),
        },
        local_untracked: crate::UntrackedMode::Exclude,
        output: crate::DiffOutput::Patch,
    };

    let patch = render(options.clone()).expect("pathspec range should render");
    assert!(patch.contains("diff --git a/src/lib.rs b/src/lib.rs"));
    assert!(patch.contains("+two"));
    assert!(!patch.contains("src/other.rs"));

    let stat = render(DiffOptions {
        output: crate::DiffOutput::Stat,
        ..options
    })
    .expect("pathspec range stat should render");
    assert!(stat.contains("src/lib.rs"));
    assert!(!stat.contains("src/other.rs"));

    fs::remove_dir_all(test_dir).expect("test directory should be removed");
}

#[test]
fn range_diff_accepts_treeish_revisions() {
    let test_dir = temp_test_dir("range-treeish-revisions");
    let repo = test_dir.join("repo");
    fs::create_dir_all(&test_dir).expect("test directory should be created");
    init_repo(&repo);
    let base_tree = git_output(["rev-parse", "HEAD^{tree}"], &repo);
    fs::write(repo.join("base.txt"), "changed\n").expect("base file should change");
    git(["add", "base.txt"], &repo);
    git(["commit", "-q", "-m", "change base"], &repo);

    let patch = render(DiffOptions {
        repo: Some(repo.clone().into()),
        source: DiffSource::Range {
            left: base_tree.into(),
            right: "HEAD".into(),
        },
        local_untracked: crate::UntrackedMode::Exclude,
        output: crate::DiffOutput::Patch,
    })
    .expect("tree object range should render");
    assert!(patch.contains("+changed"));

    let stat = render(DiffOptions {
        repo: Some(repo.clone().into()),
        source: DiffSource::Range {
            left: "HEAD~1^{tree}".into(),
            right: "HEAD".into(),
        },
        local_untracked: crate::UntrackedMode::Exclude,
        output: crate::DiffOutput::Stat,
    })
    .expect("tree-ish range stat should render");
    assert!(stat.contains("base.txt"));

    fs::remove_dir_all(test_dir).expect("test directory should be removed");
}

#[test]
fn range_diff_accepts_multi_object_left_revision() {
    let test_dir = temp_test_dir("range-multi-object-revision");
    let repo = test_dir.join("repo");
    fs::create_dir_all(&test_dir).expect("test directory should be created");
    init_repo(&repo);
    git(["branch", "-M", "main"], &repo);
    git(["checkout", "-q", "-b", "side"], &repo);
    fs::write(repo.join("side.txt"), "side\n").expect("side file should be written");
    git(["add", "side.txt"], &repo);
    git(["commit", "-q", "-m", "side"], &repo);
    git(["checkout", "-q", "main"], &repo);
    git(["merge", "-q", "--no-ff", "side", "-m", "merge"], &repo);

    let stat = render(DiffOptions {
        repo: Some(repo.clone().into()),
        source: DiffSource::Range {
            left: "HEAD^@".into(),
            right: "HEAD".into(),
        },
        local_untracked: crate::UntrackedMode::Exclude,
        output: crate::DiffOutput::Stat,
    })
    .expect("multi-object range should render");

    assert!(stat.contains("side.txt"));
    fs::remove_dir_all(test_dir).expect("test directory should be removed");
}

#[test]
fn range_diff_accepts_rev_path_tree_revisions() {
    let test_dir = temp_test_dir("range-rev-path-tree-revisions");
    let repo = test_dir.join("repo");
    fs::create_dir_all(&test_dir).expect("test directory should be created");
    init_repo(&repo);

    fs::create_dir_all(repo.join("src")).expect("source directory should be created");
    fs::write(repo.join("src/file.txt"), "one\n").expect("source file should be written");
    git(["add", "src/file.txt"], &repo);
    git(["commit", "-q", "-m", "add source"], &repo);

    fs::write(repo.join("src/file.txt"), "two\n").expect("source file should change");
    git(["add", "src/file.txt"], &repo);
    git(["commit", "-q", "-m", "change source"], &repo);

    let patch = render(DiffOptions {
        repo: Some(repo.clone().into()),
        source: DiffSource::Range {
            left: "HEAD~1:src".into(),
            right: "HEAD:src".into(),
        },
        local_untracked: crate::UntrackedMode::Exclude,
        output: crate::DiffOutput::Patch,
    })
    .expect("rev:path tree range should render");

    assert!(patch.contains("+two"));
    fs::remove_dir_all(test_dir).expect("test directory should be removed");
}

#[test]
fn range_diff_accepts_rev_path_blob_revisions() {
    let test_dir = temp_test_dir("range-rev-path-blob-revisions");
    let repo = test_dir.join("repo");
    fs::create_dir_all(&test_dir).expect("test directory should be created");
    init_repo(&repo);

    fs::write(repo.join("file.txt"), "one\n").expect("file should be written");
    git(["add", "file.txt"], &repo);
    git(["commit", "-q", "-m", "add file"], &repo);

    fs::write(repo.join("file.txt"), "two\n").expect("file should change");
    git(["add", "file.txt"], &repo);
    git(["commit", "-q", "-m", "change file"], &repo);

    let options = DiffOptions {
        repo: Some(repo.clone().into()),
        source: DiffSource::Range {
            left: "HEAD~1:file.txt".into(),
            right: "HEAD:file.txt".into(),
        },
        local_untracked: crate::UntrackedMode::Exclude,
        output: crate::DiffOutput::Patch,
    };

    let patch = render(options.clone()).expect("rev:path blob range should render");
    assert!(patch.contains("-one"));
    assert!(patch.contains("+two"));

    let stat = render(DiffOptions {
        output: crate::DiffOutput::Stat,
        ..options
    })
    .expect("rev:path blob range stat should render");
    assert!(stat.contains("file.txt"));

    fs::remove_dir_all(test_dir).expect("test directory should be removed");
}

#[test]
fn revision_operands_cannot_be_reinterpreted_as_git_diff_options() {
    let test_dir = temp_test_dir("revision-option-boundary");
    let repo = test_dir.join("repo");
    fs::create_dir_all(&test_dir).expect("test directory should be created");
    init_repo(&repo);
    let output_path = test_dir.join("poc.diff");

    let result = render(DiffOptions {
        repo: Some(repo.into()),
        source: DiffSource::Range {
            left: format!("--output={}", output_path.display()).into(),
            right: "HEAD".into(),
        },
        local_untracked: crate::UntrackedMode::Exclude,
        output: crate::DiffOutput::Patch,
    });

    assert!(result.is_err());
    assert!(!output_path.exists());
    fs::remove_dir_all(test_dir).expect("test directory should be removed");
}
