use mark_diff::{DiffLine, DiffLineKind};
use mark_syntax::{DiffBackground, HighlightedLine};
use ratatui::prelude::{Color, Line, Style};

use crate::{
    app::{unified_content_width, wrapped_line_start_columns},
    render::{
        diff::{
            content_spans_at_scroll, diff_indicator_span_for_focus, gutter_spans,
            unified_gutter_text,
        },
        grep::{
            diff_line_grep_highlight_text, grep_highlight_target_for_columns,
            highlighted_grep_text_line, scrolled_text_byte_start, unified_content_start_column,
        },
        style::base_bg,
        text::spaces,
    },
    syntax::InlineRange,
    theme::{DiffTheme, UNIFIED_GUTTER_WIDTH},
};

#[derive(Debug, Clone, Copy)]
struct UnifiedLineRender {
    width: usize,
    theme: DiffTheme,
    horizontal_scroll: usize,
    focused: bool,
    continuation: bool,
}

pub(crate) fn render_unified_line_at_scroll(
    line: &DiffLine,
    syntax: Option<&HighlightedLine>,
    inline: &[InlineRange],
    _row_index: usize,
    width: usize,
    theme: DiffTheme,
    horizontal_scroll: usize,
) -> Line<'static> {
    render_unified_line_at_scroll_with_focus(
        line,
        syntax,
        inline,
        width,
        theme,
        horizontal_scroll,
        false,
    )
}

pub(crate) fn render_unified_line_at_scroll_with_focus(
    line: &DiffLine,
    syntax: Option<&HighlightedLine>,
    inline: &[InlineRange],
    width: usize,
    theme: DiffTheme,
    horizontal_scroll: usize,
    focused: bool,
) -> Line<'static> {
    render_unified_line_segment_with_focus(
        line,
        syntax,
        inline,
        UnifiedLineRender {
            width,
            theme,
            horizontal_scroll,
            focused,
            continuation: false,
        },
    )
}

pub(crate) fn render_unified_line_wrapped_with_focus(
    line: &DiffLine,
    syntax: Option<&HighlightedLine>,
    inline: &[InlineRange],
    width: usize,
    theme: DiffTheme,
    focused: bool,
    grep_filter: &str,
) -> Vec<Line<'static>> {
    let content_width = unified_content_width(width);
    let scrolls = wrapped_line_start_columns(&line.text, content_width);
    let mut lines = Vec::with_capacity(scrolls.len());
    for (wrap_index, horizontal_scroll) in scrolls.iter().copied().enumerate() {
        let rendered = render_unified_line_segment_with_focus(
            line,
            syntax,
            inline,
            UnifiedLineRender {
                width,
                theme,
                horizontal_scroll,
                focused,
                continuation: wrap_index > 0,
            },
        );
        lines.push(highlight_wrapped_unified_grep_line(
            rendered,
            line,
            grep_filter,
            width,
            horizontal_scroll,
            theme,
        ));
    }
    lines
}

fn render_unified_line_segment_with_focus(
    line: &DiffLine,
    syntax: Option<&HighlightedLine>,
    inline: &[InlineRange],
    render: UnifiedLineRender,
) -> Line<'static> {
    let UnifiedLineRender {
        width,
        theme,
        horizontal_scroll,
        focused,
        continuation,
    } = render;

    if width == 0 {
        return Line::default();
    }

    let sign = if continuation {
        " "
    } else {
        match line.kind {
            DiffLineKind::Context => " ",
            DiffLineKind::Addition => "+",
            DiffLineKind::Deletion => "-",
            DiffLineKind::Meta => " ",
        }
    };
    let indicator_width = 1.min(width);
    let gutter_width = UNIFIED_GUTTER_WIDTH.min(width.saturating_sub(indicator_width));
    let content_width = unified_content_width(width);
    let gutter = if continuation {
        spaces(UNIFIED_GUTTER_WIDTH.saturating_sub(1)).into_owned()
    } else {
        unified_gutter_text(line.old_line, line.new_line)
    };
    let mut spans = Vec::new();
    if indicator_width > 0 {
        spans.push(diff_indicator_span_for_focus(line.kind, theme, focused));
    }
    if gutter_width > 0 {
        spans.extend(gutter_spans(&gutter, sign, gutter_width, line.kind, theme));
    }
    spans.extend(content_spans_at_scroll(
        &line.text,
        syntax,
        inline,
        line.kind,
        content_width,
        theme,
        horizontal_scroll,
    ));
    Line::from(spans)
}

fn highlight_wrapped_unified_grep_line(
    rendered: Line<'static>,
    line: &DiffLine,
    query: &str,
    width: usize,
    horizontal_scroll: usize,
    theme: DiffTheme,
) -> Line<'static> {
    if query.is_empty() {
        return rendered;
    }

    let targets = grep_highlight_target_for_columns(
        diff_line_grep_highlight_text(line),
        &rendered.spans,
        unified_content_start_column(width),
        width,
        1 + scrolled_text_byte_start(&line.text, horizontal_scroll),
    )
    .into_iter()
    .collect();
    highlighted_grep_text_line(rendered, query, targets, theme)
}

pub(crate) fn row_bg(kind: DiffLineKind, theme: DiffTheme) -> Color {
    if theme.transparent_background {
        return Color::Reset;
    }

    match (theme.diff.line_background, kind) {
        (DiffBackground::None, _) => theme.background,
        (DiffBackground::Subtle, DiffLineKind::Addition) => theme.addition_bg,
        (DiffBackground::Subtle, DiffLineKind::Deletion) => theme.deletion_bg,
        (DiffBackground::Strong, DiffLineKind::Addition) => theme.addition_inline_bg,
        (DiffBackground::Strong, DiffLineKind::Deletion) => theme.deletion_inline_bg,
        _ => theme.background,
    }
}

pub(crate) fn line_style(kind: DiffLineKind, theme: DiffTheme) -> Style {
    match kind {
        DiffLineKind::Addition => Style::default()
            .fg(theme.foreground)
            .bg(row_bg(kind, theme)),
        DiffLineKind::Deletion => Style::default()
            .fg(theme.foreground)
            .bg(row_bg(kind, theme)),
        DiffLineKind::Meta => Style::default().fg(theme.muted).bg(base_bg(theme)),
        DiffLineKind::Context => Style::default().fg(theme.foreground).bg(base_bg(theme)),
    }
}
