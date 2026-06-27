use std::{ffi::OsString, io::Write};

use crate::args::PagerLayoutArg;

use super::{
    input::pager_action,
    patch::{looks_like_patch_input, normalized_patch_input, split_patch_prelude},
    plain::{DEFAULT_TEXT_PAGER, StreamFallback, resolve_text_pager_command, stream_to_pager},
    static_diff::static_diff_output,
    terminal::{sanitized_terminal_bytes, strip_terminal_escapes},
    *,
};

mod patch;
mod routing;
mod streaming;
mod terminal;

fn env(term: Option<&str>, lv: Option<&str>, git_pager: Option<&str>, lazygit: bool) -> PagerEnv {
    PagerEnv {
        term: term.map(OsString::from),
        lv: lv.map(OsString::from),
        git_pager: git_pager.map(OsString::from),
        has_lazygit_env: lazygit,
    }
}

struct FailingWriter {
    bytes_until_error: usize,
}

impl FailingWriter {
    fn new(bytes_until_error: usize) -> Self {
        Self { bytes_until_error }
    }
}

impl Write for FailingWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        if self.bytes_until_error == 0 {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "pager stdin closed",
            ));
        }

        let bytes_written = self.bytes_until_error.min(buffer.len());
        self.bytes_until_error -= bytes_written;
        Ok(bytes_written)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
