use ratatui::prelude::Span;
use unicode_width::UnicodeWidthStr;

use crate::render::{
    headers::{
        DeltaPart, FittedPrefixedParts, HeaderSpanPart, HeaderStyles, delta_parts_width,
        push_delta_spans, push_fitted_delta_spans,
    },
    text::{fit, fit_with_ellipsis},
};

pub(crate) fn header_spans(
    prefix: &str,
    body: &str,
    delta_parts: &[DeltaPart],
    width: usize,
    styles: HeaderStyles,
) -> Vec<Span<'static>> {
    if width == 0 {
        return Vec::new();
    }

    let delta_width = delta_parts_width(delta_parts);
    if delta_width >= width {
        let mut spans = Vec::new();
        push_fitted_delta_spans(&mut spans, delta_parts, width, styles);
        return spans;
    }

    let delta_gap = usize::from(delta_width > 0);
    let left_width = width.saturating_sub(delta_width).saturating_sub(delta_gap);
    let fitted = fitted_prefixed_parts(prefix, body, left_width);
    let mut spans = Vec::new();
    let left_used = push_prefixed_spans(&mut spans, fitted, styles);
    let gap = width.saturating_sub(left_used).saturating_sub(delta_width);
    if gap > 0 {
        spans.push(Span::styled(" ".repeat(gap), styles.fill));
    }
    push_delta_spans(&mut spans, delta_parts, styles);
    spans
}

pub(crate) fn hunk_header_spans_with_delta(
    prefix_parts: &[HeaderSpanPart],
    body: &str,
    delta_parts: &[DeltaPart],
    width: usize,
    styles: HeaderStyles,
) -> Vec<Span<'static>> {
    if width == 0 {
        return Vec::new();
    }

    let delta_width = delta_parts_width(delta_parts);
    if delta_width >= width {
        let mut spans = Vec::new();
        push_fitted_delta_spans(&mut spans, delta_parts, width, styles);
        return spans;
    }

    let delta_gap = usize::from(delta_width > 0);
    let left_width = width.saturating_sub(delta_width).saturating_sub(delta_gap);
    let mut spans = Vec::new();
    let left_used =
        push_header_prefix_and_body_spans(&mut spans, prefix_parts, body, left_width, styles);
    let gap = width.saturating_sub(left_used).saturating_sub(delta_width);
    if gap > 0 {
        spans.push(Span::styled(" ".repeat(gap), styles.fill));
    }
    push_delta_spans(&mut spans, delta_parts, styles);
    spans
}

pub(crate) fn push_header_prefix_and_body_spans(
    spans: &mut Vec<Span<'static>>,
    prefix_parts: &[HeaderSpanPart],
    body: &str,
    width: usize,
    styles: HeaderStyles,
) -> usize {
    if width == 0 {
        return 0;
    }

    let prefix_width = header_span_parts_width(prefix_parts);
    if prefix_width >= width {
        return push_fitted_header_span_parts(spans, prefix_parts, width, true);
    }

    let mut used = push_header_span_parts(spans, prefix_parts);
    if body.is_empty() {
        return used;
    }

    let body_width = width.saturating_sub(used).saturating_sub(1);
    if body_width == 0 {
        return used;
    }

    spans.push(Span::styled(" ", styles.body));
    used += 1;
    let body = fit_with_ellipsis(body, body_width);
    used += body.width();
    spans.push(Span::styled(body, styles.body));
    used
}

pub(crate) fn header_span_parts_width(parts: &[HeaderSpanPart]) -> usize {
    parts.iter().map(|part| part.text.width()).sum()
}

pub(crate) fn push_header_span_parts(
    spans: &mut Vec<Span<'static>>,
    parts: &[HeaderSpanPart],
) -> usize {
    let mut used = 0;
    for part in parts {
        used += part.text.width();
        spans.push(Span::styled(part.text.clone(), part.style));
    }
    used
}

pub(crate) fn push_fitted_header_span_parts(
    spans: &mut Vec<Span<'static>>,
    parts: &[HeaderSpanPart],
    width: usize,
    ellipsis: bool,
) -> usize {
    if width == 0 {
        return 0;
    }

    let source_width = header_span_parts_width(parts);
    if !ellipsis || source_width <= width {
        return push_fitted_header_span_part_prefix(spans, parts, width);
    }

    let ellipsis_width = "...".width();
    if width <= ellipsis_width {
        let text = fit("...", width);
        let used = text.width();
        if !text.is_empty() {
            spans.push(Span::styled(
                text,
                parts.first().map(|part| part.style).unwrap_or_default(),
            ));
        }
        return used;
    }

    let prefix_width = width.saturating_sub(ellipsis_width);
    let used = push_fitted_header_span_part_prefix(spans, parts, prefix_width);
    let ellipsis_style = spans
        .last()
        .map(|span| span.style)
        .or_else(|| parts.first().map(|part| part.style))
        .unwrap_or_default();
    spans.push(Span::styled("...", ellipsis_style));
    used + ellipsis_width
}

pub(crate) fn push_fitted_header_span_part_prefix(
    spans: &mut Vec<Span<'static>>,
    parts: &[HeaderSpanPart],
    width: usize,
) -> usize {
    let mut used = 0;
    for part in parts {
        if used >= width {
            break;
        }

        let remaining = width - used;
        let part_width = part.text.width();
        if part_width <= remaining {
            if !part.text.is_empty() {
                spans.push(Span::styled(part.text.clone(), part.style));
            }
            used += part_width;
            continue;
        }

        let text = fit(&part.text, remaining);
        used += text.width();
        if !text.is_empty() {
            spans.push(Span::styled(text, part.style));
        }
        break;
    }
    used
}

pub(crate) fn fitted_prefixed_parts(prefix: &str, body: &str, width: usize) -> FittedPrefixedParts {
    if width == 0 {
        return FittedPrefixedParts {
            prefix: String::new(),
            gap: false,
            body: String::new(),
        };
    }
    if prefix.is_empty() {
        return FittedPrefixedParts {
            prefix: String::new(),
            gap: false,
            body: fit_with_ellipsis(body, width),
        };
    }
    if body.is_empty() {
        return FittedPrefixedParts {
            prefix: fit_with_ellipsis(prefix, width),
            gap: false,
            body: String::new(),
        };
    }

    let prefix_width = prefix.width();
    if prefix_width >= width {
        return FittedPrefixedParts {
            prefix: fit_with_ellipsis(prefix, width),
            gap: false,
            body: String::new(),
        };
    }

    let body_width = width.saturating_sub(prefix_width).saturating_sub(1);
    if body_width == 0 {
        return FittedPrefixedParts {
            prefix: fit(prefix, width),
            gap: false,
            body: String::new(),
        };
    }

    FittedPrefixedParts {
        prefix: prefix.to_owned(),
        gap: true,
        body: fit_with_ellipsis(body, body_width),
    }
}

pub(crate) fn push_prefixed_spans(
    spans: &mut Vec<Span<'static>>,
    fitted: FittedPrefixedParts,
    styles: HeaderStyles,
) -> usize {
    let mut used = 0;
    if !fitted.prefix.is_empty() {
        used += fitted.prefix.width();
        spans.push(Span::styled(fitted.prefix, styles.prefix));
    }
    if fitted.gap {
        used += 1;
        spans.push(Span::styled(" ", styles.body));
    }
    if !fitted.body.is_empty() {
        used += fitted.body.width();
        spans.push(Span::styled(fitted.body, styles.body));
    }
    used
}
