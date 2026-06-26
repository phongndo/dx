use mark_syntax::ToastCorner;
use ratatui::{
    Frame,
    layout::Rect,
    prelude::{Line, Modifier, Span, Style, Text},
    widgets::{Clear, Paragraph},
};
use unicode_width::UnicodeWidthStr;

use crate::{
    app::DiffApp,
    render::{style::statusline_bg, text::fit},
    theme::DIFF_INDICATOR,
    toast::{Toast, ToastLevel},
};

const TOAST_MAX_WIDTH: u16 = 48;
const TOAST_HEIGHT: u16 = 3;
const TOAST_GAP: u16 = 1;
const TOAST_X_MARGIN: u16 = 2;
const TOAST_Y_MARGIN: u16 = 1;

pub(crate) fn draw_toasts(frame: &mut Frame<'_>, app: &DiffApp, area: Rect) {
    if area.width == 0 || area.height == 0 || app.toasts.is_empty() {
        return;
    }

    let corner = app.toasts.corner();
    for (index, toast) in app.toasts.visible().enumerate() {
        let Some(toast_area) = toast_area(toast, area, corner, index) else {
            break;
        };
        frame.render_widget(Clear, toast_area);
        frame.render_widget(
            Paragraph::new(Text::from(toast_lines(
                app,
                toast,
                toast_area.width as usize,
            ))),
            toast_area,
        );
    }
}

fn toast_area(toast: &Toast, area: Rect, corner: ToastCorner, index: usize) -> Option<Rect> {
    let index = u16::try_from(index).ok()?;
    let x_margin = TOAST_X_MARGIN.min(area.width.saturating_sub(1) / 2);
    let y_margin = TOAST_Y_MARGIN.min(area.height.saturating_sub(TOAST_HEIGHT) / 2);
    let available_width = area.width.saturating_sub(x_margin.saturating_mul(2));
    let available_height = area.height.saturating_sub(y_margin.saturating_mul(2));
    let offset = index.saturating_mul(TOAST_HEIGHT.saturating_add(TOAST_GAP));
    if available_width == 0 || offset.saturating_add(TOAST_HEIGHT) > available_height {
        return None;
    }

    let max_width = available_width.clamp(1, TOAST_MAX_WIDTH);
    let natural_width = u16::try_from(toast.text.width().saturating_add(6)).unwrap_or(u16::MAX);
    let width = natural_width.clamp(1, max_width);
    let x = match corner {
        ToastCorner::TopLeft | ToastCorner::BottomLeft => area.x.saturating_add(x_margin),
        ToastCorner::TopRight | ToastCorner::BottomRight => area
            .x
            .saturating_add(area.width.saturating_sub(x_margin).saturating_sub(width)),
    };
    let y = match corner {
        ToastCorner::TopLeft | ToastCorner::TopRight => {
            area.y.saturating_add(y_margin).saturating_add(offset)
        }
        ToastCorner::BottomLeft | ToastCorner::BottomRight => area.y.saturating_add(
            area.height
                .saturating_sub(y_margin)
                .saturating_sub(TOAST_HEIGHT)
                .saturating_sub(offset),
        ),
    };

    Some(Rect {
        x,
        y,
        width,
        height: TOAST_HEIGHT,
    })
}

fn toast_lines(app: &DiffApp, toast: &Toast, width: usize) -> Vec<Line<'static>> {
    vec![
        toast_blank_line(app, toast.level, width),
        toast_content_line(app, toast, width),
        toast_blank_line(app, toast.level, width),
    ]
}

fn toast_blank_line(app: &DiffApp, level: ToastLevel, width: usize) -> Line<'static> {
    if width == 0 {
        return Line::default();
    }

    let bg = statusline_bg(app.theme);
    let accent = toast_accent_color(app, level);
    let padding = width.saturating_sub(DIFF_INDICATOR.width());

    Line::from(vec![
        Span::styled(
            DIFF_INDICATOR,
            Style::default()
                .fg(accent)
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" ".repeat(padding), Style::default().bg(bg)),
    ])
}

fn toast_content_line(app: &DiffApp, toast: &Toast, width: usize) -> Line<'static> {
    if width == 0 {
        return Line::default();
    }

    let bg = statusline_bg(app.theme);
    let accent = toast_accent_color(app, toast.level);
    let message_width = width
        .saturating_sub(DIFF_INDICATOR.width())
        .saturating_sub(3);
    let message = fit(&toast.text, message_width);
    let used = DIFF_INDICATOR
        .width()
        .saturating_add(2)
        .saturating_add(message.width());
    let padding = width.saturating_sub(used);

    Line::from(vec![
        Span::styled(
            DIFF_INDICATOR,
            Style::default()
                .fg(accent)
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("  ", Style::default().bg(bg)),
        Span::styled(
            message,
            Style::default()
                .fg(app.theme.foreground)
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" ".repeat(padding), Style::default().bg(bg)),
    ])
}

fn toast_accent_color(app: &DiffApp, level: ToastLevel) -> ratatui::prelude::Color {
    match level {
        ToastLevel::Info => app.theme.foreground,
        ToastLevel::Success => app.theme.addition_fg,
        ToastLevel::Warning => ratatui::prelude::Color::Yellow,
        ToastLevel::Error => app.theme.deletion_fg,
        ToastLevel::Debug => app.theme.muted,
    }
}
