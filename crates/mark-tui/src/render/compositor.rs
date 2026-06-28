use mark_core::MarkResult;
use ratatui::{Frame, layout::Rect};

use crate::app::AppEffect;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ComponentEventResult {
    Ignored,
    Consumed,
    Effect(AppEffect),
    Quit,
}

pub(crate) trait EventComponent<E, Ctx: ?Sized>: Sync {
    fn id(&self) -> ComponentId;

    fn handle_event(&self, event: E, ctx: &mut Ctx) -> MarkResult<ComponentEventResult>;
}

pub(crate) fn route_event_through_layers<E: Copy, Ctx: ?Sized, C>(
    layers: &[C],
    event: E,
    ctx: &mut Ctx,
) -> MarkResult<ComponentEventResult>
where
    C: EventComponent<E, Ctx>,
{
    for layer in layers.iter().rev() {
        let _component_id = layer.id();
        let result = layer.handle_event(event, ctx)?;
        if !matches!(result, ComponentEventResult::Ignored) {
            return Ok(result);
        }
    }
    Ok(ComponentEventResult::Ignored)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ComponentId {
    Header,
    FileSidebar,
    FilterBar,
    ErrorLogPanel,
    Toasts,
    AnnotationDraftBindings,
    QuitKey,
    EditorShortcut,
    MouseScrollReset,
    FilterInput,
    AnnotationInput,
    HelpMenu,
    BranchMenu,
    CommitMenu,
    ReviewInput,
    DiffMenu,
    ColorSchemePicker,
    OptionsMenu,
    AnnotationMenu,
    ErrorLog,
    Prefix,
    GlobalAction,
    OpenMenuKey,
    ErrorLogResize,
    Navigation,
    OpenMenuScroll,
    FileSidebarResize,
    DiffView,
}

pub(crate) trait RenderContext {
    fn render_rect_component(&mut self, frame: &mut Frame<'_>, id: ComponentId, area: Rect);
}

pub(crate) trait UiComponent<Ctx: ?Sized> {
    fn id(&self) -> ComponentId;

    fn render(&mut self, _frame: &mut Frame<'_>, _ctx: &mut Ctx) {}
}

pub(crate) struct Compositor<'a, Ctx: ?Sized> {
    layers: Vec<Box<dyn UiComponent<Ctx> + 'a>>,
}

impl<'a, Ctx: ?Sized> Compositor<'a, Ctx> {
    pub(crate) fn new() -> Self {
        Self { layers: Vec::new() }
    }

    pub(crate) fn push(&mut self, layer: impl UiComponent<Ctx> + 'a) {
        self.layers.push(Box::new(layer));
    }

    pub(crate) fn render(&mut self, frame: &mut Frame<'_>, ctx: &mut Ctx) {
        for layer in &mut self.layers {
            let _component_id = layer.id();
            layer.render(frame, ctx);
        }
    }
}

pub(crate) struct RectComponent {
    id: ComponentId,
    area: Rect,
}

impl RectComponent {
    pub(crate) fn new(id: ComponentId, area: Rect) -> Self {
        Self { id, area }
    }
}

impl<Ctx: RenderContext + ?Sized> UiComponent<Ctx> for RectComponent {
    fn id(&self) -> ComponentId {
        self.id
    }

    fn render(&mut self, frame: &mut Frame<'_>, ctx: &mut Ctx) {
        ctx.render_rect_component(frame, self.id, self.area);
    }
}
