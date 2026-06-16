use std::{fs, path::PathBuf};

use crate::{
    COLORSCHEME_DIR, CONFIG_DIR, CONFIG_FILE, LEGACY_SETTINGS_FILE, SETTINGS_FILE, SyntaxSettings,
    config_home, parse_settings,
};
use dx_core::{DxError, DxResult};

pub fn config_path() -> DxResult<PathBuf> {
    config_home().map(|path| path.join(CONFIG_DIR).join(CONFIG_FILE))
}

pub fn settings_path() -> DxResult<PathBuf> {
    config_home().map(|path| path.join(CONFIG_DIR).join(SETTINGS_FILE))
}

pub(crate) fn legacy_settings_path() -> DxResult<PathBuf> {
    config_home().map(|path| path.join(CONFIG_DIR).join(LEGACY_SETTINGS_FILE))
}

pub fn colorscheme_dir() -> DxResult<PathBuf> {
    config_home().map(|path| path.join(CONFIG_DIR).join(COLORSCHEME_DIR))
}

pub fn load_settings() -> DxResult<SyntaxSettings> {
    let mut path = settings_path()?;
    if !path.exists() {
        let legacy_path = legacy_settings_path()?;
        if legacy_path.exists() {
            path = legacy_path;
        }
    }
    if !path.exists() {
        return Ok(SyntaxSettings::default());
    }

    let contents = fs::read_to_string(&path)?;
    parse_settings(&contents)
        .map_err(|error| DxError::Usage(format!("failed to parse {}: {error}", path.display())))
}

pub fn cache_dir() -> DxResult<String> {
    tree_sitter_language_pack::cache_dir()
        .map_err(|error| DxError::Usage(format!("failed to resolve tree-sitter cache: {error}")))
}
