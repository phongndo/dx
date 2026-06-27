use super::*;

#[test]
fn patch_detection_ignores_ansi_color() {
    assert!(looks_like_patch_input(
        b"\x1b[1mdiff --git a/a b/a\x1b[0m\n--- a/a\n+++ b/a\n@@ -1 +1 @@\n-a\n+b\n"
    ));
}

#[test]
fn patch_detection_rejects_bare_hunk_marker() {
    assert!(!looks_like_patch_input(
        b"commit abc123\n\n    @@ -1 +1 @@\n    example text\n"
    ));
}

#[test]
fn patch_detection_rejects_unified_headers_without_changes() {
    assert!(!looks_like_patch_input(
        b"commit abc123\n\n--- not-a-diff\n+++ still-not-a-diff\n"
    ));
}

#[test]
fn patch_detection_accepts_metadata_only_git_diff() {
    assert!(looks_like_patch_input(
        b"diff --git a/old.txt b/new.txt\nrename from old.txt\nrename to new.txt\n"
    ));
}

#[test]
fn normalized_patch_input_preserves_crlf_payloads() {
    let patch =
        b"diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old\r\n+old\n";

    let normalized = normalized_patch_input(patch);
    let text = String::from_utf8_lossy(&normalized);
    let files = mark_diff::parse_patch(&text);

    assert_eq!(files.len(), 1);
    assert_eq!(files[0].hunks[0].lines[0].text, "old\r");
    assert_eq!(files[0].hunks[0].lines[1].text, "old");
}

#[test]
fn normalized_patch_input_preserves_literal_terminal_sequences() {
    let patch = b"diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old\n+\x1b[31mred\x1b[0m\n";

    let normalized = normalized_patch_input(patch);
    let text = String::from_utf8_lossy(&normalized);
    let files = mark_diff::parse_patch(&text);

    assert_eq!(files.len(), 1);
    assert_eq!(files[0].hunks[0].lines[1].text, "\x1b[31mred\x1b[0m");
}

#[test]
fn normalized_patch_input_preserves_literal_terminal_sequences_after_colored_headers() {
    let patch = b"\x1b[1mdiff --git a/a.txt b/a.txt\x1b[m\n\x1b[1m--- a/a.txt\x1b[m\n\x1b[1m+++ b/a.txt\x1b[m\n\x1b[36m@@ -1,2 +1,2 @@\x1b[m\n \x1b[33mctx\x1b[0m\n-\x1b[31mold\x1b[0m\n+\x1b[32mnew\x1b[0m\n";

    let normalized = normalized_patch_input(patch);
    let text = String::from_utf8_lossy(&normalized);
    let files = mark_diff::parse_patch(&text);

    assert_eq!(files.len(), 1);
    assert_eq!(files[0].hunks[0].lines[0].text, "\x1b[33mctx\x1b[0m");
    assert_eq!(files[0].hunks[0].lines[1].text, "\x1b[31mold\x1b[0m");
    assert_eq!(files[0].hunks[0].lines[2].text, "\x1b[32mnew\x1b[0m");
}

#[test]
fn normalized_patch_input_strips_only_git_color_wrappers() {
    let patch = b"\x1b[1mdiff --git a/a.txt b/a.txt\x1b[m\n\x1b[1m--- a/a.txt\x1b[m\n\x1b[1m+++ b/a.txt\x1b[m\n\x1b[36m@@ -1 +1 @@\x1b[m\n\x1b[31m-old\x1b[m\n\x1b[32m+\x1b[31mred\x1b[0m\x1b[m\n";

    let normalized = normalized_patch_input(patch);
    let text = String::from_utf8_lossy(&normalized);
    let files = mark_diff::parse_patch(&text);

    assert_eq!(files.len(), 1);
    assert_eq!(files[0].hunks[0].lines[0].text, "old");
    assert_eq!(files[0].hunks[0].lines[1].text, "\x1b[31mred\x1b[0m");
}

#[test]
fn normalized_patch_input_preserves_literal_line_color_sequence() {
    let patch = b"\x1b[1mdiff --git a/a.txt b/a.txt\x1b[m\n\x1b[1m--- a/a.txt\x1b[m\n\x1b[1m+++ b/a.txt\x1b[m\n\x1b[36m@@ -1 +1 @@\x1b[m\n\x1b[31m-old\x1b[m\n\x1b[32m+\x1b[m\x1b[32m\x1b[32mgreen\x1b[0m\x1b[m\n";

    let normalized = normalized_patch_input(patch);
    let text = String::from_utf8_lossy(&normalized);
    let files = mark_diff::parse_patch(&text);

    assert_eq!(files.len(), 1);
    assert_eq!(files[0].hunks[0].lines[1].text, "\x1b[32mgreen\x1b[0m");
}

#[test]
fn normalized_patch_input_strips_git_resets_inside_colored_diff_lines() {
    let patch = b"\x1b[1mdiff --git a/a.txt b/a.txt\x1b[m\n\x1b[1m--- a/a.txt\x1b[m\n\x1b[1m+++ b/a.txt\x1b[m\n\x1b[36m@@\x1b[m -1,2 +1,2 \x1b[36m@@\x1b[m fn\x1b[m\n \x1b[mcontext\x1b[m\n\x1b[31m-old\x1b[m\n\x1b[32m+\x1b[mnew\x1b[m\n";

    let normalized = normalized_patch_input(patch);
    let text = String::from_utf8_lossy(&normalized);
    let files = mark_diff::parse_patch(&text);

    assert!(!text.contains("\x1b[m"));
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].hunks[0].header, "@@ -1,2 +1,2 @@ fn");
    assert_eq!(files[0].hunks[0].lines[0].text, "context");
    assert_eq!(files[0].hunks[0].lines[2].text, "new");
}

#[test]
fn normalized_patch_input_strips_standard_git_color_wrappers() {
    let patch = b"\x1b[1mdiff --git a/a.txt b/a.txt\x1b[m\n\x1b[1m--- a/a.txt\x1b[m\n\x1b[1m+++ b/a.txt\x1b[m\n\x1b[36m@@ -1,3 +1,3 @@\x1b[m\n context before\x1b[m\n\x1b[31m-old\x1b[m\n\x1b[32m+\x1b[m\x1b[32mnew\x1b[m\n context after\x1b[m\n";

    let normalized = normalized_patch_input(patch);
    let text = String::from_utf8_lossy(&normalized);
    let files = mark_diff::parse_patch(&text);

    assert!(!text.contains('\x1b'));
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].hunks[0].lines[0].text, "context before");
    assert_eq!(files[0].hunks[0].lines[1].text, "old");
    assert_eq!(files[0].hunks[0].lines[2].text, "new");
    assert_eq!(files[0].hunks[0].lines[3].text, "context after");
}

#[test]
fn split_patch_prelude_keeps_git_show_text_out_of_rendered_patch() {
    let patch = b"commit abc123\nAuthor: Example <e@example.com>\n\n    message\n\ndiff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old\n+new\n";

    let normalized = normalized_patch_input(patch);
    let (prelude, patch) = split_patch_prelude(&normalized);

    assert_eq!(
        prelude,
        b"commit abc123\nAuthor: Example <e@example.com>\n\n    message\n\n"
    );
    assert!(patch.starts_with(b"diff --git a/a.txt b/a.txt\n"));
    assert_eq!(
        mark_diff::parse_patch(&String::from_utf8_lossy(patch)).len(),
        1
    );
}

#[test]
fn static_diff_output_prepends_git_show_prelude() {
    let input = b"commit abc123\nAuthor: Example <e@example.com>\n\n    message\n\ndiff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old\n+new\n";
    let args = PagerArgs {
        no_syntax: true,
        layout: PagerLayoutArg::Unified,
    };

    let output = static_diff_output(input, &args, false).unwrap();
    let text = String::from_utf8_lossy(&output);

    assert!(text.starts_with("commit abc123\nAuthor: Example <e@example.com>\n"));
    assert!(text.contains("message\n\n"));
    assert!(text.contains("a.txt"));
    assert!(text.contains("-old"));
    assert!(text.contains("+new"));
}

#[test]
fn normalized_patch_input_preserves_diff_after_malformed_string_escape() {
    let patch = b"diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old\n+\x1b]unterminated\ndiff --git a/b.txt b/b.txt\n--- a/b.txt\n+++ b/b.txt\n@@ -1 +1 @@\n-before\n+after\n";

    let normalized = normalized_patch_input(patch);
    let text = String::from_utf8_lossy(&normalized);
    let files = mark_diff::parse_patch(&text);

    assert!(text.contains("diff --git a/b.txt b/b.txt"));
    assert_eq!(files.len(), 2);
    assert_eq!(files[0].hunks[0].lines[1].text, "\u{1b}]unterminated");
    assert_eq!(files[1].new_path.as_deref(), Some("b.txt"));
}
