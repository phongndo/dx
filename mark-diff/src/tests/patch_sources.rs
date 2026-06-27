use super::*;

#[test]
fn patch_file_source_renders_without_git_repo() {
    let test_dir = temp_test_dir("patch-file-source");
    fs::create_dir_all(&test_dir).expect("test directory should be created");
    let patch_path = test_dir.join("change.diff");
    let patch = "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old\n+new\n";
    fs::write(&patch_path, patch).expect("patch file should be written");

    let output = render(DiffOptions {
        source: DiffSource::Patch(PatchSource::File(patch_path)),
        ..DiffOptions::default()
    })
    .expect("patch source should render");

    assert_eq!(output, patch);
    fs::remove_dir_all(test_dir).expect("test directory should be removed");
}

#[test]
fn render_bytes_preserves_non_utf8_git_diff_output() {
    let test_dir = temp_test_dir("non-utf8-diff");
    let repo = test_dir.join("repo");
    fs::create_dir_all(&test_dir).expect("test directory should be created");
    init_repo(&repo);

    fs::write(repo.join("bytes.txt"), b"same\n\xff\n")
        .expect("non-UTF-8 base file should be written");
    git(["add", "bytes.txt"], &repo);
    git(["commit", "-q", "-m", "bytes"], &repo);
    fs::write(repo.join("bytes.txt"), b"same\n\xfe\n")
        .expect("non-UTF-8 worktree file should be written");

    let expected = Command::new("git")
        .arg("-C")
        .arg(&repo)
        .args([
            "diff",
            "--binary",
            "--no-ext-diff",
            "--no-color",
            "--find-renames",
            "--end-of-options",
            "HEAD",
        ])
        .output()
        .expect("git diff should run");
    assert!(
        expected.status.success(),
        "git diff failed: {}",
        String::from_utf8_lossy(&expected.stderr)
    );
    assert!(expected.stdout.contains(&0xff));
    assert!(expected.stdout.contains(&0xfe));

    let actual = render_bytes(DiffOptions {
        repo: Some(repo.clone()),
        include_untracked: false,
        ..DiffOptions::default()
    })
    .expect("diff bytes should render");

    assert_eq!(actual, expected.stdout);
    let error = render(DiffOptions {
        repo: Some(repo),
        include_untracked: false,
        ..DiffOptions::default()
    })
    .expect_err("text rendering should reject non-UTF-8 output");
    assert!(error.to_string().contains("not valid UTF-8"));
    fs::remove_dir_all(test_dir).expect("test directory should be removed");
}

#[test]
fn patch_stdin_source_parses_stats_without_raw_patch_retention() {
    let patch = Arc::<[u8]>::from(
            b"diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1,2 @@\n-old\n+new\n+again\n".as_slice(),
        );
    let options = DiffOptions {
        source: DiffSource::Patch(PatchSource::Stdin(patch)),
        stat: true,
        ..DiffOptions::default()
    };

    let changeset = load_review_ref(&options).expect("patch source should parse");

    assert_eq!(changeset.files.len(), 1);
    assert_eq!(changeset.files[0].additions, 2);
    assert_eq!(changeset.files[0].deletions, 1);
    assert!(changeset.raw_patch.is_empty());
}

#[test]
fn patch_text_source_uses_label_title() {
    let patch = Arc::<[u8]>::from(
        b"diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old\n+new\n"
            .as_slice(),
    );
    let options = DiffOptions {
        source: DiffSource::Patch(PatchSource::Text {
            label: "github pr owner/repo#1".to_owned(),
            patch,
        }),
        ..DiffOptions::default()
    };

    let changeset = load_review_ref(&options).expect("patch source should parse");

    assert_eq!(changeset.title, "github pr owner/repo#1");
    assert_eq!(changeset.files.len(), 1);
}
