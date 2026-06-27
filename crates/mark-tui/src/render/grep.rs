mod highlight;
mod mouse;
mod target;
pub(crate) mod types;

pub(crate) use highlight::highlighted_grep_text_line;
pub(crate) use mouse::highlighted_mouse_diff_content_line;
pub(crate) use target::{
    diff_line_grep_highlight_text, grep_highlight_target_for_columns,
    grep_highlight_targets_for_row, scrolled_text_byte_start,
    split_diff_line_grep_highlight_target, unified_content_start_column,
};
