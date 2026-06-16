use std::{
    io::{self, Read},
    path::{Path, PathBuf},
    sync::Arc,
};

use dx_core::{DxError, DxResult};

use crate::{
    CliResult,
    args::{DiffArgs, SyntaxAvailableArgs, SyntaxCommand},
    write_stdout,
};

pub(crate) fn syntax(command: SyntaxCommand) -> CliResult<()> {
    match command {
        SyntaxCommand::Add(args) => {
            let result = dx_command::syntax_add(&args.languages)?;
            print_syntax_add_result(&result)?;
        }
        SyntaxCommand::Update(args) => {
            let result = dx_command::syntax_update(&args.languages, args.all)?;
            print_syntax_update_result(&result)?;
        }
        SyntaxCommand::Rm(args) => {
            let result = dx_command::syntax_remove(&args.languages)?;
            print_syntax_remove_result(&result)?;
        }
        SyntaxCommand::List => {
            print_syntax_statuses(&dx_command::syntax_statuses()?, false)?;
        }
        SyntaxCommand::Available(args) => {
            for language in dx_command::syntax_available_languages(syntax_available_filter(&args))?
            {
                write_stdout(format_args!("{language}\n"))?;
            }
        }
        SyntaxCommand::Clean => {
            let result = dx_command::syntax_clean_cache()?;
            write_stdout(format_args!(
                "removed {} parser artifacts and {} checksum records\n",
                result.parser_artifacts_removed, result.artifact_records_removed
            ))?;
            write_stdout(format_args!(
                "kept {} enabled-language config entries\n",
                result.enabled_languages_kept
            ))?;
        }
        SyntaxCommand::Path => {
            write_stdout(format_args!(
                "cache       {}\n",
                dx_command::syntax_cache_dir()?
            ))?;
            write_stdout(format_args!(
                "registry    {}\n",
                dx_command::syntax_config_path()?.display()
            ))?;
            write_stdout(format_args!(
                "config      {}\n",
                dx_command::syntax_settings_path()?.display()
            ))?;
            write_stdout(format_args!(
                "colorscheme {}\n",
                dx_command::syntax_colorscheme_dir()?.display()
            ))?;
        }
        SyntaxCommand::Doctor => {
            let report = dx_command::syntax_doctor()?;
            print_syntax_statuses(&report.statuses, true)?;
            if report.issues.is_empty() {
                write_stdout(format_args!("ok\n"))?;
            } else {
                for issue in report.issues {
                    write_stdout(format_args!(
                        "warning {}: {}\n",
                        issue.language, issue.message
                    ))?;
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn syntax_available_filter(
    args: &SyntaxAvailableArgs,
) -> dx_command::SyntaxAvailableFilter {
    if args.installed {
        dx_command::SyntaxAvailableFilter::Installed
    } else if args.enabled {
        dx_command::SyntaxAvailableFilter::Enabled
    } else {
        dx_command::SyntaxAvailableFilter::All
    }
}

pub(crate) fn diff_options(mut args: DiffArgs) -> DxResult<dx_command::DiffOptions> {
    if let Some(target) = args.pr.take() {
        return pr_diff_options(args, &target);
    }

    if let Some(patch) = args.patch {
        if args.base.is_some() || !args.revs.is_empty() {
            return Err(DxError::Usage(
                "use --patch without revisions or --base".to_owned(),
            ));
        }
        if args.staged || args.unstaged || args.no_untracked {
            return Err(DxError::Usage(
                "--staged, --unstaged, and --no-untracked do not apply to --patch".to_owned(),
            ));
        }

        return Ok(dx_command::DiffOptions {
            repo: args.repo,
            source: patch_source(patch)?,
            scope: dx_command::DiffScope::All,
            include_untracked: false,
            stat: args.stat,
        });
    }

    let source = match (args.base, args.revs.as_slice()) {
        (Some(base), []) => dx_command::DiffSource::Base(base),
        (Some(_), _) => {
            return Err(DxError::Usage(
                "use either --base or positional revisions, not both".to_owned(),
            ));
        }
        (None, []) => dx_command::DiffSource::Worktree,
        (None, [base]) => dx_command::DiffSource::Base(base.clone()),
        (None, [left, right]) => dx_command::DiffSource::Range {
            left: left.clone(),
            right: right.clone(),
        },
        (None, _) => {
            return Err(DxError::Usage(
                "dx accepts at most two revisions".to_owned(),
            ));
        }
    };

    let scope = if args.staged {
        dx_command::DiffScope::Staged
    } else if args.unstaged {
        dx_command::DiffScope::Unstaged
    } else {
        dx_command::DiffScope::All
    };

    Ok(dx_command::DiffOptions {
        repo: args.repo,
        source,
        scope,
        include_untracked: !args.no_untracked,
        stat: args.stat,
    })
}

pub(crate) fn pr_diff_options(args: DiffArgs, target: &str) -> DxResult<dx_command::DiffOptions> {
    if args.base.is_some() || !args.revs.is_empty() {
        return Err(DxError::Usage(
            "use --pr without revisions or --base".to_owned(),
        ));
    }
    if args.staged || args.unstaged || args.no_untracked {
        return Err(DxError::Usage(
            "--staged, --unstaged, and --no-untracked do not apply to dx --pr".to_owned(),
        ));
    }
    if args.patch.is_some() {
        return Err(DxError::Usage(
            "--patch does not apply to dx --pr".to_owned(),
        ));
    }

    dx_command::github_pr_diff_options(args.repo, target, args.stat)
}

pub(crate) fn patch_source(path: PathBuf) -> DxResult<dx_command::DiffSource> {
    if path == Path::new("-") {
        let mut patch = Vec::new();
        io::stdin().read_to_end(&mut patch)?;
        return Ok(dx_command::DiffSource::Patch(
            dx_command::PatchSource::Stdin(Arc::from(patch.into_boxed_slice())),
        ));
    }

    Ok(dx_command::DiffSource::Patch(
        dx_command::PatchSource::File(path),
    ))
}

pub(crate) fn print_syntax_add_result(result: &dx_command::SyntaxAddResult) -> CliResult<()> {
    for language in &result.added {
        write_stdout(format_args!("+ enabled {language}\n"))?;
    }
    for language in &result.already_enabled {
        write_stdout(format_args!("= enabled {language}\n"))?;
    }
    for language in &result.without_highlights {
        write_stdout(format_args!(
            "warning {language}: no bundled highlights query; diff will render plain text\n"
        ))?;
    }
    Ok(())
}

pub(crate) fn print_syntax_update_result(result: &dx_command::SyntaxUpdateResult) -> CliResult<()> {
    if result.updated.is_empty()
        && result.bundled.is_empty()
        && result.not_installed.is_empty()
        && result.unavailable.is_empty()
    {
        write_stdout(format_args!("no parser caches to update\n"))?;
    }
    for language in &result.updated {
        write_stdout(format_args!("~ updated parser cache {language}\n"))?;
    }
    for language in &result.bundled {
        write_stdout(format_args!("= bundled parser {language}\n"))?;
    }
    for language in &result.not_installed {
        write_stdout(format_args!("= not installed {language}\n"))?;
    }
    for language in &result.unavailable {
        write_stdout(format_args!("warning {language}: language is not known\n"))?;
    }
    for language in &result.without_highlights {
        write_stdout(format_args!(
            "warning {language}: no bundled highlights query; diff will render plain text\n"
        ))?;
    }
    Ok(())
}

pub(crate) fn print_syntax_remove_result(result: &dx_command::SyntaxRemoveResult) -> CliResult<()> {
    for language in &result.removed {
        write_stdout(format_args!("- disabled {language} in config\n"))?;
    }
    for language in &result.missing {
        write_stdout(format_args!("= not enabled in config {language}\n"))?;
    }
    for language in &result.cache_deleted {
        write_stdout(format_args!("- deleted parser cache {language}\n"))?;
    }
    for language in &result.cache_missing {
        write_stdout(format_args!("= no parser cache {language}\n"))?;
    }
    Ok(())
}

pub(crate) fn print_syntax_statuses(
    statuses: &[dx_command::SyntaxLanguageStatus],
    include_trust: bool,
) -> CliResult<()> {
    if include_trust {
        write_stdout(format_args!(
            "{:<20} {:<11} {:<9} {:<8} {:<12} {}\n",
            "language", "status", "trusted", "syntax", "version", "source"
        ))?;
    } else {
        write_stdout(format_args!(
            "{:<20} {:<11} {:<8} {:<12} {}\n",
            "language", "status", "syntax", "version", "source"
        ))?;
    }

    for status in statuses {
        let state = if status.enabled && status.installed {
            "enabled"
        } else if status.enabled {
            "missing"
        } else if status.installed {
            "installed"
        } else {
            "unknown"
        };
        let syntax = if status.has_highlights {
            "yes"
        } else {
            "plain"
        };
        let version = status.version.as_deref().unwrap_or("-");
        let source = status.source.as_deref().unwrap_or("-");
        if include_trust {
            let trusted = if status.trusted { "yes" } else { "no" };
            write_stdout(format_args!(
                "{:<20} {:<11} {:<9} {:<8} {:<12} {}\n",
                status.language, state, trusted, syntax, version, source
            ))?;
        } else {
            write_stdout(format_args!(
                "{:<20} {:<11} {:<8} {:<12} {}\n",
                status.language, state, syntax, version, source
            ))?;
        }
    }

    Ok(())
}
