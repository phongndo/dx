use super::*;

impl DiffApp {
    fn help_menu_line_scroll_delta(&self, key: KeyEvent) -> Option<isize> {
        if self
            .config
            .keymap
            .matches_help_menu_scroll(MenuAction::Down, key)
        {
            Some(1)
        } else if self
            .config
            .keymap
            .matches_help_menu_scroll(MenuAction::Up, key)
        {
            Some(-1)
        } else {
            None
        }
    }

    pub(crate) fn handle_help_menu_key(&mut self, key: KeyEvent) -> MarkResult<bool> {
        if self.config.keymap.matches_menu(MenuAction::Close, key) {
            self.close_help_menu();
            return Ok(false);
        }

        if let Some(delta) = self.help_menu_line_scroll_delta(key) {
            self.scroll_help_menu(delta);
        } else if !self.apply_help_menu_input_key(key) {
            match key.code {
                KeyCode::PageDown => {
                    let page = self.help_menu_page_scroll_rows();
                    if page > 0 {
                        self.scroll_help_menu(page as isize);
                    }
                }
                KeyCode::PageUp => {
                    let page = self.help_menu_page_scroll_rows();
                    if page > 0 {
                        self.scroll_help_menu(-(page as isize));
                    }
                }
                KeyCode::Home => self.set_help_menu_scroll(0),
                KeyCode::End => self.set_help_menu_scroll(usize::MAX),
                _ => {}
            }
        }

        Ok(false)
    }

    pub(super) fn sync_help_menu_visible_rows(&mut self) {
        if !self.overlays.help_menu_open {
            return;
        }
        let Some(visible) = help_menu_list_visible_rows(self, self.viewport.terminal_area) else {
            return;
        };
        if self.overlays.help_menu_visible_rows != visible {
            self.overlays.help_menu_visible_rows = visible;
            self.clamp_help_menu_scroll(visible);
        }
    }

    fn help_menu_page_scroll_rows(&self) -> usize {
        help_menu_list_visible_rows(self, self.viewport.terminal_area)
            .unwrap_or(self.overlays.help_menu_visible_rows)
            .max(1)
    }

    pub(crate) fn toggle_help_menu(&mut self) {
        self.overlays.help_menu_open = !self.overlays.help_menu_open;
        self.overlays.help_menu_input.clear();
        self.overlays.help_menu_input_cursor = 0;
        self.overlays.help_menu_scroll = 0;
        self.input.key_prefix_pending = None;
        if self.overlays.help_menu_open {
            self.sync_help_menu_visible_rows();
        }
        self.runtime.dirty = true;
    }

    pub(crate) fn close_help_menu(&mut self) {
        if self.overlays.help_menu_open
            || !self.overlays.help_menu_input.is_empty()
            || self.overlays.help_menu_scroll != 0
        {
            self.overlays.help_menu_open = false;
            self.overlays.help_menu_input.clear();
            self.overlays.help_menu_input_cursor = 0;
            self.overlays.help_menu_scroll = 0;
            self.input.key_prefix_pending = None;
            self.runtime.dirty = true;
        }
    }

    pub(crate) fn filtered_help_menu_rows(&self) -> Vec<HelpMenuRow> {
        let query = self.overlays.help_menu_input.trim().to_ascii_lowercase();
        if query.is_empty() {
            return HELP_MENU_ROWS.to_vec();
        }

        let mut rows = Vec::new();
        let mut index = 0;
        while index < HELP_MENU_ROWS.len() {
            let HelpMenuRow::Section(section) = HELP_MENU_ROWS[index] else {
                index += 1;
                continue;
            };
            index += 1;

            let mut section_rows = Vec::new();
            while index < HELP_MENU_ROWS.len()
                && !matches!(HELP_MENU_ROWS[index], HelpMenuRow::Section(_))
            {
                section_rows.push(HELP_MENU_ROWS[index]);
                index += 1;
            }

            let section_matches = branch_match_score(&query, section).is_some();
            let matching_rows: Vec<_> = section_rows
                .iter()
                .copied()
                .filter(|row| section_matches || self.help_menu_row_matches(&query, *row))
                .collect();

            if section_matches || !matching_rows.is_empty() {
                rows.push(HelpMenuRow::Section(section));
                rows.extend(matching_rows);
            }
        }

        rows
    }

    fn help_menu_row_matches(&self, query: &str, row: HelpMenuRow) -> bool {
        let HelpMenuRow::Binding(key, description) = row else {
            return false;
        };
        let key_label = self.help_menu_key_label(key).to_ascii_lowercase();
        let description = description.to_ascii_lowercase();
        let combined = format!("{key_label} {description}");
        branch_match_score(query, &key_label)
            .or_else(|| branch_match_score(query, &description))
            .or_else(|| branch_match_score(query, &combined))
            .is_some()
    }

    fn help_menu_key_label(&self, key: HelpMenuKey) -> String {
        match key {
            HelpMenuKey::Static(label) => label.to_owned(),
            HelpMenuKey::Global(action) => self.config.keymap.global_action_label(action),
            HelpMenuKey::GlobalPair(first, second) => format!(
                "{}/{}",
                self.config.keymap.global_action_label(first),
                self.config.keymap.global_action_label(second)
            ),
        }
    }

    pub(crate) fn scroll_help_menu(&mut self, delta: isize) {
        let len = self.filtered_help_menu_rows().len();
        if len == 0 || delta == 0 {
            return;
        }
        let visible = self.overlays.help_menu_visible_rows.max(1);
        let max_scroll = self.help_menu_max_scroll(visible);
        let next = (self.overlays.help_menu_scroll as isize + delta).clamp(0, max_scroll as isize)
            as usize;
        if self.overlays.help_menu_scroll != next {
            self.overlays.help_menu_scroll = next;
            self.runtime.dirty = true;
        }
    }

    pub(crate) fn set_help_menu_scroll(&mut self, scroll: usize) {
        let next =
            scroll.min(self.help_menu_max_scroll(self.overlays.help_menu_visible_rows.max(1)));
        if self.overlays.help_menu_scroll != next {
            self.overlays.help_menu_scroll = next;
            self.runtime.dirty = true;
        }
    }

    fn help_menu_max_scroll(&self, visible_rows: usize) -> usize {
        self.filtered_help_menu_rows()
            .len()
            .saturating_sub(visible_rows.max(1))
    }

    pub(crate) fn clamp_help_menu_scroll(&mut self, visible_rows: usize) {
        let next = self
            .overlays
            .help_menu_scroll
            .min(self.help_menu_max_scroll(visible_rows));
        if self.overlays.help_menu_scroll != next {
            self.overlays.help_menu_scroll = next;
            self.runtime.dirty = true;
        }
    }

    #[allow(dead_code)]
    pub(crate) fn push_help_menu_input(&mut self, character: char) {
        self.overlays
            .help_menu_input
            .insert(self.overlays.help_menu_input_cursor, character);
        self.overlays.help_menu_input_cursor += character.len_utf8();
        self.overlays.help_menu_scroll = 0;
        self.sync_help_menu_visible_rows();
        self.runtime.dirty = true;
    }

    #[allow(dead_code)]
    pub(crate) fn pop_help_menu_input(&mut self) {
        let result = handle_text_input_key(
            &mut self.overlays.help_menu_input,
            &mut self.overlays.help_menu_input_cursor,
            KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
        );
        if matches!(result, TextInputKeyResult::Edited) {
            self.overlays.help_menu_scroll = 0;
            self.sync_help_menu_visible_rows();
            self.runtime.dirty = true;
        }
    }

    #[allow(dead_code)]
    pub(crate) fn clear_help_menu_input(&mut self) {
        if !self.overlays.help_menu_input.is_empty() || self.overlays.help_menu_scroll != 0 {
            self.overlays.help_menu_input.clear();
            self.overlays.help_menu_input_cursor = 0;
            self.overlays.help_menu_scroll = 0;
            self.sync_help_menu_visible_rows();
            self.runtime.dirty = true;
        }
    }

    fn apply_help_menu_input_key(&mut self, key: KeyEvent) -> bool {
        match handle_text_input_key(
            &mut self.overlays.help_menu_input,
            &mut self.overlays.help_menu_input_cursor,
            key,
        ) {
            TextInputKeyResult::Edited => {
                self.overlays.help_menu_scroll = 0;
                self.sync_help_menu_visible_rows();
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
}
