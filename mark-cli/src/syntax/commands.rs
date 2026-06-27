use crate::{
    CliResult,
    args::{SyntaxAvailableArgs, SyntaxCommand},
    write_stdout,
};

use super::{
    print_syntax_add_result, print_syntax_remove_result, print_syntax_statuses,
    print_syntax_update_result,
};

pub(crate) fn syntax(command: SyntaxCommand) -> CliResult<()> {
    match command {
        SyntaxCommand::Add(args) => {
            let result = mark_command::syntax_add_with_options(
                &args.languages,
                mark_command::SyntaxAddOptions {
                    parser: args.parser,
                    query: args.query,
                    extensions: args.extensions,
                    filenames: args.filenames,
                },
            )?;
            print_syntax_add_result(&result)?;
        }
        SyntaxCommand::Update(args) => {
            let result = mark_command::syntax_update(&args.languages, args.all)?;
            print_syntax_update_result(&result)?;
        }
        SyntaxCommand::Rm(args) => {
            let result = mark_command::syntax_remove(&args.languages)?;
            print_syntax_remove_result(&result)?;
        }
        SyntaxCommand::List => {
            print_syntax_statuses(&mark_command::syntax_statuses()?, false)?;
        }
        SyntaxCommand::Available(args) => {
            for language in
                mark_command::syntax_available_languages(syntax_available_filter(&args))?
            {
                write_stdout(format_args!("{language}\n"))?;
            }
        }
        SyntaxCommand::Clean => {
            let result = mark_command::syntax_clean_cache()?;
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
                mark_command::syntax_cache_dir()?
            ))?;
            write_stdout(format_args!(
                "registry    {}\n",
                mark_command::syntax_config_path()?.display()
            ))?;
            write_stdout(format_args!(
                "config      {}\n",
                mark_command::syntax_settings_path()?.display()
            ))?;
            write_stdout(format_args!(
                "colorscheme {}\n",
                mark_command::syntax_colorscheme_dir()?.display()
            ))?;
            write_stdout(format_args!(
                "queries     {}\n",
                mark_command::syntax_queries_dir()?.display()
            ))?;
            write_stdout(format_args!(
                "parsers     {}\n",
                mark_command::syntax_parsers_dir()?.display()
            ))?;
        }
        SyntaxCommand::Doctor => {
            let report = mark_command::syntax_doctor()?;
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
) -> mark_command::SyntaxAvailableFilter {
    if args.installed {
        mark_command::SyntaxAvailableFilter::Installed
    } else if args.enabled {
        mark_command::SyntaxAvailableFilter::Enabled
    } else {
        mark_command::SyntaxAvailableFilter::All
    }
}
