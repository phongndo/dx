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
    let Cli { command, diff } = cli;
    match command {
        None => {
            reject_likely_unknown_command(&diff)?;
            run_diff(diff)
        }
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
        RevisionKind::Object
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
    Object,
}

fn unknown_command_or_revision_error(rev: &str) -> clap::Error {
    Cli::command().error(
        ErrorKind::InvalidSubcommand,
        format!("unrecognized subcommand or revision '{rev}'"),
    )
}

fn revision_status(repo: Option<&Path>, rev: &str, kind: RevisionKind) -> RevisionStatus {
    match kind {
        RevisionKind::Commit => commit_revision_status(repo, rev),
        RevisionKind::Object => match revision_expression_exists(repo, rev) {
            Some(true) => RevisionStatus::Exists,
            Some(false) => missing_revision_status(repo),
            None => RevisionStatus::Unknown,
        },
    }
}

fn commit_revision_status(repo: Option<&Path>, rev: &str) -> RevisionStatus {
    let Some(object) = resolve_revision(repo, rev) else {
        return missing_revision_status(repo);
    };

    match revision_object_matches(repo, &object, "commit") {
        Some(true) => RevisionStatus::Exists,
        Some(false) => RevisionStatus::Missing,
        None => RevisionStatus::Unknown,
    }
}

fn missing_revision_status(repo: Option<&Path>) -> RevisionStatus {
    if git_repository_available(repo) {
        RevisionStatus::Missing
    } else {
        RevisionStatus::Unknown
    }
}

fn revision_expression_exists(repo: Option<&Path>, rev: &str) -> Option<bool> {
    let output = rev_parse_verify(repo, rev)?;
    // `rev-parse --verify` exits non-zero for expressions that expand to
    // multiple objects, but still writes the resolved objects. `git diff`
    // accepts those expressions as range operands.
    if output.status.success() || !output_stdout_is_empty(&output) {
        return Some(true);
    }

    multi_revision_expression_exists(repo, rev)
}

fn resolve_revision(repo: Option<&Path>, rev: &str) -> Option<String> {
    let output = rev_parse_verify(repo, rev)?;
    if !output.status.success() {
        return None;
    }

    let revision = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if revision.is_empty() {
        None
    } else {
        Some(revision)
    }
}

fn rev_parse_verify(repo: Option<&Path>, rev: &str) -> Option<std::process::Output> {
    let mut command = ProcessCommand::new("git");
    if let Some(repo) = repo {
        command.arg("-C").arg(repo);
    }
    command
        .args(["rev-parse", "--verify", "--quiet", "--end-of-options"])
        .arg(rev);

    command.output().ok()
}

fn multi_revision_expression_exists(repo: Option<&Path>, rev: &str) -> Option<bool> {
    let mut command = ProcessCommand::new("git");
    if let Some(repo) = repo {
        command.arg("-C").arg(repo);
    }
    command
        .args(["rev-list", "--no-walk", "--quiet", "--end-of-options"])
        .arg(rev);

    command.output().ok().map(|output| output.status.success())
}

fn output_stdout_is_empty(output: &std::process::Output) -> bool {
    String::from_utf8_lossy(&output.stdout).trim().is_empty()
}

fn revision_object_matches(repo: Option<&Path>, object: &str, peel: &str) -> Option<bool> {
    let mut command = ProcessCommand::new("git");
    if let Some(repo) = repo {
        command.arg("-C").arg(repo);
    }
    command
        .args(["rev-parse", "--verify", "--quiet", "--end-of-options"])
        .arg(format!("{object}^{{{peel}}}"));

    match command.output() {
        Ok(output) => Some(output.status.success()),
        Err(_) => None,
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
        let error = run_cli(parse(&["dx", "--repo", "/definitely/not/a/repo", "ls"]))
            .expect_err("invalid target should be rejected before git diff");

        assert!(matches!(error, CliError::Clap(_)));
        let rendered = error.to_string();
        assert!(rendered.contains("unrecognized subcommand or revision 'ls'"));
        assert!(rendered.contains("Usage: dx [OPTIONS] [COMMAND|REV] [REV]"));
    }

    #[test]
    fn explicit_diff_missing_left_operand_is_revision_error() {
        let test_dir = temp_test_dir("explicit-diff-missing-left");
        let repo = test_dir.join("repo");
        init_repo(&repo);
        let repo_arg = repo.to_string_lossy().into_owned();
        let cli = parse(&[
            "dx",
            "diff",
            "--repo",
            repo_arg.as_str(),
            "--stat",
            "missing",
            "HEAD",
        ]);

        let error = run_cli(cli).expect_err("missing diff revision should be rejected");

        assert!(matches!(error, CliError::Dx(_)));
        assert!(error.to_string().contains("unknown revision `missing`"));
        fs::remove_dir_all(test_dir).expect("test directory should be removed");
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
    fn two_revision_preflight_accepts_multi_object_left_operand() {
        let test_dir = temp_test_dir("range-multi-object-preflight");
        let repo = test_dir.join("repo");
        init_repo(&repo);
        git(["branch", "-M", "main"], &repo);
        git(["checkout", "-q", "-b", "side"], &repo);
        fs::write(repo.join("side.txt"), "side\n").expect("side file should be written");
        git(["add", "side.txt"], &repo);
        git(["commit", "-q", "-m", "side"], &repo);
        git(["checkout", "-q", "main"], &repo);
        git(["merge", "-q", "--no-ff", "side", "-m", "merge"], &repo);

        reject_likely_unknown_command(&args::DiffArgs {
            revs: vec!["HEAD^@".to_owned(), "HEAD".to_owned()],
            repo: Some(repo),
            ..args::DiffArgs::default()
        })
        .expect("plain range operands should accept multi-object revisions");

        fs::remove_dir_all(test_dir).expect("test directory should be removed");
    }

    #[test]
    fn two_revision_preflight_accepts_rev_path_tree_operand() {
        let test_dir = temp_test_dir("range-rev-path-tree-preflight");
        let repo = test_dir.join("repo");
        init_repo(&repo);
        fs::create_dir_all(repo.join("src")).expect("source directory should be created");
        fs::write(repo.join("src/file.txt"), "one\n").expect("source file should be written");
        git(["add", "src/file.txt"], &repo);
        git(["commit", "-q", "-m", "add source"], &repo);
        fs::write(repo.join("src/file.txt"), "two\n").expect("source file should change");
        git(["add", "src/file.txt"], &repo);
        git(["commit", "-q", "-m", "change source"], &repo);

        reject_likely_unknown_command(&args::DiffArgs {
            revs: vec!["HEAD~1:src".to_owned(), "HEAD:src".to_owned()],
            repo: Some(repo),
            ..args::DiffArgs::default()
        })
        .expect("plain range operands should accept rev:path tree revisions");

        fs::remove_dir_all(test_dir).expect("test directory should be removed");
    }

    #[test]
    fn two_revision_preflight_accepts_rev_path_blob_operand() {
        let test_dir = temp_test_dir("range-rev-path-blob-preflight");
        let repo = test_dir.join("repo");
        init_repo(&repo);
        fs::write(repo.join("file.txt"), "one\n").expect("file should be written");
        git(["add", "file.txt"], &repo);
        git(["commit", "-q", "-m", "add file"], &repo);
        fs::write(repo.join("file.txt"), "two\n").expect("file should change");
        git(["add", "file.txt"], &repo);
        git(["commit", "-q", "-m", "change file"], &repo);

        reject_likely_unknown_command(&args::DiffArgs {
            revs: vec!["HEAD~1:file.txt".to_owned(), "HEAD:file.txt".to_owned()],
            repo: Some(repo),
            ..args::DiffArgs::default()
        })
        .expect("plain range operands should accept rev:path blob revisions");

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
