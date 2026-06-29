use super::*;

#[test]
fn parse_patch_omits_no_newline_at_end_of_file_marker() {
    let patch = "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1,2 +1,2 @@\n line\n-old\n\\ No newline at end of file\n+new\n\\ No newline at end of file\n";

    let files = parse_patch(patch);

    assert_eq!(files.len(), 1);
    assert_eq!(files[0].hunks()[0].lines.len(), 3);
    assert!(
        files[0].hunks()[0]
            .lines
            .iter()
            .all(|line| line.kind() != DiffLineKind::Meta)
    );
}

#[test]
fn parse_patch_reads_file_hunks_and_line_numbers() {
    let patch = "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1,2 +1,3 @@\n one\n-two\n+two changed\n+three\n";

    let files = parse_patch(patch);

    assert_eq!(files.len(), 1);
    assert_eq!(files[0].display_path(), "a.txt");
    assert_eq!(files[0].additions, 2);
    assert_eq!(files[0].deletions, 1);
    assert_eq!(files[0].hunks()[0].lines[0].old_line(), Some(1));
    assert_eq!(files[0].hunks()[0].lines[0].new_line(), Some(1));
    assert_eq!(files[0].hunks()[0].lines[1].old_line(), Some(2));
    assert_eq!(files[0].hunks()[0].lines[1].new_line(), None);
    assert_eq!(files[0].hunks()[0].lines[2].old_line(), None);
    assert_eq!(files[0].hunks()[0].lines[2].new_line(), Some(2));
}

#[test]
fn parse_patch_stats_counts_without_storing_hunk_lines() {
    let patch = "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1,2 +1,3 @@\n one\n-two\n+two changed\n+three\ndiff --git a/blob.bin b/blob.bin\nBinary files a/blob.bin and b/blob.bin differ\n";

    let stats = parse_patch_stats(BufReader::new(patch.as_bytes())).unwrap();

    assert_eq!(stats.files.len(), 2);
    assert_eq!(stats.files[0].display_path(), "a.txt");
    assert_eq!(stats.files[0].additions, 2);
    assert_eq!(stats.files[0].deletions, 1);
    assert_eq!(stats.files[1].display_path(), "blob.bin");
    assert!(stats.files[1].is_binary());
    assert_eq!(stats.totals.files, 2);
    assert_eq!(stats.totals.additions, 2);
    assert_eq!(stats.totals.deletions, 1);
    assert_eq!(stats.totals.binary_files, 1);
}

#[test]
fn parse_patch_stats_counts_non_utf8_hunk_lines() {
    let patch = b"diff --git a/bytes.txt b/bytes.txt\n--- a/bytes.txt\n+++ b/bytes.txt\n@@ -1 +1 @@\n-\xff\n+\xfe\n";

    let stats = parse_patch_stats(BufReader::new(patch.as_slice())).unwrap();

    assert_eq!(stats.files.len(), 1);
    assert_eq!(stats.files[0].display_path(), "bytes.txt");
    assert_eq!(stats.files[0].additions, 1);
    assert_eq!(stats.files[0].deletions, 1);
    assert_eq!(stats.totals.files, 1);
    assert_eq!(stats.totals.additions, 1);
    assert_eq!(stats.totals.deletions, 1);
}

#[test]
fn render_bytes_stat_matches_full_changeset_stat_for_patch() {
    let patch = Arc::<[u8]>::from(
            b"--- a/a.txt\n+++ b/a.txt\n@@ -1 +1,2 @@\n-old\n+new\n+next\n--- a/b.txt\n+++ b/b.txt\n@@ -2 +2 @@\n-left\n+right\n"
                .as_slice(),
        );
    let options = DiffOptions {
        source: DiffSource::Patch(PatchSource::Stdin(patch)),
        output: crate::DiffOutput::Stat,
        local_untracked: crate::UntrackedMode::Exclude,
        ..DiffOptions::default()
    };

    let streamed = String::from_utf8(render_bytes(options.clone()).unwrap()).unwrap();
    let full = render_stat(&load_review_ref(&options).unwrap());

    assert_eq!(streamed, full);
}

#[test]
fn parse_numstat_reads_regular_renamed_and_binary_files() {
    let numstat =
        b"2\t1\tsrc/lib.rs\x00-\t-\timage.bin\x000\t0\t\x00old/name.rs\x00new/name.rs\x00";

    let stats = parse_numstat(numstat.as_slice()).unwrap();

    assert_eq!(stats.files.len(), 3);
    assert_eq!(stats.files[0].display_path(), "src/lib.rs");
    assert_eq!(stats.files[1].display_path(), "image.bin");
    assert!(stats.files[1].is_binary());
    assert_eq!(stats.files[2].display_path(), "new/name.rs");
    assert_eq!(stats.totals.files, 3);
    assert_eq!(stats.totals.additions, 2);
    assert_eq!(stats.totals.deletions, 1);
    assert_eq!(stats.totals.binary_files, 1);
}

#[test]
fn parse_patch_reads_plain_unified_diff_without_git_header() {
    let patch = "--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old\n+new\n";

    let files = parse_patch(patch);

    assert_eq!(files.len(), 1);
    assert_eq!(files[0].display_path(), "a.txt");
    assert_eq!(files[0].additions, 1);
    assert_eq!(files[0].deletions, 1);
}

#[test]
fn parse_patch_preserves_distinct_plain_unified_header_paths() {
    let patch = "--- old.txt\n+++ new.txt\n@@ -1 +1 @@\n-old\n+new\n";

    let files = parse_patch(patch);

    assert_eq!(files.len(), 1);
    assert_eq!(files[0].status(), FileStatus::Modified);
    assert_eq!(files[0].old_path(), Some("old.txt"));
    assert_eq!(files[0].new_path(), Some("new.txt"));
    assert_eq!(files[0].display_path(), "new.txt");
}

#[test]
fn plain_unified_file_headers_wait_for_completed_hunks() {
    let patch = "--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n--- old marker\n+++ new marker\n--- a/b.txt\n+++ b/b.txt\n@@ -1 +1 @@\n-old\n+new\n";

    let files = parse_patch(patch);

    assert_eq!(files.len(), 2);
    assert_eq!(files[0].display_path(), "a.txt");
    assert_eq!(files[0].hunks()[0].lines[0].text(), "-- old marker");
    assert_eq!(files[0].hunks()[0].lines[1].text(), "++ new marker");
    assert_eq!(files[1].display_path(), "b.txt");
}

#[test]
fn parse_patch_dequotes_git_c_style_paths() {
    let patch = "diff --git \"a/name\\twith\\\"quote\\\\.txt\" \"b/name\\twith\\\"quote\\\\.txt\"\n--- \"a/name\\twith\\\"quote\\\\.txt\"\n+++ \"b/name\\twith\\\"quote\\\\.txt\"\n@@ -1 +1 @@\n-old\n+new\n";

    let files = parse_patch(patch);

    assert_eq!(files.len(), 1);
    assert_eq!(files[0].old_path(), Some("name\twith\"quote\\.txt"));
    assert_eq!(files[0].new_path(), Some("name\twith\"quote\\.txt"));
    assert_eq!(files[0].display_path(), "name\twith\"quote\\.txt");
}

#[test]
fn parse_patch_dequotes_git_octal_utf8_paths() {
    let patch = "diff --git \"a/\\303\\251.txt\" \"b/\\303\\251.txt\"\n--- \"a/\\303\\251.txt\"\n+++ \"b/\\303\\251.txt\"\n@@ -1 +1 @@\n-old\n+new\n";

    let files = parse_patch(patch);

    assert_eq!(files.len(), 1);
    assert_eq!(files[0].old_path(), Some("é.txt"));
    assert_eq!(files[0].new_path(), Some("é.txt"));
    assert_eq!(files[0].display_path(), "é.txt");
}

#[test]
fn parse_patch_preserves_crlf_payloads() {
    let patch = "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old\r\n+old\n";

    let files = parse_patch(patch);

    assert_eq!(files.len(), 1);
    assert_eq!(files[0].hunks()[0].lines[0].text(), "old\r");
    assert_eq!(files[0].hunks()[0].lines[1].text(), "old");
}

#[test]
fn parse_patch_dequotes_rename_and_copy_metadata_paths() {
    let renamed = parse_patch(
        "diff --git \"a/old\\tname.txt\" \"b/new\\tname.txt\"\nsimilarity index 100%\nrename from \"old\\tname.txt\"\nrename to \"new\\tname.txt\"\n",
    );
    assert_eq!(renamed[0].old_path(), Some("old\tname.txt"));
    assert_eq!(renamed[0].new_path(), Some("new\tname.txt"));

    let copied = parse_patch(
        "diff --git \"a/src\\\"file.txt\" \"b/copy\\\"file.txt\"\nsimilarity index 100%\ncopy from \"src\\\"file.txt\"\ncopy to \"copy\\\"file.txt\"\n",
    );
    assert_eq!(copied[0].old_path(), Some("src\"file.txt"));
    assert_eq!(copied[0].new_path(), Some("copy\"file.txt"));
}

#[test]
fn stat_rendering_escapes_terminal_control_characters_in_paths() {
    let patch = Arc::<[u8]>::from(
            b"diff --git \"a/evil\\033]52;c;AAAA\\007.txt\" \"b/evil\\033]52;c;AAAA\\007.txt\"\n--- \"a/evil\\033]52;c;AAAA\\007.txt\"\n+++ \"b/evil\\033]52;c;AAAA\\007.txt\"\n@@ -1 +1 @@\n-old\n+new\n"
                .as_slice(),
        );
    let output = render(DiffOptions {
        source: DiffSource::Patch(PatchSource::Stdin(patch)),
        output: crate::DiffOutput::Stat,
        ..DiffOptions::default()
    })
    .expect("stat output should render");

    assert!(!output.as_bytes().contains(&0x1b));
    assert!(!output.as_bytes().contains(&0x07));
    assert!(output.contains("\\u{1b}]52;c;AAAA\\u{7}.txt"));
}

#[test]
fn parse_patch_preserves_binary_paths_with_spaces() {
    let patch = "diff --git a/my file.bin b/my file.bin\nindex 1111111..2222222 100644\nGIT binary patch\nliteral 1\nKcmZQz1ONa4\n\n";

    let files = parse_patch(patch);

    assert_eq!(files.len(), 1);
    assert_eq!(files[0].old_path(), Some("my file.bin"));
    assert_eq!(files[0].new_path(), Some("my file.bin"));
    assert_eq!(files[0].display_path(), "my file.bin");
    assert!(files[0].is_binary());
}

#[test]
fn rename_or_copy_status_wins_over_later_mode_headers() {
    let renamed = parse_patch(
        "diff --git a/old.txt b/new.txt\nrename from old.txt\nrename to new.txt\nold mode 100644\nnew mode 100755\n",
    );
    assert_eq!(renamed[0].status(), FileStatus::Renamed);

    let copied = parse_patch(
        "diff --git a/source.txt b/copy.txt\ncopy from source.txt\ncopy to copy.txt\nold mode 100644\nnew mode 100755\n",
    );
    assert_eq!(copied[0].status(), FileStatus::Copied);
}

#[test]
fn view_model_indexes_file_and_hunk_rows() {
    let changeset = Changeset {
        repo: PathBuf::from("/repo").into(),
        title: "test".to_owned(),
        files: parse_patch(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old\n+new\n",
        ),
        raw_patch: Vec::new(),
    };
    let model = DiffViewModel::new(&changeset);

    assert_eq!(model.file_start_row(0), Some(0));
    assert_eq!(model.file_at_row(3), Some(0));
    assert_eq!(model.next_hunk_row(0), Some(1));
    assert_eq!(model.previous_hunk_row(4), Some(1));
}
