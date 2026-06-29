use mark_diff::{DiffLine, DiffLineKind};
use mark_syntax::HighlightedLine;
use ratatui::prelude::{Color, Line, Span, Style};
use unicode_width::UnicodeWidthStr;

use crate::{
    app::{DiffApp, split_cell_content_width, wrapped_line_start_columns},
    controls::DiffLayoutMode,
    render::{
        style::{base_bg, diff_indicator_span},
        text::{fit_padded, format_count},
    },
    syntax::DiffSide,
    theme::DiffTheme,
};

use super::{
    split::{
        SplitCellRender, SplitGrepRender, SplitSide, highlight_wrapped_split_grep_line,
        split_cell_spans_at_scroll, split_cell_spans_at_scroll_with_focus_and_continuation,
        wrapped_segment_scroll,
    },
    unified::{render_unified_line_at_scroll, render_unified_line_wrapped_with_focus},
};

pub(crate) fn context_show_line(
    lines: usize,
    more: bool,
    marker: &str,
    width: usize,
    theme: DiffTheme,
) -> Line<'static> {
    if width == 0 {
        return Line::default();
    }

    let suffix = if lines == 1 { "line" } else { "lines" };
    let label = if more {
        format!(
            " {marker} show {} more unchanged {suffix}",
            format_count(lines)
        )
    } else {
        format!(" {marker} show {} unchanged {suffix}", format_count(lines))
    };
    context_action_line(&label, width, theme, theme.muted)
}

pub(crate) fn context_hide_line(
    lines: usize,
    marker: &str,
    width: usize,
    theme: DiffTheme,
) -> Line<'static> {
    let suffix = if lines == 1 { "line" } else { "lines" };
    context_action_line(
        &format!(" {marker} hide {} unchanged {suffix}", format_count(lines)),
        width,
        theme,
        theme.muted,
    )
}

pub(crate) fn context_expand_marker(hunk: usize) -> &'static str {
    if hunk == 0 { "▴" } else { "▾" }
}

pub(crate) fn context_hide_marker(hunk: usize) -> &'static str {
    if hunk == 0 { "▾" } else { "▴" }
}

pub(crate) fn context_action_line(
    label: &str,
    width: usize,
    theme: DiffTheme,
    text_color: Color,
) -> Line<'static> {
    if width == 0 {
        return Line::default();
    }

    let bg = base_bg(theme);
    let mut spans = Vec::new();
    let indicator_width = 1.min(width);
    if indicator_width > 0 {
        spans.push(diff_indicator_span(DiffLineKind::Meta, theme));
    }
    let content_width = width.saturating_sub(indicator_width);
    if content_width > 0 {
        spans.push(Span::styled(
            fit_padded(label, content_width),
            Style::default().fg(text_color).bg(bg),
        ));
    }
    Line::from(spans)
}

pub(crate) fn render_context_line(
    app: &mut DiffApp,
    file: usize,
    old_line: usize,
    new_line: usize,
    row_index: usize,
    width: usize,
) -> Line<'static> {
    let theme = app.config.theme;
    let horizontal_scroll = app.viewport.horizontal_scroll;
    let side = app.context_source_side(file);
    let syntax = side.and_then(|side| {
        let line_number = match side {
            DiffSide::Old => old_line,
            DiffSide::New => new_line,
        };
        app.syntax_file_line(file, side, line_number)
    });
    let diff_line = DiffLine::context(
        old_line,
        new_line,
        app.context_line_text(file, old_line, new_line),
    );

    match app.viewport.layout {
        DiffLayoutMode::Unified => render_unified_line_at_scroll(
            &diff_line,
            syntax.as_ref(),
            &[],
            row_index,
            width,
            theme,
            horizontal_scroll,
        ),
        DiffLayoutMode::Split => render_split_context_line(
            &diff_line,
            syntax.as_ref(),
            row_index,
            width,
            theme,
            horizontal_scroll,
        ),
    }
}

pub(crate) fn render_context_line_wrapped(
    app: &mut DiffApp,
    file: usize,
    old_line: usize,
    new_line: usize,
    row_index: usize,
    width: usize,
) -> Vec<Line<'static>> {
    let theme = app.config.theme;
    let side = app.context_source_side(file);
    let syntax = side.and_then(|side| {
        let line_number = match side {
            DiffSide::Old => old_line,
            DiffSide::New => new_line,
        };
        app.syntax_file_line(file, side, line_number)
    });
    let diff_line = DiffLine::context(
        old_line,
        new_line,
        app.context_line_text(file, old_line, new_line),
    );

    match app.viewport.layout {
        DiffLayoutMode::Unified => render_unified_line_wrapped_with_focus(
            &diff_line,
            syntax.as_ref(),
            &[],
            width,
            theme,
            false,
            &app.filters.grep_filter,
        ),
        DiffLayoutMode::Split => {
            let visual_row_start = app.wrapped_visual_scroll_for_model_row(row_index);
            render_split_context_line_wrapped(
                &diff_line,
                syntax.as_ref(),
                visual_row_start,
                width,
                theme,
                &app.filters.grep_filter,
            )
        }
    }
}

pub(crate) fn render_split_context_line(
    line: &DiffLine,
    syntax: Option<&HighlightedLine>,
    row_index: usize,
    width: usize,
    theme: DiffTheme,
    horizontal_scroll: usize,
) -> Line<'static> {
    if width == 0 {
        return Line::default();
    }

    let left_width = width / 2;
    let right_width = width.saturating_sub(left_width);
    let mut spans = split_cell_spans_at_scroll(
        Some(line),
        syntax,
        &[],
        SplitCellRender {
            side: SplitSide::Old,
            row_index,
            width: left_width,
            theme,
        },
        horizontal_scroll,
    );
    spans.extend(split_cell_spans_at_scroll(
        Some(line),
        syntax,
        &[],
        SplitCellRender {
            side: SplitSide::New,
            row_index,
            width: right_width,
            theme,
        },
        horizontal_scroll,
    ));
    Line::from(spans)
}

pub(crate) fn render_split_context_line_wrapped(
    line: &DiffLine,
    syntax: Option<&HighlightedLine>,
    row_index: usize,
    width: usize,
    theme: DiffTheme,
    grep_filter: &str,
) -> Vec<Line<'static>> {
    if width == 0 {
        return vec![Line::default()];
    }

    let left_width = width / 2;
    let right_width = width.saturating_sub(left_width);
    let left_content_width = split_cell_content_width(left_width);
    let right_content_width = split_cell_content_width(right_width);
    let left_scrolls = wrapped_line_start_columns(line.text(), left_content_width);
    let right_scrolls = wrapped_line_start_columns(line.text(), right_content_width);
    let text_width = line.text().width();
    let rows = left_scrolls.len().max(right_scrolls.len());
    let mut lines = Vec::with_capacity(rows);
    for wrap_index in 0..rows {
        let left_scroll = wrapped_segment_scroll(&left_scrolls, text_width, wrap_index);
        let right_scroll = wrapped_segment_scroll(&right_scrolls, text_width, wrap_index);
        let visual_row = row_index.saturating_add(wrap_index);
        let mut spans = split_cell_spans_at_scroll_with_focus_and_continuation(
            Some(line),
            syntax,
            &[],
            SplitCellRender {
                side: SplitSide::Old,
                row_index: visual_row,
                width: left_width,
                theme,
            },
            left_scroll,
            false,
            wrap_index > 0,
        );
        spans.extend(split_cell_spans_at_scroll_with_focus_and_continuation(
            Some(line),
            syntax,
            &[],
            SplitCellRender {
                side: SplitSide::New,
                row_index: visual_row,
                width: right_width,
                theme,
            },
            right_scroll,
            false,
            wrap_index > 0,
        ));
        lines.push(highlight_wrapped_split_grep_line(
            Line::from(spans),
            Some(line),
            Some(line),
            SplitGrepRender {
                query: grep_filter,
                width,
                left_scroll,
                right_scroll,
                theme,
            },
        ));
    }
    lines
}
