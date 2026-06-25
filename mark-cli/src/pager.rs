use std::{
    borrow::Cow,
    env,
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{self, IsTerminal, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::{self, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

#[cfg(unix)]
use std::os::fd::OwnedFd;

use mark_core::MarkError;

use crate::{
    CliResult,
    args::{PagerArgs, PagerLayoutArg},
    write_stderr, write_stdout_bytes,
};

const DEFAULT_TEXT_PAGER: &str = "less -R";
const DEFAULT_STATIC_WIDTH: usize = 120;
const MIN_STATIC_WIDTH: usize = 20;
const PLAIN_TEXT_PAGER_GUARD: &str = "MARK_PAGER_PLAIN_TEXT_FALLBACK";
const PAGER_CLASSIFICATION_LIMIT: usize = 128 * 1024;
const STREAM_BUFFER_SIZE: usize = 8192;
const TEMP_SPOOL_CREATE_ATTEMPTS: usize = 16;

static NEXT_TEMP_SPOOL: AtomicU64 = AtomicU64::new(0);

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

#[derive(Debug, PartialEq, Eq)]
enum PagerInput {
    Buffered {
        input: Vec<u8>,
        action: PagerAction,
    },
    Streaming {
        prefix: Vec<u8>,
        action: StreamingPagerAction,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PagerAction {
    Passthrough,
    PlainTextPager,
    StaticDiff,
    InteractiveDiff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StreamingPagerAction {
    Passthrough,
    PlainTextPager,
}

fn read_pager_input<R: Read>(
    reader: &mut R,
    stdout_tty: bool,
    env: &PagerEnv,
    has_controlling_terminal: bool,
) -> io::Result<PagerInput> {
    if let Some(action) = input_independent_streaming_action(stdout_tty, env) {
        return Ok(PagerInput::Streaming {
            prefix: Vec::new(),
            action,
        });
    }

    let mut input = Vec::new();
    let mut buffer = [0; STREAM_BUFFER_SIZE];
    loop {
        if looks_like_patch_input(&input) {
            reader.read_to_end(&mut input)?;
            return Ok(PagerInput::Buffered {
                action: pager_action(&input, stdout_tty, env, has_controlling_terminal),
                input,
            });
        }

        if input.len() >= PAGER_CLASSIFICATION_LIMIT {
            // Git does not tell core.pager which command produced stdin. Once a
            // bounded prefix has no parseable diff, switch to streaming so
            // large non-diff commands like `git log` can be quit early.
            return Ok(PagerInput::Streaming {
                prefix: input,
                action: non_diff_streaming_action(stdout_tty, env),
            });
        }

        let read_limit = (PAGER_CLASSIFICATION_LIMIT - input.len()).min(buffer.len());
        let bytes_read = reader.read(&mut buffer[..read_limit])?;
        if bytes_read == 0 {
            return Ok(PagerInput::Buffered {
                action: pager_action(&input, stdout_tty, env, has_controlling_terminal),
                input,
            });
        }
        input.extend_from_slice(&buffer[..bytes_read]);
    }
}

fn input_independent_streaming_action(
    stdout_tty: bool,
    env: &PagerEnv,
) -> Option<StreamingPagerAction> {
    if !env.is_captured_pager_host() && (!stdout_tty || env.term_is_dumb()) {
        Some(StreamingPagerAction::Passthrough)
    } else {
        None
    }
}

fn pager_action(
    input: &[u8],
    stdout_tty: bool,
    env: &PagerEnv,
    has_controlling_terminal: bool,
) -> PagerAction {
    if input.is_empty() {
        return PagerAction::Passthrough;
    }

    if !looks_like_patch_input(input) {
        return non_diff_pager_action(stdout_tty, env);
    }

    if env.is_captured_pager_host() {
        return PagerAction::StaticDiff;
    }

    if !stdout_tty || env.term_is_dumb() {
        return PagerAction::Passthrough;
    }

    if patch_input_has_prelude(input) {
        return PagerAction::StaticDiff;
    }

    if !has_controlling_terminal {
        return PagerAction::StaticDiff;
    }

    PagerAction::InteractiveDiff
}

fn non_diff_pager_action(stdout_tty: bool, env: &PagerEnv) -> PagerAction {
    match non_diff_streaming_action(stdout_tty, env) {
        StreamingPagerAction::Passthrough => PagerAction::Passthrough,
        StreamingPagerAction::PlainTextPager => PagerAction::PlainTextPager,
    }
}

fn non_diff_streaming_action(stdout_tty: bool, env: &PagerEnv) -> StreamingPagerAction {
    if env.term_is_dumb() || !stdout_tty {
        StreamingPagerAction::Passthrough
    } else {
        StreamingPagerAction::PlainTextPager
    }
}

fn static_pager_color_enabled(stdout_tty: bool, env: &PagerEnv, no_color: bool) -> bool {
    !no_color && (stdout_tty || env.is_captured_pager_host())
}

fn write_static_diff(input: &[u8], args: &PagerArgs, color: bool) -> CliResult<()> {
    let output = static_diff_output(input, args, color)?;
    write_stdout_bytes(&output)
}

fn static_diff_output(input: &[u8], args: &PagerArgs, color: bool) -> CliResult<Vec<u8>> {
    let patch = normalized_patch_input(input);
    let (prelude, patch) = split_patch_prelude(&patch);
    let options = patch_options(patch.to_vec());
    let rendered = match mark_tui::render_static_pager(
        options,
        mark_tui::StaticPagerOptions {
            width: static_terminal_width(),
            layout: args.layout.into(),
            color,
            syntax: !args.no_syntax,
            ..mark_tui::StaticPagerOptions::default()
        },
    ) {
        Ok(rendered) => rendered,
        Err(error) => {
            write_stderr(format_args!(
                "mark: static pager render failed; falling back to raw diff: {error}\n"
            ))?;
            String::new()
        }
    };
    if rendered.is_empty() {
        let fallback = sanitized_terminal_bytes(input);
        Ok(fallback)
    } else {
        let mut output = sanitized_terminal_bytes(prelude);
        output.extend_from_slice(rendered.as_bytes());
        Ok(output)
    }
}

fn run_interactive_diff(input: Vec<u8>, args: &PagerArgs, static_color: bool) -> CliResult<()> {
    let _stdin_override = match attach_controlling_terminal_to_stdin() {
        Ok(guard) => guard,
        Err(_) => return write_static_diff(&input, args, static_color),
    };
    mark_tui::run_diff_with_live_updates_and_syntax(
        patch_options(normalized_patch_input(&input)),
        false,
        !args.no_syntax,
    )?;
    Ok(())
}

impl From<PagerLayoutArg> for mark_tui::StaticPagerLayout {
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

fn patch_options(patch: Vec<u8>) -> mark_command::DiffOptions {
    mark_command::DiffOptions {
        repo: None,
        source: mark_command::DiffSource::Patch(mark_command::PatchSource::Stdin(Arc::from(
            patch.into_boxed_slice(),
        ))),
        scope: mark_command::DiffScope::All,
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
                    "mark: pager command exited with {status}: {pager_command}\n"
                ))?;
                write_stdout_bytes(input)?;
            }
            Ok(())
        }
        Err(error) => {
            write_stderr(format_args!(
                "mark: failed to run pager command `{pager_command}`: {error}\n"
            ))?;
            write_stdout_bytes(input)
        }
    }
}

fn page_plain_text_stream<R: Read>(prefix: &[u8], input: &mut R) -> CliResult<()> {
    if env::var_os(PLAIN_TEXT_PAGER_GUARD).is_some() {
        return stream_to_stdout(prefix, input);
    }

    let configured_pager = env::var("PAGER").ok();
    let pager_command = resolve_text_pager_command(configured_pager.as_deref());
    match spawn_shell_command(&pager_command) {
        Ok(mut child) => {
            let mut fallback = StreamFallback::default();
            let write_result = if let Some(mut stdin) = child.stdin.take() {
                let result = stream_to_pager(prefix, input, &mut stdin, &mut fallback);
                drop(stdin);
                result
            } else {
                Ok(())
            };

            let status = child.wait()?;
            if let Err(error) = write_result
                && error.kind() != io::ErrorKind::BrokenPipe
            {
                return Err(error.into());
            }
            if !status.success() {
                write_stderr(format_args!(
                    "mark: pager command exited with {status}: {pager_command}\n"
                ))?;
                fallback.write_to_stdout(prefix, input)?;
            }
            Ok(())
        }
        Err(error) => {
            write_stderr(format_args!(
                "mark: failed to run pager command `{pager_command}`: {error}\n"
            ))?;
            stream_to_stdout(prefix, input)
        }
    }
}

fn stream_to_pager<R: Read, W: Write>(
    prefix: &[u8],
    input: &mut R,
    output: &mut W,
    fallback: &mut StreamFallback,
) -> io::Result<()> {
    output.write_all(prefix)?;
    let mut buffer = [0; STREAM_BUFFER_SIZE];
    loop {
        let bytes_read = input.read(&mut buffer)?;
        if bytes_read == 0 {
            return Ok(());
        }

        let chunk = &buffer[..bytes_read];
        fallback.record(chunk)?;
        output.write_all(chunk)?;
    }
}

#[derive(Default)]
struct StreamFallback {
    spool: Option<TempSpool>,
}

impl StreamFallback {
    fn record(&mut self, bytes: &[u8]) -> io::Result<()> {
        if bytes.is_empty() {
            return Ok(());
        }

        if self.spool.is_none() {
            self.spool = Some(TempSpool::new()?);
        }
        self.spool
            .as_mut()
            .expect("spool should exist")
            .write_all(bytes)
    }

    fn write_to_stdout<R: Read>(&mut self, prefix: &[u8], input: &mut R) -> CliResult<()> {
        write_stdout_bytes(prefix)?;
        if let Some(spool) = self.spool.as_mut() {
            spool.write_to_stdout()?;
        }
        stream_to_stdout(&[], input)
    }

    #[cfg(test)]
    fn write_to_writer<R: Read, W: Write>(
        &mut self,
        prefix: &[u8],
        input: &mut R,
        output: &mut W,
    ) -> io::Result<()> {
        output.write_all(prefix)?;
        if let Some(spool) = self.spool.as_mut() {
            spool.write_to_writer(output)?;
        }
        io::copy(input, output)?;
        Ok(())
    }
}

struct TempSpool {
    path: PathBuf,
    file: File,
}

impl TempSpool {
    fn new() -> io::Result<Self> {
        for _ in 0..TEMP_SPOOL_CREATE_ATTEMPTS {
            let path = temp_spool_path();
            match create_private_temp_file(&path) {
                Ok(file) => return Ok(Self { path, file }),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }

        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "failed to create pager fallback spool",
        ))
    }

    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.file.write_all(bytes)
    }

    fn write_to_stdout(&mut self) -> CliResult<()> {
        self.file.flush()?;
        self.file.seek(SeekFrom::Start(0))?;
        let mut buffer = [0; STREAM_BUFFER_SIZE];
        loop {
            let bytes_read = self.file.read(&mut buffer)?;
            if bytes_read == 0 {
                return Ok(());
            }
            write_stdout_bytes(&buffer[..bytes_read])?;
        }
    }

    #[cfg(test)]
    fn write_to_writer<W: Write>(&mut self, output: &mut W) -> io::Result<()> {
        self.file.flush()?;
        self.file.seek(SeekFrom::Start(0))?;
        io::copy(&mut self.file, output)?;
        Ok(())
    }
}

impl Drop for TempSpool {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn temp_spool_path() -> PathBuf {
    let counter = NEXT_TEMP_SPOOL.fetch_add(1, Ordering::Relaxed);
    env::temp_dir().join(format!("mark-pager-spool-{}-{counter}.tmp", process::id()))
}

fn create_private_temp_file(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

fn stream_to_stdout<R: Read>(prefix: &[u8], input: &mut R) -> CliResult<()> {
    write_stdout_bytes(prefix)?;
    let mut buffer = [0; STREAM_BUFFER_SIZE];
    loop {
        let bytes_read = input.read(&mut buffer)?;
        if bytes_read == 0 {
            return Ok(());
        }
        write_stdout_bytes(&buffer[..bytes_read])?;
    }
}

fn resolve_text_pager_command(configured_pager: Option<&str>) -> Cow<'_, str> {
    let pager_command = configured_pager
        .filter(|command| !command.trim().is_empty())
        .unwrap_or(DEFAULT_TEXT_PAGER);

    if command_invokes_mark_pager(pager_command) {
        Cow::Borrowed(DEFAULT_TEXT_PAGER)
    } else {
        Cow::Borrowed(pager_command)
    }
}

fn command_invokes_mark_pager(command: &str) -> bool {
    let Some(words) = shlex::split(command) else {
        return false;
    };
    let Some(command_index) = first_shell_command_word(&words) else {
        return false;
    };

    executable_is_mark(&words[command_index])
        && words
            .get(command_index + 1)
            .is_some_and(|argument| matches!(argument.as_str(), "pager" | "page"))
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

fn executable_is_mark(program: &str) -> bool {
    let name = program.rsplit(['/', '\\']).next().unwrap_or(program);
    let stem = name
        .strip_suffix(".exe")
        .or_else(|| name.strip_suffix(".EXE"))
        .unwrap_or(name);
    stem.eq_ignore_ascii_case("mark")
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

fn patch_input_has_prelude(input: &[u8]) -> bool {
    let normalized = normalized_patch_input(input);
    !split_patch_prelude(&normalized).0.is_empty()
}

fn parseable_patch_has_renderable_change(patch: &str) -> bool {
    let has_git_header = patch_header_lines(patch).any(|line| line.starts_with("diff --git "));
    mark_diff::parse_patch(patch)
        .iter()
        .any(|file| diff_file_has_renderable_change(file, has_git_header))
}

fn diff_file_has_renderable_change(file: &mark_diff::DiffFile, input_has_git_header: bool) -> bool {
    !file.hunks.is_empty()
        || file.is_binary
        || (input_has_git_header
            && matches!(
                file.status,
                mark_diff::FileStatus::Added
                    | mark_diff::FileStatus::Deleted
                    | mark_diff::FileStatus::Renamed
                    | mark_diff::FileStatus::Copied
                    | mark_diff::FileStatus::TypeChanged
            ))
}

fn patch_header_lines(patch: &str) -> impl Iterator<Item = &str> {
    patch
        .split_inclusive('\n')
        .map(|line| line.strip_suffix('\n').unwrap_or(line))
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
}

fn normalized_patch_input(input: &[u8]) -> Vec<u8> {
    let lines = input
        .split_inclusive(|byte| *byte == b'\n')
        .collect::<Vec<_>>();
    let strip_uncolored_context_resets = strip_uncolored_context_reset_lines(&lines);
    let mut output = Vec::with_capacity(input.len());
    for (line, strip_uncolored_context_reset) in
        lines.into_iter().zip(strip_uncolored_context_resets)
    {
        append_patch_line_without_color_wrappers(line, &mut output, strip_uncolored_context_reset);
    }
    output
}

fn strip_uncolored_context_reset_lines(lines: &[&[u8]]) -> Vec<bool> {
    // Git emits normal-color context lines as ` context\x1b[m` when the hunk
    // body is colored. Track that per hunk; colored headers alone do not prove
    // trailing resets on hunk payload lines are Git color wrappers.
    let mut strip_reset = vec![false; lines.len()];
    let mut hunk_start = None;
    let mut hunk_has_colored_body = false;

    for (index, line) in lines.iter().enumerate() {
        if hunk_header_line(line) {
            finish_hunk_color_scan(&mut strip_reset, hunk_start, index, hunk_has_colored_body);
            hunk_start = Some(index + 1);
            hunk_has_colored_body = false;
            continue;
        }

        if patch_structural_line(line) {
            finish_hunk_color_scan(&mut strip_reset, hunk_start, index, hunk_has_colored_body);
            hunk_start = None;
            hunk_has_colored_body = false;
            continue;
        }

        if hunk_start.is_some() && hunk_body_line_has_git_color(line) {
            hunk_has_colored_body = true;
        }
    }

    finish_hunk_color_scan(
        &mut strip_reset,
        hunk_start,
        lines.len(),
        hunk_has_colored_body,
    );
    strip_reset
}

fn finish_hunk_color_scan(
    strip_reset: &mut [bool],
    hunk_start: Option<usize>,
    hunk_end: usize,
    hunk_has_colored_body: bool,
) {
    if !hunk_has_colored_body {
        return;
    }

    let Some(hunk_start) = hunk_start else {
        return;
    };

    for strip_reset in &mut strip_reset[hunk_start..hunk_end] {
        *strip_reset = true;
    }
}

fn hunk_header_line(line: &[u8]) -> bool {
    let line = patch_line_without_leading_sgr(line);
    line.starts_with(b"@@ ") || {
        let mut stripped = Vec::with_capacity(line.len());
        append_without_sgr_escapes(line, &mut stripped);
        stripped.starts_with(b"@@ ")
    }
}

fn patch_structural_line(line: &[u8]) -> bool {
    diff_structural_line(patch_line_without_leading_sgr(line))
}

fn patch_line_without_leading_sgr(line: &[u8]) -> &[u8] {
    let line_break_start = line.strip_suffix(b"\n").map_or(line.len(), <[u8]>::len);
    let mut content_start = 0;
    while content_start < line_break_start {
        let Some(end) = sgr_escape_end(line, content_start) else {
            break;
        };
        content_start = end;
    }
    &line[content_start..line_break_start]
}

fn hunk_body_line_has_git_color(line: &[u8]) -> bool {
    let line_break_start = line.strip_suffix(b"\n").map_or(line.len(), <[u8]>::len);
    let mut content_start = 0;
    let mut has_leading_sgr = false;
    while content_start < line_break_start {
        let Some(end) = sgr_escape_end(line, content_start) else {
            break;
        };
        has_leading_sgr = true;
        content_start = end;
    }

    if !matches!(line.get(content_start), Some(b' ' | b'+' | b'-')) {
        return false;
    }

    if has_leading_sgr {
        return true;
    }

    content_start += 1;
    sgr_escape_end(line, content_start)
        .is_some_and(|end| sgr_escape_is_reset(&line[content_start..end]))
}

fn append_patch_line_without_color_wrappers(
    line: &[u8],
    output: &mut Vec<u8>,
    strip_uncolored_context_reset: bool,
) {
    let line_break_start = line.strip_suffix(b"\n").map_or(line.len(), <[u8]>::len);
    let mut content_start = 0;
    let mut stripped_git_color = false;
    while content_start < line_break_start {
        let Some(end) = sgr_escape_end(line, content_start) else {
            break;
        };
        stripped_git_color = true;
        content_start = end;
    }
    let leading_sgr_end = content_start;

    let structural_content = &line[content_start..line_break_start];
    if diff_structural_line(structural_content) {
        append_without_sgr_escapes(&line[content_start..line_break_start], output);
        output.extend_from_slice(&line[line_break_start..]);
        return;
    }

    let diff_prefix = line.get(content_start).copied();
    if matches!(diff_prefix, Some(b' ' | b'+' | b'-')) {
        output.push(line[content_start]);
        content_start += 1;
        let mut stripped_prefix_reset = false;
        while content_start < line_break_start {
            let Some(end) = sgr_escape_end(line, content_start) else {
                break;
            };
            if !sgr_escape_is_reset(&line[content_start..end]) {
                break;
            }
            stripped_git_color = true;
            stripped_prefix_reset = true;
            content_start = end;
        }
        // Git can reset after the +/- marker and reapply the line color before
        // the payload. Strip one copy only; an identical following SGR can be
        // literal file content.
        if stripped_prefix_reset
            && strip_reapplied_git_line_color(line, leading_sgr_end, &mut content_start)
        {
            stripped_git_color = true;
        }
    }

    let strip_trailing_reset =
        stripped_git_color || (strip_uncolored_context_reset && matches!(diff_prefix, Some(b' ')));
    let content_end = if strip_trailing_reset {
        trailing_sgr_reset_start(line, content_start, line_break_start).unwrap_or(line_break_start)
    } else {
        line_break_start
    };

    output.extend_from_slice(&line[content_start..content_end]);
    output.extend_from_slice(&line[line_break_start..]);
}

fn strip_reapplied_git_line_color(
    line: &[u8],
    leading_sgr_end: usize,
    content_start: &mut usize,
) -> bool {
    if leading_sgr_end == 0 {
        return false;
    }

    let leading_sgr = &line[..leading_sgr_end];
    if !line[*content_start..].starts_with(leading_sgr) {
        return false;
    }

    *content_start += leading_sgr.len();
    true
}

fn diff_structural_line(line: &[u8]) -> bool {
    if diff_structural_line_without_sgr(line) {
        return true;
    }

    let mut stripped = Vec::with_capacity(line.len());
    append_without_sgr_escapes(line, &mut stripped);
    stripped.len() != line.len() && diff_structural_line_without_sgr(&stripped)
}

fn diff_structural_line_without_sgr(line: &[u8]) -> bool {
    line.starts_with(b"diff --git ")
        || line.starts_with(b"index ")
        || line.starts_with(b"old mode ")
        || line.starts_with(b"new mode ")
        || line.starts_with(b"deleted file mode ")
        || line.starts_with(b"new file mode ")
        || line.starts_with(b"similarity index ")
        || line.starts_with(b"dissimilarity index ")
        || line.starts_with(b"rename from ")
        || line.starts_with(b"rename to ")
        || line.starts_with(b"copy from ")
        || line.starts_with(b"copy to ")
        || line.starts_with(b"@@ ")
        || line.starts_with(b"Binary files ")
        || line.starts_with(b"GIT binary patch")
}

fn append_without_sgr_escapes(input: &[u8], output: &mut Vec<u8>) {
    let mut index = 0;
    while index < input.len() {
        if let Some(end) = sgr_escape_end(input, index) {
            index = end;
        } else {
            output.push(input[index]);
            index += 1;
        }
    }
}

fn split_patch_prelude(input: &[u8]) -> (&[u8], &[u8]) {
    let Some(patch_start) = first_git_diff_line_start(input) else {
        return (&[], input);
    };
    input.split_at(patch_start)
}

fn first_git_diff_line_start(input: &[u8]) -> Option<usize> {
    let mut offset = 0;
    for line in input.split_inclusive(|byte| *byte == b'\n') {
        let line_without_lf = line.strip_suffix(b"\n").unwrap_or(line);
        let line_without_crlf = line_without_lf
            .strip_suffix(b"\r")
            .unwrap_or(line_without_lf);
        if line_without_crlf.starts_with(b"diff --git ") {
            return Some(offset);
        }
        offset += line.len();
    }
    None
}

fn trailing_sgr_reset_start(input: &[u8], start: usize, end: usize) -> Option<usize> {
    let mut index = start;
    while index < end {
        if let Some(escape_end) = sgr_escape_end(input, index) {
            if escape_end == end && sgr_escape_is_reset(&input[index..escape_end]) {
                return Some(index);
            }
            index = escape_end;
        } else {
            index += 1;
        }
    }
    None
}

fn sgr_escape_end(input: &[u8], index: usize) -> Option<usize> {
    if input.get(index) != Some(&0x1b) || input.get(index + 1) != Some(&b'[') {
        return None;
    }

    csi_escape_end(input, index + 2).filter(|end| input.get(end - 1) == Some(&b'm'))
}

fn sgr_escape_is_reset(escape: &[u8]) -> bool {
    let Some(parameters) = escape
        .strip_prefix(b"\x1b[")
        .and_then(|escape| escape.strip_suffix(b"m"))
    else {
        return false;
    };

    parameters.is_empty()
        || parameters
            .split(|byte| *byte == b';')
            .all(|parameter| parameter.is_empty() || parameter.iter().all(|byte| *byte == b'0'))
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
            0x1b => {
                if let Some(end) = escape_end(input, index) {
                    index = end;
                } else {
                    output.push(input[index]);
                    index += 1;
                }
            }
            byte => {
                output.push(byte);
                index += 1;
            }
        }
    }
    output
}

fn escape_end(input: &[u8], index: usize) -> Option<usize> {
    let introducer = input.get(index + 1).copied()?;
    let payload = index + 2;
    match introducer {
        b'[' => csi_escape_end(input, payload),
        b']' | b'P' | b'^' | b'_' | b'X' => string_escape_end(input, payload),
        0x20..=0x2f => input
            .get(payload)
            .filter(|byte| (0x30..=0x7e).contains(*byte))
            .map(|_| payload + 1),
        0x30..=0x7e => Some(payload),
        _ => None,
    }
}

fn csi_escape_end(input: &[u8], mut index: usize) -> Option<usize> {
    let mut seen_intermediate = false;
    while let Some(byte) = input.get(index).copied() {
        match byte {
            0x30..=0x3f if !seen_intermediate => index += 1,
            0x20..=0x2f => {
                seen_intermediate = true;
                index += 1;
            }
            0x40..=0x7e => return Some(index + 1),
            _ => return None,
        }
    }
    None
}

fn string_escape_end(input: &[u8], mut index: usize) -> Option<usize> {
    while let Some(byte) = input.get(index).copied() {
        match byte {
            0x07 => return Some(index + 1),
            b'\n' | b'\r' => return None,
            0x1b if input.get(index + 1) == Some(&b'\\') => return Some(index + 2),
            0x1b => return None,
            _ => index += 1,
        }
    }
    None
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
