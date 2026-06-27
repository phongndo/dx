use std::time::Instant;

use super::DiffApp;
use crate::toast::ToastLevel;

impl DiffApp {
    pub(crate) fn set_notice(&mut self, text: impl Into<String>) {
        if self.notifications.push_toast(ToastLevel::Info, text) {
            self.runtime.mark_dirty();
        }
    }

    pub(crate) fn set_success_notice(&mut self, text: impl Into<String>) {
        if self.notifications.push_toast(ToastLevel::Success, text) {
            self.runtime.mark_dirty();
        }
    }

    pub(crate) fn set_warning_notice(&mut self, text: impl Into<String>) {
        if self.notifications.push_toast(ToastLevel::Warning, text) {
            self.runtime.mark_dirty();
        }
    }

    pub(crate) fn set_blocked_notice(&mut self, text: impl Into<String>) {
        if self.notifications.push_toast(ToastLevel::Error, text) {
            self.runtime.mark_dirty();
        }
    }

    pub(crate) fn set_debug_notice(&mut self, text: impl Into<String>) {
        if self.notifications.push_toast(ToastLevel::Debug, text) {
            self.runtime.mark_dirty();
        }
    }

    pub(crate) fn expire_toasts(&mut self, now: Instant) {
        if self.notifications.expire_toasts(now) {
            self.runtime.mark_dirty();
        }
    }

    pub(crate) fn debug_notifications_enabled(&self) -> bool {
        self.notifications.toasts.debug_enabled()
    }
}
