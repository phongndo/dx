use dx_diff::{DiffOptions, DiffScope, DiffSource};
use ratatui::{
    Frame,
    layout::Rect,
    prelude::{Color, Line, Modifier, Span, Style, Text},
    widgets::{Block, BorderType, Clear, Padding, Paragraph},
};
use unicode_width::UnicodeWidthStr;

use crate::{
    app::{DiffApp, OptionsMenuItem, color_scheme_label, context_expansion_label},
    controls::{DiffChoice, INPUT_CURSOR},
    keymap::{Keymap, MenuAction},
    render::{
        style::{base_bg, header_bg},
        text::fit_padded,
    },
    theme::{
        DiffTheme, HELP_KEY_COLUMN_WIDTH, HELP_MENU_COLUMN_GAP, HELP_MENU_HORIZONTAL_PADDING,
        HELP_MENU_LEFT_ROWS, HELP_MENU_RIGHT_ROWS, HELP_MENU_TWO_COLUMN_MIN_WIDTH,
        HELP_MENU_VERTICAL_PADDING, HELP_MENU_WIDTH, HelpMenuKey, HelpMenuRow,
    },
};

pub(crate) fn draw_diff_menu(frame: &mut Frame<'_>, app: &mut DiffApp, area: Rect) {
    let choices = app.diff_menu_choices();
    let Some(menu_area) = diff_menu_area(app, area, &choices) else {
        app.set_rendered_diff_menu_area(None);
        return;
    };
    app.set_rendered_diff_menu_area(Some(menu_area));

    let block = diff_menu_block(app.theme);
    let inner = block.inner(menu_area);
    let active = app.current_diff_choice();
    let selected = app.diff_menu_selected.min(choices.len().saturating_sub(1));
    let mut lines: Vec<_> = choices
        .iter()
        .enumerate()
        .take(inner.height.saturating_sub(1) as usize)
        .map(|(index, choice)| {
            let highlighted = index == selected;
            let active = active == Some(*choice);
            let cursor = if highlighted { "›" } else { " " };
            let marker = if active { "✓" } else { " " };
            let label = choice.label();
            let detail = diff_choice_detail(app, *choice);
            let number = index + 1;
            let left = format!(" {number} {cursor} {marker} {label}");
            let available = inner.width as usize;
            let gap = available.saturating_sub(left.width().saturating_add(detail.width()));
            let text = fit_padded(
                &format!("{left}{}{}", " ".repeat(gap.max(2)), detail),
                available,
            );
            let mut style = Style::default()
                .fg(app.theme.foreground)
                .bg(base_bg(app.theme));
            if active {
                style = style.add_modifier(Modifier::BOLD);
            }
            if highlighted {
                style = style
                    .fg(app.theme.header)
                    .bg(header_bg(app.theme))
                    .add_modifier(Modifier::BOLD);
            }
            Line::from(Span::styled(text, style))
        })
        .collect();

    if inner.height as usize > lines.len() {
        lines.push(Line::from(Span::styled(
            fit_padded(&diff_menu_footer(&app.keymap), inner.width as usize),
            Style::default().fg(app.theme.muted).bg(base_bg(app.theme)),
        )));
    }

    frame.render_widget(Clear, menu_area);
    frame.render_widget(block, menu_area);
    frame.render_widget(
        Paragraph::new(Text::from(lines)).style(Style::default().bg(base_bg(app.theme))),
        inner,
    );
}

pub(crate) fn diff_menu_area(app: &DiffApp, area: Rect, choices: &[DiffChoice]) -> Option<Rect> {
    if !app.diff_menu_open || area.width < 24 || area.height < 5 || choices.is_empty() {
        return None;
    }

    let width = diff_menu_floating_width(app, choices).min(area.width);
    let height = (choices.len() as u16).saturating_add(3).min(area.height);
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
        .border_style(Style::default().fg(theme.muted).bg(bg))
        .style(Style::default().bg(bg))
        .padding(Padding::horizontal(1))
        .title(Line::from(Span::styled(
            " diff source ",
            Style::default()
                .fg(theme.header)
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        )))
}

fn diff_menu_floating_width(app: &DiffApp, choices: &[DiffChoice]) -> u16 {
    let footer = diff_menu_footer(&app.keymap).width();
    let rows = choices
        .iter()
        .enumerate()
        .map(|(index, choice)| {
            format!(
                " {} › ✓ {}  {} ",
                index + 1,
                choice.label(),
                diff_choice_detail(app, *choice)
            )
            .width()
        })
        .max()
        .unwrap_or_default();
    rows.max(footer).max(36).saturating_add(4).min(72) as u16
}

fn diff_menu_footer(keymap: &Keymap) -> String {
    format!(
        " 1-4 switch · {} move · {}/{} apply · {} close ",
        menu_move_label(keymap),
        keymap.menu_action_label(MenuAction::Select),
        keymap.menu_action_label(MenuAction::Confirm),
        keymap.menu_action_label(MenuAction::Close),
    )
}

fn diff_choice_detail(app: &DiffApp, choice: DiffChoice) -> String {
    match choice {
        DiffChoice::All => "HEAD → working tree".to_owned(),
        DiffChoice::Unstaged => "index → working tree".to_owned(),
        DiffChoice::Staged => "HEAD → index".to_owned(),
        DiffChoice::Branch => match app.branch_base.as_deref() {
            Some(base) => {
                let head = app
                    .branch_head
                    .as_deref()
                    .or(app.current_head.as_deref())
                    .unwrap_or("HEAD");
                format!("{head} → {base}")
            }
            None => "base unavailable".to_owned(),
        },
    }
}

pub(crate) fn draw_options_menu(frame: &mut Frame<'_>, app: &DiffApp, area: Rect) {
    if !app.options_menu_open || area.width < 24 || area.height < 5 {
        return;
    }

    let width = options_menu_width(app).min(area.width);
    let items = app.options_menu_items();
    let height = (items.len() as u16).saturating_add(3).min(area.height);
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
    let selected = app.options_menu_selected.min(items.len().saturating_sub(1));
    let mut lines: Vec<_> = items
        .iter()
        .enumerate()
        .take(inner.height.saturating_sub(1) as usize)
        .map(|(index, item)| {
            let highlighted = index == selected;
            let cursor = if highlighted { "›" } else { " " };
            let label = option_label(*item);
            let value = option_value(app, *item);
            let left = format!(" {cursor} {label}");
            let available = inner.width as usize;
            let gap = available.saturating_sub(left.width().saturating_add(value.width()));
            let text = fit_padded(
                &format!("{left}{}{}", " ".repeat(gap.max(2)), value),
                available,
            );
            let mut style = Style::default()
                .fg(app.theme.foreground)
                .bg(base_bg(app.theme));
            if highlighted {
                style = style
                    .fg(app.theme.header)
                    .bg(header_bg(app.theme))
                    .add_modifier(Modifier::BOLD);
            }
            Line::from(Span::styled(text, style))
        })
        .collect();

    if inner.height as usize > lines.len() {
        lines.push(Line::from(Span::styled(
            fit_padded(&options_menu_footer(&app.keymap), inner.width as usize),
            Style::default().fg(app.theme.muted).bg(base_bg(app.theme)),
        )));
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
        .border_style(Style::default().fg(theme.muted).bg(bg))
        .style(Style::default().bg(bg))
        .padding(Padding::horizontal(1))
        .title(Line::from(Span::styled(
            " options ",
            Style::default()
                .fg(theme.header)
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        )))
}

fn options_menu_width(app: &DiffApp) -> u16 {
    let footer = options_menu_footer(&app.keymap).width();
    let rows = app
        .options_menu_items()
        .iter()
        .map(|item| format!(" › {}  {} ", option_label(*item), option_value(app, *item)).width())
        .max()
        .unwrap_or_default();
    rows.max(footer).max(36).saturating_add(4).min(72) as u16
}

fn options_menu_footer(keymap: &Keymap) -> String {
    format!(
        " {} move · {} toggle/open · {} apply/open · {} close ",
        menu_move_label(keymap),
        keymap.menu_action_label(MenuAction::Select),
        keymap.menu_action_label(MenuAction::Confirm),
        keymap.menu_action_label(MenuAction::Close),
    )
}

fn menu_move_label(keymap: &Keymap) -> String {
    format!(
        "{}/{}",
        keymap.menu_action_label(MenuAction::Down),
        keymap.menu_action_label(MenuAction::Up)
    )
}

fn option_label(item: OptionsMenuItem) -> &'static str {
    match item {
        OptionsMenuItem::BranchHead => "Head branch",
        OptionsMenuItem::BranchBase => "Base branch",
        OptionsMenuItem::Layout => "Layout",
        OptionsMenuItem::FileSidebar => "File sidebar",
        OptionsMenuItem::IncludeUntracked => "Include untracked",
        OptionsMenuItem::LiveReload => "Live reload",
        OptionsMenuItem::ContextExpansion => "Context expand",
        OptionsMenuItem::ColorScheme => "Colorscheme",
    }
}

fn option_value(app: &DiffApp, item: OptionsMenuItem) -> String {
    match item {
        OptionsMenuItem::BranchHead => branch_option_value(app, crate::controls::BranchMenu::Head),
        OptionsMenuItem::BranchBase => branch_option_value(app, crate::controls::BranchMenu::Base),
        OptionsMenuItem::Layout => match app.options_menu_draft.layout {
            crate::controls::DiffLayoutMode::Split => "split".to_owned(),
            crate::controls::DiffLayoutMode::Unified => "unified".to_owned(),
        },
        OptionsMenuItem::FileSidebar => on_off(app.options_menu_draft.file_sidebar_open),
        OptionsMenuItem::IncludeUntracked => on_off(app.options_menu_draft.include_untracked),
        OptionsMenuItem::LiveReload if !app.live_updates_allowed => "disabled".to_owned(),
        OptionsMenuItem::LiveReload => on_off(app.options_menu_draft.live_updates_enabled),
        OptionsMenuItem::ContextExpansion => {
            context_expansion_label(app.options_menu_draft.context_expansion)
        }
        OptionsMenuItem::ColorScheme => {
            color_scheme_label(app.options_menu_draft.color_scheme).to_owned()
        }
    }
}

fn branch_option_value(app: &DiffApp, menu: crate::controls::BranchMenu) -> String {
    app.branch_ref(menu)
        .map(|branch| app.branch_label(menu, branch))
        .unwrap_or_else(|| "unavailable".to_owned())
}

fn on_off(enabled: bool) -> String {
    if enabled { "on" } else { "off" }.to_owned()
}

pub(crate) fn draw_color_scheme_picker(frame: &mut Frame<'_>, app: &DiffApp, area: Rect) {
    if !app.color_scheme_picker_open || area.width < 28 || area.height < 6 {
        return;
    }

    let width = color_scheme_picker_width(app).min(area.width);
    let height = (app.visible_color_scheme_rows() as u16)
        .saturating_add(4)
        .min(area.height);
    if width == 0 || height == 0 {
        return;
    }

    let picker_area = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    };
    let block = color_scheme_picker_block(app.theme);
    let inner = block.inner(picker_area);
    let mut lines = Vec::new();
    let input = if app.color_scheme_input.is_empty() {
        INPUT_CURSOR.to_owned()
    } else {
        format!("{}{}", app.color_scheme_input, INPUT_CURSOR)
    };
    lines.push(Line::from(Span::styled(
        fit_padded(&format!(" filter {input}"), inner.width as usize),
        Style::default()
            .fg(app.theme.foreground)
            .bg(base_bg(app.theme)),
    )));

    let choices = app.filtered_color_schemes();
    if choices.is_empty() {
        lines.push(Line::from(Span::styled(
            fit_padded(" no matching colorscheme", inner.width as usize),
            Style::default().fg(app.theme.muted).bg(base_bg(app.theme)),
        )));
    } else {
        lines.extend(
            choices
                .iter()
                .enumerate()
                .skip(app.color_scheme_scroll)
                .take(inner.height.saturating_sub(2) as usize)
                .map(|(index, choice)| {
                    let highlighted = index == app.color_scheme_selected;
                    let active = *choice == app.options_menu_draft.color_scheme;
                    let cursor = if highlighted { "›" } else { " " };
                    let marker = if active { "✓" } else { " " };
                    let label = color_scheme_label(*choice);
                    let text =
                        fit_padded(&format!(" {cursor} {marker} {label}"), inner.width as usize);
                    let mut style = Style::default()
                        .fg(app.theme.foreground)
                        .bg(base_bg(app.theme));
                    if active {
                        style = style.add_modifier(Modifier::BOLD);
                    }
                    if highlighted {
                        style = style
                            .fg(app.theme.header)
                            .bg(header_bg(app.theme))
                            .add_modifier(Modifier::BOLD);
                    }
                    Line::from(Span::styled(text, style))
                }),
        );
    }

    if inner.height as usize > lines.len() {
        lines.push(Line::from(Span::styled(
            fit_padded(
                " type filter · j/k move · Enter choose · Esc close ",
                inner.width as usize,
            ),
            Style::default().fg(app.theme.muted).bg(base_bg(app.theme)),
        )));
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
        .border_style(Style::default().fg(theme.muted).bg(bg))
        .style(Style::default().bg(bg))
        .padding(Padding::horizontal(1))
        .title(Line::from(Span::styled(
            " colorscheme ",
            Style::default()
                .fg(theme.header)
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        )))
}

fn color_scheme_picker_width(app: &DiffApp) -> u16 {
    let footer = " type filter · j/k move · Enter choose · Esc close ".width();
    let input = app
        .color_scheme_input
        .width()
        .saturating_add(" filter ".width() + 1);
    let rows = app
        .filtered_color_schemes()
        .iter()
        .map(|choice| format!(" › ✓ {} ", color_scheme_label(*choice)).width())
        .max()
        .unwrap_or_else(|| " no matching colorscheme ".width());
    rows.max(input)
        .max(footer)
        .max(36)
        .saturating_add(4)
        .min(64) as u16
}

pub(crate) fn draw_branch_menu(frame: &mut Frame<'_>, app: &DiffApp, area: Rect) {
    let Some(menu) = app.branch_menu_open else {
        return;
    };
    if area.height <= 1 || app.comparison_branches.is_empty() {
        return;
    }

    let x = app
        .branch_selector_start(menu)
        .unwrap_or_default()
        .min(area.width);
    let width = app.branch_menu_width().min(area.width.saturating_sub(x));
    let height = (app.branch_menu_height() as u16).min(area.height - 1);
    if width == 0 || height == 0 {
        return;
    }

    let menu_area = Rect {
        x: area.x + x,
        y: area.y + 1,
        width,
        height,
    };
    let selected = app.branch_ref(menu);
    let matches = app.filtered_branches();
    let lines: Vec<_> = if matches.is_empty() {
        vec![Line::from(Span::styled(
            fit_padded("   no matches", width as usize),
            Style::default()
                .fg(app.theme.muted)
                .bg(header_bg(app.theme)),
        ))]
    } else {
        matches
            .iter()
            .enumerate()
            .skip(app.branch_menu_scroll)
            .take(height as usize)
            .map(|(index, branch)| {
                let active = selected == Some(*branch);
                let highlighted = index == app.branch_menu_selected;
                let marker = if active {
                    "✓"
                } else if highlighted {
                    "›"
                } else {
                    " "
                };
                let branch_marker = app.branch_marker(menu, branch).unwrap_or(" ");
                let text = fit_padded(
                    &format!(" {marker} {branch_marker} {branch}"),
                    width as usize,
                );
                let mut style = if active || highlighted {
                    Style::default()
                        .fg(app.theme.header)
                        .bg(header_bg(app.theme))
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                        .fg(app.theme.foreground)
                        .bg(header_bg(app.theme))
                };
                if highlighted {
                    style = style.add_modifier(Modifier::REVERSED);
                }
                Line::from(Span::styled(text, style))
            })
            .collect()
    };

    frame.render_widget(Clear, menu_area);
    frame.render_widget(
        Paragraph::new(Text::from(lines)).style(Style::default().bg(header_bg(app.theme))),
        menu_area,
    );
}

pub(crate) fn draw_help_menu(frame: &mut Frame<'_>, app: &DiffApp, area: Rect) {
    if !app.help_menu_open || area.width < 4 || area.height < 3 {
        return;
    }

    let width = HELP_MENU_WIDTH.min(area.width);
    let content_width = width
        .saturating_sub(2)
        .saturating_sub(HELP_MENU_HORIZONTAL_PADDING.saturating_mul(2))
        as usize;
    let desired_height = (help_menu_content_rows(content_width) as u16)
        .saturating_add(2)
        .saturating_add(HELP_MENU_VERTICAL_PADDING.saturating_mul(2));
    let height = desired_height.min(area.height);
    if height == 0 {
        return;
    }

    let menu_area = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    };

    let block = help_menu_block(app.theme);
    let inner = block.inner(menu_area);

    frame.render_widget(Clear, menu_area);
    frame.render_widget(block, menu_area);
    frame.render_widget(
        Paragraph::new(Text::from(help_menu_lines(
            inner.width as usize,
            inner.height as usize,
            app.theme,
            &app.keymap,
        )))
        .style(Style::default().bg(help_menu_bg(app.theme))),
        inner,
    );
}

pub(crate) fn help_menu_block(theme: DiffTheme) -> Block<'static> {
    let bg = help_menu_bg(theme);
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.muted).bg(bg))
        .style(Style::default().bg(bg))
        .padding(Padding::new(
            HELP_MENU_HORIZONTAL_PADDING,
            HELP_MENU_HORIZONTAL_PADDING,
            HELP_MENU_VERTICAL_PADDING,
            HELP_MENU_VERTICAL_PADDING,
        ))
        .title(Line::from(Span::styled(
            " keybindings ",
            Style::default()
                .fg(help_menu_title_color(theme))
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        )))
}

pub(crate) fn help_menu_bg(theme: DiffTheme) -> Color {
    base_bg(theme)
}

pub(crate) fn help_menu_title_color(theme: DiffTheme) -> Color {
    theme.syntax.keyword.unwrap_or(theme.hunk)
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

pub(crate) fn help_menu_lines(
    width: usize,
    height: usize,
    theme: DiffTheme,
    keymap: &Keymap,
) -> Vec<Line<'static>> {
    if help_menu_uses_two_columns(width) {
        return (0..height.min(help_menu_content_rows(width)))
            .map(|index| help_menu_columns_line(index, width, theme, keymap))
            .collect();
    }

    HELP_MENU_LEFT_ROWS
        .iter()
        .chain(HELP_MENU_RIGHT_ROWS)
        .take(height)
        .map(|row| Line::from(help_menu_row_spans(*row, width, theme, keymap)))
        .collect()
}

pub(crate) fn help_menu_columns_line(
    index: usize,
    width: usize,
    theme: DiffTheme,
    keymap: &Keymap,
) -> Line<'static> {
    let gap_width = HELP_MENU_COLUMN_GAP.min(width);
    let left_width = width.saturating_sub(gap_width) / 2;
    let right_width = width.saturating_sub(left_width).saturating_sub(gap_width);
    let bg = help_menu_bg(theme);

    let mut spans = help_menu_row_at(HELP_MENU_LEFT_ROWS, index)
        .map(|row| help_menu_row_spans(row, left_width, theme, keymap))
        .unwrap_or_else(|| help_menu_empty_spans(left_width, bg));
    spans.push(Span::styled(" ".repeat(gap_width), Style::default().bg(bg)));
    spans.extend(
        help_menu_row_at(HELP_MENU_RIGHT_ROWS, index)
            .map(|row| help_menu_row_spans(row, right_width, theme, keymap))
            .unwrap_or_else(|| help_menu_empty_spans(right_width, bg)),
    );

    Line::from(spans)
}

pub(crate) fn help_menu_row_at(rows: &[HelpMenuRow], index: usize) -> Option<HelpMenuRow> {
    rows.get(index).copied()
}

pub(crate) fn help_menu_content_rows(width: usize) -> usize {
    if help_menu_uses_two_columns(width) {
        HELP_MENU_LEFT_ROWS.len().max(HELP_MENU_RIGHT_ROWS.len())
    } else {
        HELP_MENU_LEFT_ROWS.len() + HELP_MENU_RIGHT_ROWS.len()
    }
}

pub(crate) fn help_menu_uses_two_columns(width: usize) -> bool {
    width >= HELP_MENU_TWO_COLUMN_MIN_WIDTH
}

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

pub(crate) fn help_menu_empty_spans(width: usize, bg: Color) -> Vec<Span<'static>> {
    vec![Span::styled(" ".repeat(width), Style::default().bg(bg))]
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
        DiffSource::Patch(dx_diff::PatchSource::File(path)) => format!("patch {}", path.display()),
        DiffSource::Patch(dx_diff::PatchSource::Stdin(_)) => "patch stdin".to_owned(),
        DiffSource::Patch(dx_diff::PatchSource::Text { label, .. }) => label.clone(),
    }
}

pub(crate) fn branch_menu_width(branches: &[String]) -> u16 {
    branches
        .iter()
        .map(|branch| branch.width() + 6)
        .max()
        .unwrap_or_default() as u16
}
