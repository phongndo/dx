use super::*;

#[test]
fn difftool_source_renders_file_pair_with_display_path() {
    let test_dir = temp_test_dir("difftool-file-pair");
    fs::create_dir_all(&test_dir).expect("test directory should be created");
    fs::write(test_dir.join("local.tmp"), "old\n").expect("left file should be written");
    fs::write(test_dir.join("remote.tmp"), "new\nnext\n").expect("right file should be written");

    let options = DiffOptions {
        repo: Some(test_dir.clone().into()),
        source: DiffSource::Difftool {
            left: PathBuf::from("local.tmp").into(),
            right: PathBuf::from("remote.tmp").into(),
            path: Some(PathBuf::from("src/example.rs").into()),
        },
        local_untracked: crate::UntrackedMode::Exclude,
        output: crate::DiffOutput::Patch,
    };

    let patch = render(options.clone()).expect("difftool patch should render");
    assert!(patch.contains("diff --git a/src/example.rs b/src/example.rs"));
    assert!(patch.contains("--- a/src/example.rs"));
    assert!(patch.contains("+++ b/src/example.rs"));
    assert!(!patch.contains("local.tmp"));
    assert!(!patch.contains("remote.tmp"));

    let changeset = load_review_ref(&options).expect("difftool changeset should load");
    assert_eq!(changeset.title, "git difftool: src/example.rs");
    assert_eq!(changeset.files.len(), 1);
    assert_eq!(changeset.files[0].display_path(), "src/example.rs");
    assert_eq!(changeset.files[0].additions, 2);
    assert_eq!(changeset.files[0].deletions, 1);

    let stat = render(DiffOptions {
        output: crate::DiffOutput::Stat,
        ..options
    })
    .expect("difftool stat should render");
    assert!(stat.contains("src/example.rs"));

    fs::remove_dir_all(test_dir).expect("test directory should be removed");
}

#[test]
fn difftool_path_rewrite_ignores_hunk_body_header_like_lines() {
    let patch = b"diff --git a/left b/right\n--- a/left\n+++ b/right\n@@ -1,2 +1,2 @@\n context\n--- deleted heading\n+++ added heading\n";

    let rewritten = String::from_utf8(rewrite_difftool_patch_paths(patch, "shown.txt"))
        .expect("rewritten patch should be utf-8");

    assert!(rewritten.contains("--- a/shown.txt"));
    assert!(rewritten.contains("+++ b/shown.txt"));
    assert!(rewritten.contains("--- deleted heading"));
    assert!(rewritten.contains("+++ added heading"));
}

#[test]
fn difftool_path_rewrite_preserves_non_utf8_hunk_bytes() {
    let patch = b"diff --git a/left b/right\n--- a/left\n+++ b/right\n@@ -1 +1 @@\n-\xff\n+\xfe\n";

    let rewritten = rewrite_difftool_patch_paths(patch, "shown.txt");

    assert!(rewritten.starts_with(b"diff --git a/shown.txt b/shown.txt\n"));
    assert!(rewritten.contains(&0xff));
    assert!(rewritten.contains(&0xfe));
    assert!(rewritten.windows(3).all(|window| window != b"\xef\xbf\xbd"));
}

#[test]
fn difftool_stat_counts_non_utf8_text_hunks() {
    let test_dir = temp_test_dir("difftool-stat-non-utf8");
    fs::create_dir_all(&test_dir).expect("test directory should be created");
    fs::write(test_dir.join("local.tmp"), b"same\n\xff\n").expect("left file should be written");
    fs::write(test_dir.join("remote.tmp"), b"same\n\xfe\n").expect("right file should be written");

    let stat = String::from_utf8(
        render_bytes(DiffOptions {
            repo: Some(test_dir.clone().into()),
            source: DiffSource::Difftool {
                left: PathBuf::from("local.tmp").into(),
                right: PathBuf::from("remote.tmp").into(),
                path: Some(PathBuf::from("bytes.txt").into()),
            },
            local_untracked: crate::UntrackedMode::Exclude,
            output: crate::DiffOutput::Stat,
        })
        .expect("difftool stat should render"),
    )
    .expect("stat should be utf-8");

    assert!(stat.contains("bytes.txt"));
    assert!(stat.contains("1 insertions(+)"));
    assert!(stat.contains("1 deletions(-)"));

    fs::remove_dir_all(test_dir).expect("test directory should be removed");
}

#[test]
fn difftool_source_drops_mode_only_temp_file_changes() {
    use std::os::unix::fs::PermissionsExt;

    let test_dir = temp_test_dir("difftool-mode-only");
    fs::create_dir_all(&test_dir).expect("test directory should be created");
    fs::write(test_dir.join("local.tmp"), "same\n").expect("left file should be written");
    fs::write(test_dir.join("remote.tmp"), "same\n").expect("right file should be written");
    fs::set_permissions(
        test_dir.join("local.tmp"),
        fs::Permissions::from_mode(0o644),
    )
    .expect("left file mode should be set");
    fs::set_permissions(
        test_dir.join("remote.tmp"),
        fs::Permissions::from_mode(0o755),
    )
    .expect("right file mode should be set");

    let options = DiffOptions {
        repo: Some(test_dir.clone().into()),
        source: DiffSource::Difftool {
            left: PathBuf::from("local.tmp").into(),
            right: PathBuf::from("remote.tmp").into(),
            path: Some(PathBuf::from("mode-only.txt").into()),
        },
        local_untracked: crate::UntrackedMode::Exclude,
        output: crate::DiffOutput::Patch,
    };

    let patch = render_bytes(options.clone()).expect("patch should render");
    assert!(patch.is_empty());

    let stat = String::from_utf8(
        render_bytes(DiffOptions {
            output: crate::DiffOutput::Stat,
            ..options.clone()
        })
        .expect("stat should render"),
    )
    .expect("stat should be utf-8");
    assert!(!stat.contains("mode-only.txt"));
    assert!(stat.contains("0 files changed"));

    let changeset = load_review_ref(&options).expect("difftool changeset should load");
    assert!(changeset.files.is_empty());

    fs::remove_dir_all(test_dir).expect("test directory should be removed");
}

#[test]
fn difftool_source_suppresses_temp_file_mode_changes() {
    use std::os::unix::fs::PermissionsExt;

    let test_dir = temp_test_dir("difftool-temp-mode");
    fs::create_dir_all(&test_dir).expect("test directory should be created");
    fs::write(test_dir.join("local.tmp"), "#!/bin/sh\necho old\n")
        .expect("left file should be written");
    fs::write(test_dir.join("remote.tmp"), "#!/bin/sh\necho new\n")
        .expect("right file should be written");
    fs::set_permissions(
        test_dir.join("local.tmp"),
        fs::Permissions::from_mode(0o644),
    )
    .expect("left file mode should be set");
    fs::set_permissions(
        test_dir.join("remote.tmp"),
        fs::Permissions::from_mode(0o755),
    )
    .expect("right file mode should be set");

    let options = DiffOptions {
        repo: Some(test_dir.clone().into()),
        source: DiffSource::Difftool {
            left: PathBuf::from("local.tmp").into(),
            right: PathBuf::from("remote.tmp").into(),
            path: Some(PathBuf::from("bin/script.sh").into()),
        },
        local_untracked: crate::UntrackedMode::Exclude,
        output: crate::DiffOutput::Patch,
    };

    let patch = String::from_utf8(render_bytes(options.clone()).expect("patch should render"))
        .expect("patch should be utf-8");
    assert!(!patch.contains("old mode "));
    assert!(!patch.contains("new mode "));
    assert!(patch.contains("-echo old"));
    assert!(patch.contains("+echo new"));

    let changeset = load_review_ref(&options).expect("difftool changeset should load");
    assert_eq!(changeset.files[0].status(), FileStatus::Modified);

    fs::remove_dir_all(test_dir).expect("test directory should be removed");
}

#[test]
fn difftool_source_uses_left_display_path_for_deleted_pair() {
    let test_dir = temp_test_dir("difftool-deleted-pair");
    fs::create_dir_all(&test_dir).expect("test directory should be created");
    fs::write(test_dir.join("old-name.txt"), "gone\n").expect("left file should be written");

    let options = DiffOptions {
        repo: Some(test_dir.clone().into()),
        source: DiffSource::Difftool {
            left: PathBuf::from("old-name.txt").into(),
            right: PathBuf::from("/dev/null").into(),
            path: None,
        },
        local_untracked: crate::UntrackedMode::Exclude,
        output: crate::DiffOutput::Patch,
    };

    let patch = String::from_utf8(render_bytes(options.clone()).expect("patch should render"))
        .expect("patch should be utf-8");
    assert!(patch.contains("diff --git a/old-name.txt b/old-name.txt"));
    assert!(patch.contains("--- a/old-name.txt"));
    assert!(patch.contains("+++ /dev/null"));
    assert!(!patch.contains("a/null b/null"));

    let stat = render(DiffOptions {
        output: crate::DiffOutput::Stat,
        ..options.clone()
    })
    .expect("stat should render");
    assert!(stat.contains("old-name.txt"));
    assert!(!stat.contains(" null"));

    let changeset = load_review_ref(&options).expect("difftool changeset should load");
    assert_eq!(changeset.files[0].display_path(), "old-name.txt");
    assert_eq!(changeset.files[0].status(), FileStatus::Deleted);

    fs::remove_dir_all(test_dir).expect("test directory should be removed");
}

#[test]
fn difftool_source_rejects_missing_input_paths() {
    let test_dir = temp_test_dir("difftool-missing-input");
    fs::create_dir_all(&test_dir).expect("test directory should be created");
    fs::write(test_dir.join("local.tmp"), "left\n").expect("left file should be written");

    let error = render_bytes(DiffOptions {
        repo: Some(test_dir.clone().into()),
        source: DiffSource::Difftool {
            left: PathBuf::from("local.tmp").into(),
            right: PathBuf::from("missing.tmp").into(),
            path: Some(PathBuf::from("src/example.rs").into()),
        },
        local_untracked: crate::UntrackedMode::Exclude,
        output: crate::DiffOutput::Patch,
    })
    .expect_err("missing difftool input should fail");

    let message = error.to_string();
    assert!(message.contains("git difftool pair diff failed"));
    assert!(message.contains("Could not access"));

    fs::remove_dir_all(test_dir).expect("test directory should be removed");
}
