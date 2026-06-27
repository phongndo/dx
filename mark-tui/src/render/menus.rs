use mark_diff::{DiffOptions, DiffScope, DiffSource};
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    prelude::{Color, Line, Modifier, Span, Style, Text},
    widgets::{Block, BorderType, Clear, Padding, Paragraph},
};
use unicode_width::UnicodeWidthStr;

use crate::{
    app::{DiffApp, OptionsMenuItem, color_scheme_label, option_label},
    controls::{BranchMenu, DiffChoice, GitCommit, commit_short_sha, is_review_options},
    keymap::Keymap,
    render::{
        selector_menu::{
            ScrollableSelectorMenu, SelectorMenuInput, SelectorMenuLinesRequest,
            centered_floating_rect, floating_menu_fits_terminal, floating_menu_max_height,
            floating_menu_max_inner_height, floating_menu_max_width,
            render_scrollable_selector_menu, selector_border_color, selector_count_color,
            selector_disabled_line, selector_entry_line, selector_highlight_color,
            selector_menu_lines, selector_menu_list_rows, selector_menu_outer_height,
            selector_menu_outer_width, selector_menu_rendered_list_rows, selector_prompt_color,
            selector_row_style, selector_scrollbar_needed, selector_title_color,
            selector_width_with_scrollbar, text_with_cursor,
        },
        style::{base_bg, header_bg, input_cursor_style, spans_with_input_cursor},
        text::{fit_padded, fit_with_ellipsis},
    },
    theme::{DiffTheme, HELP_KEY_COLUMN_WIDTH, HELP_MENU_ROWS, HelpMenuKey, HelpMenuRow},
};

const HELP_DESCRIPTION_MIN_WIDTH: usize = 24;

pub(crate) fn branch_menu_list_visible_rows(app: &DiffApp, area: Rect) -> Option<usize> {
    let menu = app.refs.branch_menu_open?;
    if app.refs.comparison_branches.is_empty() {
        return None;
    }

    let pinned_rows = u16::from(app.selected_branch_menu_choice(menu).is_some());
    selector_menu_rendered_list_rows(area, app.filtered_branches().len(), pinned_rows)
}

pub(crate) fn commit_menu_list_visible_rows(app: &DiffApp, area: Rect) -> Option<usize> {
    if !app.refs.commit_menu_open || app.refs.comparison_commits.is_empty() {
        return None;
    }

    let pinned_rows = u16::from(app.selected_commit_menu_choice().is_some());
    selector_menu_rendered_list_rows(area, app.filtered_commits().len(), pinned_rows)
}

pub(crate) fn color_scheme_picker_list_visible_rows(app: &DiffApp, area: Rect) -> Option<usize> {
    if !app.overlays.color_scheme_picker_open {
        return None;
    }

    selector_menu_rendered_list_rows(area, app.filtered_color_schemes().len(), 1)
}

pub(crate) fn draw_diff_menu(frame: &mut Frame<'_>, app: &mut DiffApp, area: Rect) {
    let choices = app.filtered_diff_choices();
    let Some(menu_area) = diff_menu_area(app, area, &choices) else {
        app.set_rendered_diff_menu_area(None);
        return;
    };
    app.set_rendered_diff_menu_area(Some(menu_area));

    let block = diff_menu_block(app.config.theme);
    let inner = block.inner(menu_area);
    let selected = app
        .overlays
        .diff_menu
        .selected
        .min(choices.len().saturating_sub(1));
    let pinned_lines = app
        .selected_diff_menu_choice()
        .map(|choice| {
            selector_disabled_line(
                choice.label(),
                &app.diff_choice_detail(choice),
                inner.width as usize,
                app.config.theme,
            )
        })
        .into_iter()
        .collect();
    let content = selector_menu_lines(
        SelectorMenuLinesRequest {
            input: SelectorMenuInput {
                input: &app.overlays.diff_menu.input,
                input_cursor: app.overlays.diff_menu.input_cursor,
                matched: choices.len(),
                total: app.selectable_diff_choices().len(),
                theme: app.config.theme,
            },
            inner,
            items: &choices,
            scroll: 0,
            selected,
            pinned_lines,
            empty_message: " no matching diff types",
        },
        |_, choice, width, highlighted| {
            selector_entry_line(
                choice.label(),
                &app.diff_choice_detail(*choice),
                width,
                app.config.theme,
                highlighted,
            )
        },
    );

    render_scrollable_selector_menu(
        frame,
        ScrollableSelectorMenu {
            menu_area,
            block,
            inner,
            content,
            theme: app.config.theme,
        },
    );
}

pub(crate) fn diff_menu_area(app: &DiffApp, area: Rect, choices: &[DiffChoice]) -> Option<Rect> {
    if !app.overlays.diff_menu_open
        || !floating_menu_fits_terminal(area)
        || app.diff_menu_choices().is_empty()
    {
        return None;
    }

    let pinned_rows = u16::from(app.selected_diff_menu_choice().is_some());
    let width = floating_menu_max_width(area, diff_menu_floating_width(app, choices));
    let list_cap = selector_menu_list_rows(floating_menu_max_inner_height(area), pinned_rows);
    let list_rows = choices.len().max(1).min(list_cap);
    let height = selector_menu_outer_height(area, list_rows, pinned_rows);
    if width == 0 || height == 0 {
        return None;
    }

    Some(centered_floating_rect(area, width, height))
}

pub(crate) fn diff_menu_block(theme: DiffTheme) -> Block<'static> {
    let bg = base_bg(theme);
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(selector_border_color(theme)).bg(bg))
        .style(Style::default().bg(bg))
        .padding(Padding::horizontal(1))
        .title(Line::from(Span::styled(
            " Diff ",
            Style::default()
                .fg(selector_title_color(theme))
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        )))
        .title_alignment(Alignment::Center)
}

pub(crate) fn draw_review_input(frame: &mut Frame<'_>, app: &mut DiffApp, area: Rect) {
    let Some(menu_area) = review_input_area(app, area) else {
        app.set_rendered_review_input_area(None);
        return;
    };
    app.set_rendered_review_input_area(Some(menu_area));

    let block = review_input_block(app.config.theme);
    let inner = block.inner(menu_area);
    let bg = base_bg(app.config.theme);
    let input = text_with_cursor(&app.overlays.review_input, app.overlays.review_input_cursor);
    let prompt = fit_padded(&format!("> {input}"), inner.width as usize);
    let hint = fit_padded("Review ID for this repo", inner.width as usize);
    let prompt_style = Style::default()
        .fg(selector_prompt_color(app.config.theme))
        .bg(bg);
    let lines = vec![
        Line::from(spans_with_input_cursor(
            &prompt,
            prompt_style,
            input_cursor_style(app.config.theme, bg),
        )),
        Line::from(Span::styled(
            hint,
            Style::default()
                .fg(selector_count_color(app.config.theme))
                .bg(bg),
        )),
    ];

    frame.render_widget(Clear, menu_area);
    frame.render_widget(block, menu_area);
    frame.render_widget(
        Paragraph::new(Text::from(lines)).style(Style::default().bg(bg)),
        inner,
    );
}

pub(crate) fn review_input_area(app: &DiffApp, area: Rect) -> Option<Rect> {
    if !app.overlays.review_input_open || !floating_menu_fits_terminal(area) {
        return None;
    }

    let content_width = app
        .overlays
        .review_input
        .width()
        .saturating_add(6)
        .max("Review ID for this repo".width());
    let width = floating_menu_max_width(area, selector_menu_outer_width(content_width.max(36)));
    let height = floating_menu_max_height(area, 4);
    if width == 0 || height == 0 {
        return None;
    }

    Some(centered_floating_rect(area, width, height))
}

pub(crate) fn review_input_block(theme: DiffTheme) -> Block<'static> {
    let bg = base_bg(theme);
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(selector_border_color(theme)).bg(bg))
        .style(Style::default().bg(bg))
        .padding(Padding::horizontal(1))
        .title(Line::from(Span::styled(
            " Review ",
            Style::default()
                .fg(selector_title_color(theme))
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        )))
        .title_alignment(Alignment::Center)
}

fn diff_menu_floating_width(app: &DiffApp, choices: &[DiffChoice]) -> u16 {
    let input = app.overlays.diff_menu.input.width().saturating_add(12);
    let rows = choices
        .iter()
        .map(|choice| format!(" {}  {} ", choice.label(), app.diff_choice_detail(*choice),).width())
        .max()
        .unwrap_or_else(|| " no matching diff types ".width());
    let selected = app
        .selected_diff_menu_choice()
        .map(|choice| format!(" {}  {} ", choice.label(), app.diff_choice_detail(choice)).width())
        .unwrap_or_default();
    selector_menu_outer_width(rows.max(selected).max(input).max(42))
}

pub(crate) fn draw_options_menu(frame: &mut Frame<'_>, app: &mut DiffApp, area: Rect) {
    if !app.overlays.options_menu_open || !floating_menu_fits_terminal(area) {
        return;
    }

    let items = app.filtered_options_menu_items();
    let list_cap = selector_menu_list_rows(floating_menu_max_inner_height(area), 0);
    let list_rows = items.len().max(1).min(list_cap);
    let width = floating_menu_max_width(
        area,
        selector_width_with_scrollbar(
            options_menu_width(app, &items),
            selector_scrollbar_needed(items.len(), list_rows),
        ),
    );
    let height = selector_menu_outer_height(area, list_rows, 0);
    if width == 0 || height == 0 {
        return;
    }

    let menu_area = centered_floating_rect(area, width, height);
    let block = options_menu_block(app.config.theme);
    let inner = block.inner(menu_area);
    let list_visible_rows = selector_menu_list_rows(inner.height, 0);
    app.ensure_options_menu_selection_visible(list_visible_rows);

    let selected = app
        .overlays
        .options_menu
        .selected
        .min(items.len().saturating_sub(1));
    let content = selector_menu_lines(
        SelectorMenuLinesRequest {
            input: SelectorMenuInput {
                input: &app.overlays.options_menu.input,
                input_cursor: app.overlays.options_menu.input_cursor,
                matched: items.len(),
                total: app.options_menu_items().len(),
                theme: app.config.theme,
            },
            inner,
            items: &items,
            scroll: app.overlays.options_menu.scroll,
            selected,
            pinned_lines: Vec::new(),
            empty_message: " no matching settings",
        },
        |global_index, item, width, _| {
            selector_setting_line(
                option_label(*item),
                &app.option_value(*item),
                width,
                app.config.theme,
                global_index == selected,
            )
        },
    );

    render_scrollable_selector_menu(
        frame,
        ScrollableSelectorMenu {
            menu_area,
            block,
            inner,
            content,
            theme: app.config.theme,
        },
    );
}

pub(crate) fn options_menu_block(theme: DiffTheme) -> Block<'static> {
    let bg = base_bg(theme);
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(selector_border_color(theme)).bg(bg))
        .style(Style::default().bg(bg))
        .padding(Padding::horizontal(1))
        .title(Line::from(Span::styled(
            " Settings ",
            Style::default()
                .fg(selector_title_color(theme))
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        )))
        .title_alignment(Alignment::Center)
}

fn options_menu_width(app: &DiffApp, items: &[OptionsMenuItem]) -> u16 {
    let input = app.overlays.options_menu.input.width().saturating_add(12);
    let rows = items
        .iter()
        .map(|item| format!(" › {}  {} ", option_label(*item), app.option_value(*item)).width())
        .max()
        .unwrap_or_else(|| " no matching settings ".width());
    selector_menu_outer_width(rows.max(input).max(42))
}

fn selector_setting_line(
    label: &str,
    value: &str,
    width: usize,
    theme: DiffTheme,
    highlighted: bool,
) -> Line<'static> {
    let left = format!(" {label}");
    let value_width = value.width();
    let text = if value_width == 0 || width <= value_width.saturating_add(2) {
        format!("{left}  {value}")
    } else {
        let left_width = width.saturating_sub(value_width).saturating_sub(1);
        format!("{} {value}", fit_padded(&left, left_width))
    };

    Line::from(Span::styled(
        fit_padded(&text, width),
        selector_row_style(theme, highlighted),
    ))
}

pub(crate) fn draw_color_scheme_picker(frame: &mut Frame<'_>, app: &mut DiffApp, area: Rect) {
    if !app.overlays.color_scheme_picker_open || !floating_menu_fits_terminal(area) {
        app.overlays.rendered_color_scheme_picker_area = None;
        return;
    }

    let pinned_rows = 1u16;
    let choice_count = app.filtered_color_schemes().len();
    let list_cap = selector_menu_list_rows(floating_menu_max_inner_height(area), pinned_rows);
    let list_rows = choice_count.max(1).min(list_cap);
    let width = floating_menu_max_width(
        area,
        selector_width_with_scrollbar(
            color_scheme_picker_width(app),
            selector_scrollbar_needed(choice_count, list_rows),
        ),
    );
    let height = selector_menu_outer_height(area, list_rows.max(1), pinned_rows);
    if width == 0 || height == 0 {
        app.overlays.rendered_color_scheme_picker_area = None;
        return;
    }

    let picker_area = centered_floating_rect(area, width, height);
    app.overlays.rendered_color_scheme_picker_area = Some(picker_area);
    let block = color_scheme_picker_block(app.config.theme);
    let inner = block.inner(picker_area);
    let choices = app.filtered_color_schemes();
    let remaining_rows = selector_menu_list_rows(inner.height, pinned_rows);
    app.overlays
        .color_scheme_picker
        .ensure_selected_visible(choices.len(), remaining_rows);
    let selected = app.overlays.color_scheme_picker.selected;
    let content = selector_menu_lines(
        SelectorMenuLinesRequest {
            input: SelectorMenuInput {
                input: &app.overlays.color_scheme_picker.input,
                input_cursor: app.overlays.color_scheme_picker.input_cursor,
                matched: choices.len(),
                total: app.selectable_color_schemes().len(),
                theme: app.config.theme,
            },
            inner,
            items: &choices,
            scroll: app.overlays.color_scheme_picker.scroll,
            selected,
            pinned_lines: vec![selector_disabled_line(
                color_scheme_label(app.overlays.options_menu_draft.color_scheme),
                "",
                inner.width as usize,
                app.config.theme,
            )],
            empty_message: " no matching colorscheme",
        },
        |_, choice, width, highlighted| {
            selector_entry_line(
                color_scheme_label(*choice),
                "",
                width,
                app.config.theme,
                highlighted,
            )
        },
    );

    render_scrollable_selector_menu(
        frame,
        ScrollableSelectorMenu {
            menu_area: picker_area,
            block,
            inner,
            content,
            theme: app.config.theme,
        },
    );
}

pub(crate) fn color_scheme_picker_block(theme: DiffTheme) -> Block<'static> {
    let bg = base_bg(theme);
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(selector_border_color(theme)).bg(bg))
        .style(Style::default().bg(bg))
        .padding(Padding::horizontal(1))
        .title(Line::from(Span::styled(
            " Colorscheme ",
            Style::default()
                .fg(selector_title_color(theme))
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        )))
        .title_alignment(Alignment::Center)
}

fn color_scheme_picker_width(app: &DiffApp) -> u16 {
    let input = app
        .overlays
        .color_scheme_picker
        .input
        .width()
        .saturating_add(12);
    let rows = app
        .filtered_color_schemes()
        .iter()
        .map(|choice| format!(" {} ", color_scheme_label(*choice)).width())
        .max()
        .unwrap_or_else(|| " no matching colorscheme ".width());
    let current = format!(
        " {} ",
        color_scheme_label(app.overlays.options_menu_draft.color_scheme)
    )
    .width();
    selector_menu_outer_width(rows.max(current).max(input).max(42))
}

pub(crate) fn draw_branch_menu(frame: &mut Frame<'_>, app: &mut DiffApp, area: Rect) {
    let Some(menu) = app.refs.branch_menu_open else {
        app.set_rendered_branch_menu_area(None);
        return;
    };
    if !floating_menu_fits_terminal(area) || app.refs.comparison_branches.is_empty() {
        app.set_rendered_branch_menu_area(None);
        return;
    }

    let pinned_rows = u16::from(app.selected_branch_menu_choice(menu).is_some());
    let match_count = app.filtered_branches().len();
    let list_cap = selector_menu_list_rows(floating_menu_max_inner_height(area), pinned_rows);
    let list_rows = match_count.max(1).min(list_cap);
    let width = floating_menu_max_width(
        area,
        selector_width_with_scrollbar(
            app.branch_menu_width(),
            selector_scrollbar_needed(match_count, list_rows),
        ),
    );
    let height = selector_menu_outer_height(area, list_rows, pinned_rows);
    if width == 0 || height == 0 {
        app.set_rendered_branch_menu_area(None);
        return;
    }

    let menu_area = centered_floating_rect(area, width, height);
    app.set_rendered_branch_menu_area(Some(menu_area));

    let block = branch_menu_block(app.config.theme, menu);
    let inner = block.inner(menu_area);
    let remaining_rows = selector_menu_list_rows(inner.height, pinned_rows);
    app.ensure_branch_selection_visible_for_rows(remaining_rows);
    let matches = app.filtered_branches();
    let selected = app.refs.branch_menu.selected;
    let pinned_lines = app
        .selected_branch_menu_choice(menu)
        .map(|branch| selector_disabled_line(branch, "", inner.width as usize, app.config.theme))
        .into_iter()
        .collect();
    let content = selector_menu_lines(
        SelectorMenuLinesRequest {
            input: SelectorMenuInput {
                input: &app.refs.branch_menu.input,
                input_cursor: app.refs.branch_menu.input_cursor,
                matched: match_count,
                total: app.selectable_branch_count(menu),
                theme: app.config.theme,
            },
            inner,
            items: &matches,
            scroll: app.refs.branch_menu.scroll,
            selected,
            pinned_lines,
            empty_message: " no matching branches",
        },
        |_, branch, width, highlighted| {
            selector_entry_line(branch, "", width, app.config.theme, highlighted)
        },
    );

    render_scrollable_selector_menu(
        frame,
        ScrollableSelectorMenu {
            menu_area,
            block,
            inner,
            content,
            theme: app.config.theme,
        },
    );
}

pub(crate) fn branch_menu_block(theme: DiffTheme, menu: BranchMenu) -> Block<'static> {
    let bg = base_bg(theme);
    let title = match menu {
        BranchMenu::Head => " head branch ",
        BranchMenu::Base => " base branch ",
    };
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(selector_border_color(theme)).bg(bg))
        .style(Style::default().bg(bg))
        .padding(Padding::horizontal(1))
        .title(Line::from(Span::styled(
            title,
            Style::default()
                .fg(selector_title_color(theme))
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        )))
        .title_alignment(Alignment::Center)
}

pub(crate) fn commit_menu_block(theme: DiffTheme) -> Block<'static> {
    let bg = base_bg(theme);
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(selector_border_color(theme)).bg(bg))
        .style(Style::default().bg(bg))
        .padding(Padding::horizontal(1))
        .title(Line::from(Span::styled(
            " commit ",
            Style::default()
                .fg(selector_title_color(theme))
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        )))
        .title_alignment(Alignment::Center)
}

const COMMIT_MENU_SHA_COL_WIDTH: usize = 8;

fn commit_menu_sha_fg(theme: DiffTheme, highlighted: bool, disabled: bool) -> Color {
    if disabled {
        return theme.syntax.comment.unwrap_or(theme.muted);
    }
    if highlighted {
        return selector_highlight_color(theme);
    }
    theme.syntax.string.unwrap_or(theme.header)
}

fn commit_menu_subject_fg(theme: DiffTheme, highlighted: bool, disabled: bool) -> Color {
    if disabled {
        return theme.muted;
    }
    if highlighted {
        return selector_highlight_color(theme);
    }
    theme.foreground
}

fn commit_menu_row_bg(theme: DiffTheme, highlighted: bool) -> Color {
    if highlighted {
        header_bg(theme)
    } else {
        base_bg(theme)
    }
}

fn commit_menu_row_line(
    commit: &GitCommit,
    width: usize,
    theme: DiffTheme,
    highlighted: bool,
    disabled: bool,
) -> Line<'static> {
    let row_width = width.saturating_sub(2);
    let sha_w = COMMIT_MENU_SHA_COL_WIDTH.min(row_width);
    let gap_w = usize::from(row_width > sha_w);
    let subject_w = row_width.saturating_sub(sha_w).saturating_sub(gap_w);
    let short = commit_short_sha(commit);
    let subject = if subject_w == 0 {
        String::new()
    } else {
        fit_with_ellipsis(&commit.subject, subject_w)
    };
    let bg = commit_menu_row_bg(theme, highlighted && !disabled);
    let bold = highlighted && !disabled;
    let mut sha_style = Style::default()
        .fg(commit_menu_sha_fg(theme, highlighted, disabled))
        .bg(bg);
    let mut subject_style = Style::default()
        .fg(commit_menu_subject_fg(theme, highlighted, disabled))
        .bg(bg);
    if bold {
        sha_style = sha_style.add_modifier(Modifier::BOLD);
        subject_style = subject_style.add_modifier(Modifier::BOLD);
    }
    let gap_style = Style::default().bg(bg);
    let mut spans = vec![Span::styled(" ", gap_style)];
    if sha_w > 0 {
        spans.push(Span::styled(fit_padded(short, sha_w), sha_style));
    }
    if gap_w > 0 {
        spans.push(Span::styled(" ", gap_style));
    }
    if subject_w > 0 {
        spans.push(Span::styled(fit_padded(&subject, subject_w), subject_style));
    }
    Line::from(spans)
}

pub(crate) fn draw_commit_menu(frame: &mut Frame<'_>, app: &mut DiffApp, area: Rect) {
    if !app.refs.commit_menu_open {
        app.set_rendered_commit_menu_area(None);
        return;
    }
    if !floating_menu_fits_terminal(area) || app.refs.comparison_commits.is_empty() {
        app.set_rendered_commit_menu_area(None);
        return;
    }

    let pinned_rows = u16::from(app.selected_commit_menu_choice().is_some());
    let match_count = app.filtered_commits().len();
    let list_cap = selector_menu_list_rows(floating_menu_max_inner_height(area), pinned_rows);
    let list_rows = match_count.min(list_cap);
    let width = floating_menu_max_width(
        area,
        selector_width_with_scrollbar(
            app.commit_menu_width(),
            selector_scrollbar_needed(match_count, list_rows),
        ),
    );
    let height = selector_menu_outer_height(area, list_rows.max(1), pinned_rows);
    if width == 0 || height == 0 {
        app.set_rendered_commit_menu_area(None);
        return;
    }

    let menu_area = centered_floating_rect(area, width, height);
    app.set_rendered_commit_menu_area(Some(menu_area));

    let block = commit_menu_block(app.config.theme);
    let inner = block.inner(menu_area);
    let remaining_rows = selector_menu_list_rows(inner.height, pinned_rows);
    app.ensure_commit_selection_visible_for_rows(remaining_rows);
    let matches = app.filtered_commits();
    let selected = app.refs.commit_menu.selected;
    let pinned_lines = app
        .selected_commit_menu_choice()
        .map(|commit| {
            commit_menu_row_line(commit, inner.width as usize, app.config.theme, false, true)
        })
        .into_iter()
        .collect();
    let content = selector_menu_lines(
        SelectorMenuLinesRequest {
            input: SelectorMenuInput {
                input: &app.refs.commit_menu.input,
                input_cursor: app.refs.commit_menu.input_cursor,
                matched: match_count,
                total: app.selectable_commit_count(),
                theme: app.config.theme,
            },
            inner,
            items: &matches,
            scroll: app.refs.commit_menu.scroll,
            selected,
            pinned_lines,
            empty_message: " no matching commits",
        },
        |_, commit, width, highlighted| {
            commit_menu_row_line(commit, width, app.config.theme, highlighted, false)
        },
    );

    render_scrollable_selector_menu(
        frame,
        ScrollableSelectorMenu {
            menu_area,
            block,
            inner,
            content,
            theme: app.config.theme,
        },
    );
}

pub(crate) fn help_menu_list_visible_rows(app: &DiffApp, area: Rect) -> Option<usize> {
    let layout = help_menu_layout(app, area)?;
    Some(layout.list_visible_rows)
}

struct HelpMenuLayout {
    menu_area: Rect,
    inner: Rect,
    list_visible_rows: usize,
}

fn help_menu_layout(app: &DiffApp, area: Rect) -> Option<HelpMenuLayout> {
    if !app.overlays.help_menu_open || !floating_menu_fits_terminal(area) {
        return None;
    }

    let rows = app.filtered_help_menu_rows();
    let list_cap = selector_menu_list_rows(floating_menu_max_inner_height(area), 0);
    let list_rows = rows.len().max(1).min(list_cap);
    let width = floating_menu_max_width(
        area,
        selector_width_with_scrollbar(
            help_menu_width(app, &rows),
            selector_scrollbar_needed(rows.len(), list_rows),
        ),
    );
    let height = selector_menu_outer_height(area, list_rows, 0);
    if width == 0 || height == 0 {
        return None;
    }

    let menu_area = centered_floating_rect(area, width, height);

    let block = help_menu_block(app.config.theme);
    let inner = block.inner(menu_area);
    const HEADER_LINES: u16 = 2;
    let list_visible_rows = inner.height.saturating_sub(HEADER_LINES) as usize;
    if list_visible_rows == 0 {
        return None;
    }

    Some(HelpMenuLayout {
        menu_area,
        inner,
        list_visible_rows: list_visible_rows.max(1),
    })
}

pub(crate) fn draw_help_menu(frame: &mut Frame<'_>, app: &mut DiffApp, area: Rect) {
    let Some(layout) = help_menu_layout(app, area) else {
        return;
    };

    let rows = app.filtered_help_menu_rows();
    let inner = layout.inner;
    let remaining_rows = layout.list_visible_rows;
    app.overlays.help_menu_visible_rows = remaining_rows;
    app.clamp_help_menu_scroll(remaining_rows);
    let menu_area = layout.menu_area;

    let block = help_menu_block(app.config.theme);
    let content = selector_menu_lines(
        SelectorMenuLinesRequest {
            input: SelectorMenuInput {
                input: &app.overlays.help_menu_input,
                input_cursor: app.overlays.help_menu_input_cursor,
                matched: rows.len(),
                total: HELP_MENU_ROWS.len(),
                theme: app.config.theme,
            },
            inner,
            items: &rows,
            scroll: app.overlays.help_menu_scroll,
            selected: 0,
            pinned_lines: Vec::new(),
            empty_message: " no matching keybindings",
        },
        |_, row, width, _| help_menu_row_line(*row, width, app.config.theme, &app.config.keymap),
    );

    render_scrollable_selector_menu(
        frame,
        ScrollableSelectorMenu {
            menu_area,
            block,
            inner,
            content,
            theme: app.config.theme,
        },
    );
}

pub(crate) fn help_menu_block(theme: DiffTheme) -> Block<'static> {
    let bg = help_menu_bg(theme);
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(selector_border_color(theme)).bg(bg))
        .style(Style::default().bg(bg))
        .padding(Padding::horizontal(1))
        .title(Line::from(Span::styled(
            " Keybindings ",
            Style::default()
                .fg(selector_title_color(theme))
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        )))
        .title_alignment(Alignment::Center)
}

pub(crate) fn help_menu_bg(theme: DiffTheme) -> Color {
    base_bg(theme)
}

#[cfg(test)]
pub(crate) fn help_menu_title_color(theme: DiffTheme) -> Color {
    selector_title_color(theme)
}

pub(crate) fn help_menu_section_color(theme: DiffTheme) -> Color {
    theme.syntax.keyword.unwrap_or(theme.hunk)
}

pub(crate) fn help_menu_key_color(theme: DiffTheme) -> Color {
    theme.syntax.function.unwrap_or(theme.header)
}

pub(crate) fn help_menu_description_color(theme: DiffTheme) -> Color {
    theme.foreground
}

fn help_menu_width(app: &DiffApp, rows: &[HelpMenuRow]) -> u16 {
    let input = app.overlays.help_menu_input.width().saturating_add(12);
    let rows = rows
        .iter()
        .map(|row| help_menu_row_width(*row, &app.config.keymap))
        .max()
        .unwrap_or_else(|| " no matching keybindings ".width());
    selector_menu_outer_width(rows.max(input).max(42))
}

fn help_menu_row_width(row: HelpMenuRow, keymap: &Keymap) -> usize {
    match row {
        HelpMenuRow::Section(section) => format!(" {section}").width(),
        HelpMenuRow::Binding(keys, description) => {
            let key_label = help_menu_key_label(keys, keymap);
            HELP_KEY_COLUMN_WIDTH
                .max(key_label.width().saturating_add(3))
                .saturating_add(description.width())
        }
    }
}

#[cfg(test)]
pub(crate) fn help_menu_lines(
    width: usize,
    height: usize,
    theme: DiffTheme,
    keymap: &Keymap,
) -> Vec<Line<'static>> {
    HELP_MENU_ROWS
        .iter()
        .take(height)
        .map(|row| help_menu_row_line(*row, width, theme, keymap))
        .collect()
}

#[cfg(test)]
pub(crate) fn help_menu_content_rows(width: usize) -> usize {
    let _ = width;
    HELP_MENU_ROWS.len()
}

#[cfg(test)]
pub(crate) fn help_menu_row_spans(
    row: HelpMenuRow,
    width: usize,
    theme: DiffTheme,
    keymap: &Keymap,
) -> Vec<Span<'static>> {
    let bg = help_menu_bg(theme);
    match row {
        HelpMenuRow::Section(section) => vec![Span::styled(
            fit_padded(&format!("  {section}"), width),
            Style::default()
                .fg(help_menu_section_color(theme))
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        )],
        HelpMenuRow::Binding(keys, description) => {
            let key_label = help_menu_key_label(keys, keymap);
            let (key_width, description_width) =
                help_menu_binding_widths(&key_label, description, width);
            vec![
                Span::styled(
                    help_menu_key_text(&key_label, key_width),
                    Style::default()
                        .fg(help_menu_key_color(theme))
                        .bg(bg)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    fit_padded(description, description_width),
                    Style::default()
                        .fg(help_menu_description_color(theme))
                        .bg(bg),
                ),
            ]
        }
    }
}

pub(crate) fn help_menu_row_line(
    row: HelpMenuRow,
    width: usize,
    theme: DiffTheme,
    keymap: &Keymap,
) -> Line<'static> {
    match row {
        HelpMenuRow::Section(section) => Line::from(Span::styled(
            fit_padded(&format!(" {section}"), width),
            Style::default()
                .fg(help_menu_section_color(theme))
                .bg(help_menu_bg(theme))
                .add_modifier(Modifier::BOLD),
        )),
        HelpMenuRow::Binding(keys, description) => {
            let key_label = help_menu_key_label(keys, keymap);
            let (key_width, description_width) =
                help_menu_binding_widths(&key_label, description, width);
            Line::from(vec![
                Span::styled(
                    help_menu_key_text(&key_label, key_width),
                    Style::default()
                        .fg(help_menu_key_color(theme))
                        .bg(help_menu_bg(theme))
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    fit_padded(description, description_width),
                    Style::default()
                        .fg(help_menu_description_color(theme))
                        .bg(help_menu_bg(theme)),
                ),
            ])
        }
    }
}

fn help_menu_binding_widths(key_label: &str, description: &str, width: usize) -> (usize, usize) {
    if width <= HELP_DESCRIPTION_MIN_WIDTH {
        return (
            HELP_KEY_COLUMN_WIDTH.min(width),
            width.saturating_sub(HELP_KEY_COLUMN_WIDTH),
        );
    }

    let desired_key_width = HELP_KEY_COLUMN_WIDTH.max(key_label.width().saturating_add(3));
    let description_width = description.width();
    if desired_key_width.saturating_add(description_width) <= width {
        return (desired_key_width, width.saturating_sub(desired_key_width));
    }

    let reserved_description_width = HELP_DESCRIPTION_MIN_WIDTH.min(description_width).min(width);
    let key_width = desired_key_width.min(width.saturating_sub(reserved_description_width));
    (key_width, width.saturating_sub(key_width))
}

fn help_menu_key_text(key_label: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }

    let available = width.saturating_sub(3);
    let key = fit_with_ellipsis(key_label, available);
    fit_padded(&format!(" {key}"), width)
}

pub(crate) fn help_menu_key_label(key: HelpMenuKey, keymap: &Keymap) -> String {
    match key {
        HelpMenuKey::Static(label) => label.to_owned(),
        HelpMenuKey::Global(action) => keymap.global_action_label(action),
        HelpMenuKey::GlobalPair(first, second) => format!(
            "{}/{}",
            keymap.global_action_label(first),
            keymap.global_action_label(second)
        ),
    }
}

pub(crate) fn diff_selector_text(options: &DiffOptions) -> String {
    format!(" {} ", diff_type_label(options))
}

pub(crate) fn diff_selector_width(options: &DiffOptions) -> u16 {
    diff_selector_text(options).width() as u16
}

pub(crate) fn diff_type_label(options: &DiffOptions) -> &'static str {
    if let Some(choice) = diff_choice_from_options(options) {
        return choice.label();
    }

    match &options.source {
        DiffSource::Show(_) => "Show",
        DiffSource::Range { .. } => "Range",
        DiffSource::Difftool { .. } => "Difftool",
        DiffSource::Patch(_) => "Patch",
        DiffSource::Worktree | DiffSource::Base(_) | DiffSource::Branch { .. } => "Diff",
    }
}

pub(crate) fn diff_choice_from_options(options: &DiffOptions) -> Option<DiffChoice> {
    if is_review_options(options) {
        return Some(DiffChoice::Review);
    }

    match (&options.source, options.scope) {
        (DiffSource::Base(_) | DiffSource::Branch { .. }, DiffScope::All) => {
            Some(DiffChoice::Branch)
        }
        (DiffSource::Worktree, DiffScope::All) => Some(DiffChoice::All),
        (DiffSource::Worktree, DiffScope::Unstaged) => Some(DiffChoice::Unstaged),
        (DiffSource::Worktree, DiffScope::Staged) => Some(DiffChoice::Staged),
        (DiffSource::Show(_), DiffScope::All) => Some(DiffChoice::Show),
        _ => None,
    }
}

pub(crate) fn diff_comparison_label(options: &DiffOptions) -> String {
    match &options.source {
        DiffSource::Worktree => match options.scope {
            DiffScope::All => "HEAD → working tree".to_owned(),
            DiffScope::Staged => "HEAD → index".to_owned(),
            DiffScope::Unstaged => "index → working tree".to_owned(),
        },
        DiffSource::Show(rev) => format!("show {rev}"),
        DiffSource::Base(base) => format!("HEAD → {base}"),
        DiffSource::Branch { base, head } => format!("{head} → {base}"),
        DiffSource::Range { left, right } => format!("{left} → {right}"),
        DiffSource::Difftool { right, path, .. } => {
            format!(
                "git difftool {}",
                path.as_deref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| right.display().to_string())
            )
        }
        DiffSource::Patch(mark_diff::PatchSource::File(path)) => {
            format!("patch {}", path.display())
        }
        DiffSource::Patch(mark_diff::PatchSource::Stdin(_)) => "patch stdin".to_owned(),
        DiffSource::Patch(mark_diff::PatchSource::Text { label, .. }) => label.clone(),
    }
}

pub(crate) fn branch_menu_width(branches: &[String]) -> u16 {
    branches
        .iter()
        .map(|branch| branch.width() + 8)
        .max()
        .unwrap_or_default() as u16
}
