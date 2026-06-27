use std::{io, io::Read};

use super::{
    PAGER_CLASSIFICATION_LIMIT, STREAM_BUFFER_SIZE,
    env_state::PagerEnv,
    patch::{looks_like_patch_input, patch_input_has_prelude},
};

#[derive(Debug, PartialEq, Eq)]
pub(super) enum PagerInput {
    Buffered {
        input: Vec<u8>,
        action: PagerAction,
    },
    Streaming {
        prefix: Vec<u8>,
        action: StreamingPagerAction,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PagerAction {
    Passthrough,
    PlainTextPager,
    StaticDiff,
    InteractiveDiff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StreamingPagerAction {
    Passthrough,
    PlainTextPager,
}

pub(super) fn read_pager_input<R: Read>(
    reader: &mut R,
    stdout_tty: bool,
    env: &PagerEnv,
    has_controlling_terminal: bool,
) -> io::Result<PagerInput> {
    if let Some(action) = input_independent_streaming_action(stdout_tty, env) {
        return Ok(PagerInput::Streaming {
            prefix: Vec::new(),
            action,
        });
    }

    let mut input = Vec::new();
    let mut buffer = [0; STREAM_BUFFER_SIZE];
    loop {
        if looks_like_patch_input(&input) {
            reader.read_to_end(&mut input)?;
            return Ok(PagerInput::Buffered {
                action: pager_action(&input, stdout_tty, env, has_controlling_terminal),
                input,
            });
        }

        if input.len() >= PAGER_CLASSIFICATION_LIMIT {
            // Git does not tell core.pager which command produced stdin. Once a
            // bounded prefix has no parseable diff, switch to streaming so
            // large non-diff commands like `git log` can be quit early.
            return Ok(PagerInput::Streaming {
                prefix: input,
                action: non_diff_streaming_action(stdout_tty, env),
            });
        }

        let read_limit = (PAGER_CLASSIFICATION_LIMIT - input.len()).min(buffer.len());
        let bytes_read = reader.read(&mut buffer[..read_limit])?;
        if bytes_read == 0 {
            return Ok(PagerInput::Buffered {
                action: pager_action(&input, stdout_tty, env, has_controlling_terminal),
                input,
            });
        }
        input.extend_from_slice(&buffer[..bytes_read]);
    }
}

fn input_independent_streaming_action(
    stdout_tty: bool,
    env: &PagerEnv,
) -> Option<StreamingPagerAction> {
    if !env.is_captured_pager_host() && (!stdout_tty || env.term_is_dumb()) {
        Some(StreamingPagerAction::Passthrough)
    } else {
        None
    }
}

pub(super) fn pager_action(
    input: &[u8],
    stdout_tty: bool,
    env: &PagerEnv,
    has_controlling_terminal: bool,
) -> PagerAction {
    if input.is_empty() {
        return PagerAction::Passthrough;
    }

    if !looks_like_patch_input(input) {
        return non_diff_pager_action(stdout_tty, env);
    }

    if env.is_captured_pager_host() {
        return PagerAction::StaticDiff;
    }

    if !stdout_tty || env.term_is_dumb() {
        return PagerAction::Passthrough;
    }

    if patch_input_has_prelude(input) {
        return PagerAction::StaticDiff;
    }

    if !has_controlling_terminal {
        return PagerAction::StaticDiff;
    }

    PagerAction::InteractiveDiff
}

fn non_diff_pager_action(stdout_tty: bool, env: &PagerEnv) -> PagerAction {
    match non_diff_streaming_action(stdout_tty, env) {
        StreamingPagerAction::Passthrough => PagerAction::Passthrough,
        StreamingPagerAction::PlainTextPager => PagerAction::PlainTextPager,
    }
}

fn non_diff_streaming_action(stdout_tty: bool, env: &PagerEnv) -> StreamingPagerAction {
    if env.term_is_dumb() || !stdout_tty {
        StreamingPagerAction::Passthrough
    } else {
        StreamingPagerAction::PlainTextPager
    }
}

pub(super) fn static_pager_color_enabled(stdout_tty: bool, env: &PagerEnv, no_color: bool) -> bool {
    !no_color && (stdout_tty || env.is_captured_pager_host())
}
