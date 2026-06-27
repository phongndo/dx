use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::app::controllers::{
    filter::FilterInputContext, menu::MenuKeyContext, navigation::NavigationContext,
};

#[derive(Default)]
struct FakeKeyCtx {
    annotation_save_or_cancel: bool,
    reset_mouse_scroll_calls: usize,
    editor_shortcut: bool,
    filter_input: bool,
    annotation_input: bool,
    help_menu: Option<bool>,
    branch_menu: Option<bool>,
    commit_menu: Option<bool>,
    review_input: Option<bool>,
    diff_menu: Option<bool>,
    color_scheme_picker: Option<bool>,
    options_menu: Option<bool>,
    close_error_log: bool,
    pending_prefix: Option<bool>,
    single_global: Option<bool>,
    begin_prefix: bool,
    error_log_resize: bool,
    navigation: bool,
}

impl KeyEventContext for FakeKeyCtx {
    fn handle_annotation_save_or_cancel_key(&mut self, _key: KeyEvent) -> bool {
        self.annotation_save_or_cancel
    }

    fn reset_mouse_scroll(&mut self) {
        self.reset_mouse_scroll_calls += 1;
    }

    fn editor_shortcut_requested(&self, _key: KeyEvent) -> bool {
        self.editor_shortcut
    }

    fn handle_annotation_input_key_if_open(&mut self, _key: KeyEvent) -> bool {
        self.annotation_input
    }

    fn close_error_log_on_key(&mut self, _key: KeyEvent) -> bool {
        self.close_error_log
    }

    fn handle_pending_prefix_key(&mut self, _key: KeyEvent) -> MarkResult<Option<bool>> {
        Ok(self.pending_prefix)
    }

    fn handle_single_global_key(&mut self, _key: KeyEvent) -> MarkResult<Option<bool>> {
        Ok(self.single_global)
    }

    fn begin_prefix_if_matches(&mut self, _key: KeyEvent) -> bool {
        self.begin_prefix
    }

    fn handle_error_log_resize_key(&mut self, _key: KeyEvent) -> bool {
        self.error_log_resize
    }
}

impl FilterInputContext for FakeKeyCtx {
    fn filter_input_open(&self) -> bool {
        self.filter_input
    }

    fn handle_filter_input_key(&mut self, _key: KeyEvent) -> bool {
        self.filter_input
    }
}

impl MenuKeyContext for FakeKeyCtx {
    fn handle_help_menu_key_if_open(&mut self, _key: KeyEvent) -> MarkResult<Option<bool>> {
        Ok(self.help_menu)
    }

    fn handle_branch_menu_key_if_open(&mut self, _key: KeyEvent) -> MarkResult<Option<bool>> {
        Ok(self.branch_menu)
    }

    fn handle_commit_menu_key_if_open(&mut self, _key: KeyEvent) -> MarkResult<Option<bool>> {
        Ok(self.commit_menu)
    }

    fn handle_review_input_key_if_open(&mut self, _key: KeyEvent) -> MarkResult<Option<bool>> {
        Ok(self.review_input)
    }

    fn handle_diff_menu_key_if_open(&mut self, _key: KeyEvent) -> MarkResult<Option<bool>> {
        Ok(self.diff_menu)
    }

    fn handle_color_scheme_picker_key_if_open(
        &mut self,
        _key: KeyEvent,
    ) -> MarkResult<Option<bool>> {
        Ok(self.color_scheme_picker)
    }

    fn handle_options_menu_key_if_open(&mut self, _key: KeyEvent) -> MarkResult<Option<bool>> {
        Ok(self.options_menu)
    }
}

impl NavigationContext for FakeKeyCtx {
    fn filters_active(&self) -> bool {
        self.navigation
    }

    fn grep_filter_active(&self) -> bool {
        self.navigation
    }

    fn clear_all_filters(&mut self) {
        self.navigation = true;
    }

    fn scroll_or_focus_hunk(&mut self, _delta: isize) {
        self.navigation = true;
    }

    fn scroll_horizontally_by(&mut self, _delta: isize) {
        self.navigation = true;
    }

    fn set_scroll(&mut self, _scroll: usize) {
        self.navigation = true;
    }

    fn max_scroll(&self) -> usize {
        0
    }

    fn move_grep_match(&mut self, _delta: isize) {
        self.navigation = true;
    }
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

#[test]
fn annotation_draft_bindings_preempt_lower_key_layers() {
    let mut ctx = FakeKeyCtx {
        annotation_save_or_cancel: true,
        editor_shortcut: true,
        navigation: true,
        ..Default::default()
    };

    let result = route_event_through_layers(KEY_LAYERS, key(KeyCode::Char('s')), &mut ctx)
        .expect("route key");

    assert_eq!(result, ComponentEventResult::Consumed);
    assert_eq!(ctx.reset_mouse_scroll_calls, 0);
}

#[test]
fn mouse_scroll_reset_is_a_non_consuming_key_layer() {
    let mut ctx = FakeKeyCtx::default();

    let result = route_event_through_layers(KEY_LAYERS, key(KeyCode::Char('x')), &mut ctx)
        .expect("route key");

    assert_eq!(result, ComponentEventResult::Ignored);
    assert_eq!(ctx.reset_mouse_scroll_calls, 1);
}

#[test]
fn editor_shortcut_returns_effect_without_touching_diff_app() {
    let mut ctx = FakeKeyCtx {
        editor_shortcut: true,
        ..Default::default()
    };

    let result = route_event_through_layers(KEY_LAYERS, key(KeyCode::Char('e')), &mut ctx)
        .expect("route key");

    assert_eq!(
        result,
        ComponentEventResult::Effect(AppEffect::OpenEditorShortcut)
    );
}
