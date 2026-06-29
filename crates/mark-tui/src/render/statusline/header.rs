use ratatui::{
    Frame,
    layout::Rect,
    prelude::{Line, Modifier, Span, Style},
    widgets::Paragraph,
};
use unicode_width::UnicodeWidthStr;

use crate::{
    app::DiffApp,
    controls::BranchMenu,
    render::{
        menus::{diff_comparison_label, diff_selector_text},
        style::statusline_bg,
        text::{fit, fit_with_ellipsis, format_count, progress_label},
    },
    theme::{BRANCH_COMPARISON_SEPARATOR, STATUSLINE_SELECTOR_GAP},
};

pub(crate) fn draw_header(frame: &mut Frame<'_>, app: &DiffApp, area: Rect) {
    let line = statusline_header_line(app, area.width as usize);
    frame.render_widget(
        Paragraph::new(line).style(Style::default().bg(statusline_bg(app.config.theme))),
        area,
    );
}

pub(crate) fn statusline_header_line(app: &DiffApp, width: usize) -> Line<'static> {
    if width == 0 {
        return Line::default();
    }

    let right_max_width = statusline_right_max_width(width);
    let right = statusline_file_label(app, right_max_width);
    let right_width = right.width();
    let mut left_width = width.saturating_sub(right_width);
    let mut spans = Vec::new();

    push_statusline_left_spans(&mut spans, app, &mut left_width);
    let left_used = width.saturating_sub(right_width).saturating_sub(left_width);
    let gap = width.saturating_sub(left_used).saturating_sub(right_width);
    if gap > 0 {
        spans.push(Span::styled(
            " ".repeat(gap),
            Style::default().bg(statusline_bg(app.config.theme)),
        ));
    }
    if right_width > 0 {
        spans.push(Span::styled(
            right,
            Style::default()
                .fg(app.config.theme.statusline_info_fg)
                .bg(app.config.theme.statusline_info_bg)
                .add_modifier(Modifier::BOLD),
        ));
    }

    Line::from(spans)
}

pub(crate) fn push_statusline_left_spans(
    spans: &mut Vec<Span<'static>>,
    app: &DiffApp,
    remaining: &mut usize,
) {
    push_fitted_statusline_span(
        spans,
        diff_selector_text(&app.document.options),
        Style::default()
            .fg(app.config.theme.statusline_accent_fg)
            .bg(app.config.theme.statusline_accent_bg)
            .add_modifier(Modifier::BOLD),
        remaining,
    );
    push_fitted_statusline_span(
        spans,
        STATUSLINE_SELECTOR_GAP,
        Style::default().bg(statusline_bg(app.config.theme)),
        remaining,
    );
    if app.is_show_diff()
        && let Some(commit) = app.commit_selector_text()
    {
        push_fitted_statusline_span(
            spans,
            commit,
            Style::default()
                .fg(app.config.theme.header)
                .bg(statusline_bg(app.config.theme))
                .add_modifier(Modifier::BOLD),
            remaining,
        );
    } else if app.is_branch_diff()
        && let (Some(head), Some(base)) = (
            app.branch_selector_text(BranchMenu::Head),
            app.branch_selector_text(BranchMenu::Base),
        )
    {
        push_fitted_statusline_span(
            spans,
            head,
            Style::default()
                .fg(app.config.theme.header)
                .bg(statusline_bg(app.config.theme))
                .add_modifier(Modifier::BOLD),
            remaining,
        );
        push_fitted_statusline_span(
            spans,
            BRANCH_COMPARISON_SEPARATOR,
            Style::default()
                .fg(app.config.theme.muted)
                .bg(statusline_bg(app.config.theme)),
            remaining,
        );
        push_fitted_statusline_span(
            spans,
            base,
            Style::default()
                .fg(app.config.theme.header)
                .bg(statusline_bg(app.config.theme))
                .add_modifier(Modifier::BOLD),
            remaining,
        );
    } else {
        push_fitted_statusline_span(
            spans,
            diff_comparison_label(&app.document.options),
            Style::default()
                .fg(app.config.theme.muted)
                .bg(statusline_bg(app.config.theme)),
            remaining,
        );
    }
    push_fitted_statusline_span(
        spans,
        "  ",
        Style::default().bg(statusline_bg(app.config.theme)),
        remaining,
    );
    push_fitted_statusline_span(
        spans,
        statusline_file_count_label(app),
        Style::default()
            .fg(app.config.theme.foreground)
            .bg(statusline_bg(app.config.theme)),
        remaining,
    );
    push_fitted_statusline_span(
        spans,
        "  ",
        Style::default().bg(statusline_bg(app.config.theme)),
        remaining,
    );
    push_fitted_statusline_span(
        spans,
        format!("+{}", format_count(app.document.stats.additions)),
        Style::default()
            .fg(app.config.theme.addition_fg)
            .bg(statusline_bg(app.config.theme))
            .add_modifier(Modifier::BOLD),
        remaining,
    );
    push_fitted_statusline_span(
        spans,
        " ",
        Style::default().bg(statusline_bg(app.config.theme)),
        remaining,
    );
    push_fitted_statusline_span(
        spans,
        format!("-{}", format_count(app.document.stats.deletions)),
        Style::default()
            .fg(app.config.theme.deletion_fg)
            .bg(statusline_bg(app.config.theme))
            .add_modifier(Modifier::BOLD),
        remaining,
    );
}

pub(crate) fn statusline_file_count_label(app: &DiffApp) -> String {
    if app.filters.active() {
        format!(
            "{}/{} files",
            format_count(app.document.stats.files),
            format_count(app.document.total_stats.files)
        )
    } else {
        format!("{} files", format_count(app.document.stats.files))
    }
}

pub(crate) fn push_fitted_statusline_span(
    spans: &mut Vec<Span<'static>>,
    text: impl AsRef<str>,
    style: Style,
    remaining: &mut usize,
) {
    if *remaining == 0 {
        return;
    }

    let text = fit(text.as_ref(), *remaining);
    if text.is_empty() {
        return;
    }

    *remaining = (*remaining).saturating_sub(text.width());
    spans.push(Span::styled(text, style));
}

pub(crate) fn statusline_right_max_width(width: usize) -> usize {
    if width <= 24 {
        width
    } else {
        (width / 2).max(24).min(width)
    }
}

pub(crate) fn statusline_file_label(app: &DiffApp, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }

    let progress = progress_label(app.viewport.scroll, app.max_scroll());
    let file_count = app.document.model.visible_files().len();
    let file_number = app
        .document
        .model
        .visible_file_position(app.sidebar.selected_file.get())
        .map(|position| position + 1)
        .unwrap_or_default();
    let position = format!("{file_number}/{file_count} {progress}");
    let fallback = "No file";
    let path = app
        .document
        .changeset
        .files
        .get(app.sidebar.selected_file.get())
        .map(|file| file.display_path())
        .unwrap_or(fallback);

    let compact = format!(" {position} ");
    let compact_width = compact.width();
    if max_width <= compact_width {
        return fit(&compact, max_width);
    }

    let path_width = max_width.saturating_sub(position.width()).saturating_sub(3);
    let label = format!(" {} {} ", fit_with_ellipsis(path, path_width), position);
    if label.width() > max_width {
        fit(&label, max_width)
    } else {
        label
    }
}
