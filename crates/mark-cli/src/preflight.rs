use clap::{CommandFactory, error::ErrorKind};
use mark_core::{MarkError, MarkResult};
use mark_git::{RevisionKind, RevisionStatus, revision_status};

use crate::{
    CliError,
    args::{self, Cli},
};

pub(crate) fn reject_pre_subcommand_diff_args(cli: &Cli) -> MarkResult<()> {
    if cli.command.is_some() && has_diff_args(&cli.diff) {
        return Err(MarkError::Usage(
            "top-level diff options cannot be used before a subcommand; move supported options after the subcommand".to_owned(),
        ));
    }

    Ok(())
}

fn has_diff_args(args: &args::DiffArgs) -> bool {
    !args.revs.is_empty()
        || args.repo.repo.is_some()
        || args.base.is_some()
        || args.staged
        || args.unstaged
        || args.no_untracked
        || args.watch.no_watch
        || args.display.no_syntax
        || args.display.stat
}

pub(crate) fn reject_likely_unknown_command(args: &args::DiffArgs) -> Result<(), CliError> {
    if args.base.is_some() || args.revs.is_empty() || args.revs[0].starts_with('-') {
        return Ok(());
    }

    let rev = &args.revs[0];
    let revision_kind = if args.revs.len() == 1 {
        RevisionKind::Commit
    } else {
        RevisionKind::Object
    };
    match revision_status(args.repo.repo.as_deref(), rev, revision_kind) {
        RevisionStatus::Exists => return Ok(()),
        RevisionStatus::Missing => {}
        RevisionStatus::Unknown if looks_like_command(rev) => {}
        RevisionStatus::Unknown => return Ok(()),
    }

    Err(CliError::Clap(unknown_command_or_revision_error(rev)))
}

fn unknown_command_or_revision_error(rev: &str) -> clap::Error {
    Cli::command().error(
        ErrorKind::InvalidSubcommand,
        format!("unrecognized subcommand or revision '{rev}'"),
    )
}

fn looks_like_command(value: &str) -> bool {
    matches!(
        value,
        "ls" | "list" | "pwd" | "cd" | "rm" | "remove" | "new" | "fork" | "status"
    )
}
