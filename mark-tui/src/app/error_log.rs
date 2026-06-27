use super::*;

impl DiffApp {
    pub(crate) fn set_error_log(&mut self, text: impl Into<String>) {
        self.notifications.error_log = Some(text.into());
        self.notifications.error_log_height = ERROR_LOG_DEFAULT_HEIGHT;
        self.runtime.dirty = true;
    }

    pub(crate) fn close_error_log(&mut self) -> bool {
        if self.notifications.error_log.take().is_some() {
            self.input.key_prefix_pending = None;
            self.notifications.error_log_resizing = false;
            self.notifications.rendered_error_log_separator_row = None;
            self.runtime.dirty = true;
            true
        } else {
            false
        }
    }

    pub(crate) fn copy_error_log_to_terminal_clipboard(&mut self) {
        let mut stdout = io::stdout().lock();
        self.copy_error_log_to_writer(&mut stdout);
    }

    pub(crate) fn copy_error_log_to_writer<W: Write>(&mut self, writer: &mut W) {
        let Some(error_log) = self.notifications.error_log.clone() else {
            self.set_warning_notice("no error log to copy");
            return;
        };

        match write_osc52_clipboard(writer, &error_log) {
            Ok(()) => self.set_success_notice("error log copied"),
            Err(error) => self.set_error_log(format!("error log copy failed: {error}")),
        }
    }

    pub(crate) fn resize_error_log(&mut self, delta: isize) -> bool {
        if self.notifications.error_log.is_none() || delta == 0 {
            return false;
        }
        let current = isize::try_from(self.notifications.error_log_height).unwrap_or(isize::MAX);
        let next = current
            .saturating_add(delta)
            .clamp(ERROR_LOG_MIN_HEIGHT as isize, ERROR_LOG_MAX_HEIGHT as isize)
            as u16;
        self.set_error_log_height(next)
    }

    pub(crate) fn set_error_log_height(&mut self, height: u16) -> bool {
        if self.notifications.error_log.is_none() {
            return false;
        }
        let next = height.clamp(ERROR_LOG_MIN_HEIGHT, ERROR_LOG_MAX_HEIGHT);
        if next == self.notifications.error_log_height {
            return false;
        }
        self.notifications.error_log_height = next;
        self.runtime.dirty = true;
        true
    }

    pub(crate) fn error_log_separator_row(&self) -> Option<u16> {
        self.notifications.error_log.as_ref()?;
        self.notifications.rendered_error_log_separator_row
    }

    pub(crate) fn set_rendered_error_log_separator_row(&mut self, row: Option<u16>) {
        self.notifications.rendered_error_log_separator_row =
            row.filter(|_| self.notifications.error_log.is_some());
    }

    pub(crate) fn start_error_log_resize(&mut self, row: u16) -> bool {
        if self.error_log_separator_row() != Some(row) {
            return false;
        }
        self.notifications.error_log_resizing = true;
        self.runtime.dirty = true;
        true
    }

    pub(crate) fn resize_error_log_to_separator_row(&mut self, row: u16) -> bool {
        let Some(separator_row) = self.error_log_separator_row() else {
            return false;
        };
        let delta = i32::from(separator_row).saturating_sub(i32::from(row));
        let current = i32::from(self.notifications.error_log_height);
        let next = current.saturating_add(delta).clamp(
            i32::from(ERROR_LOG_MIN_HEIGHT),
            i32::from(ERROR_LOG_MAX_HEIGHT),
        );
        self.set_error_log_height(next as u16)
    }
}
