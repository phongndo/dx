use super::super::{
    AppEffect, DiffApp,
    controllers::{
        filter::FilterController,
        menu::{MenuController, MenuRouteResult},
        navigation::NavigationController,
    },
    is_quit_key,
};
use super::event_context::{KeyEventContext, KeyEventCtx};
use crate::render::compositor::{
    ComponentEventResult, ComponentId, EventComponent, route_event_through_layers,
};
use crossterm::event::KeyEvent;
use mark_core::MarkResult;

type KeyLayer = KeyComponent;

#[derive(Clone, Copy)]
enum KeyComponent {
    Navigation,
    ErrorLogResize,
    PrefixStart,
    SingleGlobal,
    PendingPrefix,
    ErrorLogClose,
    OpenMenu,
    AnnotationInput,
    FilterInput,
    MouseScrollReset,
    EditorShortcut,
    Quit,
    AnnotationDraftBindings,
}

impl<C: KeyEventContext> EventComponent<KeyEvent, C> for KeyComponent {
    fn id(&self) -> ComponentId {
        match self {
            Self::Navigation => ComponentId::Navigation,
            Self::ErrorLogResize => ComponentId::ErrorLogResize,
            Self::PrefixStart | Self::PendingPrefix => ComponentId::Prefix,
            Self::SingleGlobal => ComponentId::GlobalAction,
            Self::ErrorLogClose => ComponentId::ErrorLog,
            Self::OpenMenu => ComponentId::OpenMenuKey,
            Self::AnnotationInput => ComponentId::AnnotationInput,
            Self::FilterInput => ComponentId::FilterInput,
            Self::MouseScrollReset => ComponentId::MouseScrollReset,
            Self::EditorShortcut => ComponentId::EditorShortcut,
            Self::Quit => ComponentId::QuitKey,
            Self::AnnotationDraftBindings => ComponentId::AnnotationDraftBindings,
        }
    }

    fn handle_event(&self, key: KeyEvent, ctx: &mut C) -> MarkResult<ComponentEventResult> {
        match self {
            Self::Navigation => handle_navigation_key_layer(key, ctx),
            Self::ErrorLogResize => handle_error_log_resize_key_layer(key, ctx),
            Self::PrefixStart => handle_prefix_start_key_layer(key, ctx),
            Self::SingleGlobal => handle_single_global_key_layer(key, ctx),
            Self::PendingPrefix => handle_pending_prefix_key_layer(key, ctx),
            Self::ErrorLogClose => handle_error_log_close_key_layer(key, ctx),
            Self::OpenMenu => handle_open_menu_key_layer(key, ctx),
            Self::AnnotationInput => handle_annotation_input_key_layer(key, ctx),
            Self::FilterInput => handle_filter_input_key_layer(key, ctx),
            Self::MouseScrollReset => handle_mouse_scroll_reset_key_layer(key, ctx),
            Self::EditorShortcut => handle_editor_shortcut_key_layer(key, ctx),
            Self::Quit => handle_quit_key_layer(key, ctx),
            Self::AnnotationDraftBindings => handle_annotation_save_or_cancel_key_layer(key, ctx),
        }
    }
}

const NAVIGATION_KEY_COMPONENT: KeyComponent = KeyComponent::Navigation;
const ERROR_LOG_RESIZE_KEY_COMPONENT: KeyComponent = KeyComponent::ErrorLogResize;
const PREFIX_START_KEY_COMPONENT: KeyComponent = KeyComponent::PrefixStart;
const SINGLE_GLOBAL_KEY_COMPONENT: KeyComponent = KeyComponent::SingleGlobal;
const PENDING_PREFIX_KEY_COMPONENT: KeyComponent = KeyComponent::PendingPrefix;
const ERROR_LOG_CLOSE_KEY_COMPONENT: KeyComponent = KeyComponent::ErrorLogClose;
const OPEN_MENU_KEY_COMPONENT: KeyComponent = KeyComponent::OpenMenu;
const ANNOTATION_INPUT_KEY_COMPONENT: KeyComponent = KeyComponent::AnnotationInput;
const FILTER_INPUT_KEY_COMPONENT: KeyComponent = KeyComponent::FilterInput;
const MOUSE_SCROLL_RESET_KEY_COMPONENT: KeyComponent = KeyComponent::MouseScrollReset;
const EDITOR_SHORTCUT_KEY_COMPONENT: KeyComponent = KeyComponent::EditorShortcut;
const QUIT_KEY_COMPONENT: KeyComponent = KeyComponent::Quit;
const ANNOTATION_DRAFT_BINDINGS_KEY_COMPONENT: KeyComponent = KeyComponent::AnnotationDraftBindings;

const KEY_LAYERS: &[KeyLayer] = &[
    NAVIGATION_KEY_COMPONENT,
    ERROR_LOG_RESIZE_KEY_COMPONENT,
    PREFIX_START_KEY_COMPONENT,
    SINGLE_GLOBAL_KEY_COMPONENT,
    PENDING_PREFIX_KEY_COMPONENT,
    ERROR_LOG_CLOSE_KEY_COMPONENT,
    OPEN_MENU_KEY_COMPONENT,
    ANNOTATION_INPUT_KEY_COMPONENT,
    FILTER_INPUT_KEY_COMPONENT,
    MOUSE_SCROLL_RESET_KEY_COMPONENT,
    EDITOR_SHORTCUT_KEY_COMPONENT,
    QUIT_KEY_COMPONENT,
    ANNOTATION_DRAFT_BINDINGS_KEY_COMPONENT,
];

fn key_route_result(should_quit: bool) -> ComponentEventResult {
    if should_quit {
        ComponentEventResult::Quit
    } else {
        ComponentEventResult::Consumed
    }
}

fn menu_route_result(result: MenuRouteResult) -> ComponentEventResult {
    match result {
        MenuRouteResult::Ignored => ComponentEventResult::Ignored,
        MenuRouteResult::Consumed => ComponentEventResult::Consumed,
        MenuRouteResult::Quit => ComponentEventResult::Quit,
    }
}

pub(super) fn route_key_through_layers(
    app: &mut DiffApp,
    key: KeyEvent,
) -> MarkResult<ComponentEventResult> {
    let mut ctx = KeyEventCtx::new(app);
    route_event_through_layers(KEY_LAYERS, key, &mut ctx)
}

fn handle_annotation_save_or_cancel_key_layer(
    key: KeyEvent,
    ctx: &mut dyn KeyEventContext,
) -> MarkResult<ComponentEventResult> {
    Ok(if ctx.handle_annotation_save_or_cancel_key(key) {
        ComponentEventResult::Consumed
    } else {
        ComponentEventResult::Ignored
    })
}

fn handle_quit_key_layer(
    key: KeyEvent,
    _ctx: &mut dyn KeyEventContext,
) -> MarkResult<ComponentEventResult> {
    Ok(if is_quit_key(key) {
        ComponentEventResult::Quit
    } else {
        ComponentEventResult::Ignored
    })
}

fn handle_mouse_scroll_reset_key_layer(
    _key: KeyEvent,
    ctx: &mut dyn KeyEventContext,
) -> MarkResult<ComponentEventResult> {
    ctx.reset_mouse_scroll();
    Ok(ComponentEventResult::Ignored)
}

fn handle_editor_shortcut_key_layer(
    key: KeyEvent,
    ctx: &mut dyn KeyEventContext,
) -> MarkResult<ComponentEventResult> {
    Ok(if ctx.editor_shortcut_requested(key) {
        ComponentEventResult::Effect(AppEffect::OpenEditorShortcut)
    } else {
        ComponentEventResult::Ignored
    })
}

fn handle_filter_input_key_layer(
    key: KeyEvent,
    ctx: &mut dyn KeyEventContext,
) -> MarkResult<ComponentEventResult> {
    Ok(if FilterController::handle_input_key(ctx, key) {
        ComponentEventResult::Consumed
    } else {
        ComponentEventResult::Ignored
    })
}

fn handle_annotation_input_key_layer(
    key: KeyEvent,
    ctx: &mut dyn KeyEventContext,
) -> MarkResult<ComponentEventResult> {
    Ok(if ctx.handle_annotation_input_key_if_open(key) {
        ComponentEventResult::Consumed
    } else {
        ComponentEventResult::Ignored
    })
}

fn handle_open_menu_key_layer(
    key: KeyEvent,
    ctx: &mut dyn KeyEventContext,
) -> MarkResult<ComponentEventResult> {
    MenuController::route_open_menu(ctx, key).map(menu_route_result)
}

fn handle_error_log_close_key_layer(
    key: KeyEvent,
    ctx: &mut dyn KeyEventContext,
) -> MarkResult<ComponentEventResult> {
    Ok(if ctx.close_error_log_on_key(key) {
        ComponentEventResult::Consumed
    } else {
        ComponentEventResult::Ignored
    })
}

fn handle_pending_prefix_key_layer(
    key: KeyEvent,
    ctx: &mut dyn KeyEventContext,
) -> MarkResult<ComponentEventResult> {
    Ok(match ctx.handle_pending_prefix_key(key)? {
        Some(should_quit) => key_route_result(should_quit),
        None => ComponentEventResult::Ignored,
    })
}

fn handle_single_global_key_layer(
    key: KeyEvent,
    ctx: &mut dyn KeyEventContext,
) -> MarkResult<ComponentEventResult> {
    Ok(match ctx.handle_single_global_key(key)? {
        Some(should_quit) => key_route_result(should_quit),
        None => ComponentEventResult::Ignored,
    })
}

fn handle_prefix_start_key_layer(
    key: KeyEvent,
    ctx: &mut dyn KeyEventContext,
) -> MarkResult<ComponentEventResult> {
    Ok(if ctx.begin_prefix_if_matches(key) {
        ComponentEventResult::Consumed
    } else {
        ComponentEventResult::Ignored
    })
}

fn handle_error_log_resize_key_layer(
    key: KeyEvent,
    ctx: &mut dyn KeyEventContext,
) -> MarkResult<ComponentEventResult> {
    Ok(if ctx.handle_error_log_resize_key(key) {
        ComponentEventResult::Consumed
    } else {
        ComponentEventResult::Ignored
    })
}

fn handle_navigation_key_layer(
    key: KeyEvent,
    ctx: &mut dyn KeyEventContext,
) -> MarkResult<ComponentEventResult> {
    Ok(if NavigationController::handle_key(ctx, key) {
        ComponentEventResult::Consumed
    } else {
        ComponentEventResult::Ignored
    })
}

#[cfg(test)]
mod tests;
