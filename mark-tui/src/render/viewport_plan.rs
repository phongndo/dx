use crate::{
    annotation::{AnnotationDraft, AnnotationKey},
    app::DiffApp,
    model::UiRow,
    render::annotations::{annotation_compose_block_height, annotation_saved_block_height},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ViewportSlotKind {
    DiffVisual {
        visual_scroll: usize,
        model_row: usize,
    },
    AnnotationCompose {
        model_row: usize,
    },
    AnnotationSaved {
        model_row: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ViewportSlot {
    pub(crate) kind: ViewportSlotKind,
}

pub(crate) fn plan_diff_viewport_rows(app: &DiffApp, visible_rows: usize) -> Vec<ViewportSlot> {
    if app.line_wrapping {
        plan_wrapped_viewport_rows(app, visible_rows)
    } else {
        plan_unwrapped_viewport_rows(app, visible_rows)
    }
}

fn plan_unwrapped_viewport_rows(app: &DiffApp, visible_rows: usize) -> Vec<ViewportSlot> {
    let draft = app.annotation_draft.as_ref();
    let annotations = &app.annotations;
    let mut plans = Vec::with_capacity(visible_rows);

    for offset in 0..visible_rows {
        if plans.len() >= visible_rows {
            break;
        }
        let visual_row = app.scroll.saturating_add(offset);
        let Some(row) = app.model.row(visual_row) else {
            break;
        };
        plans.push(ViewportSlot {
            kind: ViewportSlotKind::DiffVisual {
                visual_scroll: visual_row,
                model_row: visual_row,
            },
        });

        if let Some(draft) = draft.filter(|d| d.key.model_row_index == visual_row) {
            push_compose_plan_slots(
                &mut plans,
                visual_row,
                draft,
                app.viewport_width,
                visible_rows,
            );
            continue;
        }

        if let Some(key) = AnnotationKey::from_ui_row(row, visual_row)
            && annotations.contains_key(&key)
            && draft.is_none_or(|d| d.key != key)
        {
            let text = annotations.get(&key).expect("key");
            push_saved_plan_slots(
                &mut plans,
                visual_row,
                text,
                app.viewport_width,
                visible_rows,
            );
        }
    }

    plans.truncate(visible_rows);
    plans
}

fn plan_wrapped_viewport_rows(app: &DiffApp, visible_rows: usize) -> Vec<ViewportSlot> {
    let draft = app.annotation_draft.as_ref();
    let annotations = &app.annotations;
    let mut plans = Vec::with_capacity(visible_rows);
    let Some((mut row_index, mut row_offset)) = app.model_row_at_scroll(app.scroll) else {
        return plans;
    };
    let mut visual_row = app.scroll;

    while plans.len() < visible_rows {
        let Some(row) = app.model.row(row_index) else {
            break;
        };
        let remaining = visible_rows.saturating_sub(plans.len());
        let wrap_rows = app.wrapped_visual_height_for_model_row(row_index);
        let mut wraps_left = wrap_rows.saturating_sub(row_offset);
        let take = wraps_left.min(remaining);

        for _ in 0..take {
            if plans.len() >= visible_rows {
                break;
            }
            plans.push(ViewportSlot {
                kind: ViewportSlotKind::DiffVisual {
                    visual_scroll: visual_row,
                    model_row: row_index,
                },
            });
            visual_row = visual_row.saturating_add(1);
            wraps_left = wraps_left.saturating_sub(1);
        }
        row_offset = 0;

        if plans.len() >= visible_rows {
            break;
        }
        if wraps_left == 0 {
            if let Some(draft) = draft.filter(|d| d.key.model_row_index == row_index) {
                push_compose_plan_slots(
                    &mut plans,
                    row_index,
                    draft,
                    app.viewport_width,
                    visible_rows,
                );
            } else if let Some(key) = AnnotationKey::from_ui_row(row, row_index)
                && let Some(text) = annotations.get(&key)
                && draft.is_none_or(|d| d.key != key)
            {
                push_saved_plan_slots(
                    &mut plans,
                    row_index,
                    text,
                    app.viewport_width,
                    visible_rows,
                );
            }
        }
        row_index = row_index.saturating_add(1);
    }

    plans.truncate(visible_rows);
    plans
}

fn push_compose_plan_slots(
    plans: &mut Vec<ViewportSlot>,
    model_row: usize,
    draft: &AnnotationDraft,
    width: usize,
    visible_rows: usize,
) {
    for _ in 0..annotation_compose_block_height(draft, width) {
        if plans.len() >= visible_rows {
            break;
        }
        plans.push(ViewportSlot {
            kind: ViewportSlotKind::AnnotationCompose { model_row },
        });
    }
}

fn push_saved_plan_slots(
    plans: &mut Vec<ViewportSlot>,
    model_row: usize,
    text: &str,
    width: usize,
    visible_rows: usize,
) {
    let block_rows = annotation_saved_block_height(text, width);
    for _ in 0..block_rows {
        if plans.len() >= visible_rows {
            break;
        }
        plans.push(ViewportSlot {
            kind: ViewportSlotKind::AnnotationSaved { model_row },
        });
    }
}

pub(crate) fn visual_scroll_for_viewport_row(app: &DiffApp, viewport_row: u16) -> Option<usize> {
    let plans = plan_diff_viewport_rows(app, app.viewport_rows.max(1));
    let slot = plans.get(usize::from(viewport_row))?;
    match slot.kind {
        ViewportSlotKind::DiffVisual { visual_scroll, .. } => Some(visual_scroll),
        _ => None,
    }
}

pub(crate) fn model_row_for_viewport_row(app: &DiffApp, viewport_row: u16) -> Option<usize> {
    let plans = plan_diff_viewport_rows(app, app.viewport_rows.max(1));
    let slot = plans.get(usize::from(viewport_row))?;
    match slot.kind {
        ViewportSlotKind::DiffVisual { model_row, .. } => Some(model_row),
        ViewportSlotKind::AnnotationCompose { model_row }
        | ViewportSlotKind::AnnotationSaved { model_row } => Some(model_row),
    }
}

pub(crate) fn compose_block_top_viewport_row(app: &DiffApp, model_row: usize) -> Option<u16> {
    let plans = plan_diff_viewport_rows(app, app.viewport_rows.max(1));
    for (index, slot) in plans.iter().enumerate() {
        if matches!(
            slot.kind,
            ViewportSlotKind::AnnotationCompose { model_row: row } if row == model_row
        ) && index > 0
            && matches!(
                plans[index - 1].kind,
                ViewportSlotKind::DiffVisual { model_row: row, .. } if row == model_row
            )
        {
            return (index <= u16::MAX as usize).then_some(index as u16);
        }
        if matches!(
            slot.kind,
            ViewportSlotKind::AnnotationCompose { model_row: row } if row == model_row
        ) && index == 0
        {
            return Some(0);
        }
    }
    None
}

pub(crate) fn compose_block_bottom_viewport_row(app: &DiffApp, model_row: usize) -> Option<u16> {
    let plans = plan_diff_viewport_rows(app, app.viewport_rows.max(1));
    plans
        .iter()
        .enumerate()
        .filter_map(|(index, slot)| {
            matches!(
                slot.kind,
                ViewportSlotKind::AnnotationCompose { model_row: row } if row == model_row
            )
            .then_some(index)
        })
        .next_back()
        .and_then(|index| (index <= u16::MAX as usize).then_some(index as u16))
}

pub(crate) fn saved_block_top_viewport_row(app: &DiffApp, model_row: usize) -> Option<u16> {
    let plans = plan_diff_viewport_rows(app, app.viewport_rows.max(1));
    plans
        .iter()
        .position(|slot| {
            matches!(
                slot.kind,
                ViewportSlotKind::AnnotationSaved { model_row: row } if row == model_row
            )
        })
        .and_then(|index| (index <= u16::MAX as usize).then_some(index as u16))
}

pub(crate) fn saved_block_bottom_viewport_row(app: &DiffApp, model_row: usize) -> Option<u16> {
    let plans = plan_diff_viewport_rows(app, app.viewport_rows.max(1));
    plans
        .iter()
        .enumerate()
        .filter_map(|(index, slot)| {
            matches!(
                slot.kind,
                ViewportSlotKind::AnnotationSaved { model_row: row } if row == model_row
            )
            .then_some(index)
        })
        .next_back()
        .and_then(|index| (index <= u16::MAX as usize).then_some(index as u16))
}

pub(crate) fn annotation_saved_model_at_top_border(
    app: &DiffApp,
    viewport_row: u16,
) -> Option<usize> {
    let plans = plan_diff_viewport_rows(app, app.viewport_rows.max(1));
    let slot = plans.get(usize::from(viewport_row))?;
    match slot.kind {
        ViewportSlotKind::AnnotationSaved { model_row } => {
            if saved_block_top_viewport_row(app, model_row) == Some(viewport_row) {
                Some(model_row)
            } else {
                None
            }
        }
        _ => None,
    }
}

pub(crate) fn row_has_diff_code_content(row: UiRow) -> bool {
    matches!(
        row,
        UiRow::UnifiedLine { .. }
            | UiRow::MetaLine { .. }
            | UiRow::SplitLine { .. }
            | UiRow::ContextLine { .. }
    )
}
