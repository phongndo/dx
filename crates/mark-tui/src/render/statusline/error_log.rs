use ratatui::{
    Frame,
    layout::Rect,
    prelude::{Line, Modifier, Span, Style, Text},
    widgets::{Paragraph, Wrap},
};
use unicode_width::UnicodeWidthStr;

use crate::{
    app::DiffApp,
    keymap::GlobalAction,
    render::{style::base_bg, text::fit},
};

pub(crate) fn draw_error_log(frame: &mut Frame<'_>, app: &DiffApp, area: Rect) {
    let Some(error_log) = app.notifications.error_log.as_deref() else {
        return;
    };
    if area.height == 0 || area.width == 0 {
        return;
    }

    let bg = base_bg(app.config.theme);
    frame.render_widget(
        Paragraph::new(error_log_header_line(app, area.width as usize))
            .style(Style::default().bg(bg)),
        Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: 1,
        },
    );

    let body_area = Rect {
        x: area.x,
        y: area.y.saturating_add(1),
        width: area.width,
        height: area.height.saturating_sub(1),
    };
    if body_area.height == 0 {
        return;
    }

    frame.render_widget(
        Paragraph::new(Text::from(error_log.to_owned()))
            .style(Style::default().fg(app.config.theme.foreground).bg(bg))
            .wrap(Wrap { trim: false }),
        body_area,
    );
}

pub(crate) fn error_log_header_line(app: &DiffApp, width: usize) -> Line<'static> {
    if width == 0 {
        return Line::default();
    }

    let bg = base_bg(app.config.theme);
    let title = "error ";
    let title_width = title.width();
    let rule_style = Style::default()
        .fg(app.config.theme.deletion_fg)
        .bg(bg)
        .add_modifier(Modifier::BOLD);
    if width <= title_width {
        return Line::from(Span::styled(fit(title, width), rule_style));
    }

    let copy_label = error_log_copy_label(app);
    let copy_width = copy_label.width();
    if copy_width == 0 || title_width.saturating_add(copy_width) >= width {
        return Line::from(Span::styled(error_log_separator(width), rule_style));
    }

    let rule_width = width.saturating_sub(title_width).saturating_sub(copy_width);
    Line::from(vec![
        Span::styled(title.to_owned(), rule_style),
        Span::styled("─".repeat(rule_width), rule_style),
        Span::styled(
            copy_label,
            Style::default()
                .fg(app.config.theme.deletion_fg)
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        ),
    ])
}

fn error_log_copy_label(app: &DiffApp) -> String {
    let key = app
        .config
        .keymap
        .global_action_label(GlobalAction::CopyErrorLog);
    if key == "unbound" {
        String::new()
    } else {
        format!(" [Copy All ({key})]")
    }
}

pub(crate) fn error_log_separator(width: usize) -> String {
    let title = "error ";
    if width == 0 {
        return String::new();
    }
    if width <= title.width() {
        return fit(title, width);
    }
    let right = width.saturating_sub(title.width());
    format!("{title}{}", "─".repeat(right))
}

#[cfg(test)]
pub(crate) fn error_log_height(app: &DiffApp, available_height: u16) -> u16 {
    if app.notifications.error_log.is_none() || available_height == 0 {
        return 0;
    }

    app.notifications
        .error_log_height
        .clamp(
            crate::app::ERROR_LOG_MIN_HEIGHT,
            crate::app::ERROR_LOG_MAX_HEIGHT,
        )
        .min(available_height)
}
