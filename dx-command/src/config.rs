use std::path::PathBuf;

use dx_core::DxResult;

pub fn config_path() -> DxResult<PathBuf> {
    dx_syntax::settings_path()
}
