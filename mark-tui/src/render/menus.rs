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
    controls::{BranchMenu, DiffChoice, GitCommit, INPUT_CURSOR, commit_short_sha},
    keymap::Keymap,
    render::{
        style::{base_bg, header_bg},
        text::{fit_padded, fit_with_ellipsis},
    },
    theme::{
        DiffTheme, HELP_KEY_COLUMN_WIDTH, HELP_MENU_ROWS, HELP_MENU_WIDTH, HelpMenuKey, HelpMenuRow,
    },
};

pub(crate) fn draw_diff_menu(frame: &mut Frame<'_>, app: &mut DiffApp, area: Rect) {
    let choices = app.filtered_diff_choices();
    let Some(menu_area) = diff_menu_area(app, area, &choices) else {
        app.set_rendered_diff_menu_area(None);
        return;
    };
    app.set_rendered_diff_menu_area(Some(menu_area));

    let block = diff_menu_block(app.theme);
    let inner = block.inner(menu_area);
    let selected = app.diff_menu_selected.min(choices.len().saturating_sub(1));
    let mut lines = vec![selector_input_line(
        &app.diff_menu_input,
        inner.width as usize,
        app.theme,
        choices.len(),
        app.selectable_diff_choices().len(),
    )];
    lines.push(selector_separator_line(inner.width as usize, app.theme));
    if let Some(choice) = app.selected_diff_menu_choice() {
        lines.push(selector_disabled_line(
            choice.label(),
            &app.diff_choice_detail(choice),
            inner.width as usize,
            app.theme,
        ));
    }
    let remaining_rows = inner.height.saturating_sub(lines.len() as u16) as usize;
    if choices.is_empty() {
        if remaining_rows > 0 {
            lines.push(selector_empty_line(
                " no matching diff types",
                inner.width as usize,
                app.theme,
            ));
        }
    } else {
        lines.extend(
            choices
                .iter()
                .enumerate()
                .take(remaining_rows)
                .map(|(index, choice)| {
                    let highlighted = index == selected;
                    selector_entry_line(
                        choice.label(),
                        &app.diff_choice_detail(*choice),
                        inner.width as usize,
                        app.theme,
                        highlighted,
                    )
                }),
        );
    }

    frame.render_widget(Clear, menu_area);
    frame.render_widget(block, menu_area);
    frame.render_widget(
        Paragraph::new(Text::from(lines)).style(Style::default().bg(base_bg(app.theme))),
        inner,
    );
}

pub(crate) fn diff_menu_area(app: &DiffApp, area: Rect, choices: &[DiffChoice]) -> Option<Rect> {
    if !app.diff_menu_open
        || area.width < 24
        || area.height < 5
        || app.diff_menu_choices().is_empty()
    {
        return None;
    }

    let width = diff_menu_floating_width(app, choices).min(area.width);
    let pinned_rows = u16::from(app.selected_diff_menu_choice().is_some());
    let rows = (choices.len().max(1) as u16).saturating_add(pinned_rows);
    let height = rows.saturating_add(4).min(area.height);
    if width == 0 || height == 0 {
        return None;
    }

    Some(Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    })
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

fn diff_menu_floating_width(app: &DiffApp, choices: &[DiffChoice]) -> u16 {
    let input = app.diff_menu_input.width().saturating_add(12);
    let rows = choices
        .iter()
        .map(|choice| format!(" {}  {} ", choice.label(), app.diff_choice_detail(*choice),).width())
        .max()
        .unwrap_or_else(|| " no matching diff types ".width());
    let selected = app
        .selected_diff_menu_choice()
        .map(|choice| format!(" {}  {} ", choice.label(), app.diff_choice_detail(choice)).width())
        .unwrap_or_default();
    rows.max(selected)
        .max(input)
        .max(42)
        .saturating_add(4)
        .min(88) as u16
}

fn selector_input_line(
    input: &str,
    width: usize,
    theme: DiffTheme,
    matched: usize,
    total: usize,
) -> Line<'static> {
    let input = if input.is_empty() {
        INPUT_CURSOR.to_owned()
    } else {
        format!("{input}{INPUT_CURSOR}")
    };
    let left = format!("> {input}");
    let right = format!("{matched}/{total}");
    let bg = base_bg(theme);
    let right_width = right.width();
    if width <= right_width {
        return Line::from(Span::styled(
            fit_padded(&right, width),
            Style::default().fg(selector_count_color(theme)).bg(bg),
        ));
    }
    let left_width = width.saturating_sub(right_width).saturating_sub(1);
    let left = fit_padded(&left, left_width);
    Line::from(vec![
        Span::styled(
            left,
            Style::default().fg(selector_prompt_color(theme)).bg(bg),
        ),
        Span::styled(" ", Style::default().bg(bg)),
        Span::styled(
            fit_padded(&right, right_width.min(width)),
            Style::default().fg(selector_count_color(theme)).bg(bg),
        ),
    ])
}

fn selector_separator_line(width: usize, theme: DiffTheme) -> Line<'static> {
    Line::from(Span::styled(
        "─".repeat(width),
        Style::default()
            .fg(selector_border_color(theme))
            .bg(base_bg(theme)),
    ))
}

fn selector_empty_line(text: &str, width: usize, theme: DiffTheme) -> Line<'static> {
    Line::from(Span::styled(
        fit_padded(text, width),
        Style::default().fg(theme.muted).bg(base_bg(theme)),
    ))
}

fn selector_disabled_line(
    label: &str,
    detail: &str,
    width: usize,
    theme: DiffTheme,
) -> Line<'static> {
    Line::from(Span::styled(
        selector_label_detail_text(label, detail, width),
        Style::default().fg(theme.muted).bg(base_bg(theme)),
    ))
}

fn selector_label_detail_text(label: &str, detail: &str, width: usize) -> String {
    let left = format!(" {label}");
    if detail.is_empty() {
        return fit_padded(&left, width);
    }

    let detail_width = detail.width();
    if width <= detail_width.saturating_add(2) {
        return fit_padded(&format!("{left}  {detail}"), width);
    }

    let left_width = width.saturating_sub(detail_width).saturating_sub(1);
    format!("{} {detail}", fit_padded(&left, left_width))
}

fn selector_row_style(theme: DiffTheme, highlighted: bool) -> Style {
    let mut style = Style::default().fg(theme.foreground).bg(base_bg(theme));
    if highlighted {
        style = style
            .fg(selector_highlight_color(theme))
            .bg(header_bg(theme))
            .add_modifier(Modifier::BOLD);
    }
    style
}

fn selector_title_color(theme: DiffTheme) -> Color {
    theme.syntax.function.unwrap_or(theme.header)
}

fn selector_prompt_color(theme: DiffTheme) -> Color {
    theme.syntax.operator.unwrap_or(selector_title_color(theme))
}

fn selector_border_color(theme: DiffTheme) -> Color {
    theme.syntax.punctuation.unwrap_or(theme.muted)
}

fn selector_count_color(theme: DiffTheme) -> Color {
    theme.syntax.comment.unwrap_or(theme.muted)
}

fn selector_highlight_color(theme: DiffTheme) -> Color {
    theme.syntax.keyword.unwrap_or(theme.header)
}

fn selector_entry_line(
    label: &str,
    detail: &str,
    width: usize,
    theme: DiffTheme,
    highlighted: bool,
) -> Line<'static> {
    Line::from(Span::styled(
        selector_label_detail_text(label, detail, width),
        selector_row_style(theme, highlighted),
    ))
}

/// Visible slice of a scrollable selector list. `global_index = scroll + visible_row`.
fn scrolled_selector_items<'a, T>(
    items: &'a [T],
    scroll: usize,
    visible_rows: usize,
) -> impl Iterator<Item = (usize, &'a T)> + 'a {
    items
        .iter()
        .skip(scroll)
        .take(visible_rows)
        .enumerate()
        .map(move |(visible_row, item)| (scroll.saturating_add(visible_row), item))
}

pub(crate) fn draw_options_menu(frame: &mut Frame<'_>, app: &mut DiffApp, area: Rect) {
    if !app.options_menu_open || area.width < 24 || area.height < 5 {
        return;
    }

    let items = app.filtered_options_menu_items();
    let width = options_menu_width(app, &items).min(area.width);
    let height = (items.len().max(1) as u16)
        .saturating_add(4)
        .min(area.height);
    if width == 0 || height == 0 {
        return;
    }

    let menu_area = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    };
    let block = options_menu_block(app.theme);
    let inner = block.inner(menu_area);
    let remaining_rows = inner.height.saturating_sub(2) as usize;
    app.ensure_options_menu_selection_visible(remaining_rows);

    let selected = app.options_menu_selected.min(items.len().saturating_sub(1));
    let mut lines = vec![selector_input_line(
        &app.options_menu_input,
        inner.width as usize,
        app.theme,
        items.len(),
        app.options_menu_items().len(),
    )];
    lines.push(selector_separator_line(inner.width as usize, app.theme));
    let remaining_rows = inner.height.saturating_sub(lines.len() as u16) as usize;
    if items.is_empty() {
        if remaining_rows > 0 {
            lines.push(selector_empty_line(
                " no matching settings",
                inner.width as usize,
                app.theme,
            ));
        }
    } else {
        let scroll = app.options_menu_scroll;
        lines.extend(scrolled_selector_items(&items, scroll, remaining_rows).map(
            |(global_index, item)| {
                selector_setting_line(
                    option_label(*item),
                    &app.option_value(*item),
                    inner.width as usize,
                    app.theme,
                    global_index == selected,
                )
            },
        ));
    }

    frame.render_widget(Clear, menu_area);
    frame.render_widget(block, menu_area);
    frame.render_widget(
        Paragraph::new(Text::from(lines)).style(Style::default().bg(base_bg(app.theme))),
        inner,
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
    let input = app.options_menu_input.width().saturating_add(12);
    let rows = items
        .iter()
        .map(|item| format!(" › {}  {} ", option_label(*item), app.option_value(*item)).width())
        .max()
        .unwrap_or_else(|| " no matching settings ".width());
    rows.max(input).max(42).saturating_add(4).min(88) as u16
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
    if !app.color_scheme_picker_open || area.width < 28 || area.height < 6 {
        app.rendered_color_scheme_picker_area = None;
        return;
    }

    let width = color_scheme_picker_width(app).min(area.width);
    let height = (app.visible_color_scheme_rows() as u16)
        .saturating_add(5)
        .min(area.height);
    if width == 0 || height == 0 {
        app.rendered_color_scheme_picker_area = None;
        return;
    }

    let picker_area = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    };
    app.rendered_color_scheme_picker_area = Some(picker_area);
    let block = color_scheme_picker_block(app.theme);
    let inner = block.inner(picker_area);
    let choices = app.filtered_color_schemes();
    let mut lines = vec![selector_input_line(
        &app.color_scheme_input,
        inner.width as usize,
        app.theme,
        choices.len(),
        app.selectable_color_schemes().len(),
    )];
    lines.push(selector_separator_line(inner.width as usize, app.theme));
    lines.push(selector_disabled_line(
        color_scheme_label(app.options_menu_draft.color_scheme),
        "",
        inner.width as usize,
        app.theme,
    ));

    let remaining_rows = inner.height.saturating_sub(lines.len() as u16) as usize;
    crate::app::ensure_selector_scroll(
        &mut app.color_scheme_scroll,
        app.color_scheme_selected,
        choices.len(),
        remaining_rows,
    );
    if choices.is_empty() {
        if remaining_rows > 0 {
            lines.push(selector_empty_line(
                " no matching colorscheme",
                inner.width as usize,
                app.theme,
            ));
        }
    } else {
        let scroll = app.color_scheme_scroll;
        let selected = app.color_scheme_selected;
        lines.extend(
            scrolled_selector_items(&choices, scroll, remaining_rows).map(
                |(global_index, choice)| {
                    let highlighted = global_index == selected;
                    let label = color_scheme_label(*choice);
                    selector_entry_line(label, "", inner.width as usize, app.theme, highlighted)
                },
            ),
        );
    }

    frame.render_widget(Clear, picker_area);
    frame.render_widget(block, picker_area);
    frame.render_widget(
        Paragraph::new(Text::from(lines)).style(Style::default().bg(base_bg(app.theme))),
        inner,
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
    let input = app.color_scheme_input.width().saturating_add(12);
    let rows = app
        .filtered_color_schemes()
        .iter()
        .map(|choice| format!(" {} ", color_scheme_label(*choice)).width())
        .max()
        .unwrap_or_else(|| " no matching colorscheme ".width());
    let current = format!(
        " {} ",
        color_scheme_label(app.options_menu_draft.color_scheme)
    )
    .width();
    rows.max(current)
        .max(input)
        .max(42)
        .saturating_add(4)
        .min(64) as u16
}

pub(crate) fn draw_branch_menu(frame: &mut Frame<'_>, app: &mut DiffApp, area: Rect) {
    let Some(menu) = app.branch_menu_open else {
        app.set_rendered_branch_menu_area(None);
        return;
    };
    if area.width < 24 || area.height < 5 || app.comparison_branches.is_empty() {
        app.set_rendered_branch_menu_area(None);
        return;
    }

    let width = app.branch_menu_width().min(area.width).min(88);
    let height = (app.branch_menu_height() as u16).min(area.height);
    if width == 0 || height == 0 {
        app.set_rendered_branch_menu_area(None);
        return;
    }

    let menu_area = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    };
    app.set_rendered_branch_menu_area(Some(menu_area));

    let block = branch_menu_block(app.theme, menu);
    let inner = block.inner(menu_area);
    let match_count = app.filtered_branches().len();
    let mut lines = vec![selector_input_line(
        &app.branch_menu_input,
        inner.width as usize,
        app.theme,
        match_count,
        app.selectable_branch_count(menu),
    )];
    lines.push(selector_separator_line(inner.width as usize, app.theme));
    if let Some(branch) = app.selected_branch_menu_choice(menu) {
        lines.push(selector_disabled_line(
            branch,
            "",
            inner.width as usize,
            app.theme,
        ));
    }
    let remaining_rows = inner.height.saturating_sub(lines.len() as u16) as usize;
    app.ensure_branch_selection_visible_for_rows(remaining_rows);
    let matches = app.filtered_branches();
    if matches.is_empty() {
        if remaining_rows > 0 {
            lines.push(selector_empty_line(
                " no matching branches",
                inner.width as usize,
                app.theme,
            ));
        }
    } else {
        let scroll = app.branch_menu_scroll;
        let selected = app.branch_menu_selected;
        lines.extend(
            scrolled_selector_items(&matches, scroll, remaining_rows).map(
                |(global_index, branch)| {
                    let highlighted = global_index == selected;
                    selector_entry_line(branch, "", inner.width as usize, app.theme, highlighted)
                },
            ),
        );
    }

    frame.render_widget(Clear, menu_area);
    frame.render_widget(block, menu_area);
    frame.render_widget(
        Paragraph::new(Text::from(lines)).style(Style::default().bg(base_bg(app.theme))),
        inner,
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
    if !app.commit_menu_open {
        app.set_rendered_commit_menu_area(None);
        return;
    }
    if area.width < 24 || area.height < 5 || app.comparison_commits.is_empty() {
        app.set_rendered_commit_menu_area(None);
        return;
    }

    let width = app.commit_menu_width().min(area.width).min(88);
    let height = (app.commit_menu_height() as u16).min(area.height);
    if width == 0 || height == 0 {
        app.set_rendered_commit_menu_area(None);
        return;
    }

    let menu_area = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    };
    app.set_rendered_commit_menu_area(Some(menu_area));

    let block = commit_menu_block(app.theme);
    let inner = block.inner(menu_area);
    let match_count = app.filtered_commits().len();
    let mut lines = vec![selector_input_line(
        &app.commit_menu_input,
        inner.width as usize,
        app.theme,
        match_count,
        app.selectable_commit_count(),
    )];
    lines.push(selector_separator_line(inner.width as usize, app.theme));
    if let Some(commit) = app.selected_commit_menu_choice() {
        lines.push(commit_menu_row_line(
            commit,
            inner.width as usize,
            app.theme,
            false,
            true,
        ));
    }
    let remaining_rows = inner.height.saturating_sub(lines.len() as u16) as usize;
    app.ensure_commit_selection_visible_for_rows(remaining_rows);
    let matches = app.filtered_commits();
    if matches.is_empty() {
        if remaining_rows > 0 {
            lines.push(selector_empty_line(
                " no matching commits",
                inner.width as usize,
                app.theme,
            ));
        }
    } else {
        let scroll = app.commit_menu_scroll;
        let selected = app.commit_menu_selected;
        lines.extend(
            scrolled_selector_items(&matches, scroll, remaining_rows).map(
                |(global_index, commit)| {
                    let highlighted = global_index == selected;
                    commit_menu_row_line(
                        commit,
                        inner.width as usize,
                        app.theme,
                        highlighted,
                        false,
                    )
                },
            ),
        );
    }

    frame.render_widget(Clear, menu_area);
    frame.render_widget(block, menu_area);
    frame.render_widget(
        Paragraph::new(Text::from(lines)).style(Style::default().bg(base_bg(app.theme))),
        inner,
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
    if !app.help_menu_open || area.width < 4 || area.height < 3 {
        return None;
    }

    let rows = app.filtered_help_menu_rows();
    let width = help_menu_width(app, &rows).min(area.width);
    let height = (rows.len().max(1) as u16)
        .saturating_add(4)
        .min(area.height);
    if height == 0 {
        return None;
    }

    let menu_area = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    };

    let block = help_menu_block(app.theme);
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
    app.help_menu_visible_rows = remaining_rows;
    app.clamp_help_menu_scroll(remaining_rows);
    let menu_area = layout.menu_area;

    let block = help_menu_block(app.theme);
    let mut lines = vec![selector_input_line(
        &app.help_menu_input,
        inner.width as usize,
        app.theme,
        rows.len(),
        HELP_MENU_ROWS.len(),
    )];
    lines.push(selector_separator_line(inner.width as usize, app.theme));
    if rows.is_empty() {
        if remaining_rows > 0 {
            lines.push(selector_empty_line(
                " no matching keybindings",
                inner.width as usize,
                app.theme,
            ));
        }
    } else {
        let scroll = app.help_menu_scroll;
        lines.extend(
            scrolled_selector_items(&rows, scroll, remaining_rows).map(|(_, row)| {
                help_menu_row_line(*row, inner.width as usize, app.theme, &app.keymap)
            }),
        );
    }

    frame.render_widget(Clear, menu_area);
    frame.render_widget(block, menu_area);
    frame.render_widget(
        Paragraph::new(Text::from(lines)).style(Style::default().bg(help_menu_bg(app.theme))),
        inner,
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
    let input = app.help_menu_input.width().saturating_add(12);
    let rows = rows
        .iter()
        .map(|row| help_menu_row_text(*row, &app.keymap).width())
        .max()
        .unwrap_or_else(|| " no matching keybindings ".width());
    rows.max(input)
        .max(42)
        .saturating_add(4)
        .min(HELP_MENU_WIDTH as usize) as u16
}

fn help_menu_row_text(row: HelpMenuRow, keymap: &Keymap) -> String {
    match row {
        HelpMenuRow::Section(section) => format!(" {section}"),
        HelpMenuRow::Binding(keys, description) => {
            format!(" {}{}", help_menu_key_label(keys, keymap), description)
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
            let key_width = HELP_KEY_COLUMN_WIDTH.min(width);
            let description_width = width.saturating_sub(key_width);
            vec![
                Span::styled(
                    fit_padded(&format!("  {key_label}"), key_width),
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
            let key_width = HELP_KEY_COLUMN_WIDTH.min(width);
            let description_width = width.saturating_sub(key_width);
            Line::from(vec![
                Span::styled(
                    fit_padded(&format!(" {key_label}"), key_width),
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

pub(crate) fn help_menu_key_label(key: HelpMenuKey, keymap: &Keymap) -> String {
    match key {
        HelpMenuKey::Static(label) => label.to_owned(),
        HelpMenuKey::Leader => keymap.leader_label(),
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
