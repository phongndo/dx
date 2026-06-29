use super::*;

#[test]
fn show_source_renders_commit_patch() {
    let test_dir = temp_test_dir("show-source");
    let repo = test_dir.join("repo");
    fs::create_dir_all(&test_dir).expect("test directory should be created");
    init_repo(&repo);
    fs::write(repo.join("base.txt"), "base\nchanged\n").expect("file should change");
    git(["add", "base.txt"], &repo);
    git(["commit", "-q", "-m", "change"], &repo);

    let expected = Command::new("git")
        .arg("-C")
        .arg(&repo)
        .args([
            "show",
            "--format=",
            "--binary",
            "--no-ext-diff",
            "--no-color",
            "--find-renames",
            "-m",
            "--end-of-options",
            "HEAD",
        ])
        .output()
        .expect("git show should run");
    assert!(
        expected.status.success(),
        "git show failed: {}",
        String::from_utf8_lossy(&expected.stderr)
    );

    let actual = render_bytes(DiffOptions {
        repo: Some(repo.clone().into()),
        source: DiffSource::Show("HEAD".into()),
        local_untracked: crate::UntrackedMode::Exclude,
        output: crate::DiffOutput::Patch,
    })
    .expect("show source should render");

    assert_eq!(actual, expected.stdout);

    let stat = String::from_utf8(
        render_bytes(DiffOptions {
            repo: Some(repo.into()),
            source: DiffSource::Show("HEAD".into()),
            local_untracked: crate::UntrackedMode::Exclude,
            output: crate::DiffOutput::Stat,
        })
        .expect("show source stats should render"),
    )
    .expect("stat should be utf-8");
    assert!(stat.contains("base.txt"));
    assert!(stat.contains("1 files changed, 1 insertions(+), 0 deletions(-)"));

    fs::remove_dir_all(test_dir).expect("test directory should be removed");
}

#[test]
fn show_source_stat_peels_annotated_tag() {
    let test_dir = temp_test_dir("show-annotated-tag-stat");
    let repo = test_dir.join("repo");
    fs::create_dir_all(&test_dir).expect("test directory should be created");
    init_repo(&repo);
    fs::write(repo.join("base.txt"), "base\nnext\n").expect("file should change");
    git(["commit", "-q", "-am", "change"], &repo);
    git(["tag", "-a", "--no-sign", "v1.0", "-m", "release"], &repo);

    let stat = String::from_utf8(
        render_bytes(DiffOptions {
            repo: Some(repo.clone().into()),
            source: DiffSource::Show("v1.0".into()),
            local_untracked: crate::UntrackedMode::Exclude,
            output: crate::DiffOutput::Stat,
        })
        .expect("show source stats should render"),
    )
    .expect("stat should be utf-8");

    assert!(stat.contains("base.txt"));
    assert!(stat.contains("1 files changed, 1 insertions(+), 0 deletions(-)"));
    fs::remove_dir_all(test_dir).expect("test directory should be removed");
}

#[test]
fn show_source_patch_peels_annotated_tag() {
    let test_dir = temp_test_dir("show-annotated-tag-patch");
    let repo = test_dir.join("repo");
    fs::create_dir_all(&test_dir).expect("test directory should be created");
    init_repo(&repo);
    fs::write(repo.join("base.txt"), "base\nnext\n").expect("file should change");
    git(["commit", "-q", "-am", "change"], &repo);
    git(
        [
            "tag",
            "-a",
            "--no-sign",
            "v1.0",
            "-m",
            "release tag metadata",
        ],
        &repo,
    );

    let patch = String::from_utf8(
        render_bytes(DiffOptions {
            repo: Some(repo.clone().into()),
            source: DiffSource::Show("v1.0".into()),
            local_untracked: crate::UntrackedMode::Exclude,
            output: crate::DiffOutput::Patch,
        })
        .expect("show source patch should render"),
    )
    .expect("patch should be utf-8");

    assert!(patch.starts_with("diff --git a/base.txt b/base.txt"));
    assert!(!patch.contains("tag v1.0"));
    assert!(!patch.contains("release tag metadata"));
    fs::remove_dir_all(test_dir).expect("test directory should be removed");
}

#[test]
fn show_source_stat_preserves_valid_revspec() {
    let test_dir = temp_test_dir("show-revspec-stat");
    let repo = test_dir.join("repo");
    fs::create_dir_all(&test_dir).expect("test directory should be created");
    init_repo(&repo);
    fs::write(repo.join("base.txt"), "base\nnext\n").expect("file should change");
    git(["commit", "-q", "-am", "change"], &repo);

    let stat = String::from_utf8(
        render_bytes(DiffOptions {
            repo: Some(repo.clone().into()),
            source: DiffSource::Show("HEAD^!".into()),
            local_untracked: crate::UntrackedMode::Exclude,
            output: crate::DiffOutput::Stat,
        })
        .expect("show source stats should render valid revspec"),
    )
    .expect("stat should be utf-8");

    assert!(stat.contains("base.txt"));
    assert!(stat.contains("1 files changed, 1 insertions(+), 0 deletions(-)"));
    fs::remove_dir_all(test_dir).expect("test directory should be removed");
}

#[test]
fn show_source_renders_merge_commit_as_parseable_parent_diffs() {
    let test_dir = temp_test_dir("show-merge-source");
    let repo = test_dir.join("repo");
    fs::create_dir_all(&test_dir).expect("test directory should be created");
    init_repo(&repo);

    git(["checkout", "-q", "-b", "left"], &repo);
    fs::write(repo.join("base.txt"), "left\n").expect("left file should change");
    git(["commit", "-q", "-am", "left"], &repo);

    git(["checkout", "-q", "-b", "right", "HEAD~1"], &repo);
    fs::write(repo.join("base.txt"), "right\n").expect("right file should change");
    git(["commit", "-q", "-am", "right"], &repo);

    git(["checkout", "-q", "left"], &repo);
    let merge = Command::new("git")
        .current_dir(&repo)
        .args(["merge", "--no-ff", "right", "-m", "merge"])
        .output()
        .expect("git merge should run");
    assert!(!merge.status.success(), "merge should conflict");
    fs::write(repo.join("base.txt"), "merged\n").expect("merge should be resolved");
    git(["add", "base.txt"], &repo);
    git(["commit", "-q", "--no-edit"], &repo);

    let changeset = load(DiffOptions {
        repo: Some(repo.clone().into()),
        source: DiffSource::Show("HEAD".into()),
        local_untracked: crate::UntrackedMode::Exclude,
        output: crate::DiffOutput::Patch,
    })
    .expect("show source should load merge diff");

    assert_eq!(changeset.files.len(), 2);
    assert!(
        changeset.files.iter().all(|file| !file.hunks().is_empty()),
        "merge parent diffs should parse into hunks"
    );
    let raw_patch = String::from_utf8_lossy(&changeset.raw_patch);
    assert!(raw_patch.contains("diff --git a/base.txt b/base.txt"));
    assert!(!raw_patch.contains("diff --cc"));
    assert!(!raw_patch.contains("@@@"));

    let stat = String::from_utf8(
        render_bytes(DiffOptions {
            repo: Some(repo.into()),
            source: DiffSource::Show("HEAD".into()),
            local_untracked: crate::UntrackedMode::Exclude,
            output: crate::DiffOutput::Stat,
        })
        .expect("show source stats should render"),
    )
    .expect("stat should be utf-8");
    assert!(stat.contains("2 files changed, 2 insertions(+), 2 deletions(-)"));

    fs::remove_dir_all(test_dir).expect("test directory should be removed");
}
