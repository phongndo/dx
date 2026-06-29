mod event_context;
mod layers;

use super::{ActionOutcome, AppAction, DiffApp, NORMAL_GLOBAL_ACTIONS};
use crate::keymap::{GlobalAction, KeyPress, MenuAction};
use crate::text_input::{TextInputKeyResult, handle_text_input_key};
use crossterm::event::{KeyCode, KeyEvent};
use mark_core::MarkResult;

use layers::route_key_through_layers;

impl DiffApp {
    #[cfg(test)]
    pub(crate) fn handle_key(&mut self, key: KeyEvent) -> MarkResult<bool> {
        let outcome = self.handle_key_with_effects(key)?;
        let legacy = outcome.clone().into_legacy_quit().unwrap_or(false);
        self.run_effects(outcome.into_effects())?;
        Ok(legacy)
    }

    pub(crate) fn handle_key_with_effects(&mut self, key: KeyEvent) -> MarkResult<ActionOutcome> {
        let mut outcome =
            ActionOutcome::from_component_event_result(route_key_through_layers(self, key)?);
        outcome.extend_effects(self.take_queued_effects());
        Ok(outcome)
    }

    pub(crate) fn handle_single_global_key(&mut self, key: KeyEvent) -> MarkResult<Option<bool>> {
        for action in NORMAL_GLOBAL_ACTIONS.iter().copied() {
            if self.config.keymap.matches_single(action, key) {
                return self.perform_global_action(action);
            }
        }

        Ok(None)
    }

    pub(crate) fn handle_prefix_key(
        &mut self,
        prefix: KeyPress,
        key: KeyEvent,
    ) -> MarkResult<bool> {
        self.input.clear_key_prefix();

        if key.code == KeyCode::Esc {
            self.runtime.dirty = true;
            return Ok(false);
        }

        for action in NORMAL_GLOBAL_ACTIONS.iter().copied() {
            if self.config.keymap.matches_prefix(action, prefix, key) {
                return Ok(self.perform_global_action(action)?.unwrap_or(false));
            }
        }

        self.runtime.dirty = true;
        Ok(false)
    }

    pub(super) fn perform_global_action(
        &mut self,
        action: GlobalAction,
    ) -> MarkResult<Option<bool>> {
        AppAction::from_global(action)
            .map(|action| self.perform_app_action(action))
            .unwrap_or(Ok(None))
    }

    pub(crate) fn handle_diff_menu_key(&mut self, key: KeyEvent) -> MarkResult<bool> {
        if self.config.keymap.matches_menu(MenuAction::Close, key) {
            self.close_diff_menu();
            return Ok(false);
        }

        if self.config.keymap.matches_menu(MenuAction::Down, key) {
            self.move_diff_menu_selection(1);
        } else if self.config.keymap.matches_menu(MenuAction::Up, key) {
            self.move_diff_menu_selection(-1);
        } else if self.config.keymap.matches_menu(MenuAction::Select, key)
            || self.config.keymap.matches_menu(MenuAction::Confirm, key)
        {
            self.select_highlighted_diff_choice();
        } else if !self.apply_diff_menu_input_key(key) {
            match key.code {
                KeyCode::Home => self.set_diff_menu_selection(0),
                KeyCode::End => self.set_diff_menu_selection(usize::MAX),
                _ => {}
            }
        }

        Ok(false)
    }

    pub(crate) fn handle_review_input_key(&mut self, key: KeyEvent) -> MarkResult<bool> {
        if self.config.keymap.matches_menu(MenuAction::Close, key) {
            self.close_review_input();
            return Ok(false);
        }

        if self.config.keymap.matches_menu(MenuAction::Select, key)
            || self.config.keymap.matches_menu(MenuAction::Confirm, key)
        {
            self.submit_review_input();
        } else {
            match handle_text_input_key(
                &mut self.overlays.review_input,
                &mut self.overlays.review_input_cursor,
                key,
            ) {
                TextInputKeyResult::Edited | TextInputKeyResult::Moved => self.runtime.dirty = true,
                TextInputKeyResult::Handled | TextInputKeyResult::Ignored => {}
            }
        }

        Ok(false)
    }

    pub(crate) fn handle_branch_menu_key(&mut self, key: KeyEvent) -> MarkResult<bool> {
        if self.config.keymap.matches_menu(MenuAction::Close, key) {
            self.close_branch_menu();
            return Ok(false);
        }

        if self.config.keymap.matches_menu(MenuAction::Down, key) {
            self.cycle_branch_completion(1);
        } else if self.config.keymap.matches_menu(MenuAction::Up, key) {
            self.cycle_branch_completion(-1);
        } else if self.config.keymap.matches_menu(MenuAction::Select, key)
            || self.config.keymap.matches_menu(MenuAction::Confirm, key)
        {
            self.select_highlighted_branch_match();
        } else if !self.apply_branch_input_key(key) {
            match key.code {
                KeyCode::PageDown => self.move_branch_selection(self.branch_menu_rows() as isize),
                KeyCode::PageUp => self.move_branch_selection(-(self.branch_menu_rows() as isize)),
                KeyCode::Home => self.set_branch_selection(0),
                KeyCode::End => self.set_branch_selection(usize::MAX),
                _ => {}
            }
        }

        Ok(false)
    }

    pub(crate) fn handle_commit_menu_key(&mut self, key: KeyEvent) -> MarkResult<bool> {
        if self.config.keymap.matches_menu(MenuAction::Close, key) {
            self.close_commit_menu();
            return Ok(false);
        }

        if self.config.keymap.matches_menu(MenuAction::Down, key) {
            self.cycle_commit_completion(1);
        } else if self.config.keymap.matches_menu(MenuAction::Up, key) {
            self.cycle_commit_completion(-1);
        } else if self.config.keymap.matches_menu(MenuAction::Select, key)
            || self.config.keymap.matches_menu(MenuAction::Confirm, key)
        {
            self.select_highlighted_commit_match();
        } else if !self.apply_commit_input_key(key) {
            match key.code {
                KeyCode::PageDown => self.move_commit_selection(self.commit_menu_rows() as isize),
                KeyCode::PageUp => self.move_commit_selection(-(self.commit_menu_rows() as isize)),
                KeyCode::Home => self.set_commit_selection(0),
                KeyCode::End => self.set_commit_selection(usize::MAX),
                _ => {}
            }
        }

        Ok(false)
    }

    pub(crate) fn handle_options_menu_key(&mut self, key: KeyEvent) -> MarkResult<bool> {
        if self.config.keymap.matches_menu(MenuAction::Close, key) {
            self.close_options_menu();
            return Ok(false);
        }

        if self.config.keymap.matches_menu(MenuAction::Down, key) {
            self.move_options_menu_selection(1);
        } else if self.config.keymap.matches_menu(MenuAction::Up, key) {
            self.move_options_menu_selection(-1);
        } else if self.config.keymap.matches_menu(MenuAction::Select, key)
            || self.config.keymap.matches_menu(MenuAction::Confirm, key)
        {
            self.activate_selected_option();
        } else if !self.apply_options_menu_input_key(key) {
            match key.code {
                KeyCode::Left => self.cycle_selected_option(-1),
                KeyCode::Right => self.cycle_selected_option(1),
                KeyCode::Home => self.set_options_menu_selection(0),
                KeyCode::End => self.set_options_menu_selection(usize::MAX),
                _ => {}
            }
        }

        Ok(false)
    }

    pub(crate) fn handle_color_scheme_picker_key(&mut self, key: KeyEvent) -> MarkResult<bool> {
        if self.config.keymap.matches_menu(MenuAction::Close, key) {
            self.close_color_scheme_picker();
            return Ok(false);
        }

        if self.config.keymap.matches_menu(MenuAction::Down, key) {
            self.move_color_scheme_selection(1);
        } else if self.config.keymap.matches_menu(MenuAction::Up, key) {
            self.move_color_scheme_selection(-1);
        } else if self.config.keymap.matches_menu(MenuAction::Select, key)
            || self.config.keymap.matches_menu(MenuAction::Confirm, key)
        {
            self.select_highlighted_color_scheme();
        } else if !self.apply_color_scheme_input_key(key) {
            match key.code {
                KeyCode::Home => self.set_color_scheme_selection(0),
                KeyCode::End => self.set_color_scheme_selection(usize::MAX),
                _ => {}
            }
        }

        Ok(false)
    }

    pub(crate) fn editor_shortcut_available(&self) -> bool {
        !self.filters.input_open()
            && !self.overlays.help_menu_is_open()
            && !self.refs.branch_menu_is_open()
            && !self.overlays.diff_menu_is_open()
            && !self.overlays.review_input_is_open()
            && !self.overlays.options_menu_is_open()
            && self.input.key_prefix_pending.is_none()
            && !self.overlays.color_scheme_picker_is_open()
            && !self.refs.commit_menu_is_open()
    }
}
