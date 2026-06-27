use crossterm::event::{KeyEvent, MouseEvent};
use mark_core::MarkResult;
use ratatui::{Frame, layout::Rect};

use crate::app::DiffApp;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ComponentEventResult {
    Ignored,
    Consumed,
    Quit,
}

pub(crate) trait Component {
    fn render(&mut self, _frame: &mut Frame<'_>, _app: &mut DiffApp) {}

    fn handle_key(
        &mut self,
        _key: KeyEvent,
        _app: &mut DiffApp,
    ) -> MarkResult<ComponentEventResult> {
        Ok(ComponentEventResult::Ignored)
    }

    fn handle_mouse(
        &mut self,
        _mouse: MouseEvent,
        _app: &mut DiffApp,
    ) -> MarkResult<ComponentEventResult> {
        Ok(ComponentEventResult::Ignored)
    }
}

type KeyHandler = fn(KeyEvent, &mut DiffApp) -> MarkResult<ComponentEventResult>;
type MouseHandler = fn(MouseEvent, &mut DiffApp) -> MarkResult<ComponentEventResult>;

pub(crate) struct EventComponent {
    handle_key: Option<KeyHandler>,
    handle_mouse: Option<MouseHandler>,
}

impl EventComponent {
    pub(crate) fn key(handle_key: KeyHandler) -> Self {
        Self {
            handle_key: Some(handle_key),
            handle_mouse: None,
        }
    }

    pub(crate) fn mouse(handle_mouse: MouseHandler) -> Self {
        Self {
            handle_key: None,
            handle_mouse: Some(handle_mouse),
        }
    }
}

impl Component for EventComponent {
    fn handle_key(&mut self, key: KeyEvent, app: &mut DiffApp) -> MarkResult<ComponentEventResult> {
        match self.handle_key {
            Some(handle_key) => handle_key(key, app),
            None => Ok(ComponentEventResult::Ignored),
        }
    }

    fn handle_mouse(
        &mut self,
        mouse: MouseEvent,
        app: &mut DiffApp,
    ) -> MarkResult<ComponentEventResult> {
        match self.handle_mouse {
            Some(handle_mouse) => handle_mouse(mouse, app),
            None => Ok(ComponentEventResult::Ignored),
        }
    }
}

pub(crate) struct Compositor<'a> {
    layers: Vec<Box<dyn Component + 'a>>,
}

impl<'a> Compositor<'a> {
    pub(crate) fn new() -> Self {
        Self { layers: Vec::new() }
    }

    pub(crate) fn push(&mut self, layer: impl Component + 'a) {
        self.layers.push(Box::new(layer));
    }

    pub(crate) fn render(&mut self, frame: &mut Frame<'_>, app: &mut DiffApp) {
        for layer in &mut self.layers {
            layer.render(frame, app);
        }
    }

    pub(crate) fn handle_key(
        &mut self,
        key: KeyEvent,
        app: &mut DiffApp,
    ) -> MarkResult<ComponentEventResult> {
        for layer in self.layers.iter_mut().rev() {
            let result = layer.handle_key(key, app)?;
            if !matches!(result, ComponentEventResult::Ignored) {
                return Ok(result);
            }
        }
        Ok(ComponentEventResult::Ignored)
    }

    pub(crate) fn handle_mouse(
        &mut self,
        mouse: MouseEvent,
        app: &mut DiffApp,
    ) -> MarkResult<ComponentEventResult> {
        for layer in self.layers.iter_mut().rev() {
            let result = layer.handle_mouse(mouse, app)?;
            if !matches!(result, ComponentEventResult::Ignored) {
                return Ok(result);
            }
        }
        Ok(ComponentEventResult::Ignored)
    }
}

pub(crate) struct RectComponent {
    area: Rect,
    render: fn(&mut Frame<'_>, &mut DiffApp, Rect),
}

impl RectComponent {
    pub(crate) fn new(area: Rect, render: fn(&mut Frame<'_>, &mut DiffApp, Rect)) -> Self {
        Self { area, render }
    }
}

impl Component for RectComponent {
    fn render(&mut self, frame: &mut Frame<'_>, app: &mut DiffApp) {
        (self.render)(frame, app, self.area);
    }
}
