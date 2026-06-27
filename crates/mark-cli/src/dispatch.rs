use crate::{
    CliResult,
    args::{self, Cli, Command},
    config,
    pager::pager,
    preflight::{reject_likely_unknown_command, reject_pre_subcommand_diff_args},
    review::{ReviewRequest, run_review},
    syntax::{diff_options, difftool_options, patch_options, review_options, show_options, syntax},
    update::update,
};

pub(crate) fn run_cli(cli: Cli) -> CliResult<()> {
    reject_pre_subcommand_diff_args(&cli)?;
    let Cli { command, diff } = cli;
    match command {
        None => {
            reject_likely_unknown_command(&diff)?;
            run_review_command(diff)
        }
        Some(Command::Config) => config::config(),
        Some(Command::Diff(args)) => run_review_command(args),
        Some(Command::Difftool(args)) => run_review_command(args),
        Some(Command::Pager(args)) => pager(args),
        Some(Command::Show(args)) => run_review_command(args),
        Some(Command::Review(args)) => run_review_command(args),
        Some(Command::Patch(args)) => run_review_command(args),
        Some(Command::Syntax { command }) => syntax(command),
        Some(Command::Update(args)) => update(args),
    }
}

trait ReviewCommand {
    fn into_review_request(self) -> CliResult<ReviewRequest>;
}

fn run_review_command(command: impl ReviewCommand) -> CliResult<()> {
    run_review(command.into_review_request()?)
}

fn review_request(
    options: mark_command::DiffOptions,
    live_updates: bool,
    syntax_enabled: bool,
) -> ReviewRequest {
    ReviewRequest {
        options,
        live_updates,
        syntax_enabled,
    }
}

impl ReviewCommand for args::DiffArgs {
    fn into_review_request(self) -> CliResult<ReviewRequest> {
        let live_updates = !self.watch.no_watch;
        let syntax_enabled = self.display.syntax_enabled();
        Ok(review_request(
            diff_options(self)?,
            live_updates,
            syntax_enabled,
        ))
    }
}

impl ReviewCommand for args::ShowArgs {
    fn into_review_request(self) -> CliResult<ReviewRequest> {
        let syntax_enabled = self.display.syntax_enabled();
        Ok(review_request(show_options(self)?, false, syntax_enabled))
    }
}

impl ReviewCommand for args::ReviewArgs {
    fn into_review_request(self) -> CliResult<ReviewRequest> {
        let syntax_enabled = self.display.syntax_enabled();
        Ok(review_request(review_options(self)?, false, syntax_enabled))
    }
}

impl ReviewCommand for args::DifftoolArgs {
    fn into_review_request(self) -> CliResult<ReviewRequest> {
        let live_updates = self.watch.watch;
        let syntax_enabled = self.display.syntax_enabled();
        Ok(review_request(
            difftool_options(self)?,
            live_updates,
            syntax_enabled,
        ))
    }
}

impl ReviewCommand for args::PatchArgs {
    fn into_review_request(self) -> CliResult<ReviewRequest> {
        let syntax_enabled = self.display.syntax_enabled();
        Ok(review_request(patch_options(self)?, false, syntax_enabled))
    }
}
