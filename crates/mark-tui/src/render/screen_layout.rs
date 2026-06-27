use crate::app::DiffApp;
use ratatui::layout::Rect;

use super::{
    sidebar::file_sidebar_width,
    snapshot::{RenderLayoutSnapshot, RenderSnapshot},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ScreenLayout {
    pub(crate) root: Rect,
    pub(crate) header: Rect,
    pub(crate) body: Rect,
    pub(crate) sidebar: Option<Rect>,
    pub(crate) diff: Rect,
    pub(crate) filter_bar: Option<Rect>,
    pub(crate) error_log: Option<Rect>,
    pub(crate) sidebar_width: u16,
}

impl ScreenLayout {
    pub(crate) fn build(app: &DiffApp, snapshot: &RenderSnapshot, area: Rect) -> Self {
        let header = Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: 1,
        };
        let filter_bar_height = u16::from(area.height > 1 && snapshot.filter_bar_visible);
        let error_log_height = snapshot.error_log_height(
            area.height
                .saturating_sub(1)
                .saturating_sub(filter_bar_height),
        );
        let body_height = area
            .height
            .saturating_sub(1)
            .saturating_sub(filter_bar_height)
            .saturating_sub(error_log_height);
        let body = Rect {
            x: area.x,
            y: area.y.saturating_add(1),
            width: area.width,
            height: body_height,
        };
        let filter_bar = (filter_bar_height > 0).then_some(Rect {
            x: area.x,
            y: body.y.saturating_add(body.height),
            width: area.width,
            height: 1,
        });
        let error_log = (error_log_height > 0).then_some(Rect {
            x: area.x,
            y: body
                .y
                .saturating_add(body.height)
                .saturating_add(filter_bar_height),
            width: area.width,
            height: error_log_height,
        });

        let sidebar_width = file_sidebar_width(app, body.width);
        let (sidebar, diff) = if sidebar_width > 0 {
            (
                Some(Rect {
                    x: body.x,
                    y: body.y,
                    width: sidebar_width,
                    height: body.height,
                }),
                Rect {
                    x: body.x.saturating_add(sidebar_width),
                    y: body.y,
                    width: body.width.saturating_sub(sidebar_width),
                    height: body.height,
                },
            )
        } else {
            (None, body)
        };

        Self {
            root: area,
            header,
            body,
            sidebar,
            diff,
            filter_bar,
            error_log,
            sidebar_width,
        }
    }

    pub(crate) fn snapshot(&self) -> RenderLayoutSnapshot {
        RenderLayoutSnapshot {
            root: self.root,
            diff: self.diff,
            body: self.body,
        }
    }
}
