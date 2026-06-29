use crossterm::event::KeyEvent;

use crate::text_input::{TextInputKeyResult, handle_text_input_key};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SelectorMovement {
    Wrapping,
    Saturating,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SelectorInputOutcome {
    Ignored,
    Handled,
    Changed,
}

impl SelectorInputOutcome {
    pub(crate) fn handled(self) -> bool {
        !matches!(self, Self::Ignored)
    }

    pub(crate) fn changed(self) -> bool {
        matches!(self, Self::Changed)
    }
}

/// Shared controller for searchable selector-style UI.
///
/// Diff type, branch, commit, options and color-scheme popups all share the
/// same interaction contract: a text filter, a selected row, an optional scroll
/// window, and movement that is either wrapping (completion-style) or saturating
/// (scrollable list-style). Keeping that behavior here avoids each menu growing
/// its own tiny state machine.
pub(crate) struct SelectorController<'a> {
    state: &'a mut SelectorState,
    item_count: usize,
    visible_rows: Option<usize>,
}

impl<'a> SelectorController<'a> {
    pub(crate) fn new(state: &'a mut SelectorState, item_count: usize) -> Self {
        Self {
            state,
            item_count,
            visible_rows: None,
        }
    }

    pub(crate) fn with_visible_rows(mut self, visible_rows: usize) -> Self {
        self.visible_rows = Some(visible_rows.max(1));
        self
    }

    pub(crate) fn move_by(&mut self, delta: isize, movement: SelectorMovement) -> bool {
        let changed = match movement {
            SelectorMovement::Wrapping => self.state.move_wrapping(self.item_count, delta),
            SelectorMovement::Saturating => self.state.move_saturating(self.item_count, delta),
        };
        if changed {
            self.ensure_selected_visible();
        }
        changed
    }

    pub(crate) fn set_selected(&mut self, selected: usize) -> bool {
        let changed = self.state.set_selected(selected, self.item_count);
        if changed {
            self.ensure_selected_visible();
        }
        changed
    }

    pub(crate) fn apply_input_key(&mut self, key: KeyEvent) -> SelectorInputOutcome {
        match self.state.apply_input_key(key) {
            TextInputKeyResult::Edited => {
                self.ensure_selected_visible();
                SelectorInputOutcome::Changed
            }
            TextInputKeyResult::Moved => SelectorInputOutcome::Changed,
            TextInputKeyResult::Handled => SelectorInputOutcome::Handled,
            TextInputKeyResult::Ignored => SelectorInputOutcome::Ignored,
        }
    }

    pub(crate) fn ensure_selected_visible(&mut self) {
        if let Some(visible_rows) = self.visible_rows {
            self.state
                .ensure_selected_visible(self.item_count, visible_rows);
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SelectorState {
    pub(crate) input: String,
    pub(crate) input_cursor: usize,
    pub(crate) selected: usize,
    pub(crate) scroll: usize,
}

impl SelectorState {
    pub(crate) fn reset(&mut self) {
        self.input.clear();
        self.input_cursor = 0;
        self.selected = 0;
        self.scroll = 0;
    }

    pub(crate) fn reset_input(&mut self) {
        self.input.clear();
        self.input_cursor = 0;
    }

    pub(crate) fn reset_input_and_scroll(&mut self) {
        self.reset_input();
        self.scroll = 0;
    }

    pub(crate) fn move_wrapping(&mut self, len: usize, delta: isize) -> bool {
        if len == 0 {
            return false;
        }

        let previous = self.selected;
        self.selected = (self.selected as isize + delta).rem_euclid(len as isize) as usize;
        self.selected != previous
    }

    pub(crate) fn move_saturating(&mut self, len: usize, delta: isize) -> bool {
        let selected = if delta < 0 {
            self.selected.saturating_sub(delta.unsigned_abs())
        } else {
            self.selected.saturating_add(delta as usize)
        };
        self.set_selected(selected, len)
    }

    pub(crate) fn set_selected(&mut self, selected: usize, len: usize) -> bool {
        let selected = selected.min(len.saturating_sub(1));
        if self.selected == selected {
            return false;
        }

        self.selected = selected;
        true
    }

    pub(crate) fn clamp(&mut self, len: usize) -> bool {
        let previous_selected = self.selected;
        let previous_scroll = self.scroll;

        if len == 0 {
            self.selected = 0;
            self.scroll = 0;
        } else {
            self.selected = self.selected.min(len.saturating_sub(1));
            self.scroll = self.scroll.min(self.selected);
        }

        self.selected != previous_selected || self.scroll != previous_scroll
    }

    #[cfg(test)]
    pub(crate) fn push_input(&mut self, character: char) {
        self.input.insert(self.input_cursor, character);
        self.input_cursor += character.len_utf8();
        self.selected = 0;
        self.scroll = 0;
    }

    #[cfg(test)]
    pub(crate) fn clear_input_and_selection(&mut self) -> bool {
        if self.input.is_empty() && self.input_cursor == 0 && self.selected == 0 && self.scroll == 0
        {
            return false;
        }

        self.reset();
        true
    }

    pub(crate) fn apply_input_key(&mut self, key: KeyEvent) -> TextInputKeyResult {
        let result = handle_text_input_key(&mut self.input, &mut self.input_cursor, key);
        if matches!(result, TextInputKeyResult::Edited) {
            self.selected = 0;
            self.scroll = 0;
        }
        result
    }

    pub(crate) fn ensure_selected_visible(&mut self, item_count: usize, visible_rows: usize) {
        ensure_selector_scroll(&mut self.scroll, self.selected, item_count, visible_rows);
    }
}

/// Keeps `selected` visible in a scrollable list of `item_count` rows.
fn ensure_selector_scroll(
    scroll: &mut usize,
    selected: usize,
    item_count: usize,
    visible_rows: usize,
) {
    if visible_rows == 0 {
        *scroll = 0;
        return;
    }

    let max_scroll = item_count.saturating_sub(visible_rows.max(1));
    if selected < *scroll {
        *scroll = selected;
    } else if selected >= scroll.saturating_add(visible_rows) {
        *scroll = selected.saturating_add(1).saturating_sub(visible_rows);
    }
    *scroll = (*scroll).min(max_scroll);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyModifiers};

    #[test]
    fn selector_controller_saturating_movement_keeps_selection_visible() {
        let mut state = SelectorState::default();
        let mut controller = SelectorController::new(&mut state, 10).with_visible_rows(3);

        assert!(controller.move_by(4, SelectorMovement::Saturating));
        assert_eq!(state.selected, 4);
        assert_eq!(state.scroll, 2);

        let mut controller = SelectorController::new(&mut state, 10).with_visible_rows(3);
        assert!(controller.move_by(-3, SelectorMovement::Saturating));
        assert_eq!(state.selected, 1);
        assert_eq!(state.scroll, 1);
    }

    #[test]
    fn selector_controller_wrapping_movement_wraps_around() {
        let mut state = SelectorState::default();
        let mut controller = SelectorController::new(&mut state, 3).with_visible_rows(2);

        assert!(controller.move_by(-1, SelectorMovement::Wrapping));
        assert_eq!(state.selected, 2);
        assert_eq!(state.scroll, 1);

        let mut controller = SelectorController::new(&mut state, 3).with_visible_rows(2);
        assert!(controller.move_by(1, SelectorMovement::Wrapping));
        assert_eq!(state.selected, 0);
        assert_eq!(state.scroll, 0);
    }

    #[test]
    fn selector_input_edit_resets_selection_and_scroll() {
        let mut state = SelectorState {
            selected: 5,
            scroll: 4,
            ..SelectorState::default()
        };
        let outcome = SelectorController::new(&mut state, 10)
            .with_visible_rows(3)
            .apply_input_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));

        assert_eq!(outcome, SelectorInputOutcome::Changed);
        assert_eq!(state.input, "x");
        assert_eq!(state.selected, 0);
        assert_eq!(state.scroll, 0);
    }
}
