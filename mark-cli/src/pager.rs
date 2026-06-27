#[cfg(test)]
use std::ffi::OsString;
#[cfg(test)]
use std::io::Write;
use std::{
    env,
    io::{self, IsTerminal},
};

use mark_core::MarkError;

#[cfg(test)]
use crate::args::PagerLayoutArg;
use crate::{CliResult, args::PagerArgs, write_stdout_bytes};

mod env_state;
mod input;
mod patch;
mod plain;
mod static_diff;
mod terminal;

use env_state::PagerEnv;
#[cfg(test)]
use input::pager_action;
use input::{
    PagerAction, PagerInput, StreamingPagerAction, read_pager_input, static_pager_color_enabled,
};
#[cfg(test)]
use patch::{looks_like_patch_input, normalized_patch_input, split_patch_prelude};
#[cfg(test)]
use plain::{DEFAULT_TEXT_PAGER, StreamFallback, resolve_text_pager_command, stream_to_pager};
use plain::{page_plain_text, page_plain_text_stream, stream_to_stdout};
#[cfg(test)]
use static_diff::static_diff_output;
use static_diff::{run_interactive_diff, write_static_diff};
use terminal::controlling_terminal_available;
#[cfg(test)]
use terminal::{sanitized_terminal_bytes, strip_terminal_escapes};

const PAGER_CLASSIFICATION_LIMIT: usize = 128 * 1024;
const STREAM_BUFFER_SIZE: usize = 8192;

pub(crate) fn pager(args: PagerArgs) -> CliResult<()> {
    if io::stdin().is_terminal() {
        return Err(MarkError::Usage(
            "mark pager reads diff text from stdin; use `git diff | mark pager`, configure `git config --global core.pager \"mark pager\"`, or run `mark` for the current worktree"
                .to_owned(),
        )
        .into());
    }

    let env = PagerEnv::current();
    let stdout_tty = io::stdout().is_terminal();
    let static_color =
        static_pager_color_enabled(stdout_tty, &env, env::var_os("NO_COLOR").is_some());
    let has_controlling_terminal = controlling_terminal_available();
    let mut stdin = io::stdin().lock();
    match read_pager_input(&mut stdin, stdout_tty, &env, has_controlling_terminal)? {
        PagerInput::Buffered { input, action } => match action {
            PagerAction::Passthrough => write_stdout_bytes(&input),
            PagerAction::PlainTextPager => page_plain_text(&input),
            PagerAction::StaticDiff => write_static_diff(&input, &args, static_color),
            PagerAction::InteractiveDiff => run_interactive_diff(input, &args, static_color),
        },
        PagerInput::Streaming { prefix, action } => match action {
            StreamingPagerAction::Passthrough => stream_to_stdout(&prefix, &mut stdin),
            StreamingPagerAction::PlainTextPager => page_plain_text_stream(&prefix, &mut stdin),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(
        term: Option<&str>,
        lv: Option<&str>,
        git_pager: Option<&str>,
        lazygit: bool,
    ) -> PagerEnv {
        PagerEnv {
            term: term.map(OsString::from),
            lv: lv.map(OsString::from),
            git_pager: git_pager.map(OsString::from),
            has_lazygit_env: lazygit,
        }
    }

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

    #[test]
    fn pager_streams_plain_text_after_classification_limit() {
        let mut input = std::io::Cursor::new(vec![b'x'; PAGER_CLASSIFICATION_LIMIT + 1]);

        let decision = read_pager_input(
            &mut input,
            true,
            &env(Some("xterm-256color"), None, None, false),
            true,
        )
        .unwrap();

        let PagerInput::Streaming { prefix, action } = decision else {
            panic!("expected streaming input");
        };
        assert_eq!(action, StreamingPagerAction::PlainTextPager);
        assert_eq!(prefix.len(), PAGER_CLASSIFICATION_LIMIT);
        assert_eq!(input.position(), PAGER_CLASSIFICATION_LIMIT as u64);
    }

    #[test]
    fn plain_text_stream_fallback_replays_prefix_and_unread_input() {
        let prefix = b"buffered prefix\n";
        let rest = b"still unread\n".to_vec();
        let mut input = std::io::Cursor::new(rest.clone());
        let mut pager = FailingWriter::new(0);
        let mut fallback = StreamFallback::default();

        let error = stream_to_pager(prefix, &mut input, &mut pager, &mut fallback).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
        assert_eq!(input.position(), 0);

        let mut output = Vec::new();
        fallback
            .write_to_writer(prefix, &mut input, &mut output)
            .unwrap();

        let mut expected = prefix.to_vec();
        expected.extend_from_slice(&rest);
        assert_eq!(output, expected);
    }

    #[test]
    fn plain_text_stream_fallback_replays_spooled_and_unread_input() {
        let prefix = b"buffered prefix\n";
        let rest = vec![b'x'; STREAM_BUFFER_SIZE + 1];
        let mut input = std::io::Cursor::new(rest.clone());
        let mut pager = FailingWriter::new(prefix.len() + 4);
        let mut fallback = StreamFallback::default();

        let error = stream_to_pager(prefix, &mut input, &mut pager, &mut fallback).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
        assert_eq!(input.position(), STREAM_BUFFER_SIZE as u64);

        let mut output = Vec::new();
        fallback
            .write_to_writer(prefix, &mut input, &mut output)
            .unwrap();

        let mut expected = prefix.to_vec();
        expected.extend_from_slice(&rest);
        assert_eq!(output, expected);
    }

    #[test]
    fn plain_text_stream_fallback_replays_fully_spooled_input() {
        let prefix = b"buffered prefix\n";
        let rest = vec![b'x'; STREAM_BUFFER_SIZE + 1];
        let mut input = std::io::Cursor::new(rest.clone());
        let mut pager = Vec::new();
        let mut fallback = StreamFallback::default();

        stream_to_pager(prefix, &mut input, &mut pager, &mut fallback).unwrap();
        assert_eq!(input.position(), rest.len() as u64);

        let mut output = Vec::new();
        fallback
            .write_to_writer(prefix, &mut input, &mut output)
            .unwrap();

        let mut expected = prefix.to_vec();
        expected.extend_from_slice(&rest);
        assert_eq!(output, expected);
    }

    #[test]
    fn pager_buffers_diff_after_detection() {
        let mut input_bytes =
            b"diff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -1 +1 @@\n-a\n+b\n".to_vec();
        input_bytes.extend(vec![b'x'; STREAM_BUFFER_SIZE * 2]);
        let expected_len = input_bytes.len();
        let mut input = std::io::Cursor::new(input_bytes);

        let decision = read_pager_input(
            &mut input,
            true,
            &env(Some("xterm-256color"), None, None, false),
            true,
        )
        .unwrap();

        let PagerInput::Buffered { input, action } = decision else {
            panic!("expected buffered input");
        };
        assert_eq!(action, PagerAction::InteractiveDiff);
        assert_eq!(input.len(), expected_len);
    }

    #[test]
    fn pager_streams_without_classification_when_action_cannot_change() {
        let mut input = std::io::Cursor::new(vec![b'x'; PAGER_CLASSIFICATION_LIMIT + 1]);

        let decision = read_pager_input(
            &mut input,
            false,
            &env(Some("xterm-256color"), None, None, false),
            true,
        )
        .unwrap();

        let PagerInput::Streaming { prefix, action } = decision else {
            panic!("expected streaming input");
        };
        assert_eq!(action, StreamingPagerAction::Passthrough);
        assert!(prefix.is_empty());
        assert_eq!(input.position(), 0);
    }

    #[test]
    fn plain_text_pager_replaces_self_referential_mark_pager() {
        assert_eq!(
            resolve_text_pager_command(Some("mark pager")),
            DEFAULT_TEXT_PAGER
        );
        assert_eq!(
            resolve_text_pager_command(Some("mark page")),
            DEFAULT_TEXT_PAGER
        );
        assert_eq!(
            resolve_text_pager_command(Some("/usr/local/bin/mark page --layout unified")),
            DEFAULT_TEXT_PAGER
        );
        assert_eq!(
            resolve_text_pager_command(Some("/usr/local/bin/mark pager --layout unified")),
            DEFAULT_TEXT_PAGER
        );
        assert_eq!(
            resolve_text_pager_command(Some("env TERM=xterm-256color mark pager")),
            DEFAULT_TEXT_PAGER
        );
        assert_eq!(
            resolve_text_pager_command(Some("command mark pager")),
            DEFAULT_TEXT_PAGER
        );
        assert_eq!(
            resolve_text_pager_command(Some("PAGER=cat exec mark pager")),
            DEFAULT_TEXT_PAGER
        );
    }

    #[test]
    fn plain_text_pager_preserves_non_self_pager_commands() {
        assert_eq!(resolve_text_pager_command(None), DEFAULT_TEXT_PAGER);
        assert_eq!(resolve_text_pager_command(Some("")), DEFAULT_TEXT_PAGER);
        assert_eq!(resolve_text_pager_command(Some("less -FRX")), "less -FRX");
        assert_eq!(
            resolve_text_pager_command(Some("delta --paging=always")),
            "delta --paging=always"
        );
        assert_eq!(resolve_text_pager_command(Some("mark diff")), "mark diff");
    }

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

    #[test]
    fn sanitized_terminal_bytes_escapes_malformed_escapes() {
        let sanitized = sanitized_terminal_bytes(b"a\x1b]unterminated\nb\x1b[31\nc");

        assert_eq!(sanitized, b"a\\u{1b}]unterminated\nb\\u{1b}[31\nc");
    }

    #[test]
    fn strip_terminal_escapes_removes_csi_and_osc_but_preserves_cr() {
        let stripped = strip_terminal_escapes(b"a\r\n\x1b[31mb\x1b[0mc\x1b]52;c;secret\x07d");

        assert_eq!(stripped, b"a\r\nbcd");
    }

    #[test]
    fn sanitized_terminal_bytes_escapes_controls_after_stripping_sequences() {
        let sanitized = sanitized_terminal_bytes(b"a\r\x07\x1b[31mb\x1b[0m\n");

        assert_eq!(sanitized, b"a\\r\\u{7}b\n");
    }

    struct FailingWriter {
        bytes_until_error: usize,
    }

    impl FailingWriter {
        fn new(bytes_until_error: usize) -> Self {
            Self { bytes_until_error }
        }
    }

    impl Write for FailingWriter {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            if self.bytes_until_error == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "pager stdin closed",
                ));
            }

            let bytes_written = self.bytes_until_error.min(buffer.len());
            self.bytes_until_error -= bytes_written;
            Ok(bytes_written)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
}
