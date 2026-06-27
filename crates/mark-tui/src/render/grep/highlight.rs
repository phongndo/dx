use ratatui::prelude::{Line, Span, Style};

use crate::{controls::TextMatcher, theme::DiffTheme};

use super::types::GrepHighlightTarget;

pub(crate) fn highlighted_grep_text_line(
    line: Line<'static>,
    query: &str,
    targets: Vec<GrepHighlightTarget>,
    theme: DiffTheme,
) -> Line<'static> {
    let Some(matcher) = TextMatcher::new(query) else {
        return line;
    };
    if targets.is_empty() {
        return line;
    }

    let ranges_by_span = grep_highlight_ranges_by_span(line.spans.len(), &targets, &matcher);
    if ranges_by_span.iter().all(Vec::is_empty) {
        return line;
    }

    let mut spans = Vec::with_capacity(line.spans.len());
    for (index, span) in line.spans.into_iter().enumerate() {
        push_highlighted_grep_spans(&mut spans, span, &ranges_by_span[index], theme);
    }
    Line::from(spans)
}

pub(crate) fn grep_highlight_ranges_by_span(
    span_count: usize,
    targets: &[GrepHighlightTarget],
    matcher: &TextMatcher,
) -> Vec<Vec<std::ops::Range<usize>>> {
    let mut ranges_by_span = vec![Vec::new(); span_count];
    for target in targets {
        let match_ranges = matcher.match_ranges(&target.text);
        if match_ranges.is_empty() {
            continue;
        }

        for span in &target.spans {
            if span.span_index >= ranges_by_span.len() || span.span_byte_start >= span.span_byte_end
            {
                continue;
            }

            let span_text_end = span.text_byte_start + (span.span_byte_end - span.span_byte_start);
            for range in &match_ranges {
                let start = range.start.max(span.text_byte_start);
                let end = range.end.min(span_text_end);
                if start < end {
                    let local_start = span.span_byte_start + (start - span.text_byte_start);
                    let local_end = span.span_byte_start + (end - span.text_byte_start);
                    ranges_by_span[span.span_index].push(local_start..local_end);
                }
            }
        }
    }

    for ranges in &mut ranges_by_span {
        merge_ranges(ranges);
    }
    ranges_by_span
}

pub(crate) fn merge_ranges(ranges: &mut Vec<std::ops::Range<usize>>) {
    if ranges.len() <= 1 {
        return;
    }

    ranges.sort_by_key(|range| (range.start, range.end));
    let mut merged: Vec<std::ops::Range<usize>> = Vec::with_capacity(ranges.len());
    for range in ranges.drain(..) {
        if let Some(previous) = merged.last_mut()
            && range.start <= previous.end
        {
            previous.end = previous.end.max(range.end);
            continue;
        }
        merged.push(range);
    }
    *ranges = merged;
}

pub(crate) fn push_highlighted_grep_spans(
    spans: &mut Vec<Span<'static>>,
    span: Span<'static>,
    ranges: &[std::ops::Range<usize>],
    theme: DiffTheme,
) {
    push_highlighted_spans(
        spans,
        span,
        ranges,
        Style::default()
            .fg(theme.search_match_fg)
            .bg(theme.search_match_bg),
    );
}

pub(super) fn push_highlighted_spans(
    spans: &mut Vec<Span<'static>>,
    span: Span<'static>,
    ranges: &[std::ops::Range<usize>],
    highlight: Style,
) {
    let text = span.content.as_ref();
    if ranges.is_empty() {
        spans.push(span);
        return;
    }

    let mut start = 0;
    for range in ranges {
        if range.start >= range.end
            || range.end > text.len()
            || !text.is_char_boundary(range.start)
            || !text.is_char_boundary(range.end)
        {
            continue;
        }
        if start < range.start {
            spans.push(Span::styled(
                text[start..range.start].to_owned(),
                span.style,
            ));
        }
        spans.push(Span::styled(
            text[range.start..range.end].to_owned(),
            span.style.patch(highlight),
        ));
        start = range.end;
    }
    if start < text.len() {
        spans.push(Span::styled(text[start..].to_owned(), span.style));
    }
}
