use std::io::{self, IsTerminal};

use crate::{CliResult, write_stdout};

use super::table::{
    ascii_output_requested, list_glyphs, render_syntax_statuses, short_sha, terminal_width,
};

pub(crate) fn print_syntax_add_result(result: &mark_command::SyntaxAddResult) -> CliResult<()> {
    for language in &result.added {
        write_stdout(format_args!("+ enabled {language}\n"))?;
    }
    for language in &result.already_enabled {
        write_stdout(format_args!("= enabled {language}\n"))?;
    }
    for language in &result.without_highlights {
        write_stdout(format_args!(
            "warning {language}: no highlights query; diff will render plain text\n"
        ))?;
    }
    for language in &result.custom_parsers {
        write_stdout(format_args!("+ trusted custom parser {language}\n"))?;
    }
    for language in &result.custom_queries {
        write_stdout(format_args!("+ installed highlights query {language}\n"))?;
    }
    for mapping in &result.custom_mappings {
        write_stdout(format_args!("+ mapped {mapping}\n"))?;
    }
    Ok(())
}

pub(crate) fn print_syntax_update_result(
    result: &mark_command::SyntaxUpdateResult,
) -> CliResult<()> {
    if result.updated.is_empty()
        && result.bundled.is_empty()
        && result.custom.is_empty()
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
    for language in &result.custom {
        write_stdout(format_args!("= custom parser {language}\n"))?;
    }
    for language in &result.not_installed {
        write_stdout(format_args!("= not installed {language}\n"))?;
    }
    for language in &result.unavailable {
        write_stdout(format_args!("warning {language}: language is not known\n"))?;
    }
    for language in &result.without_highlights {
        write_stdout(format_args!(
            "warning {language}: no highlights query; diff will render plain text\n"
        ))?;
    }
    Ok(())
}

pub(crate) fn print_syntax_remove_result(
    result: &mark_command::SyntaxRemoveResult,
) -> CliResult<()> {
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
    statuses: &[mark_command::SyntaxLanguageStatus],
    detail: bool,
) -> CliResult<()> {
    if statuses.is_empty() {
        write_stdout(format_args!("no syntax languages enabled\n"))?;
        return Ok(());
    }

    let terminal = io::stdout().is_terminal();
    let glyphs = list_glyphs(terminal && !ascii_output_requested());
    write_stdout(format_args!(
        "{}",
        render_syntax_statuses(
            statuses,
            terminal,
            glyphs,
            terminal.then(terminal_width).flatten(),
        )
    ))?;

    if !detail {
        return Ok(());
    }

    for status in statuses {
        if let Some(artifact) = &status.artifact {
            write_stdout(format_args!(
                "  {} parser={} sha256={} source={} installed_at={}\n",
                status.language,
                artifact.path.display(),
                short_sha(&artifact.sha256),
                artifact.source,
                artifact.installed_at_unix
            ))?;
        }
    }
    Ok(())
}
