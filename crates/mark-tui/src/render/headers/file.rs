use ratatui::prelude::{Line, Modifier, Span, Style};

use crate::{
    controls::DiffLayoutMode,
    render::{
        headers::{HeaderStyles, file_delta_parts, header_spans},
        style::{diff_base_bg, file_sidebar_style},
        text::status_code,
    },
    theme::DiffTheme,
};

pub(crate) fn file_separator_line(
    _layout: DiffLayoutMode,
    width: usize,
    theme: DiffTheme,
) -> Line<'static> {
    if width == 0 {
        return Line::default();
    }

    Line::from(Span::styled(
        "─".repeat(width),
        Style::default()
            .fg(theme.empty_diff)
            .bg(diff_base_bg(theme)),
    ))
}

pub(crate) fn file_header_line(
    file: &mark_diff::DiffFile,
    width: usize,
    theme: DiffTheme,
) -> Line<'static> {
    Line::from(file_header_spans(file, width, theme))
}

pub(crate) fn file_header_spans(
    file: &mark_diff::DiffFile,
    width: usize,
    theme: DiffTheme,
) -> Vec<Span<'static>> {
    let bg = diff_base_bg(theme);
    header_spans(
        status_code(file.status()),
        file.display_path(),
        &file_delta_parts(file.additions, file.deletions),
        width,
        HeaderStyles {
            prefix: file_sidebar_style(file.status(), theme)
                .bg(bg)
                .add_modifier(Modifier::BOLD),
            body: Style::default().fg(theme.foreground).bg(bg),
            fill: Style::default().bg(bg),
            addition: Style::default().fg(theme.addition_fg).bg(bg),
            deletion: Style::default().fg(theme.deletion_fg).bg(bg),
        },
    )
}
