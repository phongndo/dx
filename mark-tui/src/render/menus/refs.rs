use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    prelude::{Color, Line, Modifier, Span, Style},
    widgets::{Block, BorderType, Padding},
};
use unicode_width::UnicodeWidthStr;

use crate::{
    app::DiffApp,
    controls::{BranchMenu, GitCommit, commit_short_sha},
    render::{
        selector_menu::{
            ScrollableSelectorMenu, SelectorMenuInput, SelectorMenuLinesRequest,
            centered_floating_rect, floating_menu_fits_terminal, floating_menu_max_inner_height,
            floating_menu_max_width, render_scrollable_selector_menu, selector_border_color,
            selector_disabled_line, selector_entry_line, selector_highlight_color,
            selector_menu_lines, selector_menu_list_rows, selector_menu_outer_height,
            selector_menu_rendered_list_rows, selector_scrollbar_needed, selector_title_color,
            selector_width_with_scrollbar,
        },
        style::{base_bg, header_bg},
        text::{fit_padded, fit_with_ellipsis},
    },
    theme::DiffTheme,
};

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

pub(crate) fn draw_branch_menu(frame: &mut Frame<'_>, app: &DiffApp, area: Rect) {
    let Some(menu) = app.refs.branch_menu_open else {
        return;
    };
    let match_count = app.filtered_branches().len();
    let Some(menu_area) = branch_menu_area(app, area) else {
        return;
    };

    let block = branch_menu_block(app.config.theme, menu);
    let inner = block.inner(menu_area);
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

pub(crate) fn branch_menu_area(app: &DiffApp, area: Rect) -> Option<Rect> {
    let menu = app.refs.branch_menu_open?;
    if !floating_menu_fits_terminal(area) || app.refs.comparison_branches.is_empty() {
        return None;
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
        return None;
    }

    Some(centered_floating_rect(area, width, height))
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

pub(crate) fn draw_commit_menu(frame: &mut Frame<'_>, app: &DiffApp, area: Rect) {
    let Some(menu_area) = commit_menu_area(app, area) else {
        return;
    };
    let match_count = app.filtered_commits().len();
    let block = commit_menu_block(app.config.theme);
    let inner = block.inner(menu_area);
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

pub(crate) fn commit_menu_area(app: &DiffApp, area: Rect) -> Option<Rect> {
    if !app.refs.commit_menu_open {
        return None;
    }
    if !floating_menu_fits_terminal(area) || app.refs.comparison_commits.is_empty() {
        return None;
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
        return None;
    }

    Some(centered_floating_rect(area, width, height))
}

pub(crate) fn branch_menu_width(branches: &[String]) -> u16 {
    branches
        .iter()
        .map(|branch| branch.width() + 8)
        .max()
        .unwrap_or_default() as u16
}
