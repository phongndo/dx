use super::*;

impl DiffApp {
    pub(crate) fn toggle_diff_menu(&mut self) {
        if self.overlays.diff_menu_open {
            self.close_diff_menu();
        } else {
            self.open_diff_menu();
        }
    }

    pub(crate) fn open_diff_menu(&mut self) {
        let choices = self.diff_menu_choices();
        if choices.is_empty() {
            return;
        }
        self.overlays.diff_menu.reset();
        self.overlays.diff_menu_open = true;
        self.overlays.options_menu_open = false;
        self.close_color_scheme_picker();
        self.refs.branch_menu_open = None;
        self.overlays.rendered_branch_menu_area = None;
        self.close_review_input();
        self.close_commit_menu();
        self.runtime.dirty = true;
    }

    pub(crate) fn close_diff_menu(&mut self) {
        if self.overlays.diff_menu_open
            || !self.overlays.diff_menu.input.is_empty()
            || self.overlays.rendered_diff_menu_area.is_some()
        {
            self.overlays.diff_menu_open = false;
            self.overlays.diff_menu.reset_input();
            self.overlays.rendered_diff_menu_area = None;
            self.runtime.dirty = true;
        }
    }

    pub(crate) fn open_review_input(&mut self) {
        self.overlays.review_input.clear();
        self.overlays.review_input_cursor = 0;
        self.overlays.review_input_open = true;
        self.overlays.diff_menu_open = false;
        self.overlays.diff_menu.reset_input();
        self.overlays.rendered_diff_menu_area = None;
        self.overlays.options_menu_open = false;
        self.close_color_scheme_picker();
        self.refs.branch_menu_open = None;
        self.overlays.rendered_branch_menu_area = None;
        self.close_commit_menu();
        self.runtime.dirty = true;
    }

    pub(crate) fn close_review_input(&mut self) {
        if self.overlays.review_input_open
            || !self.overlays.review_input.is_empty()
            || self.overlays.rendered_review_input_area.is_some()
        {
            self.overlays.review_input_open = false;
            self.overlays.review_input.clear();
            self.overlays.review_input_cursor = 0;
            self.overlays.rendered_review_input_area = None;
            self.runtime.dirty = true;
        }
    }

    pub(crate) fn open_options_menu(&mut self) {
        self.overlays.options_menu_draft = OptionsDraft {
            layout: layout_setting_from_override(self.viewport.layout_override),
            live_updates_enabled: self.jobs.live_updates_enabled,
            context_expansion: self.config.theme.diff.context_expansion,
            syntax_enabled: self.config.syntax.is_some(),
            line_wrapping: self.viewport.line_wrapping,
            color_scheme: self.config.color_scheme,
            notification_mode: self.config.syntax_settings.notifications.mode,
            toast_corner: self.config.syntax_settings.notifications.corner,
            toast_timeout_ms: self.config.syntax_settings.notifications.timeout_ms,
            toast_max_visible: self.config.syntax_settings.notifications.max_visible,
        };
        let len = self.options_menu_items().len();
        self.overlays
            .options_menu
            .set_selected(self.overlays.options_menu.selected, len);
        self.overlays.options_menu.reset_input_and_scroll();
        self.overlays.options_menu_open = true;
        self.close_color_scheme_picker();
        self.overlays.diff_menu_open = false;
        self.overlays.diff_menu.reset_input();
        self.overlays.rendered_diff_menu_area = None;
        self.close_review_input();
        self.refs.branch_menu_open = None;
        self.overlays.rendered_branch_menu_area = None;
        self.close_commit_menu();
        self.runtime.dirty = true;
    }

    pub(crate) fn close_options_menu(&mut self) {
        if self.overlays.options_menu_open
            || !self.overlays.options_menu.input.is_empty()
            || self.overlays.options_menu.scroll != 0
        {
            self.overlays.options_menu_open = false;
            self.overlays.options_menu.reset();
            self.close_color_scheme_picker();
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

        self.overlays.options_menu.move_wrapping(len, delta);
        self.runtime.dirty = true;
    }

    pub(crate) fn set_options_menu_selection(&mut self, selected: usize) {
        if self
            .overlays
            .options_menu
            .set_selected(selected, self.filtered_options_menu_items().len())
        {
            self.runtime.dirty = true;
        }
    }

    pub(crate) fn ensure_options_menu_selection_visible(&mut self, visible_rows: usize) {
        let len = self.filtered_options_menu_items().len();
        self.overlays
            .options_menu
            .ensure_selected_visible(len, visible_rows);
    }

    pub(super) fn clamp_options_menu_selection_to_filtered_items(&mut self) {
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
            OptionsMenuItem::LiveReload if !self.jobs.live_updates_allowed => {
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
            OptionsMenuItem::LiveReload if !self.jobs.live_updates_allowed => {
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

    #[allow(dead_code)]
    pub(crate) fn push_options_menu_input(&mut self, character: char) {
        self.overlays.options_menu.push_input(character);
        self.runtime.dirty = true;
    }

    #[allow(dead_code)]
    pub(crate) fn pop_options_menu_input(&mut self) {
        if matches!(
            self.overlays.options_menu.pop_input(),
            TextInputKeyResult::Edited
        ) {
            self.runtime.dirty = true;
        }
    }

    #[allow(dead_code)]
    pub(crate) fn clear_options_menu_input(&mut self) {
        if self.overlays.options_menu.clear_input_and_selection() {
            self.runtime.dirty = true;
        }
    }

    pub(super) fn apply_options_menu_input_key(&mut self, key: KeyEvent) -> bool {
        match self.overlays.options_menu.apply_input_key(key) {
            TextInputKeyResult::Edited => {
                self.runtime.dirty = true;
                true
            }
            TextInputKeyResult::Moved => {
                self.runtime.dirty = true;
                true
            }
            TextInputKeyResult::Handled => true,
            TextInputKeyResult::Ignored => false,
        }
    }

    pub(crate) fn activate_selected_option(&mut self) {
        match self.highlighted_option() {
            Some(OptionsMenuItem::ColorScheme) => self.open_color_scheme_picker(),
            Some(_) => self.cycle_selected_option(1),
            None => {}
        }
    }

    pub(crate) fn open_color_scheme_picker(&mut self) {
        self.overlays.color_scheme_picker_open = true;
        self.overlays.color_scheme_preview_original =
            Some((self.config.color_scheme, self.config.theme));
        self.overlays.color_scheme_picker.reset();
        self.ensure_color_scheme_selection_visible();
        self.runtime.dirty = true;
    }

    pub(crate) fn close_color_scheme_picker(&mut self) {
        if self.overlays.color_scheme_picker_open {
            if let Some((color_scheme, theme)) = self.overlays.color_scheme_preview_original.take()
            {
                self.config.color_scheme = color_scheme;
                self.config.theme = theme;
            }
            self.overlays.color_scheme_picker_open = false;
            self.overlays.color_scheme_picker.reset_input_and_scroll();
            self.overlays.rendered_color_scheme_picker_area = None;
            self.runtime.dirty = true;
        }
    }

    pub(crate) fn selectable_color_schemes(&self) -> Vec<ColorSchemeChoice> {
        COLOR_SCHEME_CHOICES
            .iter()
            .copied()
            .filter(|choice| *choice != self.overlays.options_menu_draft.color_scheme)
            .collect()
    }

    pub(crate) fn filtered_color_schemes(&self) -> Vec<ColorSchemeChoice> {
        let choices = self.selectable_color_schemes();
        let query = self
            .overlays
            .color_scheme_picker
            .input
            .trim()
            .to_ascii_lowercase();
        if query.is_empty() {
            return choices;
        }

        let mut matches: Vec<_> = choices
            .iter()
            .enumerate()
            .filter_map(|(index, choice)| {
                let label = color_scheme_label(*choice);
                branch_match_score(&query, label).map(|score| (score, label.len(), index, *choice))
            })
            .collect();
        matches.sort_by(|left, right| {
            left.0
                .cmp(&right.0)
                .then_with(|| left.1.cmp(&right.1))
                .then_with(|| left.2.cmp(&right.2))
        });
        matches
            .into_iter()
            .map(|(_, _, _, choice)| choice)
            .collect()
    }

    pub(super) fn color_scheme_picker_rows(&self) -> usize {
        color_scheme_picker_list_visible_rows(self, self.viewport.terminal_area)
            .unwrap_or(MAX_COLOR_SCHEME_MENU_ROWS)
            .max(1)
    }

    pub(crate) fn ensure_color_scheme_selection_visible(&mut self) {
        let len = self.filtered_color_schemes().len();
        let visible_rows = self.color_scheme_picker_rows();
        self.overlays
            .color_scheme_picker
            .ensure_selected_visible(len, visible_rows);
    }

    pub(crate) fn set_color_scheme_selection(&mut self, selected: usize) {
        self.overlays
            .color_scheme_picker
            .set_selected(selected, self.filtered_color_schemes().len());
        self.ensure_color_scheme_selection_visible();
        self.preview_highlighted_color_scheme();
        self.runtime.dirty = true;
    }

    pub(crate) fn move_color_scheme_selection(&mut self, delta: isize) {
        let len = self.filtered_color_schemes().len();
        if len == 0 {
            return;
        }
        self.overlays.color_scheme_picker.move_wrapping(len, delta);
        self.ensure_color_scheme_selection_visible();
        self.preview_highlighted_color_scheme();
        self.runtime.dirty = true;
    }

    #[allow(dead_code)]
    pub(crate) fn push_color_scheme_input(&mut self, character: char) {
        self.overlays.color_scheme_picker.push_input(character);
        self.preview_highlighted_color_scheme();
        self.runtime.dirty = true;
    }

    #[allow(dead_code)]
    pub(crate) fn pop_color_scheme_input(&mut self) {
        if matches!(
            self.overlays.color_scheme_picker.pop_input(),
            TextInputKeyResult::Edited
        ) {
            self.preview_highlighted_color_scheme();
            self.runtime.dirty = true;
        }
    }

    #[allow(dead_code)]
    pub(crate) fn clear_color_scheme_input(&mut self) {
        if self
            .overlays
            .color_scheme_picker
            .clear_input_and_selection()
        {
            self.preview_highlighted_color_scheme();
            self.runtime.dirty = true;
        }
    }

    pub(super) fn apply_color_scheme_input_key(&mut self, key: KeyEvent) -> bool {
        match self.overlays.color_scheme_picker.apply_input_key(key) {
            TextInputKeyResult::Edited => {
                self.preview_highlighted_color_scheme();
                self.runtime.dirty = true;
                true
            }
            TextInputKeyResult::Moved => {
                self.runtime.dirty = true;
                true
            }
            TextInputKeyResult::Handled => true,
            TextInputKeyResult::Ignored => false,
        }
    }

    pub(crate) fn preview_highlighted_color_scheme(&mut self) {
        let Some(choice) = self
            .filtered_color_schemes()
            .get(self.overlays.color_scheme_picker.selected)
            .copied()
        else {
            return;
        };

        self.apply_color_scheme(choice);
    }

    pub(crate) fn select_highlighted_color_scheme(&mut self) {
        let Some(choice) = self
            .filtered_color_schemes()
            .get(self.overlays.color_scheme_picker.selected)
            .copied()
        else {
            self.runtime.dirty = true;
            return;
        };

        self.overlays.options_menu_draft.color_scheme = choice;
        self.overlays.color_scheme_picker_open = false;
        self.overlays.color_scheme_preview_original = None;
        self.overlays.color_scheme_picker.reset_input_and_scroll();
        self.overlays.rendered_color_scheme_picker_area = None;
        self.apply_options_menu_draft(OptionsMenuItem::ColorScheme);
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
                if !self.jobs.live_updates_allowed {
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
        let live_reload_reenabled = draft.live_updates_enabled && !self.jobs.live_updates_enabled;
        let notification_settings = NotificationSettings {
            mode: draft.notification_mode,
            corner: draft.toast_corner,
            timeout_ms: draft.toast_timeout_ms,
            max_visible: draft.toast_max_visible,
        };

        if draft.layout != layout_setting_from_override(self.viewport.layout_override) {
            self.set_layout_setting(draft.layout);
        }
        if draft.live_updates_enabled != self.jobs.live_updates_enabled {
            self.jobs.live_updates_enabled = draft.live_updates_enabled;
            self.jobs.live_reload_invalidated = false;
            self.jobs.live_reload_pending = false;
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
        let draft = self.overlays.options_menu_draft;
        #[cfg(test)]
        {
            self.config.last_persisted_options_menu_draft = Some((draft, changed_item));
        }

        if !self.config.settings_persistence_enabled {
            return;
        }

        let result = mark_syntax::settings_write_path()
            .and_then(|path| persist_options_menu_draft_to_path(&path, draft, changed_item));
        if let Err(error) = result {
            self.set_error_log(format!("settings not saved: {error}"));
        }
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

    pub(crate) fn apply_color_scheme(&mut self, color_scheme: ColorSchemeChoice) {
        let Some(config) = color_scheme_config(color_scheme) else {
            self.set_error_log("colorscheme custom cannot be reapplied from options");
            return;
        };
        let diff = self.config.theme.diff;
        match diff_theme_from_config(&config).and_then(|theme| {
            theme
                .with_color_overrides(&self.config.theme_color_overrides)
                .map(|theme| {
                    theme.with_transparent_background(self.config.theme_transparent_background)
                })
        }) {
            Ok(theme) => {
                self.config.theme = theme.with_diff_settings(diff);
                self.config.color_scheme = color_scheme;
                self.runtime.dirty = true;
            }
            Err(error) => {
                self.set_error_log(format!("colorscheme ignored: {error}"));
            }
        }
    }

    pub(crate) fn close_branch_menu(&mut self) {
        if self.refs.branch_menu_open.is_some()
            || !self.refs.branch_menu.input.is_empty()
            || self.refs.branch_menu.scroll != 0
            || self.overlays.rendered_branch_menu_area.is_some()
        {
            self.refs.branch_menu_open = None;
            self.refs.branch_menu.reset();
            self.overlays.rendered_branch_menu_area = None;
            self.runtime.dirty = true;
        }
    }

    pub(crate) fn close_commit_menu(&mut self) {
        if self.refs.commit_menu_open
            || !self.refs.commit_menu.input.is_empty()
            || self.refs.commit_menu.scroll != 0
            || self.overlays.rendered_commit_menu_area.is_some()
        {
            self.refs.commit_menu_open = false;
            self.refs.commit_menu.reset();
            self.overlays.rendered_commit_menu_area = None;
            self.runtime.dirty = true;
        }
    }

    pub(crate) fn toggle_commit_menu(&mut self) {
        if self.refs.comparison_commits.is_empty() {
            self.set_warning_notice("commit list unavailable");
            return;
        }
        if self.refs.commit_menu_open {
            self.close_commit_menu();
            return;
        }

        self.refs.commit_menu_open = true;
        self.overlays.diff_menu_open = false;
        self.overlays.diff_menu.reset_input();
        self.overlays.rendered_diff_menu_area = None;
        self.close_review_input();
        self.refs.branch_menu_open = None;
        self.refs.branch_menu.reset_input();
        self.overlays.rendered_branch_menu_area = None;
        self.overlays.options_menu_open = false;
        self.close_color_scheme_picker();
        self.refs.commit_menu.reset_input();
        self.refs.commit_menu.selected = self
            .selected_commit_menu_choice()
            .and_then(|commit| {
                self.filtered_commits()
                    .iter()
                    .position(|candidate| candidate.sha == commit.sha)
            })
            .unwrap_or_default()
            .min(self.max_commit_menu_selection());
        self.ensure_commit_selection_visible();
        self.runtime.dirty = true;
    }

    pub(crate) fn toggle_branch_menu(&mut self, menu: BranchMenu) {
        if self.refs.comparison_branches.is_empty() {
            return;
        }
        if self.refs.branch_menu_open == Some(menu) {
            self.close_branch_menu();
            return;
        }

        self.refs.branch_menu_open = Some(menu);
        self.overlays.diff_menu_open = false;
        self.overlays.diff_menu.reset_input();
        self.overlays.rendered_diff_menu_area = None;
        self.close_review_input();
        self.overlays.options_menu_open = false;
        self.close_color_scheme_picker();
        self.close_commit_menu();
        self.refs.branch_menu.reset_input();
        self.refs.branch_menu.selected = self
            .branch_ref(menu)
            .and_then(|branch| {
                self.filtered_branches()
                    .iter()
                    .position(|candidate| *candidate == branch)
            })
            .unwrap_or_default()
            .min(self.max_branch_menu_selection());
        self.ensure_branch_selection_visible();
        self.runtime.dirty = true;
    }

    pub(crate) fn branch_selector_at(&self, column: u16) -> Option<BranchMenu> {
        [BranchMenu::Head, BranchMenu::Base]
            .into_iter()
            .find(|menu| {
                let Some(start) = self.branch_selector_start(*menu) else {
                    return false;
                };
                let Some(width) = self.branch_selector_width(*menu) else {
                    return false;
                };
                column >= start && column < start.saturating_add(width)
            })
    }

    pub(crate) fn is_rendered_branch_menu_position(&self, column: u16, row: u16) -> bool {
        self.overlays
            .rendered_branch_menu_area
            .is_some_and(|area| rect_contains(area, column, row))
    }

    pub(crate) fn branch_choice_at(
        &self,
        menu: BranchMenu,
        column: u16,
        row: u16,
    ) -> Option<String> {
        if self.refs.branch_menu_open != Some(menu) {
            return None;
        }

        let menu_area = self.overlays.rendered_branch_menu_area?;
        let inner = branch_menu_block(self.config.theme, menu).inner(menu_area);
        if column < inner.x
            || column >= inner.x.saturating_add(inner.width)
            || row < inner.y.saturating_add(2)
            || row >= inner.y.saturating_add(inner.height)
        {
            return None;
        }

        let row_index = usize::from(row.saturating_sub(inner.y).saturating_sub(2));
        let pinned_rows = usize::from(self.selected_branch_menu_choice(menu).is_some());
        if row_index < pinned_rows {
            return None;
        }

        let branch_index = row_index.saturating_sub(pinned_rows);
        let rendered_choices = inner.height.saturating_sub(2 + pinned_rows as u16) as usize;
        if branch_index >= rendered_choices {
            return None;
        }

        self.filtered_branch(branch_index).map(str::to_owned)
    }

    pub(crate) fn filtered_branch(&self, row_index: usize) -> Option<&str> {
        self.filtered_branches()
            .get(self.refs.branch_menu.scroll.saturating_add(row_index))
            .copied()
    }

    pub(crate) fn move_branch_selection(&mut self, delta: isize) {
        let len = self.filtered_branches().len();
        if self.refs.branch_menu.move_saturating(len, delta) {
            self.ensure_branch_selection_visible();
            self.runtime.dirty = true;
        }
    }

    pub(crate) fn set_branch_selection(&mut self, selected: usize) {
        if self
            .refs
            .branch_menu
            .set_selected(selected, self.filtered_branches().len())
        {
            self.ensure_branch_selection_visible();
            self.runtime.dirty = true;
        }
    }

    pub(crate) fn cycle_branch_completion(&mut self, delta: isize) {
        let len = self.filtered_branches().len();
        if len == 0 {
            return;
        }

        self.refs.branch_menu.move_wrapping(len, delta);
        self.ensure_branch_selection_visible();
        self.runtime.dirty = true;
    }

    pub(crate) fn ensure_branch_selection_visible(&mut self) {
        self.ensure_branch_selection_visible_for_rows(self.branch_menu_rows());
    }

    pub(super) fn branch_menu_rows(&self) -> usize {
        branch_menu_list_visible_rows(self, self.viewport.terminal_area)
            .unwrap_or(MAX_BRANCH_MENU_ROWS)
            .max(1)
    }

    pub(crate) fn ensure_branch_selection_visible_for_rows(&mut self, visible_rows: usize) {
        let len = self.filtered_branches().len();
        self.refs
            .branch_menu
            .ensure_selected_visible(len, visible_rows);
    }

    pub(crate) fn max_branch_menu_selection(&self) -> usize {
        self.filtered_branches().len().saturating_sub(1)
    }

    pub(crate) fn max_branch_menu_scroll(&self) -> usize {
        self.max_branch_menu_scroll_for_rows(self.branch_menu_rows())
    }

    pub(crate) fn max_branch_menu_scroll_for_rows(&self, visible_rows: usize) -> usize {
        self.filtered_branches()
            .len()
            .saturating_sub(visible_rows.max(1))
    }

    pub(crate) fn is_show_diff(&self) -> bool {
        matches!(&self.document.options.source, DiffSource::Show(_))
    }

    pub(crate) fn show_rev_menu_detail(&self) -> String {
        let rev = self
            .refs
            .show_rev
            .as_deref()
            .or(match &self.document.options.source {
                DiffSource::Show(rev) => Some(rev.as_str()),
                _ => None,
            });
        match rev {
            None | Some("HEAD") => self
                .refs
                .current_head
                .clone()
                .or_else(|| current_head_label(&self.document.changeset.repo))
                .unwrap_or_else(|| "HEAD".to_owned()),
            Some(symbolic) => rev_display_label(symbolic).to_owned(),
        }
    }

    pub(crate) fn commit_menu_width(&self) -> u16 {
        let commit_width = commit_menu_width(&self.refs.comparison_commits) as usize;
        let input_width = self.refs.commit_menu.input.width().saturating_add(4);
        commit_width.max(input_width).max(36).saturating_add(4) as u16
    }

    pub(crate) fn max_commit_menu_selection(&self) -> usize {
        self.filtered_commits().len().saturating_sub(1)
    }

    pub(crate) fn max_commit_menu_scroll_for_rows(&self, visible_rows: usize) -> usize {
        self.filtered_commits()
            .len()
            .saturating_sub(visible_rows.max(1))
    }

    pub(crate) fn ensure_commit_selection_visible(&mut self) {
        self.ensure_commit_selection_visible_for_rows(self.commit_menu_rows());
    }

    pub(super) fn commit_menu_rows(&self) -> usize {
        commit_menu_list_visible_rows(self, self.viewport.terminal_area)
            .unwrap_or(MAX_BRANCH_MENU_ROWS)
            .max(1)
    }

    pub(crate) fn ensure_commit_selection_visible_for_rows(&mut self, visible_rows: usize) {
        let len = self.filtered_commits().len();
        self.refs
            .commit_menu
            .ensure_selected_visible(len, visible_rows);
    }

    pub(crate) fn move_commit_selection(&mut self, delta: isize) {
        let len = self.filtered_commits().len();
        if self.refs.commit_menu.move_saturating(len, delta) {
            self.ensure_commit_selection_visible();
            self.runtime.dirty = true;
        }
    }

    pub(crate) fn set_commit_selection(&mut self, selected: usize) {
        if self
            .refs
            .commit_menu
            .set_selected(selected, self.filtered_commits().len())
        {
            self.ensure_commit_selection_visible();
            self.runtime.dirty = true;
        }
    }

    pub(crate) fn cycle_commit_completion(&mut self, delta: isize) {
        let len = self.filtered_commits().len();
        if len == 0 {
            return;
        }

        self.refs.commit_menu.move_wrapping(len, delta);
        self.ensure_commit_selection_visible();
        self.runtime.dirty = true;
    }

    #[allow(dead_code)]
    pub(crate) fn push_commit_input(&mut self, character: char) {
        self.refs.commit_menu.push_input(character);
        self.runtime.dirty = true;
    }

    #[allow(dead_code)]
    pub(crate) fn pop_commit_input(&mut self) {
        if matches!(
            self.refs.commit_menu.pop_input(),
            TextInputKeyResult::Edited
        ) {
            self.runtime.dirty = true;
        }
    }

    #[allow(dead_code)]
    pub(crate) fn clear_commit_input(&mut self) {
        if self.refs.commit_menu.clear_input_and_selection() {
            self.runtime.dirty = true;
        }
    }

    pub(super) fn apply_commit_input_key(&mut self, key: KeyEvent) -> bool {
        match self.refs.commit_menu.apply_input_key(key) {
            TextInputKeyResult::Edited => {
                self.runtime.dirty = true;
                true
            }
            TextInputKeyResult::Moved => {
                self.runtime.dirty = true;
                true
            }
            TextInputKeyResult::Handled => true,
            TextInputKeyResult::Ignored => false,
        }
    }

    pub(crate) fn selected_commit_menu_choice(&self) -> Option<&GitCommit> {
        let rev = self.refs.show_rev.as_deref()?;
        self.refs.comparison_commits.iter().find(|commit| {
            commit.sha == rev
                || commit.sha.starts_with(rev)
                || rev.starts_with(&commit.sha[..commit.sha.len().min(7)])
        })
    }

    pub(crate) fn selectable_commit_count(&self) -> usize {
        let selected = self.selected_commit_menu_choice();
        self.refs
            .comparison_commits
            .iter()
            .filter(|commit| selected != Some(commit))
            .count()
    }

    pub(crate) fn filtered_commits(&self) -> Vec<&GitCommit> {
        let query = self.refs.commit_menu.input.trim().to_ascii_lowercase();
        let selected = self.selected_commit_menu_choice();
        if query.is_empty() {
            return self
                .refs
                .comparison_commits
                .iter()
                .filter(|commit| selected != Some(commit))
                .collect();
        }

        let mut matches: Vec<_> = self
            .refs
            .comparison_commits
            .iter()
            .enumerate()
            .filter(|(_, commit)| selected != Some(commit))
            .filter_map(|(index, commit)| {
                commit_match_score(&query, commit).map(|score| (score, index, commit))
            })
            .collect();
        matches.sort_by(|left, right| {
            left.0
                .cmp(&right.0)
                .then_with(|| left.1.cmp(&right.1))
                .then_with(|| left.2.sha.cmp(&right.2.sha))
        });
        matches.into_iter().map(|(_, _, commit)| commit).collect()
    }

    pub(crate) fn filtered_commit(&self, row_index: usize) -> Option<&GitCommit> {
        self.filtered_commits()
            .get(self.refs.commit_menu.scroll.saturating_add(row_index))
            .copied()
    }

    pub(crate) fn select_highlighted_commit_match(&mut self) {
        let Some(commit) = self
            .filtered_commits()
            .get(self.refs.commit_menu.selected)
            .map(|commit| (*commit).clone())
        else {
            self.set_warning_notice("no matching commit");
            return;
        };
        self.close_commit_menu();
        self.select_show_commit(commit.sha);
    }

    pub(crate) fn select_show_commit(&mut self, rev: String) {
        let mut options = self.document.options.clone();
        options.source = DiffSource::Show(rev.clone());
        options.scope = DiffScope::All;

        if options == self.document.options {
            self.refs.show_rev = Some(rev);
            self.runtime.dirty = true;
            return;
        }

        self.refs.show_rev = Some(rev);
        self.start_diff_load(options, "show unavailable");
    }

    pub(crate) fn commit_selector_text(&self) -> Option<String> {
        let rev = self.refs.show_rev.as_deref()?;
        let label = self
            .refs
            .comparison_commits
            .iter()
            .find(|commit| commit.sha == rev || commit.sha.starts_with(rev))
            .map(|commit| {
                let short = commit_short_sha(commit);
                if commit.subject.is_empty() {
                    short.to_owned()
                } else {
                    format!("{short} · {}", commit.subject)
                }
            })
            .unwrap_or_else(|| rev.to_owned());
        Some(format!("{label} ▾"))
    }

    pub(crate) fn commit_selector_width(&self) -> Option<u16> {
        self.commit_selector_text().map(|text| text.width() as u16)
    }

    pub(crate) fn commit_selector_start(&self) -> Option<u16> {
        if !self.is_show_diff() {
            return None;
        }
        let selector_gap = STATUSLINE_SELECTOR_GAP.width() as u16;
        Some(diff_selector_width(&self.document.options).saturating_add(selector_gap))
    }

    pub(crate) fn commit_selector_at(&self, column: u16) -> bool {
        let Some(start) = self.commit_selector_start() else {
            return false;
        };
        let Some(width) = self.commit_selector_width() else {
            return false;
        };
        column >= start && column < start.saturating_add(width)
    }

    pub(crate) fn is_rendered_commit_menu_position(&self, column: u16, row: u16) -> bool {
        self.overlays
            .rendered_commit_menu_area
            .is_some_and(|area| rect_contains(area, column, row))
    }

    pub(crate) fn is_rendered_review_input_position(&self, column: u16, row: u16) -> bool {
        self.overlays
            .rendered_review_input_area
            .is_some_and(|area| rect_contains(area, column, row))
    }

    pub(crate) fn commit_choice_at(&self, column: u16, row: u16) -> Option<String> {
        if !self.refs.commit_menu_open {
            return None;
        }

        let menu_area = self.overlays.rendered_commit_menu_area?;
        let inner = commit_menu_block(self.config.theme).inner(menu_area);
        if column < inner.x
            || column >= inner.x.saturating_add(inner.width)
            || row < inner.y.saturating_add(2)
            || row >= inner.y.saturating_add(inner.height)
        {
            return None;
        }

        let row_index = usize::from(row.saturating_sub(inner.y).saturating_sub(2));
        let pinned_rows = usize::from(self.selected_commit_menu_choice().is_some());
        if row_index < pinned_rows {
            return None;
        }

        let commit_index = row_index.saturating_sub(pinned_rows);
        let rendered_choices = inner.height.saturating_sub(2 + pinned_rows as u16) as usize;
        if commit_index >= rendered_choices {
            return None;
        }

        self.filtered_commit(commit_index)
            .map(|commit| commit.sha.clone())
    }

    pub(crate) fn filtered_branches(&self) -> Vec<&str> {
        let menu = self.refs.branch_menu_open.unwrap_or(BranchMenu::Base);
        let query = self.refs.branch_menu.input.trim().to_ascii_lowercase();
        let selected = self.selected_branch_menu_choice(menu);
        if query.is_empty() {
            let mut matches: Vec<_> = self
                .refs
                .comparison_branches
                .iter()
                .enumerate()
                .filter(|(_, branch)| selected != Some(branch.as_str()))
                .map(|(index, branch)| (self.branch_pin_rank(menu, branch), index, branch.as_str()))
                .collect();
            matches.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
            return matches.into_iter().map(|(_, _, branch)| branch).collect();
        }

        let mut matches: Vec<_> = self
            .refs
            .comparison_branches
            .iter()
            .enumerate()
            .filter(|(_, branch)| selected != Some(branch.as_str()))
            .filter_map(|(index, branch)| {
                branch_match_score(&query, branch).map(|score| {
                    (
                        self.branch_pin_rank(menu, branch),
                        score,
                        branch.len(),
                        index,
                        branch.as_str(),
                    )
                })
            })
            .collect();
        matches.sort_by(|left, right| {
            left.0
                .cmp(&right.0)
                .then_with(|| left.1.cmp(&right.1))
                .then_with(|| left.2.cmp(&right.2))
                .then_with(|| left.3.cmp(&right.3))
                .then_with(|| left.4.cmp(right.4))
        });
        matches
            .into_iter()
            .map(|(_, _, _, _, branch)| branch)
            .collect()
    }

    pub(crate) fn selected_branch_menu_choice(&self, menu: BranchMenu) -> Option<&str> {
        self.branch_ref(menu)
    }

    pub(crate) fn selectable_branch_count(&self, menu: BranchMenu) -> usize {
        let selected = self.selected_branch_menu_choice(menu);
        self.refs
            .comparison_branches
            .iter()
            .filter(|branch| selected != Some(branch.as_str()))
            .count()
    }

    pub(crate) fn branch_pin_rank(&self, menu: BranchMenu, branch: &str) -> usize {
        let current = self.refs.current_head.as_deref();
        let base = self.refs.branch_base.as_deref();
        match menu {
            BranchMenu::Head => {
                if current == Some(branch) {
                    0
                } else if base == Some(branch) {
                    1
                } else {
                    2
                }
            }
            BranchMenu::Base => {
                if base == Some(branch) {
                    0
                } else if current == Some(branch) {
                    1
                } else {
                    2
                }
            }
        }
    }

    #[allow(dead_code)]
    pub(crate) fn push_branch_input(&mut self, character: char) {
        self.refs.branch_menu.push_input(character);
        self.runtime.dirty = true;
    }

    #[allow(dead_code)]
    pub(crate) fn pop_branch_input(&mut self) {
        if matches!(
            self.refs.branch_menu.pop_input(),
            TextInputKeyResult::Edited
        ) {
            self.runtime.dirty = true;
        }
    }

    #[allow(dead_code)]
    pub(crate) fn clear_branch_input(&mut self) {
        if self.refs.branch_menu.clear_input_and_selection() {
            self.runtime.dirty = true;
        }
    }

    pub(super) fn apply_branch_input_key(&mut self, key: KeyEvent) -> bool {
        match self.refs.branch_menu.apply_input_key(key) {
            TextInputKeyResult::Edited => {
                self.runtime.dirty = true;
                true
            }
            TextInputKeyResult::Moved => {
                self.runtime.dirty = true;
                true
            }
            TextInputKeyResult::Handled => true,
            TextInputKeyResult::Ignored => false,
        }
    }

    pub(crate) fn select_highlighted_branch_match(&mut self) {
        let Some(menu) = self.refs.branch_menu_open else {
            return;
        };
        let Some(branch) = self
            .filtered_branches()
            .get(self.refs.branch_menu.selected)
            .map(|branch| (*branch).to_owned())
        else {
            self.set_warning_notice("no matching branch");
            return;
        };
        self.close_branch_menu();
        self.select_branch(menu, branch);
    }

    pub(crate) fn is_branch_diff(&self) -> bool {
        matches!(
            &self.document.options.source,
            DiffSource::Base(_) | DiffSource::Branch { .. }
        )
    }

    pub(crate) fn branch_ref(&self, menu: BranchMenu) -> Option<&str> {
        match menu {
            BranchMenu::Head => self.refs.branch_head.as_deref(),
            BranchMenu::Base => self.refs.branch_base.as_deref(),
        }
    }

    pub(crate) fn branch_selector_text(&self, menu: BranchMenu) -> Option<String> {
        let branch = self.branch_ref(menu)?;
        let label = self.branch_label(menu, branch);
        Some(format!("{label} ▾"))
    }

    pub(crate) fn branch_label(&self, menu: BranchMenu, branch: &str) -> String {
        match self.branch_marker(menu, branch) {
            Some(marker) => format!("{marker} {branch}"),
            None => branch.to_owned(),
        }
    }

    pub(crate) fn branch_marker(&self, menu: BranchMenu, branch: &str) -> Option<&'static str> {
        let current = self.refs.current_head.as_deref();
        let base = self.refs.branch_base.as_deref();
        match menu {
            BranchMenu::Head => {
                if current == Some(branch) {
                    Some(CURRENT_BRANCH_MARKER)
                } else if base == Some(branch) {
                    Some(BASE_BRANCH_MARKER)
                } else {
                    None
                }
            }
            BranchMenu::Base => {
                if base == Some(branch) {
                    Some(BASE_BRANCH_MARKER)
                } else if current == Some(branch) {
                    Some(CURRENT_BRANCH_MARKER)
                } else {
                    None
                }
            }
        }
    }

    pub(crate) fn branch_selector_width(&self, menu: BranchMenu) -> Option<u16> {
        self.branch_selector_text(menu)
            .map(|text| text.width() as u16)
    }

    pub(crate) fn branch_menu_width(&self) -> u16 {
        let branch_width = branch_menu_width(&self.refs.comparison_branches) as usize;
        let input_width = self.refs.branch_menu.input.width().saturating_add(4);
        branch_width.max(input_width).max(36).saturating_add(4) as u16
    }

    pub(crate) fn branch_selector_start(&self, menu: BranchMenu) -> Option<u16> {
        if !self.is_branch_diff() {
            return None;
        }

        let head_width = self.branch_selector_width(BranchMenu::Head)?;
        let selector_gap = STATUSLINE_SELECTOR_GAP.width() as u16;
        let head_start = diff_selector_width(&self.document.options).saturating_add(selector_gap);
        match menu {
            BranchMenu::Head => Some(head_start),
            BranchMenu::Base => Some(
                head_start
                    .saturating_add(head_width)
                    .saturating_add(BRANCH_COMPARISON_SEPARATOR.width() as u16),
            ),
        }
    }
}
