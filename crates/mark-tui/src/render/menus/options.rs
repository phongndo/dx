use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    prelude::{Line, Modifier, Span, Style},
    widgets::{Block, BorderType, Padding},
};
use unicode_width::UnicodeWidthStr;

use crate::{
    app::{DiffApp, OptionsMenuItem, color_scheme_label, option_label},
    render::{
        selector_menu::{
            ScrollableSelectorMenu, SelectorMenuInput, SelectorMenuLinesRequest,
            centered_floating_rect, floating_menu_fits_terminal, floating_menu_max_inner_height,
            floating_menu_max_width, render_scrollable_selector_menu, selector_border_color,
            selector_disabled_line, selector_entry_line, selector_menu_lines,
            selector_menu_list_rows, selector_menu_outer_height, selector_menu_outer_width,
            selector_row_style, selector_scrollbar_needed, selector_title_color,
            selector_width_with_scrollbar,
        },
        style::base_bg,
        text::fit_padded,
    },
    theme::DiffTheme,
};

pub(crate) fn draw_options_menu(frame: &mut Frame<'_>, app: &DiffApp, area: Rect) {
    let items = app.filtered_options_menu_items();
    let Some(menu_area) = options_menu_area(app, area, &items) else {
        return;
    };
    let block = options_menu_block(app.config.theme);
    let inner = block.inner(menu_area);

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

pub(crate) fn options_menu_area(
    app: &DiffApp,
    area: Rect,
    items: &[OptionsMenuItem],
) -> Option<Rect> {
    if !app.overlays.options_menu_is_open() || !floating_menu_fits_terminal(area) {
        return None;
    }

    let list_cap = selector_menu_list_rows(floating_menu_max_inner_height(area), 0);
    let list_rows = items.len().max(1).min(list_cap);
    let width = floating_menu_max_width(
        area,
        selector_width_with_scrollbar(
            options_menu_width(app, items),
            selector_scrollbar_needed(items.len(), list_rows),
        ),
    );
    let height = selector_menu_outer_height(area, list_rows, 0);
    if width == 0 || height == 0 {
        return None;
    }

    Some(centered_floating_rect(area, width, height))
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

pub(crate) fn draw_color_scheme_picker(frame: &mut Frame<'_>, app: &DiffApp, area: Rect) {
    let Some(picker_area) = color_scheme_picker_area(app, area) else {
        return;
    };
    let block = color_scheme_picker_block(app.config.theme);
    let inner = block.inner(picker_area);
    let choices = app.filtered_color_schemes();
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

pub(crate) fn color_scheme_picker_area(app: &DiffApp, area: Rect) -> Option<Rect> {
    if !app.overlays.color_scheme_picker_is_open() || !floating_menu_fits_terminal(area) {
        return None;
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
        return None;
    }

    Some(centered_floating_rect(area, width, height))
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
