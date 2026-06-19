mod args;
mod config;
mod syntax;
mod update;

use std::{
    fmt,
    io::{self, IsTerminal, Write},
    process::ExitCode,
};

use clap::Parser;
use dx_core::{DxError, DxResult};

use crate::{
    args::{Cli, Command},
    syntax::{diff_options, patch_options, show_options, syntax},
    update::update,
};

fn main() -> ExitCode {
    if let Some(exit_code) = syntax_validation_child_exit_code() {
        return exit_code;
    }

    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) if is_clean_exit_error(&error) => ExitCode::SUCCESS,
        Err(error) => {
            let _ = write_stderr(format_args!("dx: {error}\n"));
            ExitCode::from(1)
        }
    }
}

fn syntax_validation_child_exit_code() -> Option<ExitCode> {
    dx_command::run_validation_child_from_env().map(|result| match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let _ = write_stderr(format_args!("{error}\n"));
            ExitCode::from(1)
        }
    })
}

pub(crate) type CliResult<T> = Result<T, CliError>;

#[derive(Debug)]
pub(crate) enum CliError {
    Dx(DxError),
    StdoutBrokenPipe,
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Dx(error) => write!(formatter, "{error}"),
            Self::StdoutBrokenPipe => write!(formatter, "broken pipe"),
        }
    }
}

impl From<DxError> for CliError {
    fn from(error: DxError) -> Self {
        Self::Dx(error)
    }
}

impl From<io::Error> for CliError {
    fn from(error: io::Error) -> Self {
        Self::Dx(error.into())
    }
}

pub(crate) fn write_stdout(args: fmt::Arguments<'_>) -> CliResult<()> {
    io::stdout()
        .lock()
        .write_fmt(args)
        .map_err(stdout_write_error)?;
    Ok(())
}

pub(crate) fn write_stderr(args: fmt::Arguments<'_>) -> DxResult<()> {
    io::stderr().lock().write_fmt(args)?;
    Ok(())
}

fn stdout_write_error(error: io::Error) -> CliError {
    if error.kind() == io::ErrorKind::BrokenPipe {
        CliError::StdoutBrokenPipe
    } else {
        error.into()
    }
}

fn is_clean_exit_error(error: &CliError) -> bool {
    matches!(error, CliError::StdoutBrokenPipe)
}

fn run() -> CliResult<()> {
    let cli = Cli::parse();

    match cli.command {
        None => run_diff(cli.diff),
        Some(Command::Config) => config::config(),
        Some(Command::Diff(args)) => run_diff(args),
        Some(Command::Show(args)) => run_show(args),
        Some(Command::Patch(args)) => run_patch(args),
        Some(Command::Syntax { command }) => syntax(command),
        Some(Command::Update(args)) => update(args),
    }
}

fn run_diff(args: args::DiffArgs) -> CliResult<()> {
    let stat = args.stat;
    let live_updates = !args.no_watch;
    let syntax_enabled = !args.no_syntax;
    let options = diff_options(args)?;
    run_review(options, live_updates, syntax_enabled, stat)
}

fn run_show(args: args::ShowArgs) -> CliResult<()> {
    let stat = args.stat;
    let syntax_enabled = !args.no_syntax;
    let options = show_options(args)?;
    run_review(options, false, syntax_enabled, stat)
}

fn run_patch(args: args::PatchArgs) -> CliResult<()> {
    let stat = args.stat;
    let syntax_enabled = !args.no_syntax;
    let options = patch_options(args)?;
    run_review(options, false, syntax_enabled, stat)
}

fn run_review(
    options: dx_command::DiffOptions,
    live_updates: bool,
    syntax_enabled: bool,
    stat: bool,
) -> CliResult<()> {
    if io::stdout().is_terminal() && !stat {
        dx_tui::run_diff_with_live_updates_and_syntax(options, live_updates, syntax_enabled)?;
        Ok(())
    } else {
        stream_diff_to_stdout(options)
    }
}

fn stream_diff_to_stdout(options: dx_command::DiffOptions) -> CliResult<()> {
    match dx_command::diff_to_writer(options, io::stdout().lock()) {
        Ok(()) => Ok(()),
        Err(DxError::Io(error)) if error.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        Err(error) => Err(error.into()),
    }
}
