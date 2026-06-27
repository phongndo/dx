use std::env;

use crossterm::terminal as crossterm_terminal;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub(crate) fn render_syntax_statuses(
    statuses: &[mark_command::SyntaxLanguageStatus],
    color: bool,
    glyphs: ListGlyphs,
    terminal_width: Option<usize>,
) -> String {
    let headers = ["language", "status", "source", "version"];
    let rows = statuses
        .iter()
        .map(|status| {
            [
                status.language.clone(),
                syntax_status_label(status, glyphs).to_owned(),
                syntax_source_label(status).to_owned(),
                syntax_version_label(status).to_owned(),
            ]
        })
        .collect::<Vec<_>>();
    let min_widths = [6, 4, 3, 1];
    let mut widths = headers
        .iter()
        .enumerate()
        .map(|(index, header)| {
            rows.iter()
                .map(|row| display_width(&row[index]))
                .chain([display_width(header), min_widths[index]])
                .max()
                .expect("width candidates should not be empty")
        })
        .collect::<Vec<_>>();

    shrink_syntax_columns(&mut widths, min_widths, terminal_width);

    let mut output = String::new();
    for (index, header) in headers.iter().enumerate() {
        if index > 0 {
            output.push(' ');
        }
        output.push_str(&styled_cell(header, widths[index], StyleColor::Cyan, color));
    }
    output.push('\n');

    for (status, row) in statuses.iter().zip(rows) {
        for (index, value) in row.iter().enumerate() {
            if index > 0 {
                output.push(' ');
            }
            let value = truncate_middle(value, widths[index], glyphs);
            let color_for_cell = match index {
                0 => StyleColor::Magenta,
                1 => syntax_status_color(status),
                _ => StyleColor::White,
            };
            if index == 1 {
                output.push_str(&styled_centered_cell(
                    &value,
                    widths[index],
                    color_for_cell,
                    color,
                ));
            } else {
                output.push_str(&styled_cell(&value, widths[index], color_for_cell, color));
            }
        }
        output.push('\n');
    }

    output
}

pub(crate) fn shrink_syntax_columns(
    widths: &mut [usize],
    min_widths: [usize; 4],
    terminal_width: Option<usize>,
) {
    let Some(terminal_width) = terminal_width else {
        return;
    };
    while list_row_width(widths) > terminal_width {
        let Some(index) = widths
            .iter()
            .enumerate()
            .filter(|(index, width)| **width > min_widths[*index])
            .max_by_key(|(_, width)| **width)
            .map(|(index, _)| index)
        else {
            break;
        };
        widths[index] -= 1;
    }
}

pub(crate) fn syntax_status_label(
    status: &mark_command::SyntaxLanguageStatus,
    glyphs: ListGlyphs,
) -> &'static str {
    match syntax_status_kind(status) {
        SyntaxStatusKind::Ready => glyphs.clean,
        SyntaxStatusKind::Warning => glyphs.dirty,
        SyntaxStatusKind::Error => glyphs.unknown,
        SyntaxStatusKind::Disabled => "-",
    }
}

pub(crate) fn syntax_status_color(status: &mark_command::SyntaxLanguageStatus) -> StyleColor {
    match syntax_status_kind(status) {
        SyntaxStatusKind::Ready => StyleColor::Green,
        SyntaxStatusKind::Warning => StyleColor::Yellow,
        SyntaxStatusKind::Error => StyleColor::Red,
        SyntaxStatusKind::Disabled => StyleColor::White,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SyntaxStatusKind {
    Ready,
    Warning,
    Error,
    Disabled,
}

pub(crate) fn syntax_status_kind(status: &mark_command::SyntaxLanguageStatus) -> SyntaxStatusKind {
    if !status.enabled {
        SyntaxStatusKind::Disabled
    } else if !status.installed || !status.trusted {
        SyntaxStatusKind::Error
    } else if !status.has_highlights {
        SyntaxStatusKind::Warning
    } else {
        SyntaxStatusKind::Ready
    }
}

pub(crate) fn syntax_source_label(status: &mark_command::SyntaxLanguageStatus) -> &'static str {
    if status.source.as_deref() == Some("bundled") {
        "bundled"
    } else if status.source.as_deref() == Some("custom") {
        "custom"
    } else if status.artifact.is_some() {
        "cache"
    } else {
        "-"
    }
}

pub(crate) fn syntax_version_label(status: &mark_command::SyntaxLanguageStatus) -> &str {
    status.version.as_deref().unwrap_or("-")
}

pub(crate) fn short_sha(sha: &str) -> &str {
    sha.get(..12).unwrap_or(sha)
}

#[derive(Clone, Copy)]
pub(crate) struct ListGlyphs {
    pub(crate) clean: &'static str,
    pub(crate) dirty: &'static str,
    pub(crate) unknown: &'static str,
    pub(crate) ellipsis: &'static str,
}

pub(crate) fn list_glyphs(unicode: bool) -> ListGlyphs {
    if unicode {
        ListGlyphs {
            clean: "✓",
            dirty: "!",
            unknown: "?",
            ellipsis: "…",
        }
    } else {
        ListGlyphs {
            clean: "ok",
            dirty: "!",
            unknown: "?",
            ellipsis: "...",
        }
    }
}

pub(crate) fn ascii_output_requested() -> bool {
    env::var_os("MARK_ASCII").is_some()
}

pub(crate) fn terminal_width() -> Option<usize> {
    crossterm_terminal::size()
        .ok()
        .map(|(columns, _)| usize::from(columns))
        .filter(|columns| *columns > 0)
}

pub(crate) fn list_row_width(widths: &[usize]) -> usize {
    widths.iter().sum::<usize>() + widths.len().saturating_sub(1)
}

pub(crate) fn display_width(value: &str) -> usize {
    UnicodeWidthStr::width(value)
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum StyleColor {
    Green,
    Cyan,
    Magenta,
    Red,
    Yellow,
    White,
}

pub(crate) fn styled_cell(value: &str, width: usize, color: StyleColor, enabled: bool) -> String {
    styled(&plain_cell(value, width), color, enabled)
}

pub(crate) fn styled_centered_cell(
    value: &str,
    width: usize,
    color: StyleColor,
    enabled: bool,
) -> String {
    styled(&plain_centered_cell(value, width), color, enabled)
}

pub(crate) fn plain_cell(value: &str, width: usize) -> String {
    format!(
        "{value}{}",
        " ".repeat(width.saturating_sub(display_width(value)))
    )
}

pub(crate) fn plain_centered_cell(value: &str, width: usize) -> String {
    let padding = width.saturating_sub(display_width(value));
    let left = padding / 2;
    let right = padding - left;
    format!("{}{}{}", " ".repeat(left), value, " ".repeat(right))
}

pub(crate) fn truncate_middle(value: &str, width: usize, glyphs: ListGlyphs) -> String {
    if display_width(value) <= width {
        return value.to_owned();
    }
    if width == 0 {
        return String::new();
    }

    let ellipsis_width = display_width(glyphs.ellipsis);
    if width <= ellipsis_width {
        return glyphs.ellipsis.chars().take(width).collect();
    }

    let available = width - ellipsis_width;
    let prefix_width = available / 2;
    let suffix_width = available - prefix_width;
    let prefix = take_display_width(value, prefix_width);
    let suffix = take_display_width_from_end(value, suffix_width);

    format!("{prefix}{}{suffix}", glyphs.ellipsis)
}

pub(crate) fn take_display_width(value: &str, width: usize) -> String {
    let mut output = String::new();
    let mut used_width = 0;
    for character in value.chars() {
        let character_width = character.width().unwrap_or(0);
        if used_width + character_width > width {
            break;
        }
        used_width += character_width;
        output.push(character);
    }
    output
}

pub(crate) fn take_display_width_from_end(value: &str, width: usize) -> String {
    let mut output = Vec::new();
    let mut used_width = 0;
    for character in value.chars().rev() {
        let character_width = character.width().unwrap_or(0);
        if used_width + character_width > width {
            break;
        }
        used_width += character_width;
        output.push(character);
    }
    output.into_iter().rev().collect()
}

pub(crate) fn styled(value: &str, color: StyleColor, enabled: bool) -> String {
    if !enabled {
        return value.to_owned();
    }

    let code = match color {
        StyleColor::Green => "32",
        StyleColor::Cyan => "36",
        StyleColor::Magenta => "35",
        StyleColor::Red => "31",
        StyleColor::Yellow => "33",
        StyleColor::White => "37",
    };

    format!("\x1b[{code}m{value}\x1b[0m")
}
