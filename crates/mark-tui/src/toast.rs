use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

use mark_syntax::{NotificationMode, NotificationSettings, ToastCorner};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToastLevel {
    Info,
    Success,
    Warning,
    Error,
    Debug,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Toast {
    pub(crate) text: String,
    pub(crate) level: ToastLevel,
    pub(crate) expires_at: Instant,
}

#[derive(Debug, Clone)]
pub(crate) struct Toasts {
    items: VecDeque<Toast>,
    mode: NotificationMode,
    corner: ToastCorner,
    timeout: Duration,
    max_visible: usize,
}

impl Toasts {
    pub(crate) fn new(settings: NotificationSettings) -> Self {
        Self {
            items: VecDeque::new(),
            mode: settings.mode(),
            corner: settings.corner(),
            timeout: Duration::from_millis(settings.timeout().get()),
            max_visible: settings.visible_count().get(),
        }
    }

    pub(crate) fn configure(&mut self, settings: NotificationSettings) {
        self.mode = settings.mode();
        self.corner = settings.corner();
        self.timeout = Duration::from_millis(settings.timeout().get());
        self.max_visible = settings.visible_count().get();
        while self.items.len() > self.max_visible {
            self.items.pop_front();
        }
    }

    pub(crate) fn push(&mut self, level: ToastLevel, text: impl Into<String>) -> bool {
        if level == ToastLevel::Debug && self.mode != NotificationMode::Debug {
            return false;
        }

        while self.items.len() >= self.max_visible {
            self.items.pop_front();
        }
        let now = Instant::now();
        self.items.push_back(Toast {
            text: text.into(),
            level,
            expires_at: now.checked_add(self.timeout).unwrap_or(now),
        });
        true
    }

    pub(crate) fn expire(&mut self, now: Instant) -> bool {
        let before = self.items.len();
        self.items.retain(|toast| now < toast.expires_at);
        self.items.len() != before
    }

    pub(crate) fn visible(&self) -> impl DoubleEndedIterator<Item = &Toast> {
        self.items.iter().rev()
    }

    #[cfg(test)]
    pub(crate) fn latest_text(&self) -> Option<&str> {
        self.items.back().map(|toast| toast.text.as_str())
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub(crate) fn corner(&self) -> ToastCorner {
        self.corner
    }

    pub(crate) fn debug_enabled(&self) -> bool {
        self.mode == NotificationMode::Debug
    }
}
