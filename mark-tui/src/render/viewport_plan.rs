use crate::{
    annotation::{AnnotationDraft, AnnotationKey},
    app::DiffApp,
    model::UiRow,
    render::annotations::{annotation_compose_block_height, annotation_saved_block_height},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ViewportSlotKind {
    DiffVisual {
        visual_scroll: usize,
        model_row: usize,
    },
    AnnotationCompose {
        model_row: usize,
        block_row: usize,
        block_height: usize,
    },
    AnnotationSaved {
        model_row: usize,
        key: AnnotationKey,
        block_row: usize,
        block_height: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ViewportSlot {
    pub(crate) kind: ViewportSlotKind,
}

pub(crate) fn plan_diff_viewport_rows(app: &DiffApp, visible_rows: usize) -> Vec<ViewportSlot> {
    plan_diff_viewport_rows_at_scroll(app, app.viewport.scroll, visible_rows)
}

pub(crate) fn plan_diff_viewport_rows_at_scroll(
    app: &DiffApp,
    scroll: usize,
    visible_rows: usize,
) -> Vec<ViewportSlot> {
    if app.viewport.line_wrapping {
        plan_wrapped_viewport_rows(app, scroll, visible_rows)
    } else {
        plan_unwrapped_viewport_rows(app, scroll, visible_rows)
    }
}

fn plan_unwrapped_viewport_rows(
    app: &DiffApp,
    scroll: usize,
    visible_rows: usize,
) -> Vec<ViewportSlot> {
    let draft = app.annotations_state.annotation_draft.as_ref();
    let annotations = &app.annotations_state.annotations;
    let mut plans = Vec::with_capacity(visible_rows);

    for offset in 0..visible_rows {
        if plans.len() >= visible_rows {
            break;
        }
        let visual_row = scroll.saturating_add(offset);
        let Some(row) = app.document.model.row(visual_row) else {
            break;
        };
        plans.push(ViewportSlot {
            kind: ViewportSlotKind::DiffVisual {
                visual_scroll: visual_row,
                model_row: visual_row,
            },
        });

        for key in AnnotationKey::candidates_from_ui_row(&app.document.changeset, row) {
            if let Some(draft) = draft.filter(|d| d.model_row_index == visual_row && d.key == key) {
                push_compose_plan_slots(
                    &mut plans,
                    visual_row,
                    draft,
                    app.viewport.viewport_width,
                    visible_rows,
                );
            } else if let Some(text) = annotations.get(&key)
                && draft.is_none_or(|d| d.key != key)
            {
                push_saved_plan_slots(
                    &mut plans,
                    visual_row,
                    key,
                    text,
                    app.viewport.viewport_width,
                    visible_rows,
                );
            }
        }
    }

    plans.truncate(visible_rows);
    plans
}

fn plan_wrapped_viewport_rows(
    app: &DiffApp,
    scroll: usize,
    visible_rows: usize,
) -> Vec<ViewportSlot> {
    let draft = app.annotations_state.annotation_draft.as_ref();
    let annotations = &app.annotations_state.annotations;
    let mut plans = Vec::with_capacity(visible_rows);
    let Some((mut row_index, mut row_offset)) = app.model_row_at_scroll(scroll) else {
        return plans;
    };
    let mut visual_row = scroll;

    while plans.len() < visible_rows {
        let Some(row) = app.document.model.row(row_index) else {
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
            for key in AnnotationKey::candidates_from_ui_row(&app.document.changeset, row) {
                if let Some(draft) =
                    draft.filter(|d| d.model_row_index == row_index && d.key == key)
                {
                    push_compose_plan_slots(
                        &mut plans,
                        row_index,
                        draft,
                        app.viewport.viewport_width,
                        visible_rows,
                    );
                } else if let Some(text) = annotations.get(&key)
                    && draft.is_none_or(|d| d.key != key)
                {
                    push_saved_plan_slots(
                        &mut plans,
                        row_index,
                        key,
                        text,
                        app.viewport.viewport_width,
                        visible_rows,
                    );
                }
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
    let block_height = annotation_compose_block_height(draft, width);
    for block_row in 0..block_height {
        if plans.len() >= visible_rows {
            break;
        }
        plans.push(ViewportSlot {
            kind: ViewportSlotKind::AnnotationCompose {
                model_row,
                block_row,
                block_height,
            },
        });
    }
}

fn push_saved_plan_slots(
    plans: &mut Vec<ViewportSlot>,
    model_row: usize,
    key: AnnotationKey,
    text: &str,
    width: usize,
    visible_rows: usize,
) {
    let block_rows = annotation_saved_block_height(text, width);
    for block_row in 0..block_rows {
        if plans.len() >= visible_rows {
            break;
        }
        plans.push(ViewportSlot {
            kind: ViewportSlotKind::AnnotationSaved {
                model_row,
                key: key.clone(),
                block_row,
                block_height: block_rows,
            },
        });
    }
}

pub(crate) fn visual_scroll_for_viewport_row(app: &DiffApp, viewport_row: u16) -> Option<usize> {
    let plans = plan_diff_viewport_rows(app, app.viewport.viewport_rows.max(1));
    let slot = plans.get(usize::from(viewport_row))?;
    match &slot.kind {
        ViewportSlotKind::DiffVisual { visual_scroll, .. } => Some(*visual_scroll),
        _ => None,
    }
}

pub(crate) fn model_row_for_viewport_row(app: &DiffApp, viewport_row: u16) -> Option<usize> {
    let plans = plan_diff_viewport_rows(app, app.viewport.viewport_rows.max(1));
    let slot = plans.get(usize::from(viewport_row))?;
    match &slot.kind {
        ViewportSlotKind::DiffVisual { model_row, .. } => Some(*model_row),
        ViewportSlotKind::AnnotationCompose { model_row, .. }
        | ViewportSlotKind::AnnotationSaved { model_row, .. } => Some(*model_row),
    }
}

pub(crate) fn compose_block_top_viewport_row(app: &DiffApp, model_row: usize) -> Option<u16> {
    let plans = plan_diff_viewport_rows(app, app.viewport.viewport_rows.max(1));
    plans
        .iter()
        .enumerate()
        .find_map(|(index, slot)| match &slot.kind {
            ViewportSlotKind::AnnotationCompose {
                model_row: row,
                block_row: 0,
                ..
            } if *row == model_row => (index <= u16::MAX as usize).then_some(index as u16),
            _ => None,
        })
}

pub(crate) fn compose_block_bottom_viewport_row(app: &DiffApp, model_row: usize) -> Option<u16> {
    let plans = plan_diff_viewport_rows(app, app.viewport.viewport_rows.max(1));
    plans
        .iter()
        .enumerate()
        .find_map(|(index, slot)| match &slot.kind {
            ViewportSlotKind::AnnotationCompose {
                model_row: row,
                block_row,
                block_height,
            } if *row == model_row && block_row.saturating_add(1) == *block_height => {
                (index <= u16::MAX as usize).then_some(index as u16)
            }
            _ => None,
        })
}

pub(crate) fn annotation_saved_key_at_bottom_border(
    app: &DiffApp,
    viewport_row: u16,
) -> Option<(usize, AnnotationKey)> {
    let plans = plan_diff_viewport_rows(app, app.viewport.viewport_rows.max(1));
    let slot = plans.get(usize::from(viewport_row))?;
    match &slot.kind {
        ViewportSlotKind::AnnotationSaved {
            model_row,
            key,
            block_row,
            block_height,
        } if block_row.saturating_add(1) == *block_height => Some((*model_row, key.clone())),
        _ => None,
    }
}

pub(crate) fn annotation_saved_key_at_top_border(
    app: &DiffApp,
    viewport_row: u16,
) -> Option<(usize, AnnotationKey)> {
    let plans = plan_diff_viewport_rows(app, app.viewport.viewport_rows.max(1));
    let slot = plans.get(usize::from(viewport_row))?;
    match &slot.kind {
        ViewportSlotKind::AnnotationSaved {
            model_row,
            key,
            block_row: 0,
            ..
        } => Some((*model_row, key.clone())),
        _ => None,
    }
}

pub(crate) fn row_has_diff_code_content(row: UiRow) -> bool {
    matches!(
        row,
        UiRow::UnifiedLine { .. } | UiRow::SplitLine { .. } | UiRow::ContextLine { .. }
    )
}
