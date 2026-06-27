use std::{
    env,
    io::{self, IsTerminal},
};

use mark_core::MarkError;

use crate::{CliResult, args::PagerArgs, write_stdout_bytes};

mod env_state;
mod input;
mod patch;
mod plain;
mod static_diff;
mod terminal;

use env_state::PagerEnv;
use input::{
    PagerAction, PagerInput, StreamingPagerAction, read_pager_input, static_pager_color_enabled,
};
use plain::{page_plain_text, page_plain_text_stream, stream_to_stdout};
use static_diff::{run_interactive_diff, write_static_diff};
use terminal::controlling_terminal_available;

const PAGER_CLASSIFICATION_LIMIT: usize = 128 * 1024;
const STREAM_BUFFER_SIZE: usize = 8192;

pub(crate) fn pager(args: PagerArgs) -> CliResult<()> {
    if io::stdin().is_terminal() {
        return Err(MarkError::Usage(
            "mark pager reads diff text from stdin; use `git diff | mark pager`, configure `git config --global core.pager \"mark pager\"`, or run `mark` for the current worktree"
                .to_owned(),
        )
        .into());
    }

    let env = PagerEnv::current();
    let stdout_tty = io::stdout().is_terminal();
    let static_color =
        static_pager_color_enabled(stdout_tty, &env, env::var_os("NO_COLOR").is_some());
    let has_controlling_terminal = controlling_terminal_available();
    let mut stdin = io::stdin().lock();
    match read_pager_input(&mut stdin, stdout_tty, &env, has_controlling_terminal)? {
        PagerInput::Buffered { input, action } => match action {
            PagerAction::Passthrough => write_stdout_bytes(&input),
            PagerAction::PlainTextPager => page_plain_text(&input),
            PagerAction::StaticDiff => write_static_diff(&input, &args, static_color),
            PagerAction::InteractiveDiff => run_interactive_diff(input, &args, static_color),
        },
        PagerInput::Streaming { prefix, action } => match action {
            StreamingPagerAction::Passthrough => stream_to_stdout(&prefix, &mut stdin),
            StreamingPagerAction::PlainTextPager => page_plain_text_stream(&prefix, &mut stdin),
        },
    }
}

#[cfg(test)]
mod tests;
