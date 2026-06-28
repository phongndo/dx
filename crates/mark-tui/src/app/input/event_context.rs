use crossterm::event::{KeyCode, KeyEvent};
use mark_core::MarkResult;

use super::super::{
    DiffApp,
    controllers::{
        filter::FilterInputContext, menu::MenuKeyContext, navigation::NavigationContext,
    },
};
use crate::keymap::{GlobalAction, KeyPress};

pub(super) trait KeyEventContext:
    FilterInputContext + MenuKeyContext + NavigationContext
{
    fn handle_annotation_save_or_cancel_key(&mut self, key: KeyEvent) -> bool;
    fn reset_mouse_scroll(&mut self);
    fn editor_shortcut_requested(&self, key: KeyEvent) -> bool;
    fn handle_annotation_input_key_if_open(&mut self, key: KeyEvent) -> bool;
    fn close_error_log_on_key(&mut self, key: KeyEvent) -> bool;
    fn handle_pending_prefix_key(&mut self, key: KeyEvent) -> MarkResult<Option<bool>>;
    fn handle_single_global_key(&mut self, key: KeyEvent) -> MarkResult<Option<bool>>;
    fn begin_prefix_if_matches(&mut self, key: KeyEvent) -> bool;
    fn handle_error_log_resize_key(&mut self, key: KeyEvent) -> bool;
}

pub(super) struct KeyEventCtx<'a> {
    app: &'a mut DiffApp,
}

impl<'a> KeyEventCtx<'a> {
    pub(super) fn new(app: &'a mut DiffApp) -> Self {
        Self { app }
    }
}

impl KeyEventContext for KeyEventCtx<'_> {
    fn handle_annotation_save_or_cancel_key(&mut self, key: KeyEvent) -> bool {
        self.app.handle_annotation_save_or_cancel_key(key)
    }

    fn reset_mouse_scroll(&mut self) {
        self.app.input.reset_mouse_scroll();
    }

    fn editor_shortcut_requested(&self, key: KeyEvent) -> bool {
        self.app
            .config
            .keymap
            .matches_single(GlobalAction::EditHunk, key)
            && self.app.editor_shortcut_available()
    }

    fn handle_annotation_input_key_if_open(&mut self, key: KeyEvent) -> bool {
        self.app.annotations_state.annotation_draft.is_some()
            && self.app.handle_annotation_input_key(key)
    }

    fn close_error_log_on_key(&mut self, key: KeyEvent) -> bool {
        key.code == KeyCode::Esc && self.app.close_error_log()
    }

    fn handle_pending_prefix_key(&mut self, key: KeyEvent) -> MarkResult<Option<bool>> {
        if let Some(prefix) = self.app.input.take_key_prefix() {
            return self.app.handle_prefix_key(prefix, key).map(Some);
        }
        Ok(None)
    }

    fn handle_single_global_key(&mut self, key: KeyEvent) -> MarkResult<Option<bool>> {
        self.app.handle_single_global_key(key)
    }

    fn begin_prefix_if_matches(&mut self, key: KeyEvent) -> bool {
        if self.app.config.keymap.is_prefix(key) {
            self.app.input.begin_key_prefix(KeyPress::from(key));
            self.app.runtime.dirty = true;
            return true;
        }
        false
    }

    fn handle_error_log_resize_key(&mut self, key: KeyEvent) -> bool {
        if self.app.notifications.error_log.is_none() {
            return false;
        }

        match key.code {
            KeyCode::Char('+') | KeyCode::Char('=') => self.app.resize_error_log(1),
            KeyCode::Char('-') => self.app.resize_error_log(-1),
            _ => return false,
        };
        true
    }
}

impl FilterInputContext for KeyEventCtx<'_> {
    fn filter_input_open(&self) -> bool {
        self.app.filters.input_open()
    }

    fn handle_filter_input_key(&mut self, key: KeyEvent) -> bool {
        self.app.handle_filter_input_key(key)
    }
}

impl MenuKeyContext for KeyEventCtx<'_> {
    fn handle_help_menu_key_if_open(&mut self, key: KeyEvent) -> MarkResult<Option<bool>> {
        if self.app.overlays.help_menu_is_open() {
            return self.app.handle_help_menu_key(key).map(Some);
        }
        Ok(None)
    }

    fn handle_branch_menu_key_if_open(&mut self, key: KeyEvent) -> MarkResult<Option<bool>> {
        if self.app.refs.branch_menu_is_open() {
            return self.app.handle_branch_menu_key(key).map(Some);
        }
        Ok(None)
    }

    fn handle_commit_menu_key_if_open(&mut self, key: KeyEvent) -> MarkResult<Option<bool>> {
        if self.app.refs.commit_menu_is_open() {
            return self.app.handle_commit_menu_key(key).map(Some);
        }
        Ok(None)
    }

    fn handle_review_input_key_if_open(&mut self, key: KeyEvent) -> MarkResult<Option<bool>> {
        if self.app.overlays.review_input_is_open() {
            return self.app.handle_review_input_key(key).map(Some);
        }
        Ok(None)
    }

    fn handle_diff_menu_key_if_open(&mut self, key: KeyEvent) -> MarkResult<Option<bool>> {
        if self.app.overlays.diff_menu_is_open() {
            return self.app.handle_diff_menu_key(key).map(Some);
        }
        Ok(None)
    }

    fn handle_color_scheme_picker_key_if_open(
        &mut self,
        key: KeyEvent,
    ) -> MarkResult<Option<bool>> {
        if self.app.overlays.color_scheme_picker_is_open() {
            return self.app.handle_color_scheme_picker_key(key).map(Some);
        }
        Ok(None)
    }

    fn handle_options_menu_key_if_open(&mut self, key: KeyEvent) -> MarkResult<Option<bool>> {
        if self.app.overlays.options_menu_is_open()
            && !self.app.overlays.color_scheme_picker_is_open()
        {
            return self.app.handle_options_menu_key(key).map(Some);
        }
        Ok(None)
    }

    fn handle_annotation_menu_key_if_open(&mut self, key: KeyEvent) -> MarkResult<Option<bool>> {
        if self.app.overlays.annotation_menu_is_open() {
            return self.app.handle_annotation_menu_key(key).map(Some);
        }
        Ok(None)
    }
}

impl NavigationContext for KeyEventCtx<'_> {
    fn filters_active(&self) -> bool {
        self.app.filters.active()
    }

    fn grep_filter_active(&self) -> bool {
        self.app.filters.grep_active()
    }

    fn clear_all_filters(&mut self) {
        self.app.clear_all_filters();
    }

    fn scroll_or_focus_hunk(&mut self, delta: isize) {
        self.app.scroll_or_focus_hunk(delta);
    }

    fn scroll_horizontally_by(&mut self, delta: isize) {
        self.app.scroll_horizontally_by(delta);
    }

    fn set_scroll(&mut self, scroll: usize) {
        self.app.set_scroll(scroll);
    }

    fn max_scroll(&self) -> usize {
        self.app.max_scroll()
    }

    fn move_grep_match(&mut self, delta: isize) {
        self.app.move_grep_match(delta);
    }
}
