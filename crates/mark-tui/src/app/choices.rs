use super::{DiffApp, diff_choice_for_options, rect_contains};
use crate::controls::{
    BranchMenu, DiffChoice, branch_match_score, current_head_label, default_branch_base,
    is_review_options,
};
use crate::render::menus::{color_scheme_picker_block, diff_menu_block};
use crate::selector::{SelectorController, SelectorMovement};
use crossterm::event::KeyEvent;
use mark_diff::{DiffOptions, DiffScope, DiffSource};

impl DiffApp {
    pub(crate) fn diff_choice_at(&self, column: u16, row: u16) -> Option<DiffChoice> {
        let choices = self.filtered_diff_choices();
        let menu_area = self.runtime.hit_map.diff_menu_area?;
        let inner = diff_menu_block(self.config.theme).inner(menu_area);
        if column < inner.x
            || column >= inner.x.saturating_add(inner.width)
            || row < inner.y.saturating_add(2)
            || row >= inner.y.saturating_add(inner.height)
        {
            return None;
        }

        let row_index = usize::from(row.saturating_sub(inner.y).saturating_sub(2));
        let pinned_rows = usize::from(self.selected_diff_menu_choice().is_some());
        if row_index < pinned_rows {
            return None;
        }

        let choice_index = row_index.saturating_sub(pinned_rows);
        let rendered_choices = choices
            .len()
            .min(inner.height.saturating_sub(2 + pinned_rows as u16) as usize);
        if choice_index >= rendered_choices {
            return None;
        }

        choices.get(choice_index).copied()
    }

    pub(crate) fn is_rendered_diff_menu_position(&self, column: u16, row: u16) -> bool {
        self.runtime
            .hit_map
            .diff_menu_area
            .is_some_and(|area| rect_contains(area, column, row))
    }

    pub(crate) fn color_scheme_index_at(&self, column: u16, row: u16) -> Option<usize> {
        let menu_area = self.runtime.hit_map.color_scheme_picker_area?;
        let inner = color_scheme_picker_block(self.config.theme).inner(menu_area);
        let choices = self.filtered_color_schemes();
        if column < inner.x
            || column >= inner.x.saturating_add(inner.width)
            || row < inner.y.saturating_add(3)
            || row >= inner.y.saturating_add(inner.height)
        {
            return None;
        }

        let choice_index = self
            .overlays
            .color_scheme_picker
            .scroll
            .saturating_add(usize::from(row.saturating_sub(inner.y).saturating_sub(3)));
        choices.get(choice_index).map(|_| choice_index)
    }

    pub(crate) fn is_rendered_color_scheme_picker_position(&self, column: u16, row: u16) -> bool {
        self.runtime
            .hit_map
            .color_scheme_picker_area
            .is_some_and(|area| rect_contains(area, column, row))
    }

    pub(crate) fn diff_menu_choices(&self) -> Vec<DiffChoice> {
        if matches!(
            &self.document.options.source,
            DiffSource::Range { .. } | DiffSource::Difftool { .. }
        ) || (matches!(&self.document.options.source, DiffSource::Patch(_))
            && !is_review_options(&self.document.options))
        {
            return Vec::new();
        }

        let mut choices = vec![DiffChoice::All];
        if self.refs.branch_base.is_some() {
            choices.push(DiffChoice::Branch);
        }
        choices.push(DiffChoice::Show);
        choices.extend([DiffChoice::Unstaged, DiffChoice::Staged]);
        choices.push(DiffChoice::Review);
        choices
    }

    pub(crate) fn filtered_diff_choices(&self) -> Vec<DiffChoice> {
        let choices = self.selectable_diff_choices();
        let query = self.overlays.diff_menu.input.trim().to_ascii_lowercase();
        if query.is_empty() {
            return choices;
        }

        let mut matches: Vec<_> = choices
            .iter()
            .enumerate()
            .filter_map(|(index, choice)| {
                self.diff_choice_match_score(&query, *choice)
                    .map(|score| (score, index, *choice))
            })
            .collect();
        matches.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
        matches.into_iter().map(|(_, _, choice)| choice).collect()
    }

    pub(crate) fn selectable_diff_choices(&self) -> Vec<DiffChoice> {
        let selected = self.selected_diff_menu_choice();
        self.diff_menu_choices()
            .into_iter()
            .filter(|choice| Some(*choice) != selected)
            .collect()
    }

    pub(crate) fn selected_diff_menu_choice(&self) -> Option<DiffChoice> {
        let selected = self.pending_or_current_diff_choice()?;
        if selected == DiffChoice::Review {
            return None;
        }

        self.diff_menu_choices()
            .contains(&selected)
            .then_some(selected)
    }

    pub(crate) fn diff_choice_match_score(
        &self,
        query: &str,
        choice: DiffChoice,
    ) -> Option<(usize, usize)> {
        let label = choice.label().to_ascii_lowercase();
        let detail = self.diff_choice_detail(choice).to_ascii_lowercase();
        let combined = format!("{label} {detail}");
        branch_match_score(query, &label)
            .or_else(|| branch_match_score(query, &detail))
            .or_else(|| branch_match_score(query, &combined))
    }

    pub(crate) fn diff_choice_detail(&self, choice: DiffChoice) -> String {
        match choice {
            DiffChoice::All => "HEAD → working tree".to_owned(),
            DiffChoice::Unstaged => "index → working tree".to_owned(),
            DiffChoice::Staged => "HEAD → index".to_owned(),
            DiffChoice::Branch => match self.refs.branch_base.as_deref() {
                Some(base) => {
                    let head = self
                        .refs
                        .branch_head
                        .as_deref()
                        .or(self.refs.current_head.as_deref())
                        .unwrap_or("HEAD");
                    format!("{head} → {base}")
                }
                None => "base unavailable".to_owned(),
            },
            DiffChoice::Show => self.show_rev_menu_detail(),
            DiffChoice::Review => "hosted review for this repo".to_owned(),
        }
    }

    pub(crate) fn highlighted_diff_choice(&self) -> Option<DiffChoice> {
        self.filtered_diff_choices()
            .get(self.overlays.diff_menu.selected)
            .copied()
    }

    pub(crate) fn move_diff_menu_selection(&mut self, delta: isize) {
        let len = self.filtered_diff_choices().len();
        if len == 0 {
            return;
        }

        if SelectorController::new(&mut self.overlays.diff_menu, len)
            .move_by(delta, SelectorMovement::Wrapping)
        {
            self.runtime.dirty = true;
        }
    }

    pub(crate) fn set_diff_menu_selection(&mut self, selected: usize) {
        let len = self.filtered_diff_choices().len();
        if SelectorController::new(&mut self.overlays.diff_menu, len).set_selected(selected) {
            self.runtime.dirty = true;
        }
    }

    pub(super) fn apply_diff_menu_input_key(&mut self, key: KeyEvent) -> bool {
        let len = self.filtered_diff_choices().len();
        let outcome =
            SelectorController::new(&mut self.overlays.diff_menu, len).apply_input_key(key);
        if outcome.changed {
            self.runtime.dirty = true;
        }
        outcome.handled
    }

    pub(crate) fn select_highlighted_diff_choice(&mut self) {
        let Some(choice) = self.highlighted_diff_choice() else {
            return;
        };

        self.close_diff_menu();
        self.select_diff_choice(choice);
    }

    pub(crate) fn current_diff_choice(&self) -> Option<DiffChoice> {
        diff_choice_for_options(&self.document.options)
    }

    pub(crate) fn pending_or_current_diff_choice(&self) -> Option<DiffChoice> {
        if self.jobs.pending_review_load.is_some() {
            return Some(DiffChoice::Review);
        }

        self.jobs
            .pending_diff_load
            .as_ref()
            .and_then(|pending| diff_choice_for_options(&pending.options))
            .or_else(|| self.current_diff_choice())
    }

    pub(crate) fn submit_review_input(&mut self) {
        self.submit_review_input_with(Self::start_review_load);
    }

    fn submit_review_input_with(&mut self, start_review_load: impl FnOnce(&mut Self, String)) {
        let target = self.overlays.review_input.trim().to_owned();
        if target.is_empty() {
            self.set_error_log("review unavailable: enter a review ID");
            return;
        }

        self.close_review_input();
        start_review_load(self, target);
    }

    #[cfg(test)]
    pub(crate) fn submit_review_input_for_test(
        &mut self,
        start_review_load: impl FnOnce(&mut Self, String),
    ) {
        self.submit_review_input_with(start_review_load);
    }

    pub(crate) fn cycle_diff_choice(&mut self, delta: isize) {
        let choices: Vec<_> = self
            .diff_menu_choices()
            .into_iter()
            .filter(|choice| *choice != DiffChoice::Review)
            .collect();
        if choices.is_empty() || delta == 0 {
            return;
        }

        let current = self
            .pending_or_current_diff_choice()
            .and_then(|choice| choices.iter().position(|candidate| *candidate == choice));
        // Review opens an input modal, so keyboard cycling skips it. If the
        // current choice is absent, anchor just outside the cycle so the first
        // keypress lands on the first/last diff choice, matching the menu.
        let choice_count = choices.len() as isize;
        let next = match current {
            Some(current) => current as isize + delta,
            None if delta > 0 => delta - 1,
            None => delta,
        }
        .rem_euclid(choice_count) as usize;
        self.select_diff_choice(choices[next]);
    }

    pub(crate) fn select_branch(&mut self, menu: BranchMenu, branch: String) {
        let base = match menu {
            BranchMenu::Head => self.refs.branch_base.clone(),
            BranchMenu::Base => Some(branch.clone()),
        };
        let head = match menu {
            BranchMenu::Head => Some(branch.clone()),
            BranchMenu::Base => self
                .refs
                .branch_head
                .clone()
                .or_else(|| self.refs.current_head.clone())
                .or_else(|| current_head_label(&self.document.changeset.repo)),
        };
        let Some((base, head)) = base.zip(head) else {
            self.set_error_log("branch diff unavailable");
            return;
        };

        let mut options = self.document.options.clone();
        options.source = self.branch_source(base, head);
        options.scope = DiffScope::All;

        if options == self.document.options {
            self.runtime.dirty = true;
            return;
        }

        self.start_diff_load(options, "branch diff unavailable");
    }

    pub(crate) fn branch_source(&self, base: String, head: String) -> DiffSource {
        if self.refs.current_head.as_deref() == Some(head.as_str()) {
            DiffSource::Base(base)
        } else {
            DiffSource::Branch { base, head }
        }
    }

    pub(crate) fn select_diff_choice(&mut self, choice: DiffChoice) {
        if !self.diff_menu_choices().contains(&choice) {
            return;
        }

        if choice == DiffChoice::Review {
            self.open_review_input();
            return;
        }

        let Some(options) = self.options_for_choice(choice) else {
            return;
        };

        if options == self.document.options {
            self.jobs.pending_diff_load = None;
            self.jobs.pending_review_load = None;
            self.runtime.dirty = true;
            return;
        }

        self.start_diff_load(options, "diff unavailable");
    }

    pub(crate) fn options_for_choice(&self, choice: DiffChoice) -> Option<DiffOptions> {
        let mut options = self.document.options.clone();
        match choice {
            DiffChoice::Branch => {
                let base = self.refs.branch_base.clone().or_else(|| {
                    default_branch_base(&self.document.options, &self.document.changeset.repo)
                })?;
                let head = self
                    .refs
                    .branch_head
                    .clone()
                    .or_else(|| self.refs.current_head.clone())
                    .or_else(|| current_head_label(&self.document.changeset.repo))?;
                options.source = self.branch_source(base, head);
                options.scope = DiffScope::All;
            }
            DiffChoice::All => {
                options.source = DiffSource::Worktree;
                options.scope = DiffScope::All;
            }
            DiffChoice::Unstaged => {
                options.source = DiffSource::Worktree;
                options.scope = DiffScope::Unstaged;
            }
            DiffChoice::Staged => {
                options.source = DiffSource::Worktree;
                options.scope = DiffScope::Staged;
            }
            DiffChoice::Show => {
                let rev = self
                    .refs
                    .show_rev
                    .clone()
                    .unwrap_or_else(|| "HEAD".to_owned());
                options.source = DiffSource::Show(rev);
                options.scope = DiffScope::All;
            }
            DiffChoice::Review => return None,
        }

        Some(options)
    }
}
