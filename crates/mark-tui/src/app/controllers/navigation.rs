use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::theme::HORIZONTAL_SCROLL_STEP;

use super::super::is_plain_char_key;

pub(in crate::app) trait NavigationContext {
    fn filters_active(&self) -> bool;
    fn grep_filter_active(&self) -> bool;
    fn clear_all_filters(&mut self);
    fn scroll_or_focus_hunk(&mut self, delta: isize);
    fn scroll_horizontally_by(&mut self, delta: isize);
    fn set_scroll(&mut self, scroll: usize);
    fn max_scroll(&self) -> usize;
    fn move_grep_match(&mut self, delta: isize);
}

pub(in crate::app) struct NavigationController;

impl NavigationController {
    pub(in crate::app) fn handle_key<C: NavigationContext + ?Sized>(
        ctx: &mut C,
        key: KeyEvent,
    ) -> bool {
        match key.code {
            KeyCode::Esc if ctx.filters_active() => ctx.clear_all_filters(),
            KeyCode::Down | KeyCode::Char('j') => ctx.scroll_or_focus_hunk(1),
            KeyCode::Up | KeyCode::Char('k') => ctx.scroll_or_focus_hunk(-1),
            KeyCode::Left | KeyCode::Char('h') => {
                ctx.scroll_horizontally_by(-(HORIZONTAL_SCROLL_STEP as isize));
            }
            KeyCode::Right | KeyCode::Char('l') => {
                ctx.scroll_horizontally_by(HORIZONTAL_SCROLL_STEP as isize);
            }
            KeyCode::PageDown => ctx.scroll_or_focus_hunk(20),
            KeyCode::Char('d') if is_plain_char_key(key, 'd') => ctx.scroll_or_focus_hunk(20),
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                ctx.scroll_or_focus_hunk(20);
            }
            KeyCode::PageUp | KeyCode::Char('u') => ctx.scroll_or_focus_hunk(-20),
            KeyCode::Home => ctx.set_scroll(0),
            KeyCode::Char('g') if is_plain_char_key(key, 'g') => ctx.set_scroll(0),
            KeyCode::End | KeyCode::Char('G') => ctx.set_scroll(ctx.max_scroll()),
            KeyCode::Char('n') if ctx.grep_filter_active() => ctx.move_grep_match(1),
            KeyCode::Char('p') | KeyCode::Char('N') if ctx.grep_filter_active() => {
                ctx.move_grep_match(-1);
            }
            KeyCode::Char('n') | KeyCode::Char('p') | KeyCode::Char('N') => {}
            _ => return false,
        }

        true
    }
}
