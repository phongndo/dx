use mark_diff::{DiffOptions, DiffScope, DiffSource};
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    prelude::{Color, Line, Modifier, Span, Style, Text},
    widgets::{
        Block, BorderType, Clear, Padding, Paragraph, Scrollbar, ScrollbarOrientation,
        ScrollbarState,
    },
};
use unicode_width::UnicodeWidthStr;

use crate::{
    app::{DiffApp, OptionsMenuItem, color_scheme_label, option_label},
    controls::{
        BranchMenu, DiffChoice, GitCommit, INPUT_CURSOR, commit_short_sha, is_review_options,
    },
    keymap::Keymap,
    render::{
        style::{base_bg, header_bg, input_cursor_style, spans_with_input_cursor},
        text::{fit_padded, fit_with_ellipsis},
    },
    theme::{
        DiffTheme, FLOATING_MENU_MIN_HEIGHT, FLOATING_MENU_MIN_WIDTH, HELP_KEY_COLUMN_WIDTH,
        HELP_MENU_ROWS, HelpMenuKey, HelpMenuRow,
    },
};

/// Vertical chrome: bordered block title row + input line + separator (see `selector_input_line`).
pub(crate) const SELECTOR_MENU_FIXED_INNER_ROWS: u16 = 2;
const FLOATING_MENU_HORIZONTAL_MARGIN: u16 = 2;
const FLOATING_MENU_MAX_HEIGHT: u16 = 36;
const FLOATING_MENU_MAX_HEIGHT_PERCENT: u16 = 70;
const FLOATING_MENU_SMALL_HEIGHT: u16 = 12;
const HELP_DESCRIPTION_MIN_WIDTH: usize = 24;
const SELECTOR_SCROLLBAR_TRACK: &str = "│";
const SELECTOR_SCROLLBAR_THUMB: &str = "┃";

pub(crate) fn floating_menu_fits_terminal(area: Rect) -> bool {
    area.width >= FLOATING_MENU_MIN_WIDTH && area.height >= FLOATING_MENU_MIN_HEIGHT
}

pub(crate) fn floating_menu_max_width(area: Rect, content_width: u16) -> u16 {
    let terminal_cap = floating_menu_width_cap(area.width);
    if terminal_cap == 0 {
        return 0;
    }

    content_width.clamp(1, terminal_cap)
}

pub(crate) fn floating_menu_max_height(area: Rect, content_height: u16) -> u16 {
    let terminal_cap = floating_menu_height_cap(area.height);
    if terminal_cap == 0 {
        return 0;
    }

    content_height.clamp(1, terminal_cap)
}

fn floating_menu_width_cap(width: u16) -> u16 {
    if width <= FLOATING_MENU_HORIZONTAL_MARGIN.saturating_add(1) {
        width
    } else {
        width.saturating_sub(FLOATING_MENU_HORIZONTAL_MARGIN)
    }
}

fn floating_menu_height_cap(height: u16) -> u16 {
    if height <= FLOATING_MENU_SMALL_HEIGHT {
        return height;
    }

    height
        .saturating_mul(FLOATING_MENU_MAX_HEIGHT_PERCENT)
        .saturating_div(100)
        .clamp(FLOATING_MENU_MIN_HEIGHT, FLOATING_MENU_MAX_HEIGHT)
        .min(height)
}

pub(crate) fn floating_menu_max_inner_height(area: Rect) -> u16 {
    floating_menu_max_height(area, u16::MAX)
        .saturating_sub(2)
        .max(1)
}

pub(crate) fn centered_floating_rect(area: Rect, width: u16, height: u16) -> Rect {
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

pub(crate) fn selector_menu_list_rows(inner_height: u16, pinned_rows: u16) -> usize {
    inner_height
        .saturating_sub(SELECTOR_MENU_FIXED_INNER_ROWS)
        .saturating_sub(pinned_rows)
        .max(1) as usize
}

pub(crate) fn selector_menu_outer_height(area: Rect, list_rows: usize, pinned_rows: u16) -> u16 {
    let inner_list = list_rows.max(1) as u16;
    let inner = inner_list
        .saturating_add(SELECTOR_MENU_FIXED_INNER_ROWS)
        .saturating_add(pinned_rows);
    let block = inner.saturating_add(2);
    floating_menu_max_height(area, block)
}

pub(crate) fn selector_menu_outer_width(content_width: usize) -> u16 {
    content_width.saturating_add(4).min(usize::from(u16::MAX)) as u16
}

fn selector_menu_rendered_list_rows(
    area: Rect,
    item_count: usize,
    pinned_rows: u16,
) -> Option<usize> {
    if !floating_menu_fits_terminal(area) {
        return None;
    }

    let list_cap = selector_menu_list_rows(floating_menu_max_inner_height(area), pinned_rows);
    let list_rows = item_count.max(1).min(list_cap);
    let outer_height = selector_menu_outer_height(area, list_rows, pinned_rows);
    if outer_height == 0 {
        return None;
    }

    let inner_height = outer_height.saturating_sub(2);
    let fixed_rows = SELECTOR_MENU_FIXED_INNER_ROWS.saturating_add(pinned_rows);
    Some(inner_height.saturating_sub(fixed_rows) as usize)
}

pub(crate) fn branch_menu_list_visible_rows(app: &DiffApp, area: Rect) -> Option<usize> {
    let menu = app.branch_menu_open?;
    if app.comparison_branches.is_empty() {
        return None;
    }

    let pinned_rows = u16::from(app.selected_branch_menu_choice(menu).is_some());
    selector_menu_rendered_list_rows(area, app.filtered_branches().len(), pinned_rows)
}

pub(crate) fn commit_menu_list_visible_rows(app: &DiffApp, area: Rect) -> Option<usize> {
    if !app.commit_menu_open || app.comparison_commits.is_empty() {
        return None;
    }

    let pinned_rows = u16::from(app.selected_commit_menu_choice().is_some());
    selector_menu_rendered_list_rows(area, app.filtered_commits().len(), pinned_rows)
}

pub(crate) fn color_scheme_picker_list_visible_rows(app: &DiffApp, area: Rect) -> Option<usize> {
    if !app.color_scheme_picker_open {
        return None;
    }

    selector_menu_rendered_list_rows(area, app.filtered_color_schemes().len(), 1)
}

fn selector_width_with_scrollbar(width: u16, has_scrollbar: bool) -> u16 {
    width.saturating_add(u16::from(has_scrollbar))
}

fn selector_scrollbar_needed(item_count: usize, visible_rows: usize) -> bool {
    visible_rows > 0 && item_count > visible_rows
}

fn selector_list_width(inner_width: u16, has_scrollbar: bool) -> usize {
    inner_width.saturating_sub(u16::from(has_scrollbar)) as usize
}

fn render_selector_scrollbar(
    frame: &mut Frame<'_>,
    inner: Rect,
    list_start_row: u16,
    item_count: usize,
    visible_rows: usize,
    scroll: usize,
    theme: DiffTheme,
) {
    if !selector_scrollbar_needed(item_count, visible_rows) || inner.width == 0 {
        return;
    }

    let y = inner.y.saturating_add(list_start_row);
    let max_height = inner.y.saturating_add(inner.height).saturating_sub(y);
    let height = (visible_rows.min(usize::from(u16::MAX)) as u16).min(max_height);
    if height == 0 {
        return;
    }

    let area = Rect {
        x: inner.x,
        y,
        width: inner.width,
        height,
    };
    let bg = base_bg(theme);
    let max_scroll = item_count.saturating_sub(visible_rows.max(1));
    let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .begin_symbol(None)
        .end_symbol(None)
        .track_symbol(Some(SELECTOR_SCROLLBAR_TRACK))
        .track_style(Style::default().fg(selector_count_color(theme)).bg(bg))
        .thumb_symbol(SELECTOR_SCROLLBAR_THUMB)
        .thumb_style(Style::default().fg(selector_title_color(theme)).bg(bg));
    // `scroll` is the top visible row, so the scrollbar range is the number of
    // possible top-row positions rather than the number of rows in the list.
    let mut state = ScrollbarState::new(max_scroll.saturating_add(1))
        .position(scroll.min(max_scroll))
        .viewport_content_length(visible_rows);
    frame.render_stateful_widget(scrollbar, area, &mut state);
}

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
        app.diff_menu_input_cursor,
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

    let block = review_input_block(app.theme);
    let inner = block.inner(menu_area);
    let bg = base_bg(app.theme);
    let input = text_with_cursor(&app.review_input, app.review_input_cursor);
    let prompt = fit_padded(&format!("> {input}"), inner.width as usize);
    let hint = fit_padded("Review ID for this repo", inner.width as usize);
    let prompt_style = Style::default().fg(selector_prompt_color(app.theme)).bg(bg);
    let lines = vec![
        Line::from(spans_with_input_cursor(
            &prompt,
            prompt_style,
            input_cursor_style(app.theme, bg),
        )),
        Line::from(Span::styled(
            hint,
            Style::default().fg(selector_count_color(app.theme)).bg(bg),
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
    if !app.review_input_open || !floating_menu_fits_terminal(area) {
        return None;
    }

    let content_width = app
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
    selector_menu_outer_width(rows.max(selected).max(input).max(42))
}

fn selector_input_line(
    input: &str,
    cursor: usize,
    width: usize,
    theme: DiffTheme,
    matched: usize,
    total: usize,
) -> Line<'static> {
    let input = text_with_cursor(input, cursor);
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
    let text_style = Style::default().fg(selector_prompt_color(theme)).bg(bg);
    let cursor_style = input_cursor_style(theme, bg);
    let mut spans = spans_with_input_cursor(&left, text_style, cursor_style);
    spans.push(Span::styled(" ", Style::default().bg(bg)));
    spans.push(Span::styled(
        fit_padded(&right, right_width.min(width)),
        Style::default().fg(selector_count_color(theme)).bg(bg),
    ));
    Line::from(spans)
}

fn text_with_cursor(input: &str, cursor: usize) -> String {
    let cursor = cursor.min(input.len());
    if input.is_char_boundary(cursor) {
        format!("{}{}{}", &input[..cursor], INPUT_CURSOR, &input[cursor..])
    } else {
        format!("{input}{INPUT_CURSOR}")
    }
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
    if !app.options_menu_open || !floating_menu_fits_terminal(area) {
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
    let block = options_menu_block(app.theme);
    let inner = block.inner(menu_area);
    let list_visible_rows = selector_menu_list_rows(inner.height, 0);
    app.ensure_options_menu_selection_visible(list_visible_rows);

    let selected = app.options_menu_selected.min(items.len().saturating_sub(1));
    let mut lines = vec![selector_input_line(
        &app.options_menu_input,
        app.options_menu_input_cursor,
        inner.width as usize,
        app.theme,
        items.len(),
        app.options_menu_items().len(),
    )];
    lines.push(selector_separator_line(inner.width as usize, app.theme));
    let list_start_row = lines.len() as u16;
    let remaining_rows = inner.height.saturating_sub(lines.len() as u16) as usize;
    let has_scrollbar = selector_scrollbar_needed(items.len(), remaining_rows);
    let list_width = selector_list_width(inner.width, has_scrollbar);
    if items.is_empty() {
        if remaining_rows > 0 {
            lines.push(selector_empty_line(
                " no matching settings",
                list_width,
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
                    list_width,
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
    render_selector_scrollbar(
        frame,
        inner,
        list_start_row,
        items.len(),
        remaining_rows,
        app.options_menu_scroll,
        app.theme,
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
    if !app.color_scheme_picker_open || !floating_menu_fits_terminal(area) {
        app.rendered_color_scheme_picker_area = None;
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
        app.rendered_color_scheme_picker_area = None;
        return;
    }

    let picker_area = centered_floating_rect(area, width, height);
    app.rendered_color_scheme_picker_area = Some(picker_area);
    let block = color_scheme_picker_block(app.theme);
    let inner = block.inner(picker_area);
    let choices = app.filtered_color_schemes();
    let mut lines = vec![selector_input_line(
        &app.color_scheme_input,
        app.color_scheme_input_cursor,
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

    let list_start_row = lines.len() as u16;
    let remaining_rows = inner.height.saturating_sub(lines.len() as u16) as usize;
    let has_scrollbar = selector_scrollbar_needed(choices.len(), remaining_rows);
    let list_width = selector_list_width(inner.width, has_scrollbar);
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
                list_width,
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
                    selector_entry_line(label, "", list_width, app.theme, highlighted)
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
    render_selector_scrollbar(
        frame,
        inner,
        list_start_row,
        choices.len(),
        remaining_rows,
        app.color_scheme_scroll,
        app.theme,
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
    selector_menu_outer_width(rows.max(current).max(input).max(42))
}

pub(crate) fn draw_branch_menu(frame: &mut Frame<'_>, app: &mut DiffApp, area: Rect) {
    let Some(menu) = app.branch_menu_open else {
        app.set_rendered_branch_menu_area(None);
        return;
    };
    if !floating_menu_fits_terminal(area) || app.comparison_branches.is_empty() {
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

    let block = branch_menu_block(app.theme, menu);
    let inner = block.inner(menu_area);
    let mut lines = vec![selector_input_line(
        &app.branch_menu_input,
        app.branch_menu_input_cursor,
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
    let list_start_row = lines.len() as u16;
    let remaining_rows = inner.height.saturating_sub(lines.len() as u16) as usize;
    app.ensure_branch_selection_visible_for_rows(remaining_rows);
    let matches = app.filtered_branches();
    let has_scrollbar = selector_scrollbar_needed(matches.len(), remaining_rows);
    let list_width = selector_list_width(inner.width, has_scrollbar);
    if matches.is_empty() {
        if remaining_rows > 0 {
            lines.push(selector_empty_line(
                " no matching branches",
                list_width,
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
                    selector_entry_line(branch, "", list_width, app.theme, highlighted)
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
    render_selector_scrollbar(
        frame,
        inner,
        list_start_row,
        matches.len(),
        remaining_rows,
        app.branch_menu_scroll,
        app.theme,
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
    if !floating_menu_fits_terminal(area) || app.comparison_commits.is_empty() {
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

    let block = commit_menu_block(app.theme);
    let inner = block.inner(menu_area);
    let mut lines = vec![selector_input_line(
        &app.commit_menu_input,
        app.commit_menu_input_cursor,
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
    let list_start_row = lines.len() as u16;
    let remaining_rows = inner.height.saturating_sub(lines.len() as u16) as usize;
    app.ensure_commit_selection_visible_for_rows(remaining_rows);
    let matches = app.filtered_commits();
    let has_scrollbar = selector_scrollbar_needed(matches.len(), remaining_rows);
    let list_width = selector_list_width(inner.width, has_scrollbar);
    if matches.is_empty() {
        if remaining_rows > 0 {
            lines.push(selector_empty_line(
                " no matching commits",
                list_width,
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
                    commit_menu_row_line(commit, list_width, app.theme, highlighted, false)
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
    render_selector_scrollbar(
        frame,
        inner,
        list_start_row,
        matches.len(),
        remaining_rows,
        app.commit_menu_scroll,
        app.theme,
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
    if !app.help_menu_open || !floating_menu_fits_terminal(area) {
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
        app.help_menu_input_cursor,
        inner.width as usize,
        app.theme,
        rows.len(),
        HELP_MENU_ROWS.len(),
    )];
    lines.push(selector_separator_line(inner.width as usize, app.theme));
    let list_start_row = lines.len() as u16;
    let has_scrollbar = selector_scrollbar_needed(rows.len(), remaining_rows);
    let list_width = selector_list_width(inner.width, has_scrollbar);
    if rows.is_empty() {
        if remaining_rows > 0 {
            lines.push(selector_empty_line(
                " no matching keybindings",
                list_width,
                app.theme,
            ));
        }
    } else {
        let scroll = app.help_menu_scroll;
        lines.extend(
            scrolled_selector_items(&rows, scroll, remaining_rows)
                .map(|(_, row)| help_menu_row_line(*row, list_width, app.theme, &app.keymap)),
        );
    }

    frame.render_widget(Clear, menu_area);
    frame.render_widget(block, menu_area);
    frame.render_widget(
        Paragraph::new(Text::from(lines)).style(Style::default().bg(help_menu_bg(app.theme))),
        inner,
    );
    render_selector_scrollbar(
        frame,
        inner,
        list_start_row,
        rows.len(),
        remaining_rows,
        app.help_menu_scroll,
        app.theme,
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
        .map(|row| help_menu_row_width(*row, &app.keymap))
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
