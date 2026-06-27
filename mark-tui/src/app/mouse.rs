use super::*;
use crate::render::compositor::{ComponentEventResult, Compositor, EventComponent};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MouseScrollDirection {
    Up,
    Down,
}

#[derive(Debug, Default)]
pub(crate) struct MouseScroll {
    pub(crate) last_tick: Option<Instant>,
    pub(crate) direction: Option<MouseScrollDirection>,
    pub(crate) intervals: Vec<Duration>,
    pub(crate) pending_lines: f64,
    pub(crate) pending_hunk_focus_ticks: isize,
}

impl MouseScroll {
    pub(crate) fn scroll_delta(&mut self, direction: MouseScrollDirection, now: Instant) -> isize {
        let multiplier = self.multiplier(direction, now);
        self.pending_lines += multiplier;
        let lines = self.pending_lines.trunc() as isize;
        self.pending_lines -= lines as f64;

        match direction {
            MouseScrollDirection::Down => lines,
            MouseScrollDirection::Up => -lines,
        }
    }

    pub(crate) fn reset(&mut self) {
        self.last_tick = None;
        self.direction = None;
        self.intervals.clear();
        self.pending_lines = 0.0;
        self.pending_hunk_focus_ticks = 0;
    }

    pub(crate) fn reset_hunk_focus_ticks(&mut self) {
        self.pending_hunk_focus_ticks = 0;
    }

    pub(crate) fn hunk_focus_delta(&mut self, direction: MouseScrollDirection) -> isize {
        match direction {
            MouseScrollDirection::Down => self.pending_hunk_focus_ticks += 1,
            MouseScrollDirection::Up => self.pending_hunk_focus_ticks -= 1,
        }

        if self.pending_hunk_focus_ticks >= MOUSE_HUNK_FOCUS_SCROLL_TICKS {
            self.pending_hunk_focus_ticks -= MOUSE_HUNK_FOCUS_SCROLL_TICKS;
            1
        } else if self.pending_hunk_focus_ticks <= -MOUSE_HUNK_FOCUS_SCROLL_TICKS {
            self.pending_hunk_focus_ticks += MOUSE_HUNK_FOCUS_SCROLL_TICKS;
            -1
        } else {
            0
        }
    }

    pub(crate) fn multiplier(&mut self, direction: MouseScrollDirection, now: Instant) -> f64 {
        let Some(last_tick) = self.last_tick else {
            self.start_streak(direction, now);
            return 1.0;
        };

        let elapsed = now.saturating_duration_since(last_tick);
        if self.direction != Some(direction) || elapsed > MOUSE_SCROLL_STREAK_TIMEOUT {
            self.start_streak(direction, now);
            return 1.0;
        }

        if elapsed < MOUSE_SCROLL_MIN_TICK_INTERVAL {
            return 1.0;
        }

        self.last_tick = Some(now);
        self.intervals.push(elapsed);
        if self.intervals.len() > MOUSE_SCROLL_HISTORY_SIZE {
            self.intervals.remove(0);
        }

        let average_interval_ms = self
            .intervals
            .iter()
            .map(|interval| interval.as_secs_f64() * 1000.0)
            .sum::<f64>()
            / self.intervals.len() as f64;
        let velocity = MOUSE_SCROLL_REFERENCE_INTERVAL_MS / average_interval_ms;
        let multiplier =
            1.0 + MOUSE_SCROLL_ACCEL_A * ((velocity / MOUSE_SCROLL_ACCEL_TAU).exp() - 1.0);

        multiplier.min(MOUSE_SCROLL_MAX_MULTIPLIER)
    }

    pub(crate) fn start_streak(&mut self, direction: MouseScrollDirection, now: Instant) {
        self.last_tick = Some(now);
        self.direction = Some(direction);
        self.intervals.clear();
        self.pending_lines = 0.0;
        self.pending_hunk_focus_ticks = 0;
    }
}

fn route_mouse_through_layers(
    app: &mut DiffApp,
    mouse: MouseEvent,
) -> MarkResult<ComponentEventResult> {
    let mut compositor = Compositor::new();
    compositor.push(EventComponent::mouse(handle_diff_mouse_layer));
    compositor.push(EventComponent::mouse(handle_error_log_resize_mouse_layer));
    compositor.push(EventComponent::mouse(handle_options_menu_mouse_layer));
    compositor.push(EventComponent::mouse(
        handle_color_scheme_picker_mouse_layer,
    ));
    compositor.push(EventComponent::mouse(
        handle_file_sidebar_resize_mouse_layer,
    ));
    compositor.push(EventComponent::mouse(handle_help_menu_mouse_layer));
    compositor.push(EventComponent::mouse(handle_open_menu_scroll_mouse_layer));
    compositor.handle_mouse(mouse, app)
}

fn handle_open_menu_scroll_mouse_layer(
    mouse: MouseEvent,
    app: &mut DiffApp,
) -> MarkResult<ComponentEventResult> {
    Ok(if app.handle_open_menu_mouse_scroll(mouse.kind) {
        ComponentEventResult::Consumed
    } else {
        ComponentEventResult::Ignored
    })
}

fn handle_help_menu_mouse_layer(
    mouse: MouseEvent,
    app: &mut DiffApp,
) -> MarkResult<ComponentEventResult> {
    if !app.overlays.help_menu_open {
        return Ok(ComponentEventResult::Ignored);
    }

    if let MouseEventKind::Down(MouseButton::Left) = mouse.kind {
        app.close_help_menu();
    }
    app.input.mouse_scroll.reset();
    Ok(ComponentEventResult::Consumed)
}

fn handle_file_sidebar_resize_mouse_layer(
    mouse: MouseEvent,
    app: &mut DiffApp,
) -> MarkResult<ComponentEventResult> {
    if !app.sidebar.file_sidebar_resizing {
        return Ok(ComponentEventResult::Ignored);
    }

    match mouse.kind {
        MouseEventKind::Drag(MouseButton::Left) | MouseEventKind::Moved => {
            app.resize_file_sidebar_to_column(mouse.column);
            Ok(ComponentEventResult::Consumed)
        }
        MouseEventKind::Up(MouseButton::Left) => {
            app.sidebar.file_sidebar_resizing = false;
            app.resize_file_sidebar_to_column(mouse.column);
            Ok(ComponentEventResult::Consumed)
        }
        _ => Ok(ComponentEventResult::Ignored),
    }
}

fn handle_color_scheme_picker_mouse_layer(
    mouse: MouseEvent,
    app: &mut DiffApp,
) -> MarkResult<ComponentEventResult> {
    if !app.overlays.color_scheme_picker_open {
        return Ok(ComponentEventResult::Ignored);
    }

    match mouse.kind {
        MouseEventKind::Moved | MouseEventKind::Drag(MouseButton::Left) => {
            if let Some(index) = app.color_scheme_index_at(mouse.column, mouse.row) {
                app.set_color_scheme_selection(index);
            }
            Ok(ComponentEventResult::Consumed)
        }
        MouseEventKind::Down(MouseButton::Left) => {
            if let Some(index) = app.color_scheme_index_at(mouse.column, mouse.row) {
                app.set_color_scheme_selection(index);
                app.select_highlighted_color_scheme();
            } else if app.is_rendered_color_scheme_picker_position(mouse.column, mouse.row) {
                app.runtime.dirty = true;
            } else {
                app.close_color_scheme_picker();
            }
            Ok(ComponentEventResult::Consumed)
        }
        _ => Ok(ComponentEventResult::Ignored),
    }
}

fn handle_options_menu_mouse_layer(
    mouse: MouseEvent,
    app: &mut DiffApp,
) -> MarkResult<ComponentEventResult> {
    if app.overlays.options_menu_open
        && matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
    {
        app.close_options_menu();
        return Ok(ComponentEventResult::Consumed);
    }
    Ok(ComponentEventResult::Ignored)
}

fn handle_error_log_resize_mouse_layer(
    mouse: MouseEvent,
    app: &mut DiffApp,
) -> MarkResult<ComponentEventResult> {
    if !app.notifications.error_log_resizing {
        return Ok(ComponentEventResult::Ignored);
    }

    match mouse.kind {
        MouseEventKind::Drag(MouseButton::Left) | MouseEventKind::Moved => {
            app.resize_error_log_to_separator_row(mouse.row);
            Ok(ComponentEventResult::Consumed)
        }
        MouseEventKind::Up(MouseButton::Left) => {
            app.resize_error_log_to_separator_row(mouse.row);
            app.notifications.error_log_resizing = false;
            app.runtime.dirty = true;
            Ok(ComponentEventResult::Consumed)
        }
        _ => Ok(ComponentEventResult::Ignored),
    }
}

fn handle_diff_mouse_layer(
    mouse: MouseEvent,
    app: &mut DiffApp,
) -> MarkResult<ComponentEventResult> {
    app.update_diff_mouse_hover(mouse.column, mouse.row);

    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            if app.start_error_log_resize(mouse.row) {
                return Ok(ComponentEventResult::Consumed);
            }
            if app.start_file_sidebar_resize(mouse.column, mouse.row) {
                return Ok(ComponentEventResult::Consumed);
            }
            app.handle_click(mouse.column, mouse.row);
            Ok(ComponentEventResult::Consumed)
        }
        MouseEventKind::ScrollDown => {
            if app.is_file_sidebar_position(mouse.column, mouse.row) {
                app.input.mouse_scroll.reset();
                app.scroll_file_sidebar_by(1);
                return Ok(ComponentEventResult::Consumed);
            }
            app.mouse_scroll_or_focus_hunk(MouseScrollDirection::Down);
            app.update_diff_mouse_hover(mouse.column, mouse.row);
            Ok(ComponentEventResult::Consumed)
        }
        MouseEventKind::ScrollUp => {
            if app.is_file_sidebar_position(mouse.column, mouse.row) {
                app.input.mouse_scroll.reset();
                app.scroll_file_sidebar_by(-1);
                return Ok(ComponentEventResult::Consumed);
            }
            app.mouse_scroll_or_focus_hunk(MouseScrollDirection::Up);
            app.update_diff_mouse_hover(mouse.column, mouse.row);
            Ok(ComponentEventResult::Consumed)
        }
        MouseEventKind::ScrollLeft => {
            if app.is_file_sidebar_position(mouse.column, mouse.row) {
                app.input.mouse_scroll.reset();
                return Ok(ComponentEventResult::Consumed);
            }
            app.scroll_horizontally_by(-(HORIZONTAL_SCROLL_STEP as isize));
            app.update_diff_mouse_hover(mouse.column, mouse.row);
            Ok(ComponentEventResult::Consumed)
        }
        MouseEventKind::ScrollRight => {
            if app.is_file_sidebar_position(mouse.column, mouse.row) {
                app.input.mouse_scroll.reset();
                return Ok(ComponentEventResult::Consumed);
            }
            app.scroll_horizontally_by(HORIZONTAL_SCROLL_STEP as isize);
            app.update_diff_mouse_hover(mouse.column, mouse.row);
            Ok(ComponentEventResult::Consumed)
        }
        _ => Ok(ComponentEventResult::Ignored),
    }
}

impl DiffApp {
    pub(super) fn handle_open_menu_mouse_scroll(&mut self, kind: MouseEventKind) -> bool {
        let delta = match kind {
            MouseEventKind::ScrollDown => 1,
            MouseEventKind::ScrollUp => -1,
            _ => return false,
        };

        if self.overlays.help_menu_open {
            self.scroll_help_menu(delta);
        } else if self.overlays.color_scheme_picker_open {
            self.move_color_scheme_selection(delta);
        } else if self.refs.branch_menu_open.is_some() {
            self.move_branch_selection(delta);
        } else if self.refs.commit_menu_open {
            self.move_commit_selection(delta);
        } else if self.overlays.review_input_open {
            // Review input has no scrollable content, but the open modal should
            // still consume wheel events instead of scrolling the diff behind it.
        } else if self.overlays.diff_menu_open {
            self.move_diff_menu_selection(delta);
        } else if self.overlays.options_menu_open {
            self.move_options_menu_selection(delta);
        } else {
            return false;
        }

        self.input.mouse_scroll.reset();
        true
    }

    pub(crate) fn handle_mouse(&mut self, mouse: MouseEvent) -> MarkResult<()> {
        route_mouse_through_layers(self, mouse).map(|_| ())
    }

    pub(crate) fn is_file_sidebar_position(&self, column: u16, row: u16) -> bool {
        self.sidebar.file_sidebar_open
            && self.sidebar.file_sidebar_render_width > 0
            && column < self.sidebar.file_sidebar_render_width
            && row > 0
            && usize::from(row - 1) < self.visible_file_sidebar_rows()
    }

    pub(crate) fn is_file_sidebar_resize_handle(&self, column: u16, row: u16) -> bool {
        self.is_file_sidebar_position(column, row)
            && column.saturating_add(1) == self.sidebar.file_sidebar_render_width
    }

    pub(crate) fn start_file_sidebar_resize(&mut self, column: u16, row: u16) -> bool {
        if !self.is_file_sidebar_resize_handle(column, row) {
            return false;
        }

        self.sidebar.file_sidebar_resizing = true;
        self.resize_file_sidebar_to_column(column);
        true
    }

    pub(crate) fn resize_file_sidebar_to_column(&mut self, column: u16) {
        let width = column.saturating_add(1);
        self.set_file_sidebar_width(width);
    }

    pub(crate) fn handle_click(&mut self, column: u16, row: u16) {
        let clicked_selector = row == 0 && column < diff_selector_width(&self.document.options);
        let clicked_branch_selector = (row == 0)
            .then(|| self.branch_selector_at(column))
            .flatten();
        let clicked_commit_selector = row == 0 && self.commit_selector_at(column);

        if self.overlays.review_input_open {
            if self.is_rendered_review_input_position(column, row) {
                self.runtime.dirty = true;
                return;
            }

            self.close_review_input();
            if clicked_selector {
                self.toggle_diff_menu();
            }
            return;
        }

        if self.refs.commit_menu_open {
            if let Some(rev) = self.commit_choice_at(column, row) {
                self.close_commit_menu();
                self.select_show_commit(rev);
                return;
            }

            if self.is_rendered_commit_menu_position(column, row) {
                return;
            }

            if clicked_commit_selector {
                self.toggle_commit_menu();
                return;
            }

            self.close_commit_menu();
            if clicked_selector {
                self.toggle_diff_menu();
            }
            return;
        }

        if let Some(menu) = self.refs.branch_menu_open {
            if let Some(branch) = self.branch_choice_at(menu, column, row) {
                self.close_branch_menu();
                self.select_branch(menu, branch);
                return;
            }

            if self.is_rendered_branch_menu_position(column, row) {
                return;
            }

            if let Some(clicked_menu) = clicked_branch_selector {
                self.toggle_branch_menu(clicked_menu);
                return;
            }

            self.close_branch_menu();
            if clicked_selector {
                self.toggle_diff_menu();
            }
            return;
        }

        if self.overlays.diff_menu_open {
            if let Some(choice) = self.diff_choice_at(column, row) {
                self.close_diff_menu();
                self.select_diff_choice(choice);
                return;
            }

            if self.is_rendered_diff_menu_position(column, row) {
                return;
            }

            if let Some(menu) = clicked_branch_selector {
                self.close_diff_menu();
                self.toggle_branch_menu(menu);
                return;
            }

            if clicked_selector {
                self.toggle_diff_menu();
                return;
            }

            self.close_diff_menu();
            return;
        }

        if self.overlays.color_scheme_picker_open {
            self.close_color_scheme_picker();
            return;
        }

        if self.overlays.options_menu_open {
            self.close_options_menu();
            return;
        }

        if clicked_selector {
            self.toggle_diff_menu();
        } else if clicked_commit_selector {
            self.toggle_commit_menu();
        } else if let Some(menu) = clicked_branch_selector {
            self.toggle_branch_menu(menu);
        } else if !self.handle_file_sidebar_click(column, row) {
            self.handle_diff_click(column, row);
        }
    }

    pub(crate) fn handle_file_sidebar_click(&mut self, column: u16, row: u16) -> bool {
        if !self.is_file_sidebar_position(column, row) {
            return false;
        }

        let position = self
            .sidebar
            .file_sidebar_scroll
            .saturating_add(usize::from(row - 1));
        let Some(file) = self.document.model.visible_files().get(position).copied() else {
            return false;
        };

        self.select_file(file);
        true
    }

    pub(crate) fn handle_diff_click(&mut self, column: u16, row: u16) -> bool {
        let Some((diff_column, viewport_row)) = self.diff_viewport_position(column, row) else {
            return false;
        };
        let width = self.viewport.viewport_width;
        if annotation_submit_hit_at_column(diff_column, width)
            && self.handle_annotation_submit_click(viewport_row)
        {
            return true;
        }
        if annotation_edit_hit_at_column(diff_column, width)
            && self.handle_annotation_edit_click(viewport_row)
        {
            return true;
        }
        if annotation_close_hit_at_column(diff_column, width)
            && self.handle_annotation_close_click(viewport_row)
        {
            return true;
        }
        if self
            .viewport
            .mouse_hover
            .is_some_and(|(_, hover_row)| hover_row == viewport_row)
            && self.try_open_annotation_draft_at_viewport_row(viewport_row, diff_column)
        {
            return true;
        }

        let Some(model_row) = model_row_for_viewport_row(self, viewport_row) else {
            return false;
        };
        self.handle_context_at_row(model_row)
    }

    pub(super) fn diff_viewport_position(&self, column: u16, row: u16) -> Option<(u16, u16)> {
        let area = self.viewport.rendered_diff_area?;
        if area.width == 0
            || area.height == 0
            || column < area.x
            || row < area.y
            || column >= area.x.saturating_add(area.width)
            || row >= area.y.saturating_add(area.height)
        {
            return None;
        }

        Some((column.saturating_sub(area.x), row.saturating_sub(area.y)))
    }

    pub(crate) fn annotation_anchor_visual_scroll(&self, model_row_index: usize) -> usize {
        if self.viewport.line_wrapping {
            let start = self.wrapped_visual_scroll_for_model_row(model_row_index);
            let height = self.wrapped_visual_height_for_model_row(model_row_index);
            start.saturating_add(height.saturating_sub(1))
        } else {
            model_row_index
        }
    }

    pub(crate) fn annotation_label(&self, key: &AnnotationKey) -> Option<String> {
        Some(format!("{} {}{}", key.path, key.side.label(), key.line))
    }

    pub(super) fn handle_annotation_submit_click(&mut self, viewport_row: u16) -> bool {
        let Some(draft) = self.annotations_state.annotation_draft.as_ref() else {
            return false;
        };
        if compose_block_bottom_viewport_row(self, draft.model_row_index) != Some(viewport_row) {
            return false;
        }
        let draft = self
            .annotations_state
            .annotation_draft
            .take()
            .expect("draft");
        self.commit_annotation_draft(draft);
        true
    }

    pub(super) fn handle_annotation_edit_click(&mut self, viewport_row: u16) -> bool {
        if self.annotations_state.annotation_draft.is_some() {
            return false;
        }
        let Some((model_row, key)) = annotation_saved_key_at_bottom_border(self, viewport_row)
        else {
            return false;
        };
        self.open_annotation_draft_for_key(key, model_row)
    }

    pub(super) fn handle_annotation_close_click(&mut self, viewport_row: u16) -> bool {
        if let Some(draft) = self.annotations_state.annotation_draft.as_ref() {
            if compose_block_top_viewport_row(self, draft.model_row_index) == Some(viewport_row) {
                self.annotations_state.annotation_draft = None;
                self.set_scroll_with_grep_sync(
                    self.viewport.scroll,
                    false,
                    HunkFocusScrollBehavior::Preserve,
                );
                self.runtime.dirty = true;
                return true;
            }
            return false;
        }

        if self.filters.filter_input.is_some() {
            return false;
        }

        let Some((_model_row, key)) = annotation_saved_key_at_top_border(self, viewport_row) else {
            return false;
        };
        if self.annotations_state.annotations.remove(&key).is_some() {
            self.set_scroll_with_grep_sync(
                self.viewport.scroll,
                false,
                HunkFocusScrollBehavior::Preserve,
            );
            self.runtime.dirty = true;
            return true;
        }
        false
    }

    pub(super) fn try_open_annotation_draft_at_viewport_row(
        &mut self,
        viewport_row: u16,
        column: u16,
    ) -> bool {
        if self.filters.filter_input.is_some() {
            return false;
        }
        if self.annotations_state.annotation_draft.is_some() {
            return false;
        }
        let Some(visual_row) = visual_scroll_for_viewport_row(self, viewport_row) else {
            return false;
        };
        let row_index = if self.viewport.line_wrapping {
            let Some((row_index, _)) = self.model_row_at_scroll(visual_row) else {
                return false;
            };
            row_index
        } else {
            visual_row
        };
        let Some(row) = self.document.model.row(row_index) else {
            return false;
        };
        if !crate::render::viewport_plan::row_has_diff_code_content(row) {
            return false;
        }
        if self.annotation_anchor_visual_scroll(row_index) != visual_row {
            return false;
        }
        let Some(key) = self.annotation_key_for_add_click(row, column) else {
            return false;
        };
        self.open_annotation_draft_for_key(key, row_index)
    }

    pub(super) fn annotation_key_for_add_click(
        &self,
        row: UiRow,
        column: u16,
    ) -> Option<AnnotationKey> {
        if !annotation_hit_at_column(column, self.viewport.viewport_width) {
            return None;
        }
        AnnotationKey::from_ui_row(&self.document.changeset, row)
    }

    pub(super) fn open_annotation_draft_for_key(
        &mut self,
        key: AnnotationKey,
        model_row_index: usize,
    ) -> bool {
        if self.filters.filter_input.is_some() {
            return false;
        }
        let existing = self
            .annotations_state
            .annotations
            .get(&key)
            .cloned()
            .unwrap_or_default();
        let cursor = existing.len();
        self.annotations_state.annotation_draft = Some(AnnotationDraft {
            key,
            model_row_index,
            input: existing,
            cursor,
        });
        self.ensure_annotation_draft_visible();
        self.runtime.dirty = true;
        true
    }

    pub(super) fn ensure_annotation_draft_visible(&mut self) {
        let Some((model_row, anchor, desired_scroll)) = self
            .annotations_state
            .annotation_draft
            .as_ref()
            .map(|draft| {
                let anchor = self.annotation_anchor_visual_scroll(draft.model_row_index);
                let height = annotation_compose_block_height(draft, self.viewport.viewport_width);
                (
                    draft.model_row_index,
                    anchor,
                    annotation_scroll_for_block(anchor, height, self.viewport.viewport_rows),
                )
            })
        else {
            return;
        };

        if compose_block_bottom_viewport_row(self, model_row).is_some() {
            return;
        }
        if desired_scroll != self.viewport.scroll {
            self.set_scroll_with_grep_sync(
                desired_scroll,
                false,
                HunkFocusScrollBehavior::Preserve,
            );
        }

        // The compose block is emitted only while the annotated row's anchor is still visible.
        // If the draft is too tall for the viewport, the footer can never be shown; do not
        // chase it past the anchor or the editor disappears entirely.
        let max_scroll = self.max_scroll().min(anchor);
        while compose_block_bottom_viewport_row(self, model_row).is_none()
            && self.viewport.scroll < max_scroll
        {
            let previous_scroll = self.viewport.scroll;
            self.set_scroll_with_grep_sync(
                self.viewport.scroll.saturating_add(1),
                false,
                HunkFocusScrollBehavior::Preserve,
            );
            if self.viewport.scroll == previous_scroll {
                break;
            }
        }
    }
    pub(crate) fn update_diff_mouse_hover(&mut self, column: u16, row: u16) {
        let next = self.diff_mouse_hover_in_diff_area(column, row);
        if self.viewport.mouse_hover != next {
            self.viewport.mouse_hover = next;
            self.runtime.dirty = true;
        }
    }

    pub(crate) fn clear_diff_mouse_hover(&mut self) {
        if self.viewport.mouse_hover.take().is_some() {
            self.runtime.dirty = true;
        }
    }

    fn diff_mouse_hover_in_diff_area(&self, column: u16, row: u16) -> Option<(u16, u16)> {
        if self.diff_modal_blocks_mouse_hover() {
            return None;
        }
        let area = self.viewport.rendered_diff_area?;
        if area.width == 0 || area.height == 0 {
            return None;
        }
        if column < area.x
            || row < area.y
            || column >= area.x.saturating_add(area.width)
            || row >= area.y.saturating_add(area.height)
        {
            return None;
        }
        Some((column.saturating_sub(area.x), row.saturating_sub(area.y)))
    }

    pub(crate) fn diff_modal_blocks_mouse_hover(&self) -> bool {
        self.overlays.help_menu_open
            || self.overlays.color_scheme_picker_open
            || self.overlays.options_menu_open
            || self.overlays.diff_menu_open
            || self.overlays.review_input_open
            || self.refs.commit_menu_open
            || self.refs.branch_menu_open.is_some()
            || self.filters.filter_input.is_some()
            || self.annotations_state.annotation_draft.is_some()
    }

    pub(crate) fn diff_mouse_highlight_visual_row(&self) -> Option<usize> {
        let (_, viewport_row) = self.viewport.mouse_hover?;
        visual_scroll_for_viewport_row(self, viewport_row)
    }
}
