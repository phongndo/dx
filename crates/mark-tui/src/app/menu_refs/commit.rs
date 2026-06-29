use super::super::{DiffApp, rect_contains};
use crate::controls::{
    GitCommit, commit_match_score, commit_menu_width, commit_short_sha, current_head_label,
    rev_display_label,
};
use crate::render::menus::{commit_menu_block, commit_menu_list_visible_rows, diff_selector_width};
use crate::selector::{SelectorController, SelectorMovement};
use crate::theme::{MAX_BRANCH_MENU_ROWS, STATUSLINE_SELECTOR_GAP};
use crossterm::event::KeyEvent;
use mark_diff::DiffSource;
use unicode_width::UnicodeWidthStr;

impl DiffApp {
    pub(crate) fn close_commit_menu(&mut self) {
        if self.refs.close_commit_menu(&mut self.overlays) {
            self.runtime.hit_map.commit_menu_area = None;
            self.runtime.mark_dirty();
        }
    }

    pub(crate) fn toggle_commit_menu(&mut self) {
        if self.refs.comparison_commits.is_empty() {
            self.set_warning_notice("commit list unavailable");
            return;
        }
        if self.refs.commit_menu_is_open() {
            self.close_commit_menu();
            return;
        }

        self.refs.open_commit_menu();
        self.overlays.close_diff_menu();
        self.runtime.hit_map.diff_menu_area = None;
        self.close_review_input();
        self.refs.close_branch_menu(&mut self.overlays);
        self.refs.branch_menu.reset_input();
        self.set_rendered_branch_menu_area(None);
        self.close_options_menu();
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

    pub(crate) fn commit_menu_rows(&self) -> usize {
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
        let rows = self.commit_menu_rows();
        if SelectorController::new(&mut self.refs.commit_menu, len)
            .with_visible_rows(rows)
            .move_by(delta, SelectorMovement::Saturating)
        {
            self.runtime.dirty = true;
        }
    }

    pub(crate) fn set_commit_selection(&mut self, selected: usize) {
        let len = self.filtered_commits().len();
        let rows = self.commit_menu_rows();
        if SelectorController::new(&mut self.refs.commit_menu, len)
            .with_visible_rows(rows)
            .set_selected(selected)
        {
            self.runtime.dirty = true;
        }
    }

    pub(crate) fn cycle_commit_completion(&mut self, delta: isize) {
        let len = self.filtered_commits().len();
        if len == 0 {
            return;
        }

        let rows = self.commit_menu_rows();
        if SelectorController::new(&mut self.refs.commit_menu, len)
            .with_visible_rows(rows)
            .move_by(delta, SelectorMovement::Wrapping)
        {
            self.runtime.dirty = true;
        }
    }

    pub(crate) fn apply_commit_input_key(&mut self, key: KeyEvent) -> bool {
        let len = self.filtered_commits().len();
        let rows = self.commit_menu_rows();
        let outcome = SelectorController::new(&mut self.refs.commit_menu, len)
            .with_visible_rows(rows)
            .apply_input_key(key);
        if outcome.changed() {
            self.runtime.dirty = true;
        }
        outcome.handled()
    }

    pub(crate) fn selected_commit_menu_choice(&self) -> Option<&GitCommit> {
        let rev = self.refs.show_rev.as_deref()?;
        self.refs.comparison_commits.iter().find(|commit| {
            commit.sha.as_str() == rev
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
        self.select_show_commit(commit.sha.to_string());
    }

    pub(crate) fn select_show_commit(&mut self, rev: String) {
        let mut options = self.document.options.clone();
        options.source = DiffSource::Show(rev.clone().into());

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
            .find(|commit| commit.sha.as_str() == rev || commit.sha.starts_with(rev))
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
        self.runtime
            .hit_map
            .commit_menu_area
            .is_some_and(|area| rect_contains(area, column, row))
    }

    pub(crate) fn is_rendered_review_input_position(&self, column: u16, row: u16) -> bool {
        self.runtime
            .hit_map
            .review_input_area
            .is_some_and(|area| rect_contains(area, column, row))
    }

    pub(crate) fn commit_choice_at(&self, column: u16, row: u16) -> Option<String> {
        if !self.refs.commit_menu_is_open() {
            return None;
        }

        let menu_area = self.runtime.hit_map.commit_menu_area?;
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
            .map(|commit| commit.sha.to_string())
    }
}
