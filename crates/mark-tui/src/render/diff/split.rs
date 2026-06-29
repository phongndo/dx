use mark_diff::{DiffLine, DiffLineKind};
use mark_syntax::HighlightedLine;
use ratatui::prelude::{Line, Span, Style};
use unicode_width::UnicodeWidthStr;

use crate::{
    app::{DiffApp, split_cell_content_width, wrapped_line_start_columns},
    render::{
        grep::{highlighted_grep_text_line, split_diff_line_grep_highlight_target},
        style::base_bg,
        text::spaces,
    },
    syntax::{DiffSide, InlineRange},
    theme::{DiffTheme, GUTTER_WIDTH, line_gutter_bg},
};

use super::{
    content_spans_at_scroll, diff_indicator_span_for_focus, empty_diff_fill_from, gutter_spans,
    split_gutter_text,
};

#[derive(Debug, Clone, Copy)]
pub(crate) struct SplitLineRender {
    pub(crate) file: usize,
    pub(crate) hunk: usize,
    pub(crate) left: Option<usize>,
    pub(crate) right: Option<usize>,
    pub(crate) row_index: usize,
    pub(crate) width: usize,
    pub(crate) focused: bool,
}

pub(crate) fn render_split_line_with_focus(
    app: &mut DiffApp,
    render: SplitLineRender,
) -> Line<'static> {
    let SplitLineRender {
        file,
        hunk,
        left,
        right,
        row_index,
        width,
        focused,
    } = render;
    if width == 0 {
        return Line::default();
    }
    let theme = app.config.theme;
    let horizontal_scroll = app.viewport.horizontal_scroll;

    let left_syntax = left.and_then(|index| app.syntax_line(file, hunk, index, DiffSide::Old));
    let right_syntax = right.and_then(|index| app.syntax_line(file, hunk, index, DiffSide::New));
    let left_inline = left
        .map(|index| app.inline_ranges(file, hunk, index))
        .unwrap_or_default();
    let right_inline = right
        .map(|index| app.inline_ranges(file, hunk, index))
        .unwrap_or_default();

    let left_width = width / 2;
    let right_width = width.saturating_sub(left_width);
    let lines = &app.document.changeset.files[file].hunks()[hunk].lines;
    let left_line = left.and_then(|index| lines.get(index));
    let right_line = right.and_then(|index| lines.get(index));
    let mut spans = split_cell_spans_at_scroll_with_focus(
        left_line,
        left_syntax.as_ref(),
        &left_inline,
        SplitCellRender {
            side: SplitSide::Old,
            row_index,
            width: left_width,
            theme,
        },
        horizontal_scroll,
        focused,
    );
    spans.extend(split_cell_spans_at_scroll_with_focus(
        right_line,
        right_syntax.as_ref(),
        &right_inline,
        SplitCellRender {
            side: SplitSide::New,
            row_index,
            width: right_width,
            theme,
        },
        horizontal_scroll,
        focused,
    ));
    Line::from(spans)
}

pub(crate) fn render_split_line_wrapped_with_focus(
    app: &mut DiffApp,
    render: SplitLineRender,
) -> Vec<Line<'static>> {
    let SplitLineRender {
        file,
        hunk,
        left,
        right,
        row_index,
        width,
        focused,
    } = render;
    if width == 0 {
        return vec![Line::default()];
    }
    let theme = app.config.theme;

    let left_syntax = left.and_then(|index| app.syntax_line(file, hunk, index, DiffSide::Old));
    let right_syntax = right.and_then(|index| app.syntax_line(file, hunk, index, DiffSide::New));
    let left_inline = left
        .map(|index| app.inline_ranges(file, hunk, index))
        .unwrap_or_default();
    let right_inline = right
        .map(|index| app.inline_ranges(file, hunk, index))
        .unwrap_or_default();

    let left_width = width / 2;
    let right_width = width.saturating_sub(left_width);
    let lines = &app.document.changeset.files[file].hunks()[hunk].lines;
    let left_line = left.and_then(|index| lines.get(index));
    let right_line = right.and_then(|index| lines.get(index));
    let left_content_width = split_cell_content_width(left_width);
    let right_content_width = split_cell_content_width(right_width);
    let left_scrolls = left_line
        .map(|line| wrapped_line_start_columns(line.text(), left_content_width))
        .unwrap_or_else(|| vec![0]);
    let right_scrolls = right_line
        .map(|line| wrapped_line_start_columns(line.text(), right_content_width))
        .unwrap_or_else(|| vec![0]);
    let left_text_width = left_line.map(|line| line.text().width()).unwrap_or(0);
    let right_text_width = right_line.map(|line| line.text().width()).unwrap_or(0);
    let rows = left_scrolls.len().max(right_scrolls.len()).max(1);
    let visual_row_start = app.wrapped_visual_scroll_for_model_row(row_index);
    let mut rendered_lines = Vec::with_capacity(rows);
    for wrap_index in 0..rows {
        let left_scroll = wrapped_segment_scroll(&left_scrolls, left_text_width, wrap_index);
        let right_scroll = wrapped_segment_scroll(&right_scrolls, right_text_width, wrap_index);
        let visual_row = visual_row_start.saturating_add(wrap_index);
        let mut spans = split_cell_spans_at_scroll_with_focus_and_continuation(
            left_line,
            left_syntax.as_ref(),
            &left_inline,
            SplitCellRender {
                side: SplitSide::Old,
                row_index: visual_row,
                width: left_width,
                theme,
            },
            left_scroll,
            focused,
            wrap_index > 0,
        );
        spans.extend(split_cell_spans_at_scroll_with_focus_and_continuation(
            right_line,
            right_syntax.as_ref(),
            &right_inline,
            SplitCellRender {
                side: SplitSide::New,
                row_index: visual_row,
                width: right_width,
                theme,
            },
            right_scroll,
            focused,
            wrap_index > 0,
        ));
        let line = Line::from(spans);
        rendered_lines.push(highlight_wrapped_split_grep_line(
            line,
            left_line,
            right_line,
            SplitGrepRender {
                query: &app.filters.grep_filter,
                width,
                left_scroll,
                right_scroll,
                theme,
            },
        ));
    }
    rendered_lines
}

pub(super) fn wrapped_segment_scroll(
    starts: &[usize],
    text_width: usize,
    wrap_index: usize,
) -> usize {
    starts.get(wrap_index).copied().unwrap_or(text_width)
}

#[derive(Debug, Clone, Copy)]
pub(super) struct SplitGrepRender<'a> {
    pub(super) query: &'a str,
    pub(super) width: usize,
    pub(super) left_scroll: usize,
    pub(super) right_scroll: usize,
    pub(super) theme: DiffTheme,
}

pub(super) fn highlight_wrapped_split_grep_line(
    rendered: Line<'static>,
    left_line: Option<&DiffLine>,
    right_line: Option<&DiffLine>,
    render: SplitGrepRender<'_>,
) -> Line<'static> {
    let SplitGrepRender {
        query,
        width,
        left_scroll,
        right_scroll,
        theme,
    } = render;

    if query.is_empty() {
        return rendered;
    }

    let left_width = width / 2;
    let right_width = width.saturating_sub(left_width);
    let mut targets = Vec::with_capacity(2);
    if let Some(target) = left_line.and_then(|line| {
        split_diff_line_grep_highlight_target(line, &rendered.spans, 0, left_width, left_scroll)
    }) {
        targets.push(target);
    }
    if let Some(target) = right_line.and_then(|line| {
        split_diff_line_grep_highlight_target(
            line,
            &rendered.spans,
            left_width,
            right_width,
            right_scroll,
        )
    }) {
        targets.push(target);
    }

    highlighted_grep_text_line(rendered, query, targets, theme)
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum SplitSide {
    Old,
    New,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SplitCellRender {
    pub(crate) side: SplitSide,
    pub(crate) row_index: usize,
    pub(crate) width: usize,
    pub(crate) theme: DiffTheme,
}

pub(crate) fn split_cell_spans_at_scroll(
    line: Option<&DiffLine>,
    syntax: Option<&HighlightedLine>,
    inline: &[InlineRange],
    render: SplitCellRender,
    horizontal_scroll: usize,
) -> Vec<Span<'static>> {
    split_cell_spans_at_scroll_with_focus(line, syntax, inline, render, horizontal_scroll, false)
}

pub(crate) fn split_cell_spans_at_scroll_with_focus(
    line: Option<&DiffLine>,
    syntax: Option<&HighlightedLine>,
    inline: &[InlineRange],
    render: SplitCellRender,
    horizontal_scroll: usize,
    focused: bool,
) -> Vec<Span<'static>> {
    split_cell_spans_at_scroll_with_focus_and_continuation(
        line,
        syntax,
        inline,
        render,
        horizontal_scroll,
        focused,
        false,
    )
}

pub(super) fn split_cell_spans_at_scroll_with_focus_and_continuation(
    line: Option<&DiffLine>,
    syntax: Option<&HighlightedLine>,
    inline: &[InlineRange],
    render: SplitCellRender,
    horizontal_scroll: usize,
    focused: bool,
    continuation: bool,
) -> Vec<Span<'static>> {
    let SplitCellRender {
        side,
        row_index,
        width,
        theme,
    } = render;

    if width == 0 {
        return Vec::new();
    }

    let Some(line) = line else {
        let empty_kind = DiffLineKind::Context;
        let indicator_width = 1.min(width);
        let gutter_width = GUTTER_WIDTH.min(width.saturating_sub(indicator_width));
        let content_width = split_cell_content_width(width);
        let mut spans = Vec::new();
        if indicator_width > 0 {
            spans.push(diff_indicator_span_for_focus(empty_kind, theme, focused));
        }
        if gutter_width > 0 {
            spans.push(Span::styled(
                spaces(gutter_width),
                Style::default().bg(line_gutter_bg(empty_kind, theme)),
            ));
        }
        if content_width > 0 {
            spans.push(Span::styled(
                empty_diff_fill_from(
                    content_width,
                    row_index,
                    indicator_width + gutter_width + horizontal_scroll,
                ),
                Style::default().fg(theme.empty_diff).bg(base_bg(theme)),
            ));
        }
        return spans;
    };

    let indicator_width = 1.min(width);
    let gutter_width = GUTTER_WIDTH.min(width.saturating_sub(indicator_width));
    let content_width = split_cell_content_width(width);
    let line_number = if continuation {
        None
    } else {
        match side {
            SplitSide::Old => line.old_line(),
            SplitSide::New => line.new_line(),
        }
    };
    let sign = if continuation {
        " "
    } else {
        match (side, line.kind()) {
            (SplitSide::Old, DiffLineKind::Deletion) => "-",
            (SplitSide::New, DiffLineKind::Addition) => "+",
            _ => " ",
        }
    };

    let mut spans = Vec::new();
    if indicator_width > 0 {
        spans.push(diff_indicator_span_for_focus(line.kind(), theme, focused));
    }
    if gutter_width > 0 {
        spans.extend(gutter_spans(
            &split_gutter_text(line_number),
            sign,
            gutter_width,
            line.kind(),
            theme,
        ));
    }
    spans.extend(content_spans_at_scroll(
        line.text(),
        syntax,
        inline,
        line.kind(),
        content_width,
        theme,
        horizontal_scroll,
    ));
    spans
}
