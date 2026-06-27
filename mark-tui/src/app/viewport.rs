use super::RenderedDiffRow;
use crate::model::{UiModel, UiRow};
use std::ops::Range;

pub(crate) fn max_scroll_for_viewport(row_count: usize, viewport_rows: usize) -> usize {
    row_count.saturating_sub(viewport_rows.max(1))
}

pub(crate) fn max_scroll_for_annotated_viewport(
    row_count: usize,
    viewport_rows: usize,
    mut annotation_blocks: Vec<(usize, usize)>,
) -> usize {
    if row_count == 0 {
        return 0;
    }

    annotation_blocks.retain(|(anchor, height)| *anchor < row_count && *height > 0);
    if annotation_blocks.is_empty() {
        return max_scroll_for_viewport(row_count, viewport_rows);
    }

    annotation_blocks.sort_unstable_by_key(|(anchor, _)| *anchor);
    let mut merged_blocks: Vec<(usize, usize)> = Vec::with_capacity(annotation_blocks.len());
    for (anchor, height) in annotation_blocks {
        if let Some((last_anchor, last_height)) = merged_blocks.last_mut()
            && *last_anchor == anchor
        {
            *last_height = last_height.saturating_add(height);
            continue;
        }
        merged_blocks.push((anchor, height));
    }

    let annotation_rows = merged_blocks
        .iter()
        .fold(0usize, |total, (_, height)| total.saturating_add(*height));
    let target_rendered_scroll = row_count
        .saturating_add(annotation_rows)
        .saturating_sub(viewport_rows.max(1));
    if target_rendered_scroll == 0 {
        return 0;
    }

    // `scroll` is expressed in diff visual rows, while annotations add rendered
    // rows after their anchors. Project the last rendered viewport start back to
    // the first diff visual row at or after that rendered position; if that
    // position lands inside an annotation, scrolling to the next diff row reveals
    // rows hidden by the annotation block. If there is no next diff row, fall back
    // to the final anchor so an oversized trailing annotation remains reachable.
    let mut annotation_rows_before = 0usize;
    let mut first_row_in_range = 0usize;
    for (anchor, height) in merged_blocks {
        let candidate = target_rendered_scroll.saturating_sub(annotation_rows_before);
        if candidate <= anchor {
            let projected_scroll = candidate.max(first_row_in_range).min(row_count - 1);
            return projected_scroll;
        }

        annotation_rows_before = annotation_rows_before.saturating_add(height);
        first_row_in_range = anchor.saturating_add(1).min(row_count);
    }

    if first_row_in_range < row_count {
        let projected_scroll = target_rendered_scroll
            .saturating_sub(annotation_rows_before)
            .max(first_row_in_range)
            .min(row_count - 1);
        return projected_scroll;
    }

    row_count - 1
}

pub(crate) fn annotation_scroll_for_block(
    anchor_visual_scroll: usize,
    block_height: usize,
    viewport_rows: usize,
) -> usize {
    anchor_visual_scroll
        .saturating_add(1)
        .saturating_add(block_height)
        .saturating_sub(viewport_rows.max(1))
        .min(anchor_visual_scroll)
}

pub(crate) fn viewport_center_offset(viewport_rows: usize) -> usize {
    viewport_rows.saturating_sub(1) / 2
}

pub(crate) fn viewport_focus_offset(
    scroll: usize,
    row_count: usize,
    viewport_rows: usize,
) -> usize {
    if row_count == 0 {
        return 0;
    }

    let viewport_rows = viewport_rows.max(1);
    let visible_rows = viewport_rows.min(row_count);
    let center = viewport_center_offset(visible_rows);
    if row_count <= viewport_rows {
        return center;
    }

    let bottom = visible_rows.saturating_sub(1);
    let max_scroll = max_scroll_for_viewport(row_count, viewport_rows);
    let scroll = scroll.min(max_scroll);
    let distance_to_end = max_scroll.saturating_sub(scroll);
    let top_ramp = scroll.min(center);
    let bottom_ramp = bottom.saturating_sub(distance_to_end);

    top_ramp.max(bottom_ramp).min(bottom)
}

pub(crate) fn hunk_focus_row_range(
    model: &UiModel,
    file: usize,
    hunk: usize,
) -> Option<(Range<usize>, usize)> {
    let mut range = model.hunk_row_range(file, hunk)?;
    let hunk_start = range.start;

    while range.start > 0
        && model
            .row(range.start - 1)
            .is_some_and(row_extends_hunk_focus_before)
    {
        range.start -= 1;
    }

    while range.end < model.len()
        && model
            .row(range.end)
            .is_some_and(row_extends_hunk_focus_after)
    {
        range.end += 1;
    }

    Some((range, hunk_start))
}

fn row_extends_hunk_focus_before(row: UiRow) -> bool {
    matches!(
        row,
        UiRow::FileHeader(_)
            | UiRow::Collapsed { .. }
            | UiRow::ContextLine { .. }
            | UiRow::ContextHide { .. }
    )
}

fn row_extends_hunk_focus_after(row: UiRow) -> bool {
    matches!(
        row,
        UiRow::Collapsed { .. } | UiRow::ContextLine { .. } | UiRow::ContextHide { .. }
    )
}

pub(crate) fn find_rendered_diff_row_outward<T>(
    rendered_rows: &[RenderedDiffRow],
    focus_viewport_row: usize,
    mut find: impl FnMut(RenderedDiffRow) -> Option<T>,
) -> Option<T> {
    let max_viewport_row = rendered_rows.iter().map(|row| row.viewport_row).max()?;
    let max_distance = focus_viewport_row.max(max_viewport_row.saturating_sub(focus_viewport_row));

    for distance in 0..=max_distance {
        if let Some(viewport_row) = focus_viewport_row.checked_add(distance)
            && viewport_row <= max_viewport_row
            && let Some(rendered_row) = rendered_rows
                .iter()
                .find(|row| row.viewport_row == viewport_row)
            && let Some(found) = find(*rendered_row)
        {
            return Some(found);
        }
        if distance > 0
            && let Some(viewport_row) = focus_viewport_row.checked_sub(distance)
            && let Some(rendered_row) = rendered_rows
                .iter()
                .find(|row| row.viewport_row == viewport_row)
            && let Some(found) = find(*rendered_row)
        {
            return Some(found);
        }
    }

    None
}
