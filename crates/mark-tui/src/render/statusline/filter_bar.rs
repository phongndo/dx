use ratatui::{
    Frame,
    layout::Rect,
    prelude::{Line, Modifier, Span, Style},
    widgets::Paragraph,
};
use unicode_width::UnicodeWidthStr;

use crate::{
    app::DiffApp,
    controls::{DiffFilterKind, INPUT_CURSOR},
    render::{
        style::statusline_bg,
        text::{fit, format_count},
    },
};

pub(crate) fn draw_filter_bar(frame: &mut Frame<'_>, app: &DiffApp, area: Rect) {
    let line = filter_bar_line(app, area.width as usize);
    frame.render_widget(
        Paragraph::new(line).style(Style::default().bg(statusline_bg(app.config.theme))),
        area,
    );
}

pub(crate) fn filter_bar_visible(app: &DiffApp) -> bool {
    app.filters.filter_input.is_some() || app.filters.active()
}

pub(crate) fn filter_bar_line(app: &DiffApp, width: usize) -> Line<'static> {
    if width == 0 {
        return Line::default();
    }

    if filter_bar_visible(app) {
        return filter_status_line(app, width);
    }

    blank_filter_bar_line(app, width)
}

pub(crate) fn filter_status_line(app: &DiffApp, width: usize) -> Line<'static> {
    let bg = statusline_bg(app.config.theme);
    let right = filter_status_right_label(app);
    let right_width = right.as_deref().map(str::width).unwrap_or_default();
    let mut left_width = width.saturating_sub(right_width);
    let mut spans = Vec::new();

    if app.filters.filter_input == Some(DiffFilterKind::File) || !app.filters.file_filter.is_empty()
    {
        push_file_filter_bar_spans(app, &mut spans, &mut left_width);
    }

    if app.filters.filter_input == Some(DiffFilterKind::Grep) || !app.filters.grep_filter.is_empty()
    {
        if !spans.is_empty() {
            push_filter_bar_span(&mut spans, "  ", Style::default().bg(bg), &mut left_width);
        }
        push_grep_filter_bar_spans(app, &mut spans, &mut left_width);
    }

    let left_used = width.saturating_sub(right_width).saturating_sub(left_width);
    let gap = width.saturating_sub(left_used).saturating_sub(right_width);
    if gap > 0 {
        spans.push(Span::styled(" ".repeat(gap), Style::default().bg(bg)));
    }
    if let Some(right) = right
        && right_width > 0
    {
        spans.push(Span::styled(
            right,
            Style::default().fg(app.config.theme.muted).bg(bg),
        ));
    }

    Line::from(spans)
}

pub(crate) fn blank_filter_bar_line(app: &DiffApp, width: usize) -> Line<'static> {
    Line::from(Span::styled(
        " ".repeat(width),
        Style::default().bg(statusline_bg(app.config.theme)),
    ))
}

pub(crate) fn filter_status_right_label(app: &DiffApp) -> Option<String> {
    if app.filter_busy() {
        return Some("…".to_owned());
    }

    if !app.filters.grep_filter.is_empty() {
        let total = app.filters.grep_matches.len();
        if total == 0 {
            return Some("0".to_owned());
        }
        let current = app
            .filters
            .selected_grep_match
            .map(|index| index.saturating_add(1).min(total))
            .unwrap_or(1);
        let total = if app.filters.grep_matches_truncated {
            "10k+".to_owned()
        } else {
            format_count(total)
        };
        return Some(format!("{}/{total}", format_count(current)));
    }

    (!app.filters.file_filter.is_empty()).then(|| {
        format!(
            "{}/{} files",
            format_count(app.document.stats.files),
            format_count(app.document.total_stats.files)
        )
    })
}

pub(crate) fn push_file_filter_bar_spans(
    app: &DiffApp,
    spans: &mut Vec<Span<'static>>,
    remaining: &mut usize,
) {
    let bg = statusline_bg(app.config.theme);
    let query = filter_bar_query(app, DiffFilterKind::File);
    let active = app.filters.filter_input == Some(DiffFilterKind::File);
    push_filter_bar_span(
        spans,
        "@",
        Style::default()
            .fg(app.config.theme.statusline_fg)
            .bg(bg)
            .add_modifier(Modifier::BOLD),
        remaining,
    );
    if active {
        push_active_filter_query_spans(app, DiffFilterKind::File, spans, remaining);
        push_filter_bar_cursor_span(app, spans, remaining);
        push_active_filter_query_suffix_spans(app, DiffFilterKind::File, spans, remaining);
    } else {
        push_filter_bar_span(
            spans,
            query,
            Style::default().fg(app.config.theme.statusline_fg).bg(bg),
            remaining,
        );
    }
}

pub(crate) fn push_grep_filter_bar_spans(
    app: &DiffApp,
    spans: &mut Vec<Span<'static>>,
    remaining: &mut usize,
) {
    let bg = statusline_bg(app.config.theme);
    let query = filter_bar_query(app, DiffFilterKind::Grep);
    let active = app.filters.filter_input == Some(DiffFilterKind::Grep);
    push_filter_bar_span(
        spans,
        "/",
        Style::default()
            .fg(app.config.theme.statusline_fg)
            .bg(bg)
            .add_modifier(Modifier::BOLD),
        remaining,
    );
    if active {
        push_active_filter_query_spans(app, DiffFilterKind::Grep, spans, remaining);
        push_filter_bar_cursor_span(app, spans, remaining);
        push_active_filter_query_suffix_spans(app, DiffFilterKind::Grep, spans, remaining);
    } else {
        push_filter_bar_span(
            spans,
            query,
            Style::default().fg(app.config.theme.statusline_fg).bg(bg),
            remaining,
        );
    }
}

fn push_active_filter_query_spans(
    app: &DiffApp,
    kind: DiffFilterKind,
    spans: &mut Vec<Span<'static>>,
    remaining: &mut usize,
) {
    let (prefix, _) = active_filter_query_parts(app, kind);
    push_filter_bar_span(
        spans,
        prefix,
        Style::default()
            .fg(app.config.theme.statusline_fg)
            .bg(statusline_bg(app.config.theme)),
        remaining,
    );
}

fn push_active_filter_query_suffix_spans(
    app: &DiffApp,
    kind: DiffFilterKind,
    spans: &mut Vec<Span<'static>>,
    remaining: &mut usize,
) {
    let (_, suffix) = active_filter_query_parts(app, kind);
    push_filter_bar_span(
        spans,
        suffix,
        Style::default()
            .fg(app.config.theme.statusline_fg)
            .bg(statusline_bg(app.config.theme)),
        remaining,
    );
}

fn active_filter_query_parts(app: &DiffApp, kind: DiffFilterKind) -> (&str, &str) {
    let query = app.filters.input_query(kind);
    let cursor = app.filters.input_cursor(kind).min(query.len());
    if query.is_char_boundary(cursor) {
        (&query[..cursor], &query[cursor..])
    } else {
        (query, "")
    }
}

pub(crate) fn push_filter_bar_cursor_span(
    app: &DiffApp,
    spans: &mut Vec<Span<'static>>,
    remaining: &mut usize,
) {
    push_filter_bar_span(
        spans,
        INPUT_CURSOR,
        crate::render::style::input_cursor_style(app.config.theme, statusline_bg(app.config.theme)),
        remaining,
    );
}

pub(crate) fn filter_bar_query(app: &DiffApp, kind: DiffFilterKind) -> &str {
    if app.filters.filter_input == Some(kind) {
        app.filters.input_query(kind)
    } else {
        app.filters.query(kind)
    }
}

pub(crate) fn push_filter_bar_span(
    spans: &mut Vec<Span<'static>>,
    text: &str,
    style: Style,
    remaining: &mut usize,
) {
    if *remaining == 0 {
        return;
    }

    let text = fit(text, *remaining);
    if text.is_empty() {
        return;
    }

    *remaining = (*remaining).saturating_sub(text.width());
    spans.push(Span::styled(text, style));
}
