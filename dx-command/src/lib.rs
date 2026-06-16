mod diff;
mod syntax;

pub use dx_diff::{DiffOptions, DiffScope, DiffSource, PatchSource};
pub use dx_syntax::{
    SyntaxAddResult, SyntaxAvailableFilter, SyntaxCleanResult, SyntaxDoctorReport,
    SyntaxLanguageStatus, SyntaxLimits, SyntaxMode, SyntaxRemoveResult, SyntaxSettings,
    SyntaxThemeConfig, SyntaxThemeSource, SyntaxUpdateResult,
};

pub use diff::{diff, diff_bytes, diff_to_writer, github_pr_diff_options};
pub use syntax::{
    syntax_add, syntax_available_languages, syntax_cache_dir, syntax_clean_cache,
    syntax_colorscheme_dir, syntax_config_path, syntax_doctor, syntax_remove, syntax_settings_path,
    syntax_statuses, syntax_update,
};
