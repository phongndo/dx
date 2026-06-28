mod annotation_menu;
mod diff;
mod help;
mod options;
mod refs;

pub(crate) use annotation_menu::{
    annotation_menu_area, annotation_menu_list_visible_rows, draw_annotation_menu,
};
pub(crate) use diff::{
    diff_comparison_label, diff_menu_area, diff_menu_block, diff_selector_text,
    diff_selector_width, draw_diff_menu, draw_review_input, review_input_area,
};
pub(crate) use help::{draw_help_menu, help_menu_list_visible_rows};
#[cfg(test)]
pub(crate) use help::{
    help_menu_bg, help_menu_content_rows, help_menu_lines, help_menu_row_line, help_menu_row_spans,
    help_menu_title_color,
};
pub(crate) use options::{
    color_scheme_picker_area, color_scheme_picker_block, draw_color_scheme_picker,
    draw_options_menu, options_menu_area, options_menu_block,
};
pub(crate) use refs::{
    branch_menu_area, branch_menu_block, branch_menu_list_visible_rows, branch_menu_width,
    color_scheme_picker_list_visible_rows, commit_menu_area, commit_menu_block,
    commit_menu_list_visible_rows, draw_branch_menu, draw_commit_menu,
};
