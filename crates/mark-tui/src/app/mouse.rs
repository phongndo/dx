use crossterm::event::MouseEvent;
use mark_core::MarkResult;

use super::{ActionOutcome, DiffApp};
use crate::render::compositor::{
    ComponentEventResult, ComponentId, EventComponent, route_event_through_layers,
};

mod annotation_clicks;
mod click;
mod event_context;
mod hover;
mod scroll;
mod sidebar;

pub(crate) use scroll::{MouseScroll, MouseScrollDirection};

use event_context::{MouseEventContext, MouseEventCtx};

type MouseLayer = MouseComponent;

#[derive(Clone, Copy)]
enum MouseComponent {
    Diff,
    ErrorLogResize,
    OptionsMenu,
    ColorSchemePicker,
    FileSidebarResize,
    HelpMenu,
    OpenMenuScroll,
}

impl<C: MouseEventContext> EventComponent<MouseEvent, C> for MouseComponent {
    fn id(&self) -> ComponentId {
        match self {
            Self::Diff => ComponentId::DiffView,
            Self::ErrorLogResize => ComponentId::ErrorLogResize,
            Self::OptionsMenu => ComponentId::OptionsMenu,
            Self::ColorSchemePicker => ComponentId::ColorSchemePicker,
            Self::FileSidebarResize => ComponentId::FileSidebarResize,
            Self::HelpMenu => ComponentId::HelpMenu,
            Self::OpenMenuScroll => ComponentId::OpenMenuScroll,
        }
    }

    fn handle_event(&self, mouse: MouseEvent, ctx: &mut C) -> MarkResult<ComponentEventResult> {
        match self {
            Self::Diff => handle_diff_mouse_layer(mouse, ctx),
            Self::ErrorLogResize => handle_error_log_resize_mouse_layer(mouse, ctx),
            Self::OptionsMenu => handle_options_menu_mouse_layer(mouse, ctx),
            Self::ColorSchemePicker => handle_color_scheme_picker_mouse_layer(mouse, ctx),
            Self::FileSidebarResize => handle_file_sidebar_resize_mouse_layer(mouse, ctx),
            Self::HelpMenu => handle_help_menu_mouse_layer(mouse, ctx),
            Self::OpenMenuScroll => handle_open_menu_scroll_mouse_layer(mouse, ctx),
        }
    }
}

const DIFF_MOUSE_COMPONENT: MouseComponent = MouseComponent::Diff;
const ERROR_LOG_RESIZE_MOUSE_COMPONENT: MouseComponent = MouseComponent::ErrorLogResize;
const OPTIONS_MENU_MOUSE_COMPONENT: MouseComponent = MouseComponent::OptionsMenu;
const COLOR_SCHEME_PICKER_MOUSE_COMPONENT: MouseComponent = MouseComponent::ColorSchemePicker;
const FILE_SIDEBAR_RESIZE_MOUSE_COMPONENT: MouseComponent = MouseComponent::FileSidebarResize;
const HELP_MENU_MOUSE_COMPONENT: MouseComponent = MouseComponent::HelpMenu;
const OPEN_MENU_SCROLL_MOUSE_COMPONENT: MouseComponent = MouseComponent::OpenMenuScroll;

const MOUSE_LAYERS: &[MouseLayer] = &[
    DIFF_MOUSE_COMPONENT,
    ERROR_LOG_RESIZE_MOUSE_COMPONENT,
    OPTIONS_MENU_MOUSE_COMPONENT,
    COLOR_SCHEME_PICKER_MOUSE_COMPONENT,
    FILE_SIDEBAR_RESIZE_MOUSE_COMPONENT,
    HELP_MENU_MOUSE_COMPONENT,
    OPEN_MENU_SCROLL_MOUSE_COMPONENT,
];

fn route_mouse_through_layers(
    app: &mut DiffApp,
    mouse: MouseEvent,
) -> MarkResult<ComponentEventResult> {
    let mut ctx = MouseEventCtx::new(app);
    route_event_through_layers(MOUSE_LAYERS, mouse, &mut ctx)
}

fn consumed_if(consumed: bool) -> ComponentEventResult {
    if consumed {
        ComponentEventResult::Consumed
    } else {
        ComponentEventResult::Ignored
    }
}

fn handle_open_menu_scroll_mouse_layer(
    mouse: MouseEvent,
    ctx: &mut dyn MouseEventContext,
) -> MarkResult<ComponentEventResult> {
    Ok(consumed_if(ctx.handle_open_menu_scroll(mouse.kind)))
}

fn handle_help_menu_mouse_layer(
    mouse: MouseEvent,
    ctx: &mut dyn MouseEventContext,
) -> MarkResult<ComponentEventResult> {
    Ok(consumed_if(ctx.handle_help_menu_mouse(mouse)))
}

fn handle_file_sidebar_resize_mouse_layer(
    mouse: MouseEvent,
    ctx: &mut dyn MouseEventContext,
) -> MarkResult<ComponentEventResult> {
    Ok(consumed_if(ctx.handle_file_sidebar_resize_mouse(mouse)))
}

fn handle_color_scheme_picker_mouse_layer(
    mouse: MouseEvent,
    ctx: &mut dyn MouseEventContext,
) -> MarkResult<ComponentEventResult> {
    Ok(consumed_if(ctx.handle_color_scheme_picker_mouse(mouse)))
}

fn handle_options_menu_mouse_layer(
    mouse: MouseEvent,
    ctx: &mut dyn MouseEventContext,
) -> MarkResult<ComponentEventResult> {
    Ok(consumed_if(ctx.handle_options_menu_mouse(mouse)))
}

fn handle_error_log_resize_mouse_layer(
    mouse: MouseEvent,
    ctx: &mut dyn MouseEventContext,
) -> MarkResult<ComponentEventResult> {
    Ok(consumed_if(ctx.handle_error_log_resize_mouse(mouse)))
}

fn handle_diff_mouse_layer(
    mouse: MouseEvent,
    ctx: &mut dyn MouseEventContext,
) -> MarkResult<ComponentEventResult> {
    Ok(consumed_if(ctx.handle_diff_mouse(mouse)))
}

impl DiffApp {
    pub(crate) fn handle_mouse_with_effects(
        &mut self,
        mouse: MouseEvent,
    ) -> MarkResult<ActionOutcome> {
        let mut outcome =
            ActionOutcome::from_component_event_result(route_mouse_through_layers(self, mouse)?);
        outcome.extend_effects(self.take_queued_effects());
        Ok(outcome)
    }

    #[cfg(test)]
    pub(crate) fn handle_mouse(&mut self, mouse: MouseEvent) -> MarkResult<()> {
        let outcome = self.handle_mouse_with_effects(mouse)?;
        self.run_effects(outcome.into_effects())
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyModifiers, MouseEvent, MouseEventKind};

    use super::*;

    #[derive(Default)]
    struct FakeMouseCtx {
        open_menu_scroll: bool,
        help_menu: bool,
        file_sidebar_resize: bool,
        color_scheme_picker: bool,
        options_menu: bool,
        error_log_resize: bool,
        diff: bool,
        diff_calls: usize,
    }

    impl MouseEventContext for FakeMouseCtx {
        fn handle_open_menu_scroll(&mut self, _kind: MouseEventKind) -> bool {
            self.open_menu_scroll
        }

        fn handle_help_menu_mouse(&mut self, _mouse: MouseEvent) -> bool {
            self.help_menu
        }

        fn handle_file_sidebar_resize_mouse(&mut self, _mouse: MouseEvent) -> bool {
            self.file_sidebar_resize
        }

        fn handle_color_scheme_picker_mouse(&mut self, _mouse: MouseEvent) -> bool {
            self.color_scheme_picker
        }

        fn handle_options_menu_mouse(&mut self, _mouse: MouseEvent) -> bool {
            self.options_menu
        }

        fn handle_error_log_resize_mouse(&mut self, _mouse: MouseEvent) -> bool {
            self.error_log_resize
        }

        fn handle_diff_mouse(&mut self, _mouse: MouseEvent) -> bool {
            self.diff_calls += 1;
            self.diff
        }
    }

    fn mouse(kind: MouseEventKind) -> MouseEvent {
        MouseEvent {
            kind,
            column: 1,
            row: 1,
            modifiers: KeyModifiers::NONE,
        }
    }

    #[test]
    fn open_menu_scroll_preempts_diff_mouse_layer() {
        let mut ctx = FakeMouseCtx {
            open_menu_scroll: true,
            diff: true,
            ..Default::default()
        };

        let result =
            route_event_through_layers(MOUSE_LAYERS, mouse(MouseEventKind::ScrollDown), &mut ctx)
                .expect("route mouse");

        assert_eq!(result, ComponentEventResult::Consumed);
        assert_eq!(ctx.diff_calls, 0);
    }

    #[test]
    fn diff_layer_handles_mouse_when_overlays_ignore_it() {
        let mut ctx = FakeMouseCtx {
            diff: true,
            ..Default::default()
        };

        let result =
            route_event_through_layers(MOUSE_LAYERS, mouse(MouseEventKind::Moved), &mut ctx)
                .expect("route mouse");

        assert_eq!(result, ComponentEventResult::Consumed);
        assert_eq!(ctx.diff_calls, 1);
    }
}
