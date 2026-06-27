use ratatui::prelude::{Span, Style};
use unicode_width::UnicodeWidthStr;

use crate::render::{
    headers::{DeltaKind, DeltaPart, HeaderStyles},
    text::fit,
};

pub(crate) fn file_delta_parts(additions: usize, deletions: usize) -> Vec<DeltaPart> {
    vec![
        DeltaPart {
            text: format!("+{additions}"),
            kind: DeltaKind::Addition,
        },
        DeltaPart {
            text: format!("-{deletions}"),
            kind: DeltaKind::Deletion,
        },
    ]
}

pub(crate) fn compact_delta_parts(additions: usize, deletions: usize) -> Vec<DeltaPart> {
    let mut parts = Vec::with_capacity(2);
    if additions > 0 {
        parts.push(DeltaPart {
            text: format!("+{additions}"),
            kind: DeltaKind::Addition,
        });
    }
    if deletions > 0 {
        parts.push(DeltaPart {
            text: format!("-{deletions}"),
            kind: DeltaKind::Deletion,
        });
    }
    parts
}

pub(crate) fn delta_parts_width(parts: &[DeltaPart]) -> usize {
    parts
        .iter()
        .map(|part| part.text.width())
        .sum::<usize>()
        .saturating_add(parts.len().saturating_sub(1))
}

pub(crate) fn push_delta_spans(
    spans: &mut Vec<Span<'static>>,
    delta_parts: &[DeltaPart],
    styles: HeaderStyles,
) {
    for (index, part) in delta_parts.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled(" ", styles.fill));
        }
        spans.push(Span::styled(
            part.text.clone(),
            delta_style(part.kind, styles),
        ));
    }
}

pub(crate) fn push_fitted_delta_spans(
    spans: &mut Vec<Span<'static>>,
    delta_parts: &[DeltaPart],
    width: usize,
    styles: HeaderStyles,
) {
    let mut remaining = width;
    for (index, part) in delta_parts.iter().enumerate() {
        if remaining == 0 {
            return;
        }
        if index > 0 {
            spans.push(Span::styled(" ", styles.fill));
            remaining = remaining.saturating_sub(1);
        }
        if remaining == 0 {
            return;
        }

        let text = fit(&part.text, remaining);
        remaining = remaining.saturating_sub(text.width());
        if !text.is_empty() {
            spans.push(Span::styled(text, delta_style(part.kind, styles)));
        }
    }

    if remaining > 0 {
        spans.push(Span::styled(" ".repeat(remaining), styles.fill));
    }
}

pub(crate) fn delta_style(kind: DeltaKind, styles: HeaderStyles) -> Style {
    match kind {
        DeltaKind::Addition => styles.addition,
        DeltaKind::Deletion => styles.deletion,
    }
}
