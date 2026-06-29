use std::{
    io::{BufWriter, Write},
    sync::Arc,
};

use crate::{
    CliResult,
    args::{PagerArgs, PagerLayoutArg},
    write_stderr, write_stdout_bytes, write_stdout_io,
};

use super::{
    patch::{normalized_patch_input, split_patch_prelude},
    terminal::{attach_controlling_terminal_to_stdin, sanitized_terminal_bytes},
};

const DEFAULT_STATIC_WIDTH: usize = 120;
const MIN_STATIC_WIDTH: usize = 20;

pub(super) fn write_static_diff(input: &[u8], args: &PagerArgs, color: bool) -> CliResult<()> {
    let patch = normalized_patch_input(input);
    let (prelude, patch) = split_patch_prelude(&patch);
    let options = patch_options(patch.to_vec());
    let changeset = match mark_diff::load_review_ref(&options) {
        Ok(changeset) => changeset,
        Err(error) => {
            write_stderr(format_args!(
                "mark: static pager render failed; falling back to raw diff: {error}\n"
            ))?;
            return write_stdout_bytes(&sanitized_terminal_bytes(input));
        }
    };
    if changeset.files.is_empty() {
        return write_stdout_bytes(&sanitized_terminal_bytes(input));
    }
    let pager_options = static_pager_options(args, color);
    let prelude = sanitized_terminal_bytes(prelude);
    write_stdout_io(|stdout| {
        let mut stdout = BufWriter::with_capacity(1024 * 1024, stdout);
        stdout.write_all(&prelude)?;
        mark_tui::render_static_changeset_to_writer(
            options,
            changeset,
            pager_options,
            &mut stdout,
        )?;
        stdout.flush()
    })?;
    Ok(())
}

#[cfg(test)]
pub(super) fn static_diff_output(
    input: &[u8],
    args: &PagerArgs,
    color: bool,
) -> CliResult<Vec<u8>> {
    let patch = normalized_patch_input(input);
    let (prelude, patch) = split_patch_prelude(&patch);
    let options = patch_options(patch.to_vec());
    let rendered = match mark_tui::render_static_pager(options, static_pager_options(args, color)) {
        Ok(rendered) => rendered,
        Err(error) => {
            write_stderr(format_args!(
                "mark: static pager render failed; falling back to raw diff: {error}\n"
            ))?;
            String::new()
        }
    };
    if rendered.is_empty() {
        let fallback = sanitized_terminal_bytes(input);
        Ok(fallback)
    } else {
        let mut output = sanitized_terminal_bytes(prelude);
        output.extend_from_slice(rendered.as_bytes());
        Ok(output)
    }
}

fn static_pager_options(args: &PagerArgs, color: bool) -> mark_tui::StaticPagerOptions {
    mark_tui::StaticPagerOptions {
        width: static_terminal_width(),
        layout: args.layout.into(),
        color,
        syntax: !args.no_syntax,
        ..mark_tui::StaticPagerOptions::default()
    }
}

pub(super) fn run_interactive_diff(
    input: Vec<u8>,
    args: &PagerArgs,
    static_color: bool,
) -> CliResult<()> {
    let _stdin_override = match attach_controlling_terminal_to_stdin() {
        Ok(guard) => guard,
        Err(_) => return write_static_diff(&input, args, static_color),
    };
    mark_tui::run_diff_with_options(
        patch_options(normalized_patch_input(&input)),
        mark_tui::DiffRunOptions {
            live_updates: false,
            syntax_enabled: !args.no_syntax,
        },
    )?;
    Ok(())
}

impl From<PagerLayoutArg> for mark_tui::StaticPagerLayout {
    fn from(layout: PagerLayoutArg) -> Self {
        match layout {
            PagerLayoutArg::Auto => Self::Auto,
            PagerLayoutArg::Split => Self::Split,
            PagerLayoutArg::Unified => Self::Unified,
        }
    }
}

fn static_terminal_width() -> usize {
    crossterm::terminal::size()
        .ok()
        .map(|(columns, _)| usize::from(columns))
        .filter(|columns| *columns > 0)
        .unwrap_or(DEFAULT_STATIC_WIDTH)
        .max(MIN_STATIC_WIDTH)
}

fn patch_options(patch: Vec<u8>) -> mark_command::DiffOptions {
    mark_command::DiffOptions {
        repo: None,
        source: mark_command::DiffSource::Patch(mark_command::PatchSource::Stdin(Arc::from(
            patch.into_boxed_slice(),
        ))),
        local_untracked: mark_command::UntrackedMode::Exclude,
        output: mark_command::DiffOutput::Patch,
    }
}
