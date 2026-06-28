use crossterm::event::KeyEvent;
use mark_core::MarkResult;

pub(in crate::app) trait MenuKeyContext {
    fn handle_help_menu_key_if_open(&mut self, key: KeyEvent) -> MarkResult<Option<bool>>;
    fn handle_branch_menu_key_if_open(&mut self, key: KeyEvent) -> MarkResult<Option<bool>>;
    fn handle_commit_menu_key_if_open(&mut self, key: KeyEvent) -> MarkResult<Option<bool>>;
    fn handle_review_input_key_if_open(&mut self, key: KeyEvent) -> MarkResult<Option<bool>>;
    fn handle_diff_menu_key_if_open(&mut self, key: KeyEvent) -> MarkResult<Option<bool>>;
    fn handle_color_scheme_picker_key_if_open(&mut self, key: KeyEvent)
    -> MarkResult<Option<bool>>;
    fn handle_options_menu_key_if_open(&mut self, key: KeyEvent) -> MarkResult<Option<bool>>;
    fn handle_annotation_menu_key_if_open(&mut self, key: KeyEvent) -> MarkResult<Option<bool>>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::app) enum MenuRouteResult {
    Ignored,
    Consumed,
    Quit,
}

impl MenuRouteResult {
    pub(in crate::app) fn from_optional_quit(should_quit: Option<bool>) -> Self {
        match should_quit {
            Some(true) => Self::Quit,
            Some(false) => Self::Consumed,
            None => Self::Ignored,
        }
    }
}

pub(in crate::app) struct MenuController;

impl MenuController {
    pub(in crate::app) fn route_open_menu<C: MenuKeyContext + ?Sized>(
        ctx: &mut C,
        key: KeyEvent,
    ) -> MarkResult<MenuRouteResult> {
        for handle in [
            C::handle_help_menu_key_if_open,
            C::handle_branch_menu_key_if_open,
            C::handle_commit_menu_key_if_open,
            C::handle_review_input_key_if_open,
            C::handle_diff_menu_key_if_open,
            C::handle_color_scheme_picker_key_if_open,
            C::handle_options_menu_key_if_open,
            C::handle_annotation_menu_key_if_open,
        ] {
            let result = MenuRouteResult::from_optional_quit(handle(ctx, key)?);
            if result != MenuRouteResult::Ignored {
                return Ok(result);
            }
        }

        Ok(MenuRouteResult::Ignored)
    }
}
