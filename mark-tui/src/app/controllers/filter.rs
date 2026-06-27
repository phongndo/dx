use crossterm::event::KeyEvent;

pub(in crate::app) trait FilterInputContext {
    fn filter_input_open(&self) -> bool;
    fn handle_filter_input_key(&mut self, key: KeyEvent) -> bool;
}

pub(in crate::app) struct FilterController;

impl FilterController {
    pub(in crate::app) fn handle_input_key<C: FilterInputContext + ?Sized>(
        ctx: &mut C,
        key: KeyEvent,
    ) -> bool {
        ctx.filter_input_open() && ctx.handle_filter_input_key(key)
    }
}
