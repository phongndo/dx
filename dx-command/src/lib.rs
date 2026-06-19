mod config;
mod diff;
mod syntax;

pub use dx_diff::{
    DiffOptions, DiffScope, DiffSet, DiffSetItem, DiffSource, PatchSource, diffset_from_file,
};
pub use dx_syntax::{
    SyntaxAddOptions, SyntaxAddResult, SyntaxAvailableFilter, SyntaxCleanResult,
    SyntaxDoctorReport, SyntaxLanguageStatus, SyntaxLimits, SyntaxMode, SyntaxRemoveResult,
    SyntaxSettings, SyntaxThemeConfig, SyntaxThemeSource, SyntaxUpdateResult,
    run_validation_child_from_env,
};

pub use config::config_path;
pub use diff::{diff, diff_bytes, diff_to_writer, github_pr_diff_options};
pub use syntax::{
    syntax_add, syntax_add_with_options, syntax_available_languages, syntax_cache_dir,
    syntax_clean_cache, syntax_colorscheme_dir, syntax_config_path, syntax_doctor,
    syntax_parsers_dir, syntax_queries_dir, syntax_remove, syntax_settings_path, syntax_statuses,
    syntax_update,
};
