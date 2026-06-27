use std::{
    borrow::Cow,
    env,
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::{self, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};

use crate::{CliResult, write_stderr, write_stdout_bytes};

use super::STREAM_BUFFER_SIZE;

pub(super) const DEFAULT_TEXT_PAGER: &str = "less -R";
const PLAIN_TEXT_PAGER_GUARD: &str = "MARK_PAGER_PLAIN_TEXT_FALLBACK";
const TEMP_SPOOL_CREATE_ATTEMPTS: usize = 16;

static NEXT_TEMP_SPOOL: AtomicU64 = AtomicU64::new(0);

pub(super) fn page_plain_text(input: &[u8]) -> CliResult<()> {
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

pub(super) fn page_plain_text_stream<R: Read>(prefix: &[u8], input: &mut R) -> CliResult<()> {
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

pub(super) fn stream_to_pager<R: Read, W: Write>(
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
pub(super) struct StreamFallback {
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
    pub(super) fn write_to_writer<R: Read, W: Write>(
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

pub(super) fn stream_to_stdout<R: Read>(prefix: &[u8], input: &mut R) -> CliResult<()> {
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

pub(super) fn resolve_text_pager_command(configured_pager: Option<&str>) -> Cow<'_, str> {
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
