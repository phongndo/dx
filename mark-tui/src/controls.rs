mod diff_choice;
mod git_queries;
mod layout;
mod matcher;
mod refs;

pub(crate) use diff_choice::{DiffChoice, DiffFilterKind, is_review_options};
pub(crate) use layout::{
    CrosstermTerminal, DiffLayoutMode, INPUT_CURSOR, default_layout_for_width,
};
#[cfg(test)]
pub(crate) use matcher::diff_line_grep_text_matches;
pub(crate) use matcher::{
    TextMatcher, branch_match_score, diff_line_grep_prefix, diff_stats_for_files,
    filtered_file_indices,
};
pub(crate) use refs::{
    BranchMenu, GitCommit, branch_base_from_options, branch_head_from_options, commit_match_score,
    commit_menu_width, commit_short_sha, comparison_branches, comparison_commits,
    current_head_label, default_branch_base, rev_display_label,
};
