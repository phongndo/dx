use std::{
    borrow::Cow,
    env,
    ffi::OsString,
    fs::OpenOptions,
    io::{self, IsTerminal, Read, Write},
    process::{Command, Stdio},
    sync::Arc,
};

#[cfg(unix)]
use std::os::fd::OwnedFd;

use dx_core::DxError;

use crate::{
    CliResult,
    args::{PagerArgs, PagerLayoutArg},
    write_stderr, write_stdout_bytes,
};

const DEFAULT_TEXT_PAGER: &str = "less -R";
const DEFAULT_STATIC_WIDTH: usize = 120;
const MIN_STATIC_WIDTH: usize = 20;
const PLAIN_TEXT_PAGER_GUARD: &str = "DX_PAGER_PLAIN_TEXT_FALLBACK";

pub(crate) fn pager(args: PagerArgs) -> CliResult<()> {
    if io::stdin().is_terminal() {
        return Err(DxError::Usage(
            "dx pager reads diff text from stdin; use `git diff | dx pager`, configure `git config --global core.pager \"dx pager\"`, or run `dx` for the current worktree"
                .to_owned(),
        )
        .into());
    }

    let mut input = Vec::new();
    io::stdin().read_to_end(&mut input)?;

    let env = PagerEnv::current();
    let stdout_tty = io::stdout().is_terminal();
    match pager_action(&input, stdout_tty, &env, controlling_terminal_available()) {
        PagerAction::Passthrough => write_stdout_bytes(&input),
        PagerAction::PlainTextPager => page_plain_text(&input),
        PagerAction::StaticDiff => write_static_diff(&input, &args),
        PagerAction::InteractiveDiff => run_interactive_diff(input, &args),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PagerAction {
    Passthrough,
    PlainTextPager,
    StaticDiff,
    InteractiveDiff,
}

fn pager_action(
    input: &[u8],
    stdout_tty: bool,
    env: &PagerEnv,
    has_controlling_terminal: bool,
) -> PagerAction {
    if !looks_like_patch_input(input) {
        return if env.term_is_dumb() || !stdout_tty {
            PagerAction::Passthrough
        } else {
            PagerAction::PlainTextPager
        };
    }

    if env.is_captured_pager_host() {
        return PagerAction::StaticDiff;
    }

    if !stdout_tty || env.term_is_dumb() {
        return PagerAction::Passthrough;
    }

    if !has_controlling_terminal {
        return PagerAction::StaticDiff;
    }

    PagerAction::InteractiveDiff
}

fn write_static_diff(input: &[u8], args: &PagerArgs) -> CliResult<()> {
    let patch = normalized_patch_input(input);
    let options = patch_options(patch);
    let color = io::stdout().is_terminal() && env::var_os("NO_COLOR").is_none();
    let rendered = match dx_tui::render_static_pager(
        options,
        dx_tui::StaticPagerOptions {
            width: static_terminal_width(),
            layout: args.layout.into(),
            color,
            syntax: !args.no_syntax,
            ..dx_tui::StaticPagerOptions::default()
        },
    ) {
        Ok(rendered) => rendered,
        Err(error) => {
            write_stderr(format_args!(
                "dx: static pager render failed; falling back to raw diff: {error}\n"
            ))?;
            String::new()
        }
    };
    if rendered.is_empty() {
        let fallback = sanitized_terminal_bytes(input);
        write_stdout_bytes(&fallback)
    } else {
        write_stdout_bytes(rendered.as_bytes())
    }
}

fn run_interactive_diff(input: Vec<u8>, args: &PagerArgs) -> CliResult<()> {
    let _stdin_override = match attach_controlling_terminal_to_stdin() {
        Ok(guard) => guard,
        Err(_) => return write_static_diff(&input, args),
    };
    dx_tui::run_diff_with_live_updates_and_syntax(
        patch_options(normalized_patch_input(&input)),
        false,
        !args.no_syntax,
    )?;
    Ok(())
}

impl From<PagerLayoutArg> for dx_tui::StaticPagerLayout {
    fn from(layout: PagerLayoutArg) -> Self {
        match layout {
            PagerLayoutArg::Auto => Self::Auto,
            PagerLayoutArg::Split => Self::Split,
            PagerLayoutArg::Unified => Self::Unified,
        }
    }
}

fn static_terminal_width() -> usize {
    crossterm::terminal::size()
        .ok()
        .map(|(columns, _)| usize::from(columns))
        .filter(|columns| *columns > 0)
        .unwrap_or(DEFAULT_STATIC_WIDTH)
        .max(MIN_STATIC_WIDTH)
}

fn patch_options(patch: Vec<u8>) -> dx_command::DiffOptions {
    dx_command::DiffOptions {
        repo: None,
        source: dx_command::DiffSource::Patch(dx_command::PatchSource::Stdin(Arc::from(
            patch.into_boxed_slice(),
        ))),
        scope: dx_command::DiffScope::All,
        include_untracked: false,
        stat: false,
    }
}

fn page_plain_text(input: &[u8]) -> CliResult<()> {
    if env::var_os(PLAIN_TEXT_PAGER_GUARD).is_some() {
        return write_stdout_bytes(input);
    }

    let configured_pager = env::var("PAGER").ok();
    let pager_command = resolve_text_pager_command(configured_pager.as_deref());
    match spawn_shell_command(&pager_command) {
        Ok(mut child) => {
            if let Some(mut stdin) = child.stdin.take()
                && let Err(error) = stdin.write_all(input)
                && error.kind() != io::ErrorKind::BrokenPipe
            {
                return Err(error.into());
            }

            let status = child.wait()?;
            if !status.success() {
                write_stderr(format_args!(
                    "dx: pager command exited with {status}: {pager_command}\n"
                ))?;
                write_stdout_bytes(input)?;
            }
            Ok(())
        }
        Err(error) => {
            write_stderr(format_args!(
                "dx: failed to run pager command `{pager_command}`: {error}\n"
            ))?;
            write_stdout_bytes(input)
        }
    }
}

fn resolve_text_pager_command(configured_pager: Option<&str>) -> Cow<'_, str> {
    let pager_command = configured_pager
        .filter(|command| !command.trim().is_empty())
        .unwrap_or(DEFAULT_TEXT_PAGER);

    if command_invokes_dx_pager(pager_command) {
        Cow::Borrowed(DEFAULT_TEXT_PAGER)
    } else {
        Cow::Borrowed(pager_command)
    }
}

fn command_invokes_dx_pager(command: &str) -> bool {
    let Some(words) = shlex::split(command) else {
        return false;
    };
    let Some(command_index) = first_shell_command_word(&words) else {
        return false;
    };

    executable_is_dx(&words[command_index])
        && words
            .get(command_index + 1)
            .is_some_and(|argument| argument == "pager")
}

fn first_shell_command_word(words: &[String]) -> Option<usize> {
    let mut index = 0;
    while index < words.len() {
        match words[index].as_str() {
            "command" | "exec" => index += 1,
            "env" => {
                index += 1;
                while index < words.len() {
                    match words[index].as_str() {
                        "--" => {
                            index += 1;
                            break;
                        }
                        "-u" | "--unset" => index += 2,
                        "-i" | "--ignore-environment" => index += 1,
                        option if option.starts_with("--unset=") => index += 1,
                        option if option.starts_with('-') => index += 1,
                        assignment if is_env_assignment(assignment) => index += 1,
                        _ => break,
                    }
                }
            }
            assignment if is_env_assignment(assignment) => index += 1,
            _ => return Some(index),
        }
    }
    None
}

fn is_env_assignment(word: &str) -> bool {
    let Some((name, _)) = word.split_once('=') else {
        return false;
    };
    let mut characters = name.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

fn executable_is_dx(program: &str) -> bool {
    let name = program.rsplit(['/', '\\']).next().unwrap_or(program);
    let stem = name
        .strip_suffix(".exe")
        .or_else(|| name.strip_suffix(".EXE"))
        .unwrap_or(name);
    stem.eq_ignore_ascii_case("dx")
}

#[cfg(unix)]
fn spawn_shell_command(command: &str) -> io::Result<std::process::Child> {
    let shell = env::var_os("SHELL").unwrap_or_else(|| OsString::from("/bin/sh"));
    Command::new(shell)
        .arg("-c")
        .arg(command)
        .env(PLAIN_TEXT_PAGER_GUARD, "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
}

#[cfg(windows)]
fn spawn_shell_command(command: &str) -> io::Result<std::process::Child> {
    Command::new("cmd")
        .arg("/C")
        .arg(command)
        .env(PLAIN_TEXT_PAGER_GUARD, "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
}

fn looks_like_patch_input(input: &[u8]) -> bool {
    let normalized = normalized_patch_input(input);
    let text = String::from_utf8_lossy(&normalized);
    parseable_patch_has_renderable_change(&text)
}

fn parseable_patch_has_renderable_change(patch: &str) -> bool {
    let has_git_header = patch_header_lines(patch).any(|line| line.starts_with("diff --git "));
    dx_diff::parse_patch(patch)
        .iter()
        .any(|file| diff_file_has_renderable_change(file, has_git_header))
}

fn diff_file_has_renderable_change(file: &dx_diff::DiffFile, input_has_git_header: bool) -> bool {
    !file.hunks.is_empty()
        || file.is_binary
        || (input_has_git_header
            && matches!(
                file.status,
                dx_diff::FileStatus::Added
                    | dx_diff::FileStatus::Deleted
                    | dx_diff::FileStatus::Renamed
                    | dx_diff::FileStatus::Copied
                    | dx_diff::FileStatus::TypeChanged
            ))
}

fn patch_header_lines(patch: &str) -> impl Iterator<Item = &str> {
    patch
        .split_inclusive('\n')
        .map(|line| line.strip_suffix('\n').unwrap_or(line))
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
}

fn normalized_patch_input(input: &[u8]) -> Vec<u8> {
    strip_terminal_escapes(input)
}

fn sanitized_terminal_bytes(input: &[u8]) -> Vec<u8> {
    let stripped = strip_terminal_escapes(input);
    let text = String::from_utf8_lossy(&stripped);
    let mut output = String::with_capacity(text.len());
    for character in text.chars() {
        if character.is_control() && !matches!(character, '\n' | '\t') {
            output.extend(character.escape_default());
        } else {
            output.push(character);
        }
    }
    output.into_bytes()
}

fn strip_terminal_escapes(input: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len());
    let mut index = 0;
    while index < input.len() {
        match input[index] {
            0x1b => skip_escape(input, &mut index),
            byte => {
                output.push(byte);
                index += 1;
            }
        }
    }
    output
}

fn skip_escape(input: &[u8], index: &mut usize) {
    *index += 1;
    let Some(introducer) = input.get(*index).copied() else {
        return;
    };
    *index += 1;

    match introducer {
        b'[' => skip_csi(input, index),
        b']' | b'P' | b'^' | b'_' | b'X' => skip_string_escape(input, index),
        0x20..=0x2f if *index < input.len() => *index += 1,
        _ => {}
    }
}

fn skip_csi(input: &[u8], index: &mut usize) {
    while let Some(byte) = input.get(*index).copied() {
        *index += 1;
        if (0x40..=0x7e).contains(&byte) {
            break;
        }
    }
}

fn skip_string_escape(input: &[u8], index: &mut usize) {
    while let Some(byte) = input.get(*index).copied() {
        *index += 1;
        if byte == 0x07 {
            break;
        }
        if byte == 0x1b && input.get(*index) == Some(&b'\\') {
            *index += 1;
            break;
        }
    }
}

#[cfg(unix)]
fn controlling_terminal_available() -> bool {
    OpenOptions::new().read(true).open("/dev/tty").is_ok()
}

#[cfg(not(unix))]
fn controlling_terminal_available() -> bool {
    false
}

#[cfg(unix)]
fn attach_controlling_terminal_to_stdin() -> io::Result<Option<StdinOverride>> {
    if io::stdin().is_terminal() {
        return Ok(None);
    }

    let stdin = io::stdin();
    let original = rustix::io::dup(&stdin).map_err(io::Error::from)?;
    let tty = OpenOptions::new().read(true).write(true).open("/dev/tty")?;
    rustix::stdio::dup2_stdin(&tty).map_err(io::Error::from)?;
    Ok(Some(StdinOverride { original }))
}

#[cfg(not(unix))]
fn attach_controlling_terminal_to_stdin() -> io::Result<Option<StdinOverride>> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "attaching redirected pager stdin to the controlling terminal is unsupported on this platform",
    ))
}

#[cfg(unix)]
struct StdinOverride {
    original: OwnedFd,
}

#[cfg(unix)]
impl Drop for StdinOverride {
    fn drop(&mut self) {
        let _ = rustix::stdio::dup2_stdin(&self.original);
    }
}

#[cfg(not(unix))]
struct StdinOverride;

#[derive(Debug, Clone, PartialEq, Eq)]
struct PagerEnv {
    term: Option<OsString>,
    lv: Option<OsString>,
    git_pager: Option<OsString>,
    has_lazygit_env: bool,
}

impl PagerEnv {
    fn current() -> Self {
        Self {
            term: env::var_os("TERM"),
            lv: env::var_os("LV"),
            git_pager: env::var_os("GIT_PAGER"),
            has_lazygit_env: env::vars_os()
                .any(|(key, _)| key.to_string_lossy().starts_with("LAZYGIT")),
        }
    }

    fn term_is_dumb(&self) -> bool {
        self.term.as_deref() == Some(std::ffi::OsStr::new("dumb"))
    }

    fn is_captured_pager_host(&self) -> bool {
        self.term_is_dumb()
            && (self.lv.as_deref() == Some(std::ffi::OsStr::new("-c"))
                || self.git_pager.is_some()
                || self.has_lazygit_env)
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
                &env(Some("dumb"), None, Some("dx pager"), false),
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
    fn plain_text_pager_replaces_self_referential_dx_pager() {
        assert_eq!(
            resolve_text_pager_command(Some("dx pager")),
            DEFAULT_TEXT_PAGER
        );
        assert_eq!(
            resolve_text_pager_command(Some("/usr/local/bin/dx pager --layout unified")),
            DEFAULT_TEXT_PAGER
        );
        assert_eq!(
            resolve_text_pager_command(Some("env TERM=xterm-256color dx pager")),
            DEFAULT_TEXT_PAGER
        );
        assert_eq!(
            resolve_text_pager_command(Some("command dx pager")),
            DEFAULT_TEXT_PAGER
        );
        assert_eq!(
            resolve_text_pager_command(Some("PAGER=cat exec dx pager")),
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
        assert_eq!(resolve_text_pager_command(Some("dx diff")), "dx diff");
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
        let files = dx_diff::parse_patch(&text);

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].hunks[0].lines[0].text, "old\r");
        assert_eq!(files[0].hunks[0].lines[1].text, "old");
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
}
