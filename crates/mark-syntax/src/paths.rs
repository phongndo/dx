use std::{fs, path::PathBuf};

use crate::{
    COLORSCHEME_DIR, CONFIG_DIR, CONFIG_FILE, LEGACY_SETTINGS_FILE, PARSER_DIR, QUERY_DIR,
    SETTINGS_FILE, SyntaxSettings, config_home, parse_settings,
};
use mark_core::{MarkError, MarkResult};

pub fn config_path() -> MarkResult<PathBuf> {
    config_home().map(|path| path.join(CONFIG_DIR).join(CONFIG_FILE))
}

pub fn settings_path() -> MarkResult<PathBuf> {
    config_home().map(|path| path.join(CONFIG_DIR).join(SETTINGS_FILE))
}

pub fn settings_write_path() -> MarkResult<PathBuf> {
    Ok(settings_write_path_from_paths(
        settings_path()?,
        legacy_settings_path()?,
    ))
}

pub(crate) fn legacy_settings_path() -> MarkResult<PathBuf> {
    config_home().map(|path| path.join(CONFIG_DIR).join(LEGACY_SETTINGS_FILE))
}

pub(crate) fn settings_write_path_from_paths(
    settings_path: PathBuf,
    legacy_settings_path: PathBuf,
) -> PathBuf {
    if !settings_path.exists() && legacy_settings_path.exists() {
        legacy_settings_path
    } else {
        settings_path
    }
}

pub fn colorscheme_dir() -> MarkResult<PathBuf> {
    config_home().map(|path| path.join(CONFIG_DIR).join(COLORSCHEME_DIR))
}

pub fn queries_dir() -> MarkResult<PathBuf> {
    config_home().map(|path| path.join(CONFIG_DIR).join(QUERY_DIR))
}

pub fn parsers_dir() -> MarkResult<PathBuf> {
    config_home().map(|path| path.join(CONFIG_DIR).join(PARSER_DIR))
}

pub fn load_settings() -> MarkResult<SyntaxSettings> {
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
        .map_err(|error| MarkError::Usage(format!("failed to parse {}: {error}", path.display())))
}

pub fn cache_dir() -> MarkResult<String> {
    tree_sitter_language_pack::cache_dir()
        .map_err(|error| MarkError::Usage(format!("failed to resolve tree-sitter cache: {error}")))
}
