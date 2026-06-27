use ratatui::prelude::{Line, Span, Style};

use crate::{controls::DiffLayoutMode, theme::DiffTheme};

use super::{
    highlight::push_highlighted_spans,
    target::{span_position_for_column, split_content_start_column, unified_content_start_column},
};

pub(crate) fn highlighted_mouse_diff_content_line(
    line: Line<'static>,
    layout: DiffLayoutMode,
    width: usize,
    theme: DiffTheme,
) -> Line<'static> {
    let spans = line.spans;
    if spans.is_empty() || width == 0 {
        return Line::from(spans);
    }

    let column_ranges = diff_content_column_ranges(layout, width);
    let highlight = mouse_diff_content_line_style(theme);
    let ranges_by_span: Vec<Vec<std::ops::Range<usize>>> = (0..spans.len())
        .map(|index| byte_ranges_for_column_ranges(&spans, index, &column_ranges))
        .collect();
    let mut output = Vec::with_capacity(spans.len());
    for (index, span) in spans.into_iter().enumerate() {
        let ranges = &ranges_by_span[index];
        if ranges.is_empty() {
            output.push(span);
        } else {
            push_highlighted_spans(&mut output, span, ranges, highlight);
        }
    }
    Line::from(output)
}

fn diff_content_column_ranges(layout: DiffLayoutMode, width: usize) -> Vec<(usize, usize)> {
    match layout {
        DiffLayoutMode::Unified => vec![(unified_content_start_column(width), width)],
        DiffLayoutMode::Split => {
            let left_width = width / 2;
            let right_width = width.saturating_sub(left_width);
            vec![
                (split_content_start_column(left_width), left_width),
                (
                    left_width.saturating_add(split_content_start_column(right_width)),
                    width,
                ),
            ]
        }
    }
}

fn byte_ranges_for_column_ranges(
    spans: &[Span<'_>],
    span_index: usize,
    column_ranges: &[(usize, usize)],
) -> Vec<std::ops::Range<usize>> {
    let mut ranges = Vec::new();
    for &(start_column, end_column) in column_ranges {
        if start_column >= end_column {
            continue;
        }
        let start = span_position_for_column(spans, start_column);
        let end = span_position_for_column(spans, end_column);
        if span_index < start.span_index || span_index > end.span_index {
            continue;
        }
        let text = spans[span_index].content.as_ref();
        let span_start = if span_index == start.span_index {
            start.byte_index
        } else {
            0
        };
        let span_end = if span_index == end.span_index {
            end.byte_index
        } else {
            text.len()
        };
        if span_start < span_end
            && text.is_char_boundary(span_start)
            && text.is_char_boundary(span_end)
        {
            ranges.push(span_start..span_end);
        }
    }
    ranges
}

fn mouse_diff_content_line_style(theme: DiffTheme) -> Style {
    Style::default().bg(theme.cursor_line_bg)
}
