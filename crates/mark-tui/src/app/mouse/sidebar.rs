use super::super::DiffApp;

impl DiffApp {
    pub(crate) fn is_file_sidebar_position(&self, column: u16, row: u16) -> bool {
        self.sidebar
            .is_position(column, row, self.visible_file_sidebar_rows())
    }

    pub(crate) fn is_file_sidebar_resize_handle(&self, column: u16, row: u16) -> bool {
        self.sidebar
            .is_resize_handle(column, row, self.visible_file_sidebar_rows())
    }

    pub(crate) fn start_file_sidebar_resize(&mut self, column: u16, row: u16) -> bool {
        if !self.is_file_sidebar_resize_handle(column, row) {
            return false;
        }

        self.sidebar.start_resize();
        self.resize_file_sidebar_to_column(column);
        true
    }

    pub(crate) fn resize_file_sidebar_to_column(&mut self, column: u16) {
        let width = column.saturating_add(1);
        self.set_file_sidebar_width(width);
    }
}
