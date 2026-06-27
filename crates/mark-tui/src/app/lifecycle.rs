use std::time::{Duration, Instant};

use crossterm::event::KeyEvent;

use super::{DiffApp, is_quit_key};
use crate::keymap::GlobalAction;

impl DiffApp {
    pub(crate) fn event_poll(&self) -> Duration {
        self.jobs.event_poll(Instant::now())
    }

    pub(crate) fn ignore_post_editor_quit_key(&mut self, key: KeyEvent, now: Instant) -> bool {
        let Some(ignore_until) = self.jobs.post_editor_quit_key_ignore_until else {
            return false;
        };
        if now >= ignore_until {
            self.jobs.post_editor_quit_key_ignore_until = None;
            return false;
        }

        is_quit_key(key) || self.config.keymap.matches_single(GlobalAction::Quit, key)
    }

    pub(crate) fn mark_live_reload_invalidated(&mut self) {
        self.invalidate_diff_cache();
        self.jobs.mark_live_reload_invalidated();
    }

    pub(crate) fn mark_live_reload_pending(&mut self) {
        self.invalidate_diff_cache();
        self.jobs.mark_live_reload_pending();
        if self.debug_notifications_enabled() {
            self.set_success_notice("refreshing");
        }
        self.runtime.mark_dirty();
    }
}
