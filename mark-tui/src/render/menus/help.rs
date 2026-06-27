use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    prelude::{Color, Line, Modifier, Span, Style},
    widgets::{Block, BorderType, Padding},
};
use unicode_width::UnicodeWidthStr;

use crate::{
    app::DiffApp,
    keymap::Keymap,
    render::{
        selector_menu::{
            ScrollableSelectorMenu, SelectorMenuInput, SelectorMenuLinesRequest,
            centered_floating_rect, floating_menu_fits_terminal, floating_menu_max_inner_height,
            floating_menu_max_width, render_scrollable_selector_menu, selector_border_color,
            selector_menu_lines, selector_menu_list_rows, selector_menu_outer_height,
            selector_menu_outer_width, selector_scrollbar_needed, selector_title_color,
            selector_width_with_scrollbar,
        },
        style::base_bg,
        text::{fit_padded, fit_with_ellipsis},
    },
    theme::{DiffTheme, HELP_KEY_COLUMN_WIDTH, HELP_MENU_ROWS, HelpMenuKey, HelpMenuRow},
};

const HELP_DESCRIPTION_MIN_WIDTH: usize = 24;

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

pub(crate) fn draw_help_menu(frame: &mut Frame<'_>, app: &DiffApp, area: Rect) {
    let Some(layout) = help_menu_layout(app, area) else {
        return;
    };

    let rows = app.filtered_help_menu_rows();
    let inner = layout.inner;
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
