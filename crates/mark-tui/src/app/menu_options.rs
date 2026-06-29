mod color_scheme;

use super::{
    AppEffect, COLOR_SCHEME_CHOICES, COMMON_OPTIONS_MENU_ITEMS, DiffApp, OptionsDraft,
    OptionsMenuItem, SyntaxStartupMode, checkbox, color_scheme_label, context_expansion_label,
    layout_setting_from_override, layout_setting_label, next_context_expansion,
    next_layout_setting, next_notification_mode, next_toast_corner, next_toast_max_visible,
    next_toast_timeout_ms, notification_mode_label, on_off_search, option_label,
    previous_context_expansion, toast_corner_label, toast_timeout_label,
};
use crate::controls::branch_match_score;
use crate::selector::{SelectorController, SelectorMovement};
use crate::syntax::SyntaxRuntime;
use crossterm::event::KeyEvent;
use mark_core::MarkResult;
use mark_syntax::NotificationSettings;

impl DiffApp {
    pub(crate) fn open_options_menu(&mut self) {
        self.close_color_scheme_picker();
        self.overlays.options_menu_draft = OptionsDraft {
            layout: layout_setting_from_override(self.viewport.layout_override),
            live_updates_enabled: self.jobs.live_updates.enabled(),
            context_expansion: self.config.theme.diff.context_expansion,
            syntax_enabled: self.config.syntax.is_some(),
            line_wrapping: self.viewport.line_wrapping,
            color_scheme: self.config.color_scheme,
            notification_mode: self.config.syntax_settings.notifications.mode(),
            toast_corner: self.config.syntax_settings.notifications.corner(),
            toast_timeout_ms: self.config.syntax_settings.notifications.timeout_ms(),
            toast_max_visible: self.config.syntax_settings.notifications.max_visible(),
        };
        let len = self.options_menu_items().len();
        self.overlays
            .options_menu
            .set_selected(self.overlays.options_menu.selected, len);
        self.overlays.options_menu.reset_input_and_scroll();
        self.overlays.open_options_menu();
        self.overlays.hide_diff_menu();
        self.overlays.diff_menu.reset_input();
        self.set_rendered_diff_menu_area(None);
        self.close_review_input();
        self.close_branch_menu();
        self.close_commit_menu();
        self.runtime.dirty = true;
    }

    pub(crate) fn close_options_menu(&mut self) {
        if self.overlays.color_scheme_picker_is_open() {
            self.close_color_scheme_picker();
        }
        if self.overlays.close_options_menu() {
            self.runtime.dirty = true;
        }
    }

    pub(crate) fn highlighted_option(&self) -> Option<OptionsMenuItem> {
        self.filtered_options_menu_items()
            .get(self.overlays.options_menu.selected)
            .copied()
    }

    pub(crate) fn move_options_menu_selection(&mut self, delta: isize) {
        let len = self.filtered_options_menu_items().len();
        if len == 0 {
            return;
        }

        if SelectorController::new(&mut self.overlays.options_menu, len)
            .move_by(delta, SelectorMovement::Wrapping)
        {
            self.runtime.dirty = true;
        }
    }

    pub(crate) fn set_options_menu_selection(&mut self, selected: usize) {
        let len = self.filtered_options_menu_items().len();
        if SelectorController::new(&mut self.overlays.options_menu, len).set_selected(selected) {
            self.runtime.dirty = true;
        }
    }

    pub(crate) fn ensure_options_menu_selection_visible(&mut self, visible_rows: usize) {
        let len = self.filtered_options_menu_items().len();
        self.overlays
            .options_menu
            .ensure_selected_visible(len, visible_rows);
    }

    pub(crate) fn clamp_options_menu_selection_to_filtered_items(&mut self) {
        let len = self.filtered_options_menu_items().len();
        if self.overlays.options_menu.clamp(len) {
            self.runtime.dirty = true;
        }
    }

    pub(crate) fn options_menu_items(&self) -> &'static [OptionsMenuItem] {
        COMMON_OPTIONS_MENU_ITEMS
    }

    pub(crate) fn filtered_options_menu_items(&self) -> Vec<OptionsMenuItem> {
        let items = self.options_menu_items();
        let query = self.overlays.options_menu.input.trim().to_ascii_lowercase();
        if query.is_empty() {
            return items.to_vec();
        }

        let mut matches: Vec<_> = items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                self.option_match_score(&query, *item)
                    .map(|score| (score, index, *item))
            })
            .collect();
        matches.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
        matches.into_iter().map(|(_, _, item)| item).collect()
    }

    pub(crate) fn option_match_score(
        &self,
        query: &str,
        item: OptionsMenuItem,
    ) -> Option<(usize, usize)> {
        let label = option_label(item).to_ascii_lowercase();
        let value = self.option_value(item).to_ascii_lowercase();
        let search_value = self.option_search_value(item).to_ascii_lowercase();
        let combined = format!("{label} {value} {search_value}");
        branch_match_score(query, &label)
            .or_else(|| branch_match_score(query, &value))
            .or_else(|| branch_match_score(query, &search_value))
            .or_else(|| branch_match_score(query, &combined))
    }

    pub(crate) fn option_search_value(&self, item: OptionsMenuItem) -> String {
        match item {
            OptionsMenuItem::Layout => {
                layout_setting_label(self.overlays.options_menu_draft.layout).to_owned()
            }
            OptionsMenuItem::LiveReload if !self.jobs.live_updates.allowed() => {
                "off disabled".to_owned()
            }
            OptionsMenuItem::LiveReload => {
                on_off_search(self.overlays.options_menu_draft.live_updates_enabled)
            }
            OptionsMenuItem::ContextExpansion => {
                context_expansion_label(self.overlays.options_menu_draft.context_expansion)
            }
            OptionsMenuItem::SyntaxHighlighting => {
                on_off_search(self.overlays.options_menu_draft.syntax_enabled)
            }
            OptionsMenuItem::LineWrapping => {
                on_off_search(self.overlays.options_menu_draft.line_wrapping)
            }
            OptionsMenuItem::ColorScheme => {
                color_scheme_label(self.overlays.options_menu_draft.color_scheme).to_owned()
            }
            OptionsMenuItem::NotificationMode => {
                notification_mode_label(self.overlays.options_menu_draft.notification_mode)
                    .to_owned()
            }
            OptionsMenuItem::ToastCorner => {
                toast_corner_label(self.overlays.options_menu_draft.toast_corner).to_owned()
            }
            OptionsMenuItem::ToastTimeout => {
                toast_timeout_label(self.overlays.options_menu_draft.toast_timeout_ms)
            }
            OptionsMenuItem::ToastMaxVisible => self
                .overlays
                .options_menu_draft
                .toast_max_visible
                .to_string(),
        }
    }

    pub(crate) fn option_value(&self, item: OptionsMenuItem) -> String {
        match item {
            OptionsMenuItem::Layout => {
                format!(
                    "[{}]",
                    layout_setting_label(self.overlays.options_menu_draft.layout)
                )
            }
            OptionsMenuItem::LiveReload if !self.jobs.live_updates.allowed() => {
                "[ ] disabled".to_owned()
            }
            OptionsMenuItem::LiveReload => {
                checkbox(self.overlays.options_menu_draft.live_updates_enabled)
            }
            OptionsMenuItem::ContextExpansion => {
                format!(
                    "[{}]",
                    context_expansion_label(self.overlays.options_menu_draft.context_expansion)
                )
            }
            OptionsMenuItem::SyntaxHighlighting => {
                checkbox(self.overlays.options_menu_draft.syntax_enabled)
            }
            OptionsMenuItem::LineWrapping => {
                checkbox(self.overlays.options_menu_draft.line_wrapping)
            }
            OptionsMenuItem::ColorScheme => {
                format!(
                    "[{}]",
                    color_scheme_label(self.overlays.options_menu_draft.color_scheme)
                )
            }
            OptionsMenuItem::NotificationMode => {
                format!(
                    "[{}]",
                    notification_mode_label(self.overlays.options_menu_draft.notification_mode)
                )
            }
            OptionsMenuItem::ToastCorner => {
                format!(
                    "[{}]",
                    toast_corner_label(self.overlays.options_menu_draft.toast_corner)
                )
            }
            OptionsMenuItem::ToastTimeout => {
                format!(
                    "[{}]",
                    toast_timeout_label(self.overlays.options_menu_draft.toast_timeout_ms)
                )
            }
            OptionsMenuItem::ToastMaxVisible => {
                format!("[{}]", self.overlays.options_menu_draft.toast_max_visible)
            }
        }
    }

    pub(crate) fn apply_options_menu_input_key(&mut self, key: KeyEvent) -> bool {
        let len = self.filtered_options_menu_items().len();
        let outcome =
            SelectorController::new(&mut self.overlays.options_menu, len).apply_input_key(key);
        if outcome.changed() {
            self.runtime.dirty = true;
        }
        outcome.handled()
    }

    pub(crate) fn activate_selected_option(&mut self) {
        match self.highlighted_option() {
            Some(OptionsMenuItem::ColorScheme) => self.open_color_scheme_picker(),
            Some(_) => self.cycle_selected_option(1),
            None => {}
        }
    }

    pub(crate) fn cycle_selected_option(&mut self, delta: isize) {
        let Some(changed_item) = self.highlighted_option() else {
            return;
        };

        match changed_item {
            OptionsMenuItem::Layout => {
                self.overlays.options_menu_draft.layout =
                    next_layout_setting(self.overlays.options_menu_draft.layout, delta);
            }
            OptionsMenuItem::LiveReload => {
                if !self.jobs.live_updates.allowed() {
                    self.set_error_log("live reload disabled by --no-watch");
                    return;
                }
                self.overlays.options_menu_draft.live_updates_enabled =
                    !self.overlays.options_menu_draft.live_updates_enabled;
            }
            OptionsMenuItem::ContextExpansion => {
                self.overlays.options_menu_draft.context_expansion = if delta < 0 {
                    previous_context_expansion(self.overlays.options_menu_draft.context_expansion)
                } else {
                    next_context_expansion(self.overlays.options_menu_draft.context_expansion)
                };
            }
            OptionsMenuItem::SyntaxHighlighting => {
                self.overlays.options_menu_draft.syntax_enabled =
                    !self.overlays.options_menu_draft.syntax_enabled;
            }
            OptionsMenuItem::LineWrapping => {
                self.overlays.options_menu_draft.line_wrapping =
                    !self.overlays.options_menu_draft.line_wrapping;
            }
            OptionsMenuItem::ColorScheme => {
                let choices = COLOR_SCHEME_CHOICES;
                let current = choices
                    .iter()
                    .position(|choice| *choice == self.overlays.options_menu_draft.color_scheme)
                    .unwrap_or_default();
                let next = (current as isize + delta).rem_euclid(choices.len() as isize) as usize;
                self.overlays.options_menu_draft.color_scheme = choices[next];
            }
            OptionsMenuItem::NotificationMode => {
                self.overlays.options_menu_draft.notification_mode =
                    next_notification_mode(self.overlays.options_menu_draft.notification_mode);
            }
            OptionsMenuItem::ToastCorner => {
                self.overlays.options_menu_draft.toast_corner =
                    next_toast_corner(self.overlays.options_menu_draft.toast_corner, delta);
            }
            OptionsMenuItem::ToastTimeout => {
                self.overlays.options_menu_draft.toast_timeout_ms =
                    next_toast_timeout_ms(self.overlays.options_menu_draft.toast_timeout_ms, delta);
            }
            OptionsMenuItem::ToastMaxVisible => {
                self.overlays.options_menu_draft.toast_max_visible = next_toast_max_visible(
                    self.overlays.options_menu_draft.toast_max_visible,
                    delta,
                );
            }
        }

        self.apply_options_menu_draft(changed_item);
    }

    fn apply_options_menu_draft(&mut self, changed_item: OptionsMenuItem) {
        let draft = self.overlays.options_menu_draft;
        let live_reload_reenabled = draft.live_updates_enabled && !self.jobs.live_updates.enabled();
        let notification_settings = NotificationSettings::new(
            draft.notification_mode,
            draft.toast_corner,
            draft.toast_timeout_ms,
            draft.toast_max_visible,
        );

        if draft.layout != layout_setting_from_override(self.viewport.layout_override) {
            self.set_layout_setting(draft.layout);
        }
        if draft.live_updates_enabled != self.jobs.live_updates.enabled() {
            self.jobs
                .live_updates
                .set_user_enabled(draft.live_updates_enabled);
            self.jobs.live_diff_failed_options = None;
            self.runtime.dirty = true;
        }
        if draft.context_expansion != self.config.theme.diff.context_expansion {
            self.config.theme.diff.context_expansion = draft.context_expansion;
            self.runtime.dirty = true;
        }
        if draft.color_scheme != self.config.color_scheme {
            self.apply_color_scheme(draft.color_scheme);
        }
        if draft.syntax_enabled != self.config.syntax.is_some() {
            self.set_syntax_enabled(draft.syntax_enabled);
        }
        if draft.line_wrapping != self.viewport.line_wrapping {
            let next_scroll = if draft.line_wrapping {
                self.wrapped_visual_scroll_for_model_row(self.viewport.scroll)
            } else {
                self.model_row_at_scroll(self.viewport.scroll)
                    .map(|(row, _)| row)
                    .unwrap_or_default()
            };
            self.viewport.line_wrapping = draft.line_wrapping;
            self.set_scroll(next_scroll);
            self.set_horizontal_scroll(self.viewport.horizontal_scroll);
            self.runtime.dirty = true;
        }
        if notification_settings != self.config.syntax_settings.notifications {
            self.config.syntax_settings.notifications = notification_settings;
            self.notifications.toasts.configure(notification_settings);
            self.runtime.dirty = true;
        }
        self.persist_options_menu_draft(changed_item);

        if live_reload_reenabled {
            self.invalidate_diff_cache();
            self.start_uncached_diff_load(self.document.options.clone(), "reload failed");
        } else {
            self.runtime.dirty = true;
        }
        self.clamp_options_menu_selection_to_filtered_items();
    }

    fn persist_options_menu_draft(&mut self, changed_item: OptionsMenuItem) {
        self.queue_effect(AppEffect::PersistOptionsMenuDraft {
            draft: self.overlays.options_menu_draft,
            changed_item,
        });
    }

    pub(crate) fn set_syntax_enabled(&mut self, enabled: bool) {
        if enabled == self.config.syntax.is_some() {
            self.runtime.dirty = true;
            return;
        }

        if !enabled {
            self.config.syntax = None;
            self.overlays.options_menu_draft.syntax_enabled = false;
            self.runtime.dirty = true;
            return;
        }

        match self.start_syntax_runtime() {
            Ok(Some(mut syntax)) => {
                syntax.clear(self.document.generation);
                self.config.syntax = Some(syntax);
                self.overlays.options_menu_draft.syntax_enabled = true;
                self.runtime.dirty = true;
            }
            Ok(None) => {
                self.overlays.options_menu_draft.syntax_enabled = false;
                self.set_error_log("syntax highlighting unavailable: no languages enabled");
            }
            Err(error) => {
                self.overlays.options_menu_draft.syntax_enabled = false;
                self.set_error_log(format!("syntax highlighting unavailable: {error}"));
            }
        }
    }

    fn start_syntax_runtime(&self) -> MarkResult<Option<SyntaxRuntime>> {
        match &self.config.syntax_startup_mode {
            SyntaxStartupMode::Config | SyntaxStartupMode::Disabled => {
                SyntaxRuntime::start(&self.config.syntax_settings)
            }
            SyntaxStartupMode::Languages(languages) => Ok(SyntaxRuntime::start_with_languages(
                languages.clone(),
                self.config.syntax_limits,
            )),
        }
    }
}
