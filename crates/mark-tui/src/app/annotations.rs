use super::{
    DiffApp, HunkFocusScrollBehavior, POST_EDITOR_QUIT_KEY_IGNORE, create_annotation_scratch_file,
    normalize_annotation_editor_contents, viewport_center_offset,
};
use crate::annotation::{AnnotationDraft, AnnotationKey};
use crate::editor::{configured_editor, open_text_in_editor};
use crate::keymap::{AnnotationMenuAction, GlobalAction, MenuAction};
use crate::render::viewport_plan::{ViewportSlotKind, plan_diff_viewport_rows_at_scroll};
use crate::selector::{SelectorController, SelectorMovement};
use crate::text_input::{TextInputKeyResult, handle_text_input_key};
use crossterm::event::{KeyCode, KeyEvent};
use mark_core::MarkResult;
use mark_diff::FileStatus;
use std::fs;
use std::time::Instant;

#[derive(Debug, Clone)]
pub(crate) struct AnnotationMenuItem {
    pub(crate) key: AnnotationKey,
    pub(crate) model_row: usize,
    pub(crate) anchor_scroll: usize,
    pub(crate) status: FileStatus,
    pub(crate) label: String,
    pub(crate) preview: String,
    pub(crate) text: String,
}

impl DiffApp {
    pub(crate) fn open_annotation_menu(&mut self) {
        if self.annotations_state.annotations.is_empty() {
            self.set_notice("no annotations");
            return;
        }
        self.overlays.annotation_menu.reset();
        self.overlays.annotation_menu_open = true;
        self.overlays.diff_menu_open = false;
        self.overlays.options_menu_open = false;
        self.close_color_scheme_picker();
        self.refs.branch_menu_open = None;
        self.set_rendered_branch_menu_area(None);
        self.close_review_input();
        self.close_commit_menu();
        self.runtime.dirty = true;
    }

    pub(crate) fn close_annotation_menu(&mut self) {
        if self.overlays.close_annotation_menu() {
            self.runtime.hit_map.annotation_menu_area = None;
            self.runtime.dirty = true;
        }
    }

    pub(crate) fn annotation_menu_items(&self) -> Vec<AnnotationMenuItem> {
        let mut items = self
            .annotations_state
            .annotations
            .iter()
            .filter_map(|(key, text)| {
                let model_row = self.annotation_model_row(key)?;
                let status = self
                    .document
                    .changeset
                    .files
                    .iter()
                    .find(|file| {
                        file.old_path.as_deref() == Some(key.path.as_str())
                            || file.new_path.as_deref() == Some(key.path.as_str())
                    })
                    .map(|file| file.status)
                    .unwrap_or(FileStatus::Unknown);
                Some(AnnotationMenuItem {
                    key: key.clone(),
                    model_row,
                    anchor_scroll: self.annotation_anchor_visual_scroll(model_row),
                    status,
                    label: self
                        .annotation_label(key)
                        .unwrap_or_else(|| format!("{}", key.line)),
                    preview: text
                        .lines()
                        .find(|line| !line.trim().is_empty())
                        .unwrap_or("")
                        .trim()
                        .to_owned(),
                    text: text.clone(),
                })
            })
            .collect::<Vec<_>>();
        items.sort_by(|a, b| (a.anchor_scroll, &a.label).cmp(&(b.anchor_scroll, &b.label)));
        items
    }

    pub(crate) fn filtered_annotation_menu_items(&self) -> Vec<AnnotationMenuItem> {
        let query = self
            .overlays
            .annotation_menu
            .input
            .trim()
            .to_ascii_lowercase();
        self.annotation_menu_items()
            .into_iter()
            .filter(|item| {
                query.is_empty()
                    || item.label.to_ascii_lowercase().contains(&query)
                    || item.preview.to_ascii_lowercase().contains(&query)
            })
            .collect()
    }

    pub(crate) fn move_annotation_menu_selection(&mut self, delta: isize) {
        let len = self.filtered_annotation_menu_items().len();
        if SelectorController::new(&mut self.overlays.annotation_menu, len)
            .move_by(delta, SelectorMovement::Saturating)
        {
            self.runtime.dirty = true;
        }
    }

    pub(crate) fn set_annotation_menu_selection(&mut self, selected: usize) {
        let len = self.filtered_annotation_menu_items().len();
        if SelectorController::new(&mut self.overlays.annotation_menu, len).set_selected(selected) {
            self.runtime.dirty = true;
        }
    }

    pub(crate) fn ensure_annotation_menu_selection_visible(&mut self, visible_rows: usize) {
        let len = self.filtered_annotation_menu_items().len();
        self.overlays
            .annotation_menu
            .ensure_selected_visible(len, visible_rows);
    }

    pub(crate) fn handle_annotation_menu_key(&mut self, key: KeyEvent) -> MarkResult<bool> {
        if self.config.keymap.matches_menu(MenuAction::Close, key) {
            self.close_annotation_menu();
            return Ok(false);
        }
        if self.config.keymap.matches_menu(MenuAction::Down, key) {
            self.move_annotation_menu_selection(1);
        } else if self.config.keymap.matches_menu(MenuAction::Up, key) {
            self.move_annotation_menu_selection(-1);
        } else if self
            .config
            .keymap
            .matches_annotation_menu(AnnotationMenuAction::Jump, key)
        {
            self.edit_selected_annotation(false);
        } else if self
            .config
            .keymap
            .matches_annotation_menu(AnnotationMenuAction::EditExternal, key)
        {
            self.edit_selected_annotation(true);
        } else if self
            .config
            .keymap
            .matches_annotation_menu(AnnotationMenuAction::Remove, key)
        {
            self.remove_selected_annotation();
        } else {
            let len = self.filtered_annotation_menu_items().len();
            let outcome = SelectorController::new(&mut self.overlays.annotation_menu, len)
                .apply_input_key(key);
            if outcome.handled {
                if outcome.changed {
                    self.runtime.dirty = true;
                }
                return Ok(false);
            }
            match key.code {
                KeyCode::PageDown => self.move_annotation_menu_selection(10),
                KeyCode::PageUp => self.move_annotation_menu_selection(-10),
                KeyCode::Home => self.set_annotation_menu_selection(0),
                KeyCode::End => self.set_annotation_menu_selection(usize::MAX),
                _ => {}
            }
        }
        Ok(false)
    }

    fn selected_annotation_menu_item(&self) -> Option<AnnotationMenuItem> {
        self.filtered_annotation_menu_items()
            .get(self.overlays.annotation_menu.selected)
            .cloned()
    }

    fn edit_selected_annotation(&mut self, external: bool) {
        let Some(item) = self.selected_annotation_menu_item() else {
            return;
        };
        let text = self
            .annotations_state
            .annotations
            .get(&item.key)
            .cloned()
            .unwrap_or_default();
        self.close_annotation_menu();
        self.jump_to_annotation(&item.key);
        self.annotations_state.annotation_draft = Some(AnnotationDraft {
            key: item.key,
            model_row_index: item.model_row,
            input: text.clone(),
            cursor: text.len(),
        });
        if external {
            self.open_annotation_draft_in_editor();
        }
        self.runtime.dirty = true;
    }

    fn remove_selected_annotation(&mut self) {
        let Some(item) = self.selected_annotation_menu_item() else {
            return;
        };
        self.annotations_state.annotations.remove(&item.key);
        let len = self.filtered_annotation_menu_items().len();
        self.overlays.annotation_menu.clamp(len);
        if len == 0 {
            self.close_annotation_menu();
        }
        self.runtime.dirty = true;
    }

    pub(crate) fn jump_to_annotation(&mut self, key: &AnnotationKey) {
        let Some(target_model_row) = self.annotation_model_row(key) else {
            return;
        };
        let target_anchor = self.annotation_anchor_visual_scroll(target_model_row);
        let target_scroll =
            target_anchor.saturating_sub(viewport_center_offset(self.viewport.viewport_rows));
        let target_scroll = self.scroll_with_model_row_rendered(target_scroll, target_model_row);
        self.set_scroll_with_grep_sync(
            target_scroll.min(self.max_scroll()),
            false,
            HunkFocusScrollBehavior::Preserve,
        );
    }

    pub(super) fn annotation_model_row(&self, key: &AnnotationKey) -> Option<usize> {
        self.document
            .model
            .rows
            .iter()
            .enumerate()
            .find_map(|(index, row)| {
                AnnotationKey::candidates_from_ui_row(&self.document.changeset, *row)
                    .into_iter()
                    .any(|row_key| row_key == *key)
                    .then_some(index)
            })
    }

    pub(crate) fn move_annotation(&mut self, delta: isize) {
        if self.annotations_state.annotations.is_empty() {
            self.set_notice("no annotations");
            return;
        }

        let mut targets = self
            .annotations_state
            .annotations
            .keys()
            .filter_map(|key| self.annotation_model_row(key))
            .map(|row| (self.annotation_anchor_visual_scroll(row), row))
            .collect::<Vec<_>>();
        targets.sort_unstable();
        targets.dedup();

        if targets.is_empty() {
            self.set_notice("annotations are hidden");
            return;
        }

        let focus_scroll = self.annotation_navigation_focus_scroll();
        let target = if delta < 0 {
            targets
                .iter()
                .rev()
                .copied()
                .find(|(anchor, _)| *anchor < focus_scroll)
                .unwrap_or_else(|| {
                    *targets
                        .last()
                        .expect("annotation targets should not be empty")
                })
        } else {
            targets
                .iter()
                .copied()
                .find(|(anchor, _)| *anchor > focus_scroll)
                .unwrap_or_else(|| targets[0])
        };
        let (target_anchor, target_model_row) = target;
        let target_scroll =
            target_anchor.saturating_sub(viewport_center_offset(self.viewport.viewport_rows));
        let target_scroll = self.scroll_with_model_row_rendered(target_scroll, target_model_row);

        self.set_scroll_with_grep_sync(
            target_scroll.min(self.max_scroll()),
            false,
            HunkFocusScrollBehavior::Preserve,
        );
    }

    pub(super) fn annotation_navigation_focus_scroll(&self) -> usize {
        let focus_viewport_row = self.rendered_viewport_focus_row(self.viewport.viewport_rows);
        let plans = plan_diff_viewport_rows_at_scroll(
            self,
            self.viewport.scroll,
            self.viewport.viewport_rows.max(1),
        );

        let Some(slot) = plans.get(focus_viewport_row).or_else(|| plans.last()) else {
            return self.viewport.scroll.saturating_add(focus_viewport_row);
        };
        // When the viewport focus lands inside an annotation block, navigate from
        // that block's owner row instead of a raw scroll position hidden by notes.
        match &slot.kind {
            ViewportSlotKind::DiffVisual { visual_scroll, .. } => *visual_scroll,
            ViewportSlotKind::AnnotationCompose { model_row, .. }
            | ViewportSlotKind::AnnotationSaved { model_row, .. } => {
                self.annotation_anchor_visual_scroll(*model_row)
            }
        }
    }

    pub(super) fn reanchor_annotation_draft(&mut self) {
        let Some(key) = self
            .annotations_state
            .annotation_draft
            .as_ref()
            .map(|draft| draft.key.clone())
        else {
            return;
        };
        let Some(model_row_index) = self.annotation_model_row(&key) else {
            self.annotations_state.annotation_draft = None;
            self.runtime.dirty = true;
            return;
        };
        if let Some(draft) = self.annotations_state.annotation_draft.as_mut()
            && draft.model_row_index != model_row_index
        {
            draft.model_row_index = model_row_index;
            self.runtime.dirty = true;
        }
    }

    pub(crate) fn handle_annotation_input_key(&mut self, key: KeyEvent) -> bool {
        if self.annotations_state.annotation_draft.is_none() {
            return false;
        }
        if self
            .config
            .keymap
            .matches_single(GlobalAction::CancelMark, key)
        {
            self.annotations_state.annotation_draft = None;
            self.set_scroll_with_grep_sync(
                self.viewport.scroll,
                false,
                HunkFocusScrollBehavior::Preserve,
            );
            self.runtime.dirty = true;
            return true;
        }
        if self
            .config
            .keymap
            .matches_single(GlobalAction::SaveMark, key)
        {
            let draft = self
                .annotations_state
                .annotation_draft
                .take()
                .expect("draft");
            self.commit_annotation_draft(draft);
            return true;
        }
        let Some(draft) = self.annotations_state.annotation_draft.as_mut() else {
            return false;
        };
        let mut keep_visible = false;
        match key.code {
            KeyCode::Enter => {
                draft.input.insert(draft.cursor, '\n');
                draft.cursor += 1;
                self.runtime.dirty = true;
                keep_visible = true;
            }
            _ => match handle_text_input_key(&mut draft.input, &mut draft.cursor, key) {
                TextInputKeyResult::Edited | TextInputKeyResult::Moved => {
                    self.runtime.dirty = true;
                    keep_visible = true;
                }
                TextInputKeyResult::Ignored | TextInputKeyResult::Handled => {}
            },
        }
        if keep_visible {
            self.ensure_annotation_draft_visible();
        }
        true
    }

    pub(super) fn handle_annotation_save_or_cancel_key(&mut self, key: KeyEvent) -> bool {
        if self.annotations_state.annotation_draft.is_none()
            || !(self
                .config
                .keymap
                .matches_single(GlobalAction::CancelMark, key)
                || self
                    .config
                    .keymap
                    .matches_single(GlobalAction::SaveMark, key))
        {
            return false;
        }

        self.handle_annotation_input_key(key)
    }

    pub(super) fn commit_annotation_draft(&mut self, draft: AnnotationDraft) {
        if draft.input.trim().is_empty() {
            self.annotations_state.annotations.remove(&draft.key);
        } else {
            self.annotations_state
                .annotations
                .insert(draft.key, draft.input);
        }
        self.set_scroll_with_grep_sync(
            self.viewport.scroll,
            false,
            HunkFocusScrollBehavior::Preserve,
        );
        self.runtime.dirty = true;
    }

    pub(crate) fn open_annotation_draft_in_editor(&mut self) {
        let Some(draft) = self.annotations_state.annotation_draft.take() else {
            return;
        };
        let Some(editor) = configured_editor() else {
            self.annotations_state.annotation_draft = Some(draft);
            self.set_warning_notice("set $VISUAL, $GIT_EDITOR, or $EDITOR to edit annotation");
            return;
        };
        let scratch = match create_annotation_scratch_file(&draft.input) {
            Ok(scratch) => scratch,
            Err(error) => {
                self.annotations_state.annotation_draft = Some(draft);
                self.set_error_log(format!("annotation editor failed: {error}"));
                return;
            }
        };
        self.runtime.request_terminal_clear();
        let status_result = open_text_in_editor(&editor, &scratch.path);
        self.jobs.post_editor_quit_key_ignore_until =
            Some(Instant::now() + POST_EDITOR_QUIT_KEY_IGNORE);
        match status_result {
            Ok(status) if status.success() => match fs::read_to_string(&scratch.path) {
                Ok(contents) => {
                    let mut updated = draft;
                    updated.input = normalize_annotation_editor_contents(&contents);
                    updated.cursor = updated.input.len();
                    self.commit_annotation_draft(updated);
                    self.set_success_notice("annotation saved");
                }
                Err(error) => {
                    self.annotations_state.annotation_draft = Some(draft);
                    self.set_error_log(format!("annotation read failed: {error}"));
                }
            },
            Ok(_) => {
                self.annotations_state.annotation_draft = Some(draft);
                self.set_warning_notice("annotation editor closed");
            }
            Err(error) => {
                self.annotations_state.annotation_draft = Some(draft);
                self.set_error_log(format!("annotation editor failed: {error}"));
            }
        }
        self.runtime.dirty = true;
    }
}
