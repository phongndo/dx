use super::*;

impl DiffApp {
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
        self.runtime.terminal_clear_requested = true;
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
