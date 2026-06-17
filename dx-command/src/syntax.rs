use std::path::PathBuf;

use crate::{
    SyntaxAddOptions, SyntaxAddResult, SyntaxAvailableFilter, SyntaxCleanResult,
    SyntaxDoctorReport, SyntaxLanguageStatus, SyntaxRemoveResult, SyntaxUpdateResult,
};
use dx_core::DxResult;

pub fn syntax_add(languages: &[String]) -> DxResult<SyntaxAddResult> {
    dx_syntax::add_languages(languages)
}

pub fn syntax_add_with_options(
    languages: &[String],
    options: SyntaxAddOptions,
) -> DxResult<SyntaxAddResult> {
    dx_syntax::add_languages_with_options(languages, options)
}

pub fn syntax_update(languages: &[String], all: bool) -> DxResult<SyntaxUpdateResult> {
    dx_syntax::update_languages(languages, all)
}

pub fn syntax_remove(languages: &[String]) -> DxResult<SyntaxRemoveResult> {
    dx_syntax::remove_languages(languages)
}

pub fn syntax_statuses() -> DxResult<Vec<SyntaxLanguageStatus>> {
    dx_syntax::language_statuses()
}

pub fn syntax_available_languages(filter: SyntaxAvailableFilter) -> DxResult<Vec<String>> {
    dx_syntax::available_languages(filter)
}

pub fn syntax_clean_cache() -> DxResult<SyntaxCleanResult> {
    dx_syntax::clean_cache()
}

pub fn syntax_cache_dir() -> DxResult<String> {
    dx_syntax::cache_dir()
}

pub fn syntax_config_path() -> DxResult<PathBuf> {
    dx_syntax::config_path()
}

pub fn syntax_settings_path() -> DxResult<PathBuf> {
    dx_syntax::settings_path()
}

pub fn syntax_colorscheme_dir() -> DxResult<PathBuf> {
    dx_syntax::colorscheme_dir()
}

pub fn syntax_queries_dir() -> DxResult<PathBuf> {
    dx_syntax::queries_dir()
}

pub fn syntax_parsers_dir() -> DxResult<PathBuf> {
    dx_syntax::parsers_dir()
}

pub fn syntax_doctor() -> DxResult<SyntaxDoctorReport> {
    dx_syntax::doctor()
}
