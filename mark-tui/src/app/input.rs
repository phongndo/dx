use super::*;
use crate::render::compositor::{ComponentEventResult, Compositor, EventComponent};

fn key_route_result(should_quit: bool) -> ComponentEventResult {
    if should_quit {
        ComponentEventResult::Quit
    } else {
        ComponentEventResult::Consumed
    }
}

fn route_key_through_layers(app: &mut DiffApp, key: KeyEvent) -> MarkResult<ComponentEventResult> {
    let mut compositor = Compositor::new();
    compositor.push(EventComponent::key(handle_navigation_key_layer));
    compositor.push(EventComponent::key(handle_error_log_resize_key_layer));
    compositor.push(EventComponent::key(handle_prefix_start_key_layer));
    compositor.push(EventComponent::key(handle_single_global_key_layer));
    compositor.push(EventComponent::key(handle_pending_prefix_key_layer));
    compositor.push(EventComponent::key(handle_error_log_close_key_layer));
    compositor.push(EventComponent::key(handle_options_menu_key_layer));
    compositor.push(EventComponent::key(handle_color_scheme_picker_key_layer));
    compositor.push(EventComponent::key(handle_diff_menu_key_layer));
    compositor.push(EventComponent::key(handle_review_input_key_layer));
    compositor.push(EventComponent::key(handle_commit_menu_key_layer));
    compositor.push(EventComponent::key(handle_branch_menu_key_layer));
    compositor.push(EventComponent::key(handle_help_menu_key_layer));
    compositor.push(EventComponent::key(handle_annotation_input_key_layer));
    compositor.push(EventComponent::key(handle_filter_input_key_layer));
    compositor.push(EventComponent::key(handle_mouse_scroll_reset_key_layer));
    compositor.push(EventComponent::key(handle_quit_key_layer));
    compositor.push(EventComponent::key(
        handle_annotation_save_or_cancel_key_layer,
    ));
    compositor.handle_key(key, app)
}

fn handle_annotation_save_or_cancel_key_layer(
    key: KeyEvent,
    app: &mut DiffApp,
) -> MarkResult<ComponentEventResult> {
    Ok(if app.handle_annotation_save_or_cancel_key(key) {
        ComponentEventResult::Consumed
    } else {
        ComponentEventResult::Ignored
    })
}

fn handle_quit_key_layer(key: KeyEvent, _app: &mut DiffApp) -> MarkResult<ComponentEventResult> {
    Ok(if is_quit_key(key) {
        ComponentEventResult::Quit
    } else {
        ComponentEventResult::Ignored
    })
}

fn handle_mouse_scroll_reset_key_layer(
    _key: KeyEvent,
    app: &mut DiffApp,
) -> MarkResult<ComponentEventResult> {
    app.input.mouse_scroll.reset();
    Ok(ComponentEventResult::Ignored)
}

fn handle_filter_input_key_layer(
    key: KeyEvent,
    app: &mut DiffApp,
) -> MarkResult<ComponentEventResult> {
    Ok(
        if app.filters.filter_input.is_some() && app.handle_filter_input_key(key) {
            ComponentEventResult::Consumed
        } else {
            ComponentEventResult::Ignored
        },
    )
}

fn handle_annotation_input_key_layer(
    key: KeyEvent,
    app: &mut DiffApp,
) -> MarkResult<ComponentEventResult> {
    Ok(
        if app.annotations_state.annotation_draft.is_some() && app.handle_annotation_input_key(key)
        {
            ComponentEventResult::Consumed
        } else {
            ComponentEventResult::Ignored
        },
    )
}

fn handle_help_menu_key_layer(
    key: KeyEvent,
    app: &mut DiffApp,
) -> MarkResult<ComponentEventResult> {
    if app.overlays.help_menu_open {
        return app.handle_help_menu_key(key).map(key_route_result);
    }
    Ok(ComponentEventResult::Ignored)
}

fn handle_branch_menu_key_layer(
    key: KeyEvent,
    app: &mut DiffApp,
) -> MarkResult<ComponentEventResult> {
    if app.refs.branch_menu_open.is_some() {
        return app.handle_branch_menu_key(key).map(key_route_result);
    }
    Ok(ComponentEventResult::Ignored)
}

fn handle_commit_menu_key_layer(
    key: KeyEvent,
    app: &mut DiffApp,
) -> MarkResult<ComponentEventResult> {
    if app.refs.commit_menu_open {
        return app.handle_commit_menu_key(key).map(key_route_result);
    }
    Ok(ComponentEventResult::Ignored)
}

fn handle_review_input_key_layer(
    key: KeyEvent,
    app: &mut DiffApp,
) -> MarkResult<ComponentEventResult> {
    if app.overlays.review_input_open {
        return app.handle_review_input_key(key).map(key_route_result);
    }
    Ok(ComponentEventResult::Ignored)
}

fn handle_diff_menu_key_layer(
    key: KeyEvent,
    app: &mut DiffApp,
) -> MarkResult<ComponentEventResult> {
    if app.overlays.diff_menu_open {
        return app.handle_diff_menu_key(key).map(key_route_result);
    }
    Ok(ComponentEventResult::Ignored)
}

fn handle_color_scheme_picker_key_layer(
    key: KeyEvent,
    app: &mut DiffApp,
) -> MarkResult<ComponentEventResult> {
    if app.overlays.color_scheme_picker_open {
        return app
            .handle_color_scheme_picker_key(key)
            .map(key_route_result);
    }
    Ok(ComponentEventResult::Ignored)
}

fn handle_options_menu_key_layer(
    key: KeyEvent,
    app: &mut DiffApp,
) -> MarkResult<ComponentEventResult> {
    if app.overlays.options_menu_open && !app.overlays.color_scheme_picker_open {
        return app.handle_options_menu_key(key).map(key_route_result);
    }
    Ok(ComponentEventResult::Ignored)
}

fn handle_error_log_close_key_layer(
    key: KeyEvent,
    app: &mut DiffApp,
) -> MarkResult<ComponentEventResult> {
    Ok(if key.code == KeyCode::Esc && app.close_error_log() {
        ComponentEventResult::Consumed
    } else {
        ComponentEventResult::Ignored
    })
}

fn handle_pending_prefix_key_layer(
    key: KeyEvent,
    app: &mut DiffApp,
) -> MarkResult<ComponentEventResult> {
    if let Some(prefix) = app.input.key_prefix_pending.take() {
        return app.handle_prefix_key(prefix, key).map(key_route_result);
    }
    Ok(ComponentEventResult::Ignored)
}

fn handle_single_global_key_layer(
    key: KeyEvent,
    app: &mut DiffApp,
) -> MarkResult<ComponentEventResult> {
    match app.handle_single_global_key(key)? {
        Some(should_quit) => Ok(key_route_result(should_quit)),
        None => Ok(ComponentEventResult::Ignored),
    }
}

fn handle_prefix_start_key_layer(
    key: KeyEvent,
    app: &mut DiffApp,
) -> MarkResult<ComponentEventResult> {
    Ok(if app.config.keymap.is_prefix(key) {
        app.input.key_prefix_pending = Some(KeyPress::from(key));
        app.runtime.dirty = true;
        ComponentEventResult::Consumed
    } else {
        ComponentEventResult::Ignored
    })
}

fn handle_error_log_resize_key_layer(
    key: KeyEvent,
    app: &mut DiffApp,
) -> MarkResult<ComponentEventResult> {
    if app.notifications.error_log.is_some() {
        match key.code {
            KeyCode::Char('+') | KeyCode::Char('=') => {
                app.resize_error_log(1);
                return Ok(ComponentEventResult::Consumed);
            }
            KeyCode::Char('-') => {
                app.resize_error_log(-1);
                return Ok(ComponentEventResult::Consumed);
            }
            _ => {}
        }
    }
    Ok(ComponentEventResult::Ignored)
}

fn handle_navigation_key_layer(
    key: KeyEvent,
    app: &mut DiffApp,
) -> MarkResult<ComponentEventResult> {
    match key.code {
        KeyCode::Esc if app.filters_active() => app.clear_all_filters(),
        KeyCode::Down | KeyCode::Char('j') => app.scroll_or_focus_hunk(1),
        KeyCode::Up | KeyCode::Char('k') => app.scroll_or_focus_hunk(-1),
        KeyCode::Left | KeyCode::Char('h') => {
            app.scroll_horizontally_by(-(HORIZONTAL_SCROLL_STEP as isize));
        }
        KeyCode::Right | KeyCode::Char('l') => {
            app.scroll_horizontally_by(HORIZONTAL_SCROLL_STEP as isize);
        }
        KeyCode::PageDown => app.scroll_or_focus_hunk(20),
        KeyCode::Char('d') if is_plain_char_key(key, 'd') => app.scroll_or_focus_hunk(20),
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.scroll_or_focus_hunk(20);
        }
        KeyCode::PageUp | KeyCode::Char('u') => app.scroll_or_focus_hunk(-20),
        KeyCode::Home => app.set_scroll(0),
        KeyCode::Char('g') if is_plain_char_key(key, 'g') => app.set_scroll(0),
        KeyCode::End | KeyCode::Char('G') => app.set_scroll(app.max_scroll()),
        KeyCode::Char('n') if !app.filters.grep_filter.is_empty() => app.move_grep_match(1),
        KeyCode::Char('p') | KeyCode::Char('N') if !app.filters.grep_filter.is_empty() => {
            app.move_grep_match(-1);
        }
        KeyCode::Char('n') | KeyCode::Char('p') | KeyCode::Char('N') => {}
        _ => return Ok(ComponentEventResult::Ignored),
    }

    Ok(ComponentEventResult::Consumed)
}

impl DiffApp {
    pub(crate) fn handle_key(&mut self, key: KeyEvent) -> MarkResult<bool> {
        route_key_through_layers(self, key)
            .map(|result| matches!(result, ComponentEventResult::Quit))
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
        self.input.key_prefix_pending = None;

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
        self.filters.filter_input.is_none()
            && !self.overlays.help_menu_open
            && self.refs.branch_menu_open.is_none()
            && !self.overlays.diff_menu_open
            && !self.overlays.review_input_open
            && !self.overlays.options_menu_open
            && self.input.key_prefix_pending.is_none()
            && !self.overlays.color_scheme_picker_open
            && !self.refs.commit_menu_open
    }
}
