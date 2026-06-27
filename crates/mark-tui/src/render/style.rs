use mark_diff::{DiffLineKind, FileStatus};
use mark_syntax::DiffSignStyle;
use ratatui::prelude::{Color, Modifier, Span, Style};

use crate::{
    controls::INPUT_CURSOR,
    theme::{DIFF_INDICATOR, DiffTheme, line_gutter_bg},
};

pub(crate) fn file_sidebar_style(status: FileStatus, theme: DiffTheme) -> Style {
    let color = match status {
        FileStatus::Added | FileStatus::Copied => theme.addition_fg,
        FileStatus::Deleted => theme.deletion_fg,
        FileStatus::Modified | FileStatus::Renamed | FileStatus::TypeChanged => theme.hunk,
        FileStatus::Unknown => theme.muted,
    };
    Style::default().fg(color)
}

pub(crate) fn diff_sign_style(kind: DiffLineKind, theme: DiffTheme) -> Style {
    let mut style = Style::default()
        .fg(diff_indicator_fg(kind, theme))
        .bg(line_gutter_bg(kind, theme));
    if theme.diff.sign_style == DiffSignStyle::Bold
        && matches!(kind, DiffLineKind::Addition | DiffLineKind::Deletion)
    {
        style = style.add_modifier(Modifier::BOLD);
    }
    style
}

pub(crate) fn diff_indicator_span(kind: DiffLineKind, theme: DiffTheme) -> Span<'static> {
    Span::styled(DIFF_INDICATOR, diff_indicator_style(kind, theme))
}

pub(crate) fn focused_diff_indicator_span(kind: DiffLineKind, theme: DiffTheme) -> Span<'static> {
    Span::styled(DIFF_INDICATOR, focused_diff_indicator_style(kind, theme))
}

pub(crate) fn diff_indicator_style(kind: DiffLineKind, theme: DiffTheme) -> Style {
    Style::default()
        .fg(diff_indicator_fg(kind, theme))
        .bg(line_gutter_bg(kind, theme))
}

pub(crate) fn focused_diff_indicator_style(kind: DiffLineKind, theme: DiffTheme) -> Style {
    diff_indicator_style(kind, theme)
        .fg(theme.hunk)
        .add_modifier(Modifier::BOLD)
}

pub(crate) fn diff_indicator_fg(kind: DiffLineKind, theme: DiffTheme) -> Color {
    match kind {
        DiffLineKind::Addition => theme.addition_fg,
        DiffLineKind::Deletion => theme.deletion_fg,
        DiffLineKind::Context | DiffLineKind::Meta => theme.muted,
    }
}

pub(crate) fn base_bg(theme: DiffTheme) -> Color {
    if theme.transparent_background {
        Color::Reset
    } else {
        theme.background
    }
}

pub(crate) fn header_bg(theme: DiffTheme) -> Color {
    if theme.transparent_background {
        Color::Reset
    } else {
        theme.gutter_bg
    }
}

pub(crate) fn statusline_bg(theme: DiffTheme) -> Color {
    if theme.transparent_background {
        Color::Reset
    } else {
        theme.statusline_bg
    }
}

pub(crate) fn input_cursor_style(theme: DiffTheme, bg: Color) -> Style {
    Style::default()
        .fg(theme.cursor)
        .bg(bg)
        .add_modifier(Modifier::BOLD)
}

/// Split `text` on the block caret and apply `cursor_style` to each caret span.
pub(crate) fn spans_with_input_cursor(
    text: &str,
    text_style: Style,
    cursor_style: Style,
) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut rest = text;
    while let Some(index) = rest.find(INPUT_CURSOR) {
        if index > 0 {
            spans.push(Span::styled(rest[..index].to_owned(), text_style));
        }
        spans.push(Span::styled(INPUT_CURSOR.to_owned(), cursor_style));
        rest = &rest[index + INPUT_CURSOR.len()..];
    }
    if !rest.is_empty() {
        spans.push(Span::styled(rest.to_owned(), text_style));
    }
    if spans.is_empty() {
        spans.push(Span::styled(String::new(), text_style));
    }
    spans
}
