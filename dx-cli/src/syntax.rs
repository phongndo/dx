use std::{
    env,
    io::{self, IsTerminal, Read},
    path::{Path, PathBuf},
    sync::Arc,
};

use crossterm::terminal as crossterm_terminal;
use dx_core::{DxError, DxResult};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::{
    CliResult,
    args::{DiffArgs, SyntaxAvailableArgs, SyntaxCommand},
    write_stdout,
};

pub(crate) fn syntax(command: SyntaxCommand) -> CliResult<()> {
    match command {
        SyntaxCommand::Add(args) => {
            let result = dx_command::syntax_add_with_options(
                &args.languages,
                dx_command::SyntaxAddOptions {
                    parser: args.parser,
                    query: args.query,
                    extensions: args.extensions,
                    filenames: args.filenames,
                },
            )?;
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
            write_stdout(format_args!(
                "queries     {}\n",
                dx_command::syntax_queries_dir()?.display()
            ))?;
            write_stdout(format_args!(
                "parsers     {}\n",
                dx_command::syntax_parsers_dir()?.display()
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

pub(crate) fn print_syntax_update_result(result: &dx_command::SyntaxUpdateResult) -> CliResult<()> {
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

pub(crate) fn render_syntax_statuses(
    statuses: &[dx_command::SyntaxLanguageStatus],
    color: bool,
    glyphs: ListGlyphs,
    terminal_width: Option<usize>,
) -> String {
    let headers = ["language", "status", "source", "version"];
    let rows = statuses
        .iter()
        .map(|status| {
            [
                status.language.clone(),
                syntax_status_label(status, glyphs).to_owned(),
                syntax_source_label(status).to_owned(),
                syntax_version_label(status).to_owned(),
            ]
        })
        .collect::<Vec<_>>();
    let min_widths = [6, 4, 3, 1];
    let mut widths = headers
        .iter()
        .enumerate()
        .map(|(index, header)| {
            rows.iter()
                .map(|row| display_width(&row[index]))
                .chain([display_width(header), min_widths[index]])
                .max()
                .expect("width candidates should not be empty")
        })
        .collect::<Vec<_>>();

    shrink_syntax_columns(&mut widths, min_widths, terminal_width);

    let mut output = String::new();
    for (index, header) in headers.iter().enumerate() {
        if index > 0 {
            output.push(' ');
        }
        output.push_str(&styled_cell(header, widths[index], StyleColor::Cyan, color));
    }
    output.push('\n');

    for (status, row) in statuses.iter().zip(rows) {
        for (index, value) in row.iter().enumerate() {
            if index > 0 {
                output.push(' ');
            }
            let value = truncate_middle(value, widths[index], glyphs);
            let color_for_cell = match index {
                0 => StyleColor::Magenta,
                1 => syntax_status_color(status),
                _ => StyleColor::White,
            };
            if index == 1 {
                output.push_str(&styled_centered_cell(
                    &value,
                    widths[index],
                    color_for_cell,
                    color,
                ));
            } else {
                output.push_str(&styled_cell(&value, widths[index], color_for_cell, color));
            }
        }
        output.push('\n');
    }

    output
}

pub(crate) fn shrink_syntax_columns(
    widths: &mut [usize],
    min_widths: [usize; 4],
    terminal_width: Option<usize>,
) {
    let Some(terminal_width) = terminal_width else {
        return;
    };
    while list_row_width(widths) > terminal_width {
        let Some(index) = widths
            .iter()
            .enumerate()
            .filter(|(index, width)| **width > min_widths[*index])
            .max_by_key(|(_, width)| **width)
            .map(|(index, _)| index)
        else {
            break;
        };
        widths[index] -= 1;
    }
}

pub(crate) fn syntax_status_label(
    status: &dx_command::SyntaxLanguageStatus,
    glyphs: ListGlyphs,
) -> &'static str {
    match syntax_status_kind(status) {
        SyntaxStatusKind::Ready => glyphs.clean,
        SyntaxStatusKind::Warning => glyphs.dirty,
        SyntaxStatusKind::Error => glyphs.unknown,
        SyntaxStatusKind::Disabled => "-",
    }
}

pub(crate) fn syntax_status_color(status: &dx_command::SyntaxLanguageStatus) -> StyleColor {
    match syntax_status_kind(status) {
        SyntaxStatusKind::Ready => StyleColor::Green,
        SyntaxStatusKind::Warning => StyleColor::Yellow,
        SyntaxStatusKind::Error => StyleColor::Red,
        SyntaxStatusKind::Disabled => StyleColor::White,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SyntaxStatusKind {
    Ready,
    Warning,
    Error,
    Disabled,
}

pub(crate) fn syntax_status_kind(status: &dx_command::SyntaxLanguageStatus) -> SyntaxStatusKind {
    if !status.enabled {
        SyntaxStatusKind::Disabled
    } else if !status.installed || !status.trusted {
        SyntaxStatusKind::Error
    } else if !status.has_highlights {
        SyntaxStatusKind::Warning
    } else {
        SyntaxStatusKind::Ready
    }
}

pub(crate) fn syntax_source_label(status: &dx_command::SyntaxLanguageStatus) -> &'static str {
    if status.source.as_deref() == Some("bundled") {
        "bundled"
    } else if status.source.as_deref() == Some("custom") {
        "custom"
    } else if status.artifact.is_some() {
        "cache"
    } else {
        "-"
    }
}

pub(crate) fn syntax_version_label(status: &dx_command::SyntaxLanguageStatus) -> &str {
    status.version.as_deref().unwrap_or("-")
}

pub(crate) fn short_sha(sha: &str) -> &str {
    sha.get(..12).unwrap_or(sha)
}

#[derive(Clone, Copy)]
pub(crate) struct ListGlyphs {
    pub(crate) clean: &'static str,
    pub(crate) dirty: &'static str,
    pub(crate) unknown: &'static str,
    pub(crate) ellipsis: &'static str,
}

pub(crate) fn list_glyphs(unicode: bool) -> ListGlyphs {
    if unicode {
        ListGlyphs {
            clean: "✓",
            dirty: "!",
            unknown: "?",
            ellipsis: "…",
        }
    } else {
        ListGlyphs {
            clean: "ok",
            dirty: "!",
            unknown: "?",
            ellipsis: "...",
        }
    }
}

pub(crate) fn ascii_output_requested() -> bool {
    env::var_os("DX_ASCII").is_some()
}

pub(crate) fn terminal_width() -> Option<usize> {
    crossterm_terminal::size()
        .ok()
        .map(|(columns, _)| usize::from(columns))
        .filter(|columns| *columns > 0)
}

pub(crate) fn list_row_width(widths: &[usize]) -> usize {
    widths.iter().sum::<usize>() + widths.len().saturating_sub(1)
}

pub(crate) fn display_width(value: &str) -> usize {
    UnicodeWidthStr::width(value)
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum StyleColor {
    Green,
    Cyan,
    Magenta,
    Red,
    Yellow,
    White,
}

pub(crate) fn styled_cell(value: &str, width: usize, color: StyleColor, enabled: bool) -> String {
    styled(&plain_cell(value, width), color, enabled)
}

pub(crate) fn styled_centered_cell(
    value: &str,
    width: usize,
    color: StyleColor,
    enabled: bool,
) -> String {
    styled(&plain_centered_cell(value, width), color, enabled)
}

pub(crate) fn plain_cell(value: &str, width: usize) -> String {
    format!(
        "{value}{}",
        " ".repeat(width.saturating_sub(display_width(value)))
    )
}

pub(crate) fn plain_centered_cell(value: &str, width: usize) -> String {
    let padding = width.saturating_sub(display_width(value));
    let left = padding / 2;
    let right = padding - left;
    format!("{}{}{}", " ".repeat(left), value, " ".repeat(right))
}

pub(crate) fn truncate_middle(value: &str, width: usize, glyphs: ListGlyphs) -> String {
    if display_width(value) <= width {
        return value.to_owned();
    }
    if width == 0 {
        return String::new();
    }

    let ellipsis_width = display_width(glyphs.ellipsis);
    if width <= ellipsis_width {
        return glyphs.ellipsis.chars().take(width).collect();
    }

    let available = width - ellipsis_width;
    let prefix_width = available / 2;
    let suffix_width = available - prefix_width;
    let prefix = take_display_width(value, prefix_width);
    let suffix = take_display_width_from_end(value, suffix_width);

    format!("{prefix}{}{suffix}", glyphs.ellipsis)
}

pub(crate) fn take_display_width(value: &str, width: usize) -> String {
    let mut output = String::new();
    let mut used_width = 0;
    for character in value.chars() {
        let character_width = character.width().unwrap_or(0);
        if used_width + character_width > width {
            break;
        }
        used_width += character_width;
        output.push(character);
    }
    output
}

pub(crate) fn take_display_width_from_end(value: &str, width: usize) -> String {
    let mut output = Vec::new();
    let mut used_width = 0;
    for character in value.chars().rev() {
        let character_width = character.width().unwrap_or(0);
        if used_width + character_width > width {
            break;
        }
        used_width += character_width;
        output.push(character);
    }
    output.into_iter().rev().collect()
}

pub(crate) fn styled(value: &str, color: StyleColor, enabled: bool) -> String {
    if !enabled {
        return value.to_owned();
    }

    let code = match color {
        StyleColor::Green => "32",
        StyleColor::Cyan => "36",
        StyleColor::Magenta => "35",
        StyleColor::Red => "31",
        StyleColor::Yellow => "33",
        StyleColor::White => "37",
    };

    format!("\x1b[{code}m{value}\x1b[0m")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(
        language: &str,
        enabled: bool,
        installed: bool,
        trusted: bool,
        has_highlights: bool,
    ) -> dx_command::SyntaxLanguageStatus {
        dx_command::SyntaxLanguageStatus {
            language: language.to_owned(),
            enabled,
            installed,
            trusted,
            has_highlights,
            version: installed.then(|| "1.9.0-rc.18".to_owned()),
            artifact: None,
            source: installed.then(|| "bundled".to_owned()),
        }
    }

    #[test]
    fn syntax_status_output_uses_compact_table() {
        let output = render_syntax_statuses(
            &[
                status("rust", true, true, true, true),
                status("typescript", true, true, true, false),
                status("elixir", false, true, true, true),
            ],
            false,
            list_glyphs(false),
            None,
        );

        assert!(output.contains("language"));
        assert!(output.contains("status"));
        assert!(output.contains("version"));
        let headers = output
            .lines()
            .next()
            .expect("header should render")
            .split_whitespace()
            .collect::<Vec<_>>();
        assert_eq!(headers, ["language", "status", "source", "version"]);
        assert!(output.contains("rust"));
        assert!(output.contains("ok"));
        assert!(output.contains("typescript"));
        assert!(output.contains("!"));
        assert!(output.contains("elixir"));
        assert!(output.contains("-"));
        assert!(!output.contains("enabled"));
        assert!(!output.contains("syntax"));
        assert!(!output.contains("trusted"));
    }

    #[test]
    fn syntax_status_output_centers_unicode_status() {
        let output = render_syntax_statuses(
            &[status("rust", true, true, true, true)],
            false,
            list_glyphs(true),
            None,
        );

        let rust_line = output
            .lines()
            .find(|line| line.starts_with("rust"))
            .expect("rust status row should render");

        assert!(rust_line.contains("  ✓   "));
    }

    #[test]
    fn syntax_status_output_truncates_to_terminal_width() {
        let output = render_syntax_statuses(
            &[status("very-long-language-name", true, true, true, true)],
            false,
            list_glyphs(false),
            Some(31),
        );

        for line in output.lines() {
            assert!(display_width(line) <= 31, "line too wide: {line}");
        }
    }
}
