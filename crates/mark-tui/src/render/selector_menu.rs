use ratatui::{
    Frame,
    layout::Rect,
    prelude::{Color, Line, Modifier, Span, Style, Text},
    widgets::{Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
};
use unicode_width::UnicodeWidthStr;

use crate::{
    controls::INPUT_CURSOR,
    render::{
        style::{base_bg, header_bg, input_cursor_style, spans_with_input_cursor},
        text::fit_padded,
    },
    theme::{DiffTheme, FLOATING_MENU_MIN_HEIGHT, FLOATING_MENU_MIN_WIDTH},
};

/// Vertical chrome: bordered block title row + input line + separator.
pub(crate) const SELECTOR_MENU_FIXED_INNER_ROWS: u16 = 2;
const FLOATING_MENU_HORIZONTAL_MARGIN: u16 = 2;
const FLOATING_MENU_MAX_HEIGHT: u16 = 36;
const FLOATING_MENU_MAX_HEIGHT_PERCENT: u16 = 70;
const FLOATING_MENU_SMALL_HEIGHT: u16 = 12;
const SELECTOR_SCROLLBAR_TRACK: &str = "│";
const SELECTOR_SCROLLBAR_THUMB: &str = "┃";

pub(crate) struct SelectorMenuInput<'a> {
    pub(crate) input: &'a str,
    pub(crate) input_cursor: usize,
    pub(crate) matched: usize,
    pub(crate) total: usize,
    pub(crate) theme: DiffTheme,
}

pub(crate) struct SelectorMenuLines {
    pub(crate) lines: Vec<Line<'static>>,
    pub(crate) list_start_row: u16,
    pub(crate) item_count: usize,
    pub(crate) visible_rows: usize,
    pub(crate) scroll: usize,
}

pub(crate) struct SelectorMenuLinesRequest<'a, T> {
    pub(crate) input: SelectorMenuInput<'a>,
    pub(crate) inner: Rect,
    pub(crate) items: &'a [T],
    pub(crate) scroll: usize,
    pub(crate) selected: usize,
    pub(crate) pinned_lines: Vec<Line<'static>>,
    pub(crate) empty_message: &'static str,
}

pub(crate) struct ScrollableSelectorMenu {
    pub(crate) menu_area: Rect,
    pub(crate) block: ratatui::widgets::Block<'static>,
    pub(crate) inner: Rect,
    pub(crate) content: SelectorMenuLines,
    pub(crate) theme: DiffTheme,
}

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

pub(crate) fn selector_menu_rendered_list_rows(
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

pub(crate) fn selector_width_with_scrollbar(width: u16, has_scrollbar: bool) -> u16 {
    width.saturating_add(u16::from(has_scrollbar))
}

pub(crate) fn selector_scrollbar_needed(item_count: usize, visible_rows: usize) -> bool {
    visible_rows > 0 && item_count > visible_rows
}

fn selector_list_width(inner_width: u16, has_scrollbar: bool) -> usize {
    inner_width.saturating_sub(u16::from(has_scrollbar)) as usize
}

pub(crate) fn selector_menu_lines<T>(
    request: SelectorMenuLinesRequest<'_, T>,
    mut render_item: impl FnMut(usize, &T, usize, bool) -> Line<'static>,
) -> SelectorMenuLines {
    let SelectorMenuLinesRequest {
        input,
        inner,
        items,
        scroll,
        selected,
        pinned_lines,
        empty_message,
    } = request;
    let mut lines = vec![selector_input_line(
        input.input,
        input.input_cursor,
        inner.width as usize,
        input.theme,
        input.matched,
        input.total,
    )];
    lines.push(selector_separator_line(inner.width as usize, input.theme));
    lines.extend(pinned_lines);

    let list_start_row = lines.len() as u16;
    let visible_rows = inner.height.saturating_sub(lines.len() as u16) as usize;
    let has_scrollbar = selector_scrollbar_needed(items.len(), visible_rows);
    let list_width = selector_list_width(inner.width, has_scrollbar);

    if items.is_empty() {
        if visible_rows > 0 {
            lines.push(selector_empty_line(empty_message, list_width, input.theme));
        }
    } else {
        lines.extend(scrolled_selector_items(items, scroll, visible_rows).map(
            |(global_index, item)| {
                render_item(global_index, item, list_width, global_index == selected)
            },
        ));
    }

    SelectorMenuLines {
        lines,
        list_start_row,
        item_count: items.len(),
        visible_rows,
        scroll,
    }
}

pub(crate) fn render_scrollable_selector_menu(frame: &mut Frame<'_>, menu: ScrollableSelectorMenu) {
    let list_start_row = menu.content.list_start_row;
    let item_count = menu.content.item_count;
    let visible_rows = menu.content.visible_rows;
    let scroll = menu.content.scroll;
    frame.render_widget(Clear, menu.menu_area);
    frame.render_widget(menu.block, menu.menu_area);
    frame.render_widget(
        Paragraph::new(Text::from(menu.content.lines))
            .style(Style::default().bg(base_bg(menu.theme))),
        menu.inner,
    );
    render_selector_scrollbar(
        frame,
        menu.inner,
        list_start_row,
        item_count,
        visible_rows,
        scroll,
        menu.theme,
    );
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

pub(crate) fn selector_input_line(
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

pub(crate) fn text_with_cursor(input: &str, cursor: usize) -> String {
    let cursor = cursor.min(input.len());
    if input.is_char_boundary(cursor) {
        format!("{}{}{}", &input[..cursor], INPUT_CURSOR, &input[cursor..])
    } else {
        format!("{input}{INPUT_CURSOR}")
    }
}

pub(crate) fn selector_separator_line(width: usize, theme: DiffTheme) -> Line<'static> {
    Line::from(Span::styled(
        "─".repeat(width),
        Style::default()
            .fg(selector_border_color(theme))
            .bg(base_bg(theme)),
    ))
}

pub(crate) fn selector_empty_line(text: &str, width: usize, theme: DiffTheme) -> Line<'static> {
    Line::from(Span::styled(
        fit_padded(text, width),
        Style::default().fg(theme.muted).bg(base_bg(theme)),
    ))
}

pub(crate) fn selector_disabled_line(
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

pub(crate) fn selector_row_style(theme: DiffTheme, highlighted: bool) -> Style {
    let mut style = Style::default().fg(theme.foreground).bg(base_bg(theme));
    if highlighted {
        style = style
            .fg(selector_highlight_color(theme))
            .bg(header_bg(theme))
            .add_modifier(Modifier::BOLD);
    }
    style
}

pub(crate) fn selector_title_color(theme: DiffTheme) -> Color {
    theme.syntax.function.unwrap_or(theme.header)
}

pub(crate) fn selector_prompt_color(theme: DiffTheme) -> Color {
    theme.syntax.operator.unwrap_or(selector_title_color(theme))
}

pub(crate) fn selector_border_color(theme: DiffTheme) -> Color {
    theme.syntax.punctuation.unwrap_or(theme.muted)
}

pub(crate) fn selector_count_color(theme: DiffTheme) -> Color {
    theme.syntax.comment.unwrap_or(theme.muted)
}

pub(crate) fn selector_highlight_color(theme: DiffTheme) -> Color {
    theme.syntax.keyword.unwrap_or(theme.header)
}

pub(crate) fn selector_entry_line(
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
