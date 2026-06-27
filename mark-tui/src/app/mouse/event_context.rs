use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

use super::super::DiffApp;
use super::scroll::MouseScrollDirection;
use crate::theme::HORIZONTAL_SCROLL_STEP;

pub(super) trait MouseEventContext {
    fn handle_open_menu_scroll(&mut self, kind: MouseEventKind) -> bool;
    fn handle_help_menu_mouse(&mut self, mouse: MouseEvent) -> bool;
    fn handle_file_sidebar_resize_mouse(&mut self, mouse: MouseEvent) -> bool;
    fn handle_color_scheme_picker_mouse(&mut self, mouse: MouseEvent) -> bool;
    fn handle_options_menu_mouse(&mut self, mouse: MouseEvent) -> bool;
    fn handle_error_log_resize_mouse(&mut self, mouse: MouseEvent) -> bool;
    fn handle_diff_mouse(&mut self, mouse: MouseEvent) -> bool;
}

pub(super) struct MouseEventCtx<'a> {
    app: &'a mut DiffApp,
}

impl<'a> MouseEventCtx<'a> {
    pub(super) fn new(app: &'a mut DiffApp) -> Self {
        Self { app }
    }
}

impl MouseEventContext for MouseEventCtx<'_> {
    fn handle_open_menu_scroll(&mut self, kind: MouseEventKind) -> bool {
        self.app.handle_open_menu_mouse_scroll(kind)
    }

    fn handle_help_menu_mouse(&mut self, mouse: MouseEvent) -> bool {
        if !self.app.overlays.help_menu_is_open() {
            return false;
        }

        if let MouseEventKind::Down(MouseButton::Left) = mouse.kind {
            self.app.close_help_menu();
        }
        self.app.input.reset_mouse_scroll();
        true
    }

    fn handle_file_sidebar_resize_mouse(&mut self, mouse: MouseEvent) -> bool {
        if !self.app.sidebar.file_sidebar_resizing {
            return false;
        }

        match mouse.kind {
            MouseEventKind::Drag(MouseButton::Left) | MouseEventKind::Moved => {
                self.app.resize_file_sidebar_to_column(mouse.column);
                true
            }
            MouseEventKind::Up(MouseButton::Left) => {
                self.app.sidebar.finish_resize();
                self.app.resize_file_sidebar_to_column(mouse.column);
                true
            }
            _ => false,
        }
    }

    fn handle_color_scheme_picker_mouse(&mut self, mouse: MouseEvent) -> bool {
        if !self.app.overlays.color_scheme_picker_is_open() {
            return false;
        }

        match mouse.kind {
            MouseEventKind::Moved | MouseEventKind::Drag(MouseButton::Left) => {
                if let Some(index) = self.app.color_scheme_index_at(mouse.column, mouse.row) {
                    self.app.set_color_scheme_selection(index);
                }
                true
            }
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(index) = self.app.color_scheme_index_at(mouse.column, mouse.row) {
                    self.app.set_color_scheme_selection(index);
                    self.app.select_highlighted_color_scheme();
                } else if self
                    .app
                    .is_rendered_color_scheme_picker_position(mouse.column, mouse.row)
                {
                    self.app.runtime.dirty = true;
                } else {
                    self.app.close_color_scheme_picker();
                }
                true
            }
            _ => false,
        }
    }

    fn handle_options_menu_mouse(&mut self, mouse: MouseEvent) -> bool {
        if self.app.overlays.options_menu_is_open()
            && matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
        {
            self.app.close_options_menu();
            return true;
        }
        false
    }

    fn handle_error_log_resize_mouse(&mut self, mouse: MouseEvent) -> bool {
        if !self.app.notifications.error_log_resizing {
            return false;
        }

        match mouse.kind {
            MouseEventKind::Drag(MouseButton::Left) | MouseEventKind::Moved => {
                self.app.resize_error_log_to_separator_row(mouse.row);
                true
            }
            MouseEventKind::Up(MouseButton::Left) => {
                self.app.resize_error_log_to_separator_row(mouse.row);
                self.app.notifications.error_log_resizing = false;
                self.app.runtime.dirty = true;
                true
            }
            _ => false,
        }
    }

    fn handle_diff_mouse(&mut self, mouse: MouseEvent) -> bool {
        self.app.update_diff_mouse_hover(mouse.column, mouse.row);

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if self.app.start_error_log_resize(mouse.row) {
                    return true;
                }
                if self.app.start_file_sidebar_resize(mouse.column, mouse.row) {
                    return true;
                }
                self.app.handle_click(mouse.column, mouse.row);
                true
            }
            MouseEventKind::ScrollDown => {
                if self.app.is_file_sidebar_position(mouse.column, mouse.row) {
                    self.app.input.reset_mouse_scroll();
                    self.app.scroll_file_sidebar_by(1);
                    return true;
                }
                self.app
                    .mouse_scroll_or_focus_hunk(MouseScrollDirection::Down);
                self.app.update_diff_mouse_hover(mouse.column, mouse.row);
                true
            }
            MouseEventKind::ScrollUp => {
                if self.app.is_file_sidebar_position(mouse.column, mouse.row) {
                    self.app.input.reset_mouse_scroll();
                    self.app.scroll_file_sidebar_by(-1);
                    return true;
                }
                self.app
                    .mouse_scroll_or_focus_hunk(MouseScrollDirection::Up);
                self.app.update_diff_mouse_hover(mouse.column, mouse.row);
                true
            }
            MouseEventKind::ScrollLeft => {
                if self.app.is_file_sidebar_position(mouse.column, mouse.row) {
                    self.app.input.reset_mouse_scroll();
                    return true;
                }
                self.app
                    .scroll_horizontally_by(-(HORIZONTAL_SCROLL_STEP as isize));
                self.app.update_diff_mouse_hover(mouse.column, mouse.row);
                true
            }
            MouseEventKind::ScrollRight => {
                if self.app.is_file_sidebar_position(mouse.column, mouse.row) {
                    self.app.input.reset_mouse_scroll();
                    return true;
                }
                self.app
                    .scroll_horizontally_by(HORIZONTAL_SCROLL_STEP as isize);
                self.app.update_diff_mouse_hover(mouse.column, mouse.row);
                true
            }
            _ => false,
        }
    }
}
