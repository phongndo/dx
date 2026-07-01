use mark_diff::{DiffOptions, DiffSource};
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    prelude::{Line, Modifier, Span, Style, Text},
    widgets::{Block, BorderType, Clear, Padding, Paragraph},
};
use unicode_width::UnicodeWidthStr;

use crate::{
    app::DiffApp,
    controls::{DiffChoice, is_review_options},
    render::{
        selector_menu::{
            ScrollableSelectorMenu, SelectorMenuInput, SelectorMenuLinesRequest,
            centered_floating_rect, floating_menu_fits_terminal, floating_menu_max_height,
            floating_menu_max_inner_height, floating_menu_max_width,
            render_scrollable_selector_menu, selector_border_color, selector_count_color,
            selector_disabled_line, selector_entry_line, selector_menu_lines,
            selector_menu_list_rows, selector_menu_outer_height, selector_menu_outer_width,
            selector_prompt_color, selector_title_color, text_with_cursor,
        },
        style::{base_bg, input_cursor_style, spans_with_input_cursor},
        text::fit_padded,
    },
    theme::DiffTheme,
};

pub(crate) fn draw_diff_menu(frame: &mut Frame<'_>, app: &DiffApp, area: Rect) {
    let choices = app.filtered_diff_choices();
    let Some(menu_area) = diff_menu_area(app, area, &choices) else {
        return;
    };

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
    if !app.overlays.diff_menu_is_open()
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

pub(crate) fn draw_review_input(frame: &mut Frame<'_>, app: &DiffApp, area: Rect) {
    let Some(menu_area) = review_input_area(app, area) else {
        return;
    };

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
    if !app.overlays.review_input_is_open() || !floating_menu_fits_terminal(area) {
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

    match &options.source {
        DiffSource::Base(_) | DiffSource::Branch { .. } => Some(DiffChoice::Branch),
        DiffSource::Worktree => Some(DiffChoice::All),
        DiffSource::Show(_) => Some(DiffChoice::Show),
        _ => None,
    }
}

pub(crate) fn diff_comparison_label(options: &DiffOptions) -> String {
    match &options.source {
        DiffSource::Worktree => "HEAD → working tree".to_owned(),
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
        DiffSource::Patch(mark_diff::PatchSource::Text { label, .. })
        | DiffSource::Patch(mark_diff::PatchSource::Review { label, .. }) => label.to_string(),
    }
}
