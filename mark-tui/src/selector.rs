use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::text_input::{TextInputKeyResult, handle_text_input_key};

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

    pub(crate) fn push_input(&mut self, character: char) {
        self.input.insert(self.input_cursor, character);
        self.input_cursor += character.len_utf8();
        self.selected = 0;
        self.scroll = 0;
    }

    pub(crate) fn pop_input(&mut self) -> TextInputKeyResult {
        let result = handle_text_input_key(
            &mut self.input,
            &mut self.input_cursor,
            KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
        );
        if matches!(result, TextInputKeyResult::Edited) {
            self.selected = 0;
            self.scroll = 0;
        }
        result
    }

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
