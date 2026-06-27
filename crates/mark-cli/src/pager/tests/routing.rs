use super::*;

#[test]
fn pager_routes_regular_diff_tty_to_interactive() {
    let action = pager_action(
        b"diff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -1 +1 @@\n-a\n+b\n",
        true,
        &env(Some("xterm-256color"), None, None, false),
        true,
    );

    assert_eq!(action, PagerAction::InteractiveDiff);
}

#[test]
fn pager_routes_captured_hosts_to_static_diff() {
    let input = b"diff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -1 +1 @@\n-a\n+b\n";

    assert_eq!(
        pager_action(input, true, &env(Some("dumb"), None, None, true), true),
        PagerAction::StaticDiff
    );
    assert_eq!(
        pager_action(
            input,
            true,
            &env(Some("dumb"), Some("-c"), None, false),
            true
        ),
        PagerAction::StaticDiff
    );
    assert_eq!(
        pager_action(
            input,
            true,
            &env(Some("dumb"), None, Some("mark pager"), false),
            true,
        ),
        PagerAction::StaticDiff
    );
    assert_eq!(
        pager_action(input, false, &env(Some("dumb"), None, None, true), true),
        PagerAction::StaticDiff
    );
}

#[test]
fn static_pager_colors_captured_hosts_without_stdout_tty() {
    assert!(static_pager_color_enabled(
        false,
        &env(Some("dumb"), None, None, true),
        false
    ));
    assert!(static_pager_color_enabled(
        false,
        &env(Some("dumb"), None, Some("mark pager"), false),
        false
    ));
    assert!(!static_pager_color_enabled(
        false,
        &env(Some("dumb"), None, None, true),
        true
    ));
    assert!(!static_pager_color_enabled(
        false,
        &env(Some("xterm-256color"), None, None, false),
        false
    ));
}

#[test]
fn pager_passthroughs_diff_when_stdout_is_not_tty() {
    let action = pager_action(
        b"diff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -1 +1 @@\n-a\n+b\n",
        false,
        &env(Some("xterm-256color"), None, None, false),
        true,
    );

    assert_eq!(action, PagerAction::Passthrough);
}

#[test]
fn pager_falls_back_to_static_diff_without_controlling_terminal() {
    let action = pager_action(
        b"diff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -1 +1 @@\n-a\n+b\n",
        true,
        &env(Some("xterm-256color"), None, None, false),
        false,
    );

    assert_eq!(action, PagerAction::StaticDiff);
}

#[test]
fn pager_routes_git_show_prelude_to_static_diff() {
    let action = pager_action(
            b"commit abc123\nAuthor: Example <e@example.com>\n\n    message\n\ndiff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -1 +1 @@\n-a\n+b\n",
            true,
            &env(Some("xterm-256color"), None, None, false),
            true,
        );

    assert_eq!(action, PagerAction::StaticDiff);
}

#[test]
fn pager_passthroughs_dumb_non_captured_terminal() {
    let action = pager_action(
        b"diff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -1 +1 @@\n-a\n+b\n",
        true,
        &env(Some("dumb"), None, None, false),
        true,
    );

    assert_eq!(action, PagerAction::Passthrough);
}

#[test]
fn pager_pages_plain_text_on_regular_tty() {
    let action = pager_action(
        b"commit abc123\n",
        true,
        &env(Some("xterm-256color"), None, None, false),
        true,
    );

    assert_eq!(action, PagerAction::PlainTextPager);
}

#[test]
fn pager_passthroughs_empty_input() {
    let action = pager_action(
        b"",
        true,
        &env(Some("xterm-256color"), None, None, false),
        true,
    );

    assert_eq!(action, PagerAction::Passthrough);
}
