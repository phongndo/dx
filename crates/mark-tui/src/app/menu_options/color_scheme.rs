use super::super::{
    COLOR_SCHEME_CHOICES, ColorSchemeChoice, DiffApp, MAX_COLOR_SCHEME_MENU_ROWS, OptionsMenuItem,
    color_scheme_config, color_scheme_label,
};
use crate::{
    controls::branch_match_score,
    render::menus::color_scheme_picker_list_visible_rows,
    selector::{SelectorController, SelectorMovement},
    theme::diff_theme_from_config,
};
use crossterm::event::KeyEvent;

impl DiffApp {
    pub(crate) fn open_color_scheme_picker(&mut self) {
        self.overlays.color_scheme_picker_open = true;
        self.overlays.color_scheme_preview_original =
            Some((self.config.color_scheme, self.config.theme));
        self.overlays.color_scheme_picker.reset();
        self.ensure_color_scheme_selection_visible();
        self.runtime.dirty = true;
    }

    pub(crate) fn close_color_scheme_picker(&mut self) {
        let (closed, preview_original) = self.overlays.close_color_scheme_picker();
        if !closed {
            return;
        }

        if let Some((color_scheme, theme)) = preview_original {
            self.config.color_scheme = color_scheme;
            self.config.theme = theme;
        }
        self.runtime.hit_map.color_scheme_picker_area = None;
        self.runtime.dirty = true;
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

    pub(crate) fn color_scheme_picker_rows(&self) -> usize {
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
        let len = self.filtered_color_schemes().len();
        let rows = self.color_scheme_picker_rows();
        let _changed = SelectorController::new(&mut self.overlays.color_scheme_picker, len)
            .with_visible_rows(rows)
            .set_selected(selected);
        self.preview_highlighted_color_scheme();
        self.runtime.dirty = true;
    }

    pub(crate) fn move_color_scheme_selection(&mut self, delta: isize) {
        let len = self.filtered_color_schemes().len();
        if len == 0 {
            return;
        }
        let rows = self.color_scheme_picker_rows();
        if SelectorController::new(&mut self.overlays.color_scheme_picker, len)
            .with_visible_rows(rows)
            .move_by(delta, SelectorMovement::Wrapping)
        {
            self.preview_highlighted_color_scheme();
            self.runtime.dirty = true;
        }
    }

    pub(crate) fn apply_color_scheme_input_key(&mut self, key: KeyEvent) -> bool {
        let len = self.filtered_color_schemes().len();
        let rows = self.color_scheme_picker_rows();
        let outcome = SelectorController::new(&mut self.overlays.color_scheme_picker, len)
            .with_visible_rows(rows)
            .apply_input_key(key);
        if outcome.changed {
            self.preview_highlighted_color_scheme();
            self.runtime.dirty = true;
        }
        outcome.handled
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
        self.set_rendered_color_scheme_picker_area(None);
        self.apply_options_menu_draft(OptionsMenuItem::ColorScheme);
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
}
