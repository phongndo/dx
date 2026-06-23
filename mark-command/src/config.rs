use std::path::PathBuf;

use mark_core::MarkResult;

pub fn config_path() -> MarkResult<PathBuf> {
    mark_syntax::settings_path()
}
