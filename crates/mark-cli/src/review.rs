use std::io::{self, IsTerminal};

use mark_core::MarkError;

use crate::CliResult;

pub(crate) struct ReviewRequest {
    pub(crate) options: mark_command::DiffOptions,
    pub(crate) live_updates: bool,
    pub(crate) syntax_enabled: bool,
}

pub(crate) fn run_review(request: ReviewRequest) -> CliResult<()> {
    if io::stdout().is_terminal() && !request.options.stat {
        mark_tui::run_diff_with_live_updates_and_syntax(
            request.options,
            request.live_updates,
            request.syntax_enabled,
        )?;
        Ok(())
    } else {
        stream_diff_to_stdout(request.options)
    }
}

fn stream_diff_to_stdout(options: mark_command::DiffOptions) -> CliResult<()> {
    match mark_command::diff_to_writer(options, io::stdout().lock()) {
        Ok(()) => Ok(()),
        Err(MarkError::Io(error)) if error.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        Err(error) => Err(error.into()),
    }
}
