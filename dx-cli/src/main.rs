mod args;
mod config;
mod pager;
mod syntax;
mod update;

use std::{
    fmt,
    io::{self, IsTerminal, Write},
    path::Path,
    process::Command as ProcessCommand,
    process::ExitCode,
};

use clap::{CommandFactory, Parser, error::ErrorKind};
use dx_core::{DxError, DxResult};

use crate::{
    args::{Cli, Command},
    pager::pager,
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
        Err(CliError::Clap(error)) => {
            let exit_code = error.exit_code();
            let _ = error.print();
            ExitCode::from(u8::try_from(exit_code).unwrap_or(1))
        }
        Err(error) => {
            let _ = write_stderr(format_args!(
                "{} {error}\n",
                styled_error_prefix(io::stderr().is_terminal())
            ));
            ExitCode::from(1)
        }
    }
}

fn styled_error_prefix(color: bool) -> &'static str {
    if color { "\x1b[31mdx:\x1b[0m" } else { "dx:" }
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
    Clap(clap::Error),
    StdoutBrokenPipe,
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Dx(error) => write!(formatter, "{error}"),
            Self::Clap(error) => write!(formatter, "{error}"),
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

pub(crate) fn write_stdout_bytes(bytes: &[u8]) -> CliResult<()> {
    io::stdout()
        .lock()
        .write_all(bytes)
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
    run_cli(cli)
}

fn run_cli(cli: Cli) -> CliResult<()> {
    reject_pre_subcommand_diff_args(&cli)?;
    match cli.command {
        None => run_diff(cli.diff),
        Some(Command::Config) => config::config(),
        Some(Command::Diff(args)) => run_diff(args),
        Some(Command::Pager(args)) => pager(args),
        Some(Command::Show(args)) => run_show(args),
        Some(Command::Patch(args)) => run_patch(args),
        Some(Command::Syntax { command }) => syntax(command),
        Some(Command::Update(args)) => update(args),
    }
}

fn reject_pre_subcommand_diff_args(cli: &Cli) -> DxResult<()> {
    if cli.command.is_some() && has_diff_args(&cli.diff) {
        return Err(DxError::Usage(
            "top-level diff options cannot be used before a subcommand; move supported options after the subcommand".to_owned(),
        ));
    }

    Ok(())
}

fn has_diff_args(args: &args::DiffArgs) -> bool {
    !args.revs.is_empty()
        || args.pr.is_some()
        || args.repo.is_some()
        || args.base.is_some()
        || args.staged
        || args.unstaged
        || args.no_untracked
        || args.patch.is_some()
        || args.no_watch
        || args.no_syntax
        || args.stat
}

fn run_diff(args: args::DiffArgs) -> CliResult<()> {
    reject_likely_unknown_command(&args)?;
    let stat = args.stat;
    let live_updates = !args.no_watch;
    let syntax_enabled = !args.no_syntax;
    let options = diff_options(args)?;
    run_review(options, live_updates, syntax_enabled, stat)
}

fn reject_likely_unknown_command(args: &args::DiffArgs) -> CliResult<()> {
    if args.base.is_some()
        || args.pr.is_some()
        || args.patch.is_some()
        || args.revs.is_empty()
        || args.revs[0].starts_with('-')
    {
        return Ok(());
    }

    let rev = &args.revs[0];
    let revision_kind = if args.revs.len() == 1 {
        RevisionKind::Commit
    } else {
        RevisionKind::Tree
    };
    match revision_status(args.repo.as_deref(), rev, revision_kind) {
        RevisionStatus::Exists => return Ok(()),
        RevisionStatus::Missing => {}
        RevisionStatus::Unknown if looks_like_command(rev) => {}
        RevisionStatus::Unknown => return Ok(()),
    }

    Err(CliError::Clap(unknown_command_or_revision_error(rev)))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RevisionStatus {
    Exists,
    Missing,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RevisionKind {
    Commit,
    Tree,
}

impl RevisionKind {
    fn peel(self) -> &'static str {
        match self {
            Self::Commit => "commit",
            Self::Tree => "tree",
        }
    }
}

fn unknown_command_or_revision_error(rev: &str) -> clap::Error {
    Cli::command().error(
        ErrorKind::InvalidSubcommand,
        format!("unrecognized subcommand or revision '{rev}'"),
    )
}

fn revision_status(repo: Option<&Path>, rev: &str, kind: RevisionKind) -> RevisionStatus {
    let mut command = ProcessCommand::new("git");
    if let Some(repo) = repo {
        command.arg("-C").arg(repo);
    }
    command
        .args(["rev-parse", "--verify", "--quiet", "--end-of-options"])
        .arg(format!("{rev}^{{{}}}", kind.peel()));

    match command.output() {
        Ok(output) if output.status.success() => RevisionStatus::Exists,
        Ok(_) if git_repository_available(repo) => RevisionStatus::Missing,
        _ => RevisionStatus::Unknown,
    }
}

fn git_repository_available(repo: Option<&Path>) -> bool {
    let mut command = ProcessCommand::new("git");
    if let Some(repo) = repo {
        command.arg("-C").arg(repo);
    }
    command.args(["rev-parse", "--show-toplevel"]);

    command.output().is_ok_and(|output| output.status.success())
}

fn looks_like_command(value: &str) -> bool {
    matches!(
        value,
        "ls" | "list" | "pwd" | "cd" | "rm" | "remove" | "new" | "fork" | "status"
    )
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

#[cfg(test)]
mod tests {
    use std::{
        env, fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    use clap::Parser;

    use super::*;

    fn parse(args: &[&str]) -> Cli {
        Cli::try_parse_from(args).expect("args should parse")
    }

    fn temp_test_dir(name: &str) -> PathBuf {
        env::temp_dir().join(format!(
            "dx-cli-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time should be after unix epoch")
                .as_nanos()
        ))
    }

    fn init_repo(repo: &Path) {
        fs::create_dir_all(repo).expect("repo directory should be created");
        git(["init", "-q"], repo);
        git(["config", "user.email", "test@example.com"], repo);
        git(["config", "user.name", "Test"], repo);
        fs::write(repo.join("base.txt"), "base\n").expect("base file should be written");
        git(["add", "base.txt"], repo);
        git(["commit", "-q", "-m", "init"], repo);
    }

    fn git<const N: usize>(args: [&str; N], cwd: &Path) {
        let output = ProcessCommand::new("git")
            .current_dir(cwd)
            .args(args)
            .output()
            .expect("git should run");
        assert!(
            output.status.success(),
            "git failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn rejects_top_level_diff_options_before_source_subcommands() {
        let error = run_cli(parse(&["dx", "--stat", "show", "HEAD"]))
            .expect_err("top-level --stat should be rejected before show");
        assert!(
            error
                .to_string()
                .contains("top-level diff options cannot be used before a subcommand")
        );

        let error = run_cli(parse(&[
            "dx",
            "--repo",
            "/tmp/repo",
            "patch",
            "changes.diff",
        ]))
        .expect_err("top-level --repo should be rejected before patch");
        assert!(
            error
                .to_string()
                .contains("top-level diff options cannot be used before a subcommand")
        );
    }

    #[test]
    fn unknown_single_top_level_target_renders_clap_style_error() {
        let error = reject_likely_unknown_command(&args::DiffArgs {
            revs: vec!["ls".to_owned()],
            repo: Some(PathBuf::from("/definitely/not/a/repo")),
            ..args::DiffArgs::default()
        })
        .expect_err("invalid target should be rejected before git diff");

        assert!(matches!(error, CliError::Clap(_)));
        let rendered = error.to_string();
        assert!(rendered.contains("unrecognized subcommand or revision 'ls'"));
        assert!(rendered.contains("Usage: dx [OPTIONS] [COMMAND|REV] [REV]"));
    }

    #[test]
    fn non_command_target_without_repo_is_left_for_git_error() {
        reject_likely_unknown_command(&args::DiffArgs {
            revs: vec!["HEAD".to_owned()],
            repo: Some(PathBuf::from("/definitely/not/a/repo")),
            ..args::DiffArgs::default()
        })
        .expect("non-command targets should not hide repository errors");
    }

    #[test]
    fn two_revision_preflight_accepts_treeish_left_operand() {
        let test_dir = temp_test_dir("range-treeish-preflight");
        let repo = test_dir.join("repo");
        init_repo(&repo);

        reject_likely_unknown_command(&args::DiffArgs {
            revs: vec!["HEAD^{tree}".to_owned(), "HEAD".to_owned()],
            repo: Some(repo),
            ..args::DiffArgs::default()
        })
        .expect("plain range operands should accept tree-ish revisions");

        fs::remove_dir_all(test_dir).expect("test directory should be removed");
    }

    #[test]
    fn single_revision_preflight_keeps_commitish_validation() {
        let test_dir = temp_test_dir("single-commitish-preflight");
        let repo = test_dir.join("repo");
        init_repo(&repo);

        let error = reject_likely_unknown_command(&args::DiffArgs {
            revs: vec!["HEAD^{tree}".to_owned()],
            repo: Some(repo),
            ..args::DiffArgs::default()
        })
        .expect_err("single-revision base diffs should still require commit-ish revisions");

        assert!(matches!(error, CliError::Clap(_)));
        fs::remove_dir_all(test_dir).expect("test directory should be removed");
    }
}
