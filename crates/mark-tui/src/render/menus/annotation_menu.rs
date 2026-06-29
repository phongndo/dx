use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    prelude::{Line, Modifier, Span, Style},
    widgets::{Block, BorderType, Clear, Padding, Paragraph},
};
use unicode_width::UnicodeWidthStr;

use crate::{
    app::{AnnotationMenuItem, DiffApp},
    render::{
        selector_menu::{
            centered_floating_rect, floating_menu_fits_terminal, floating_menu_max_inner_height,
            floating_menu_max_width, selector_border_color, selector_empty_line,
            selector_input_line, selector_menu_list_rows, selector_menu_outer_height,
            selector_menu_outer_width, selector_row_style, selector_separator_line,
            selector_title_color,
        },
        style::base_bg,
        text::{fit_padded, status_code},
    },
    theme::DiffTheme,
};

pub(crate) fn draw_annotation_menu(frame: &mut Frame<'_>, app: &DiffApp, area: Rect) {
    let items = app.filtered_annotation_menu_items();
    let Some(menu_area) = annotation_menu_area(app, area, &items) else {
        return;
    };
    let block = annotation_menu_block(app.config.theme);
    let inner = block.inner(menu_area);
    let selected = app
        .overlays
        .annotation_menu
        .selected
        .min(items.len().saturating_sub(1));
    let content = annotation_menu_lines(app, &items, inner, selected);
    frame.render_widget(Clear, menu_area);
    frame.render_widget(block, menu_area);
    frame.render_widget(
        Paragraph::new(content).style(Style::default().bg(base_bg(app.config.theme))),
        inner,
    );
}

fn annotation_menu_lines(
    app: &DiffApp,
    items: &[AnnotationMenuItem],
    inner: Rect,
    selected: usize,
) -> Vec<Line<'static>> {
    let theme = app.config.theme;
    let width = inner.width as usize;
    let mut lines = vec![selector_input_line(
        &app.overlays.annotation_menu.input,
        app.overlays.annotation_menu.input_cursor,
        width,
        theme,
        items.len(),
        app.annotation_menu_items().len(),
    )];
    lines.push(selector_separator_line(width, theme));

    let visible_items = annotation_menu_visible_items(inner.height).max(1);
    if items.is_empty() {
        lines.push(selector_empty_line(
            " no matching annotations",
            width,
            theme,
        ));
        return lines;
    }
    for (index, item) in items
        .iter()
        .enumerate()
        .skip(app.overlays.annotation_menu.scroll)
        .take(visible_items)
    {
        lines.extend(annotation_menu_item_lines(
            item,
            width,
            theme,
            index == selected,
        ));
    }
    lines
}

pub(crate) fn annotation_menu_area(
    app: &DiffApp,
    area: Rect,
    items: &[AnnotationMenuItem],
) -> Option<Rect> {
    if !app.overlays.annotation_menu_is_open() || !floating_menu_fits_terminal(area) {
        return None;
    }
    let list_cap = annotation_menu_visible_items(floating_menu_max_inner_height(area));
    let list_rows = items.len().max(1).min(list_cap).saturating_mul(2);
    let width = floating_menu_max_width(area, annotation_menu_width(app, items));
    let height = selector_menu_outer_height(area, list_rows, 0);
    if width == 0 || height == 0 {
        return None;
    }
    Some(centered_floating_rect(area, width, height))
}

pub(crate) fn annotation_menu_list_visible_rows(app: &DiffApp, area: Rect) -> Option<usize> {
    let items = app.filtered_annotation_menu_items();
    let menu_area = annotation_menu_area(app, area, &items)?;
    let inner = annotation_menu_block(app.config.theme).inner(menu_area);
    Some(annotation_menu_visible_items(inner.height))
}

fn annotation_menu_visible_items(inner_height: u16) -> usize {
    selector_menu_list_rows(inner_height, 0)
        .saturating_div(2)
        .max(1)
}

pub(crate) fn annotation_menu_block(theme: DiffTheme) -> Block<'static> {
    let bg = base_bg(theme);
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(selector_border_color(theme)).bg(bg))
        .style(Style::default().bg(bg))
        .padding(Padding::horizontal(1))
        .title(Line::from(Span::styled(
            " Annotations ",
            Style::default()
                .fg(selector_title_color(theme))
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        )))
        .title_alignment(Alignment::Center)
}

fn annotation_menu_width(app: &DiffApp, items: &[AnnotationMenuItem]) -> u16 {
    let input = app
        .overlays
        .annotation_menu
        .input
        .width()
        .saturating_add(12);
    let rows = items
        .iter()
        .map(|item| {
            let body = item.text.lines().next().unwrap_or("");
            format!(" {} {} {} ", status_code(item.status), item.label, body).width()
        })
        .max()
        .unwrap_or(24);
    selector_menu_outer_width(rows.max(input).max(80))
}

fn annotation_menu_item_lines(
    item: &AnnotationMenuItem,
    width: usize,
    theme: DiffTheme,
    selected: bool,
) -> [Line<'static>; 2] {
    let style = selector_row_style(theme, selected);
    let status = Span::styled(
        fit_padded(status_code(item.status), 2),
        style
            .fg(status_color(item.status, theme))
            .add_modifier(Modifier::BOLD),
    );
    let header = fit_padded(&item.label, width.saturating_sub(3));
    let body = if item.text.trim().is_empty() {
        "(empty annotation)"
    } else {
        item.text.lines().next().unwrap_or("")
    };
    [
        Line::from(vec![status, Span::styled(format!(" {header}"), style)]),
        Line::from(Span::styled(
            format!("   {}", fit_padded(body, width.saturating_sub(3))),
            style,
        )),
    ]
}

fn status_color(status: mark_diff::FileStatus, theme: DiffTheme) -> ratatui::prelude::Color {
    match status {
        mark_diff::FileStatus::Added | mark_diff::FileStatus::Copied => theme.addition_fg,
        mark_diff::FileStatus::Deleted => theme.deletion_fg,
        mark_diff::FileStatus::Modified
        | mark_diff::FileStatus::Renamed
        | mark_diff::FileStatus::TypeChanged => theme.hunk,
        mark_diff::FileStatus::Unknown => theme.muted,
    }
}
