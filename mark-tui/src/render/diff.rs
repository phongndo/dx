use mark_diff::{DiffLine, DiffLineKind};
use mark_syntax::{DiffBackground, HighlightedLine, SyntaxClass};
use ratatui::{
    Frame,
    layout::Rect,
    prelude::{Color, Line, Modifier, Span, Style, Text},
    widgets::Paragraph,
};
use unicode_width::UnicodeWidthStr;

use crate::{
    annotation::AnnotationKey,
    app::{DiffApp, split_cell_content_width, unified_content_width, wrapped_line_start_columns},
    controls::DiffLayoutMode,
    model::UiRow,
    render::{
        annotations::{
            append_annotation_add_button, render_annotation_compose_block,
            render_annotation_saved_block,
        },
        grep::{
            diff_line_grep_highlight_text, grep_highlight_target_for_columns,
            grep_highlight_targets_for_row, highlighted_grep_text_line,
            highlighted_mouse_diff_content_line, scrolled_text_byte_start,
            split_diff_line_grep_highlight_target, unified_content_start_column,
        },
        headers::{
            file_header_line, file_separator_line, hunk_header_line, hunk_header_line_with_focus,
        },
        style::{base_bg, diff_indicator_span, diff_sign_style, focused_diff_indicator_span},
        text::{
            fit, fit_padded, fit_padded_from, fit_with_width, format_count, skip_display_prefix,
            spaces,
        },
    },
    syntax::{DiffSide, InlineRange, unified_syntax_side},
    theme::{
        DiffTheme, EMPTY_DIFF_FILL, EMPTY_DIFF_FILL_SPACING, GUTTER_WIDTH, UNIFIED_GUTTER_WIDTH,
        line_gutter_bg, line_gutter_fg,
    },
};

pub(crate) fn draw_diff(frame: &mut Frame<'_>, app: &mut DiffApp, area: Rect) {
    if app.model.is_empty() {
        let message = if app.filters_active() && !app.base_changeset.files.is_empty() {
            "No files match filters."
        } else {
            "No changes."
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                message,
                Style::default().fg(app.theme.muted),
            )))
            .style(Style::default().bg(base_bg(app.theme))),
            area,
        );
        return;
    }

    let visible_rows = area.height as usize;
    app.prepare_syntax_for_viewport(visible_rows);
    let width = area.width as usize;
    let lines = build_diff_viewport_lines(app, width, visible_rows);
    frame.render_widget(
        Paragraph::new(Text::from(lines)).style(Style::default().bg(base_bg(app.theme))),
        area,
    );
}

pub(crate) fn build_diff_viewport_lines(
    app: &mut DiffApp,
    width: usize,
    visible_rows: usize,
) -> Vec<Line<'static>> {
    if app.line_wrapping {
        return build_wrapped_viewport_lines(app, width, visible_rows);
    }

    let mouse_line_highlight = mouse_line_highlight_for_viewport(app);
    let theme = app.theme;
    let layout = app.layout;
    let draft = app.annotation_draft.clone();
    let annotations = app.annotations.clone();
    let focused_hunk = app.focused_hunk_for_viewport(visible_rows);
    let mut lines = Vec::with_capacity(visible_rows);

    for offset in 0..visible_rows {
        if lines.len() >= visible_rows {
            break;
        }
        let visual_row = app.scroll.saturating_add(offset);
        let Some(row) = app.model.row(visual_row) else {
            break;
        };
        let mut line = render_row_with_focus(app, visual_row, row, width, focused_hunk);
        if mouse_line_highlight.is_some_and(|highlight_row| highlight_row == visual_row)
            && row_has_diff_code_content(row)
            && draft.is_none()
        {
            line = highlighted_mouse_diff_content_line(line, layout, width, theme);
            line = append_annotation_add_button(line, width, theme);
        }
        lines.push(line);

        if draft
            .as_ref()
            .is_some_and(|d| d.key.model_row_index == visual_row)
        {
            let draft = draft.as_ref().expect("draft");
            let label = app.annotation_label(&draft.key);
            push_annotation_block(
                &mut lines,
                render_annotation_compose_block(draft, width, theme, label.as_deref()),
                visible_rows,
            );
            continue;
        }

        if let Some(key) = AnnotationKey::from_ui_row(row, visual_row)
            && let Some(text) = annotations.get(&key)
            && draft.as_ref().is_none_or(|d| d.key != key)
        {
            let label = app.annotation_label(&key);
            push_annotation_block(
                &mut lines,
                render_annotation_saved_block(text, width, theme, label.as_deref()),
                visible_rows,
            );
        }
    }

    lines.truncate(visible_rows);
    lines
}

fn build_wrapped_viewport_lines(
    app: &mut DiffApp,
    width: usize,
    visible_rows: usize,
) -> Vec<Line<'static>> {
    let mouse_line_highlight = mouse_line_highlight_for_viewport(app);
    let theme = app.theme;
    let layout = app.layout;
    let draft = app.annotation_draft.clone();
    let annotations = app.annotations.clone();
    let focused_hunk = app.focused_hunk_for_viewport(visible_rows);
    let mut lines = Vec::with_capacity(visible_rows);
    let Some((mut row_index, mut row_offset)) = app.model_row_at_scroll(app.scroll) else {
        return lines;
    };
    let mut visual_row = app.scroll;
    while lines.len() < visible_rows {
        let Some(row) = app.model.row(row_index) else {
            break;
        };
        let remaining = visible_rows.saturating_sub(lines.len());
        let rendered = render_row_wrapped_with_focus(app, row_index, row, width, focused_hunk);
        let wrap_count = rendered.len().saturating_sub(row_offset);
        for (wrap_index, line) in rendered
            .into_iter()
            .skip(row_offset)
            .take(remaining)
            .enumerate()
        {
            let mut line = line;
            let is_last_wrap = wrap_index + 1 == wrap_count.min(remaining);
            let anchor_visual = app.annotation_anchor_visual_scroll(row_index);
            if mouse_line_highlight.is_some_and(|highlight_row| highlight_row == visual_row)
                && row_has_diff_code_content(row)
                && draft.is_none()
            {
                line = highlighted_mouse_diff_content_line(line, layout, width, theme);
                if visual_row == anchor_visual {
                    line = append_annotation_add_button(line, width, theme);
                }
            }
            lines.push(line);
            visual_row = visual_row.saturating_add(1);
            if lines.len() >= visible_rows {
                break;
            }
            if is_last_wrap {
                if draft
                    .as_ref()
                    .is_some_and(|d| d.key.model_row_index == row_index)
                {
                    let draft = draft.as_ref().expect("draft");
                    let label = app.annotation_label(&draft.key);
                    push_annotation_block(
                        &mut lines,
                        render_annotation_compose_block(draft, width, theme, label.as_deref()),
                        visible_rows,
                    );
                } else if let Some(key) = AnnotationKey::from_ui_row(row, row_index)
                    && let Some(text) = annotations.get(&key)
                    && draft.as_ref().is_none_or(|d| d.key != key)
                {
                    let label = app.annotation_label(&key);
                    push_annotation_block(
                        &mut lines,
                        render_annotation_saved_block(text, width, theme, label.as_deref()),
                        visible_rows,
                    );
                }
            }
        }
        row_offset = 0;
        row_index = row_index.saturating_add(1);
    }
    lines.truncate(visible_rows);
    lines
}

fn push_annotation_block(
    lines: &mut Vec<Line<'static>>,
    block: Vec<Line<'static>>,
    visible_rows: usize,
) {
    for line in block {
        if lines.len() >= visible_rows {
            break;
        }
        lines.push(line);
    }
}

fn mouse_line_highlight_for_viewport(app: &DiffApp) -> Option<usize> {
    if app.diff_modal_blocks_mouse_hover() {
        return None;
    }
    app.diff_mouse_highlight_visual_row()
}

fn row_has_diff_code_content(row: UiRow) -> bool {
    matches!(
        row,
        UiRow::UnifiedLine { .. }
            | UiRow::MetaLine { .. }
            | UiRow::SplitLine { .. }
            | UiRow::ContextLine { .. }
    )
}

pub(crate) fn render_row(
    app: &mut DiffApp,
    row_index: usize,
    row: UiRow,
    width: usize,
) -> Line<'static> {
    render_row_with_focus(app, row_index, row, width, None)
}

pub(crate) fn render_row_wrapped_with_focus(
    app: &mut DiffApp,
    row_index: usize,
    row: UiRow,
    width: usize,
    focused_hunk: Option<(usize, usize)>,
) -> Vec<Line<'static>> {
    let theme = app.theme;
    let hunk_focused = row
        .hunk_key()
        .is_some_and(|hunk_key| Some(hunk_key) == focused_hunk);

    match row {
        UiRow::ContextLine {
            file,
            old_line,
            new_line,
        } => render_context_line_wrapped(app, file, old_line, new_line, row_index, width),
        UiRow::UnifiedLine { file, hunk, line } => {
            let kind = app.changeset.files[file].hunks[hunk].lines[line].kind;
            let syntax =
                unified_syntax_side(kind).and_then(|side| app.syntax_line(file, hunk, line, side));
            let inline = app.inline_ranges(file, hunk, line);
            let diff_line = &app.changeset.files[file].hunks[hunk].lines[line];
            render_unified_line_wrapped_with_focus(
                diff_line,
                syntax.as_ref(),
                &inline,
                width,
                theme,
                hunk_focused,
                &app.grep_filter,
            )
        }
        UiRow::MetaLine { file, hunk, line } => {
            let diff_line = &app.changeset.files[file].hunks[hunk].lines[line];
            render_unified_line_wrapped_with_focus(
                diff_line,
                None,
                &[],
                width,
                theme,
                hunk_focused,
                &app.grep_filter,
            )
        }
        UiRow::SplitLine {
            file,
            hunk,
            left,
            right,
        } => render_split_line_wrapped_with_focus(
            app,
            SplitLineRender {
                file,
                hunk,
                left,
                right,
                row_index,
                width,
                focused: hunk_focused,
            },
        ),
        _ => vec![render_row_with_focus(
            app,
            row_index,
            row,
            width,
            focused_hunk,
        )],
    }
}

pub(crate) fn render_row_with_focus(
    app: &mut DiffApp,
    row_index: usize,
    row: UiRow,
    width: usize,
    focused_hunk: Option<(usize, usize)>,
) -> Line<'static> {
    let theme = app.theme;
    let horizontal_scroll = app.horizontal_scroll;
    let hunk_focused = row
        .hunk_key()
        .is_some_and(|hunk_key| Some(hunk_key) == focused_hunk);
    let mut line = match row {
        UiRow::FileSeparator => file_separator_line(app.layout, width, theme),
        UiRow::FileHeader(file_index) => {
            let file = &app.changeset.files[file_index];
            file_header_line(file, width, theme)
        }
        UiRow::BinaryFile(file_index) => {
            let file = &app.changeset.files[file_index];
            let message = if file.is_binary {
                "binary file"
            } else {
                "no textual changes"
            };
            Line::from(Span::styled(
                fit_padded(&format!("  {message}"), width),
                Style::default().fg(theme.muted),
            ))
        }
        UiRow::Collapsed {
            lines, expanded, ..
        } => context_show_line(app.context_expand_count(lines), expanded > 0, width, theme),
        UiRow::ContextLine {
            file,
            old_line,
            new_line,
        } => render_context_line(app, file, old_line, new_line, row_index, width),
        UiRow::ContextHide { lines, .. } => context_hide_line(lines, width, theme),
        UiRow::HunkHeader { file, hunk } => {
            let hunk = &app.changeset.files[file].hunks[hunk];
            if hunk_focused {
                hunk_header_line_with_focus(hunk, width, theme, true)
            } else {
                hunk_header_line(hunk, width, theme)
            }
        }
        UiRow::UnifiedLine { file, hunk, line } => {
            let kind = app.changeset.files[file].hunks[hunk].lines[line].kind;
            let syntax =
                unified_syntax_side(kind).and_then(|side| app.syntax_line(file, hunk, line, side));
            let inline = app.inline_ranges(file, hunk, line);
            let diff_line = &app.changeset.files[file].hunks[hunk].lines[line];
            render_unified_line_at_scroll_with_focus(
                diff_line,
                syntax.as_ref(),
                &inline,
                width,
                theme,
                horizontal_scroll,
                hunk_focused,
            )
        }
        UiRow::MetaLine { file, hunk, line } => {
            let diff_line = &app.changeset.files[file].hunks[hunk].lines[line];
            render_unified_line_at_scroll_with_focus(
                diff_line,
                None,
                &[],
                width,
                theme,
                horizontal_scroll,
                hunk_focused,
            )
        }
        UiRow::SplitLine {
            file,
            hunk,
            left,
            right,
        } => render_split_line_with_focus(
            app,
            SplitLineRender {
                file,
                hunk,
                left,
                right,
                row_index,
                width,
                focused: hunk_focused,
            },
        ),
    };

    if !app.grep_filter.is_empty() {
        let targets = grep_highlight_targets_for_row(app, row, &line, width);
        line = highlighted_grep_text_line(line, &app.grep_filter, targets, theme);
    }
    line
}

pub(crate) fn context_show_line(
    lines: usize,
    more: bool,
    width: usize,
    theme: DiffTheme,
) -> Line<'static> {
    if width == 0 {
        return Line::default();
    }

    let suffix = if lines == 1 { "line" } else { "lines" };
    let label = if more {
        format!(" ▾ show {} more {suffix}", format_count(lines))
    } else {
        format!(" ▾ show {} {suffix}", format_count(lines))
    };
    context_action_line(&label, width, theme, theme.muted)
}

pub(crate) fn context_hide_line(lines: usize, width: usize, theme: DiffTheme) -> Line<'static> {
    let suffix = if lines == 1 { "line" } else { "lines" };
    context_action_line(
        &format!(" ▴ hide {} {suffix}", format_count(lines)),
        width,
        theme,
        theme.muted,
    )
}

pub(crate) fn context_action_line(
    label: &str,
    width: usize,
    theme: DiffTheme,
    text_color: Color,
) -> Line<'static> {
    if width == 0 {
        return Line::default();
    }

    let bg = base_bg(theme);
    let mut spans = Vec::new();
    let indicator_width = 1.min(width);
    if indicator_width > 0 {
        spans.push(diff_indicator_span(DiffLineKind::Meta, theme));
    }
    let content_width = width.saturating_sub(indicator_width);
    if content_width > 0 {
        spans.push(Span::styled(
            fit_padded(label, content_width),
            Style::default().fg(text_color).bg(bg),
        ));
    }
    Line::from(spans)
}

pub(crate) fn render_context_line(
    app: &mut DiffApp,
    file: usize,
    old_line: usize,
    new_line: usize,
    row_index: usize,
    width: usize,
) -> Line<'static> {
    let theme = app.theme;
    let horizontal_scroll = app.horizontal_scroll;
    let side = app.context_source_side(file);
    let syntax = side.and_then(|side| {
        let line_number = match side {
            DiffSide::Old => old_line,
            DiffSide::New => new_line,
        };
        app.syntax_file_line(file, side, line_number)
    });
    let diff_line = DiffLine {
        kind: DiffLineKind::Context,
        old_line: Some(old_line),
        new_line: Some(new_line),
        text: app.context_line_text(file, old_line, new_line),
    };

    match app.layout {
        DiffLayoutMode::Unified => render_unified_line_at_scroll(
            &diff_line,
            syntax.as_ref(),
            &[],
            row_index,
            width,
            theme,
            horizontal_scroll,
        ),
        DiffLayoutMode::Split => render_split_context_line(
            &diff_line,
            syntax.as_ref(),
            row_index,
            width,
            theme,
            horizontal_scroll,
        ),
    }
}

pub(crate) fn render_context_line_wrapped(
    app: &mut DiffApp,
    file: usize,
    old_line: usize,
    new_line: usize,
    row_index: usize,
    width: usize,
) -> Vec<Line<'static>> {
    let theme = app.theme;
    let side = app.context_source_side(file);
    let syntax = side.and_then(|side| {
        let line_number = match side {
            DiffSide::Old => old_line,
            DiffSide::New => new_line,
        };
        app.syntax_file_line(file, side, line_number)
    });
    let diff_line = DiffLine {
        kind: DiffLineKind::Context,
        old_line: Some(old_line),
        new_line: Some(new_line),
        text: app.context_line_text(file, old_line, new_line),
    };

    match app.layout {
        DiffLayoutMode::Unified => render_unified_line_wrapped_with_focus(
            &diff_line,
            syntax.as_ref(),
            &[],
            width,
            theme,
            false,
            &app.grep_filter,
        ),
        DiffLayoutMode::Split => {
            let visual_row_start = app.wrapped_visual_scroll_for_model_row(row_index);
            render_split_context_line_wrapped(
                &diff_line,
                syntax.as_ref(),
                visual_row_start,
                width,
                theme,
                &app.grep_filter,
            )
        }
    }
}

pub(crate) fn render_split_context_line(
    line: &DiffLine,
    syntax: Option<&HighlightedLine>,
    row_index: usize,
    width: usize,
    theme: DiffTheme,
    horizontal_scroll: usize,
) -> Line<'static> {
    if width == 0 {
        return Line::default();
    }

    let left_width = width / 2;
    let right_width = width.saturating_sub(left_width);
    let mut spans = split_cell_spans_at_scroll(
        Some(line),
        syntax,
        &[],
        SplitCellRender {
            side: SplitSide::Old,
            row_index,
            width: left_width,
            theme,
        },
        horizontal_scroll,
    );
    spans.extend(split_cell_spans_at_scroll(
        Some(line),
        syntax,
        &[],
        SplitCellRender {
            side: SplitSide::New,
            row_index,
            width: right_width,
            theme,
        },
        horizontal_scroll,
    ));
    Line::from(spans)
}

pub(crate) fn render_split_context_line_wrapped(
    line: &DiffLine,
    syntax: Option<&HighlightedLine>,
    row_index: usize,
    width: usize,
    theme: DiffTheme,
    grep_filter: &str,
) -> Vec<Line<'static>> {
    if width == 0 {
        return vec![Line::default()];
    }

    let left_width = width / 2;
    let right_width = width.saturating_sub(left_width);
    let left_content_width = split_cell_content_width(left_width);
    let right_content_width = split_cell_content_width(right_width);
    let left_scrolls = wrapped_line_start_columns(&line.text, left_content_width);
    let right_scrolls = wrapped_line_start_columns(&line.text, right_content_width);
    let text_width = line.text.width();
    let rows = left_scrolls.len().max(right_scrolls.len());
    let mut lines = Vec::with_capacity(rows);
    for wrap_index in 0..rows {
        let left_scroll = wrapped_segment_scroll(&left_scrolls, text_width, wrap_index);
        let right_scroll = wrapped_segment_scroll(&right_scrolls, text_width, wrap_index);
        let visual_row = row_index.saturating_add(wrap_index);
        let mut spans = split_cell_spans_at_scroll_with_focus_and_continuation(
            Some(line),
            syntax,
            &[],
            SplitCellRender {
                side: SplitSide::Old,
                row_index: visual_row,
                width: left_width,
                theme,
            },
            left_scroll,
            false,
            wrap_index > 0,
        );
        spans.extend(split_cell_spans_at_scroll_with_focus_and_continuation(
            Some(line),
            syntax,
            &[],
            SplitCellRender {
                side: SplitSide::New,
                row_index: visual_row,
                width: right_width,
                theme,
            },
            right_scroll,
            false,
            wrap_index > 0,
        ));
        lines.push(highlight_wrapped_split_grep_line(
            Line::from(spans),
            Some(line),
            Some(line),
            SplitGrepRender {
                query: grep_filter,
                width,
                left_scroll,
                right_scroll,
                theme,
            },
        ));
    }
    lines
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

#[derive(Debug, Clone, Copy)]
struct UnifiedLineRender {
    width: usize,
    theme: DiffTheme,
    horizontal_scroll: usize,
    focused: bool,
    continuation: bool,
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

pub(crate) fn diff_indicator_span_for_focus(
    kind: DiffLineKind,
    theme: DiffTheme,
    focused: bool,
) -> Span<'static> {
    if focused {
        focused_diff_indicator_span(kind, theme)
    } else {
        diff_indicator_span(kind, theme)
    }
}

fn unified_gutter_text(old_line: Option<usize>, new_line: Option<usize>) -> String {
    let mut gutter = String::with_capacity(UNIFIED_GUTTER_WIDTH);
    push_right_aligned_number(&mut gutter, old_line, 5);
    gutter.push(' ');
    push_right_aligned_number(&mut gutter, new_line, 5);
    gutter.push(' ');
    gutter
}

fn split_gutter_text(line: Option<usize>) -> String {
    let mut gutter = String::with_capacity(GUTTER_WIDTH.saturating_sub(1));
    push_right_aligned_number(&mut gutter, line, 5);
    gutter.push(' ');
    gutter
}

fn push_right_aligned_number(out: &mut String, line: Option<usize>, width: usize) {
    let Some(mut value) = line else {
        out.extend(std::iter::repeat_n(' ', width));
        return;
    };

    let mut digits = [0u8; 39];
    let mut len = 0usize;
    loop {
        digits[digits.len() - 1 - len] = b'0' + (value % 10) as u8;
        len += 1;
        value /= 10;
        if value == 0 {
            break;
        }
    }

    if len < width {
        out.extend(std::iter::repeat_n(' ', width - len));
    }
    for digit in &digits[digits.len() - len..] {
        out.push(*digit as char);
    }
}

pub(crate) fn gutter_spans(
    body: &str,
    sign: &str,
    width: usize,
    kind: DiffLineKind,
    theme: DiffTheme,
) -> Vec<Span<'static>> {
    if width == 0 {
        return Vec::new();
    }

    let body_style = Style::default()
        .fg(line_gutter_fg(kind, theme))
        .bg(line_gutter_bg(kind, theme));
    if sign.trim().is_empty() || width == 1 {
        return vec![Span::styled(
            fit_padded(&format!("{body}{sign}"), width),
            body_style,
        )];
    }

    let sign_width = 1;
    let body_width = width.saturating_sub(sign_width);
    vec![
        Span::styled(fit_padded(body, body_width), body_style),
        Span::styled(fit(sign, sign_width), diff_sign_style(kind, theme)),
    ]
}

pub(crate) fn empty_diff_fill_from(width: usize, row_index: usize, column_offset: usize) -> String {
    let mut fill = String::with_capacity(width.saturating_mul(EMPTY_DIFF_FILL.len_utf8()));
    for column in 0..width {
        fill.push(
            if (column + column_offset + row_index) % EMPTY_DIFF_FILL_SPACING == 0 {
                EMPTY_DIFF_FILL
            } else {
                ' '
            },
        );
    }
    fill
}

pub(crate) fn content_spans_at_scroll(
    text: &str,
    syntax: Option<&HighlightedLine>,
    inline: &[InlineRange],
    kind: DiffLineKind,
    width: usize,
    theme: DiffTheme,
    horizontal_scroll: usize,
) -> Vec<Span<'static>> {
    if width == 0 {
        return Vec::new();
    }

    let valid_inline;
    let inline = if inline.is_empty() {
        &[][..]
    } else {
        valid_inline = valid_inline_ranges(text, inline);
        valid_inline.as_slice()
    };
    let syntax = syntax.filter(|syntax| syntax_line_matches_text(syntax, text));
    if syntax.is_none() && inline.is_empty() {
        return vec![Span::styled(
            fit_padded_from(text, horizontal_scroll, width),
            line_style(kind, theme),
        )];
    }

    let span_capacity = syntax.map_or(1, |syntax| syntax.segments.len()) + inline.len() * 2 + 1;
    let mut writer =
        ContentSpanWriter::new(inline, kind, width, theme, horizontal_scroll, span_capacity);

    if let Some(syntax) = syntax {
        let mut byte_start = 0usize;
        for segment in &syntax.segments {
            if !writer.push_segment(
                &segment.text,
                byte_start,
                syntax_style(segment.class, kind, theme),
            ) {
                break;
            }
            byte_start += segment.text.len();
        }
    } else {
        writer.push_segment(text, 0, line_style(kind, theme));
    }

    writer.finish()
}

pub(crate) fn valid_inline_ranges(text: &str, ranges: &[InlineRange]) -> Vec<InlineRange> {
    if ranges.is_empty() {
        return Vec::new();
    }

    let mut valid = Vec::with_capacity(ranges.len());
    for range in ranges {
        let byte_start = range.byte_start.min(text.len());
        let byte_end = range.byte_end.min(text.len());
        if byte_start < byte_end
            && text.is_char_boundary(byte_start)
            && text.is_char_boundary(byte_end)
        {
            valid.push(InlineRange {
                byte_start,
                byte_end,
            });
        }
    }
    if valid.len() > 1 {
        valid.sort_by_key(|range| (range.byte_start, range.byte_end));
    }
    valid
}

pub(crate) struct ContentSpanWriter<'a> {
    spans: Vec<Span<'static>>,
    inline: &'a [InlineRange],
    kind: DiffLineKind,
    width: usize,
    skip: usize,
    used: usize,
    theme: DiffTheme,
}

impl<'a> ContentSpanWriter<'a> {
    pub(crate) fn new(
        inline: &'a [InlineRange],
        kind: DiffLineKind,
        width: usize,
        theme: DiffTheme,
        horizontal_scroll: usize,
        span_capacity: usize,
    ) -> Self {
        Self {
            spans: Vec::with_capacity(span_capacity),
            inline,
            kind,
            width,
            skip: horizontal_scroll,
            used: 0,
            theme,
        }
    }

    pub(crate) fn push_segment(
        &mut self,
        segment_text: &str,
        segment_byte_start: usize,
        style: Style,
    ) -> bool {
        let segment_byte_end = segment_byte_start + segment_text.len();
        let mut cursor = segment_byte_start;

        for range in self.inline {
            if self.used >= self.width {
                return false;
            }
            if range.byte_end <= cursor {
                continue;
            }
            if range.byte_start >= segment_byte_end {
                break;
            }

            let normal_end = range.byte_start.min(segment_byte_end);
            if !self.push_piece(segment_text, segment_byte_start, cursor, normal_end, style) {
                return false;
            }

            let inline_start = range.byte_start.max(cursor).min(segment_byte_end);
            let inline_end = range.byte_end.min(segment_byte_end);
            if !self.push_piece(
                segment_text,
                segment_byte_start,
                inline_start,
                inline_end,
                inline_style(style, self.kind, self.theme),
            ) {
                return false;
            }
            cursor = inline_end;
        }

        self.push_piece(
            segment_text,
            segment_byte_start,
            cursor,
            segment_byte_end,
            style,
        )
    }

    pub(crate) fn push_piece(
        &mut self,
        segment_text: &str,
        segment_byte_start: usize,
        byte_start: usize,
        byte_end: usize,
        style: Style,
    ) -> bool {
        if byte_start >= byte_end {
            return true;
        }
        let remaining = self.width.saturating_sub(self.used);
        if remaining == 0 {
            return false;
        }

        let relative_start = byte_start.saturating_sub(segment_byte_start);
        let relative_end = byte_end.saturating_sub(segment_byte_start);
        let mut piece = &segment_text[relative_start..relative_end];
        if self.skip > 0 {
            let (visible, skipped) = skip_display_prefix(piece, self.skip);
            self.skip = self.skip.saturating_sub(skipped);
            piece = visible;
            if piece.is_empty() {
                return true;
            }
        }
        let (fitted, fitted_width, complete) = fit_with_width(piece, remaining);
        if fitted.is_empty() {
            return false;
        }

        self.used += fitted_width;
        self.spans.push(Span::styled(fitted, style));
        complete
    }

    pub(crate) fn finish(mut self) -> Vec<Span<'static>> {
        if self.used < self.width {
            self.spans.push(Span::styled(
                spaces(self.width - self.used),
                line_style(self.kind, self.theme),
            ));
        }
        self.spans
    }
}

pub(crate) fn syntax_line_matches_text(syntax: &HighlightedLine, text: &str) -> bool {
    let mut remaining = text;
    for segment in &syntax.segments {
        if !remaining.starts_with(&segment.text) {
            return false;
        }
        remaining = &remaining[segment.text.len()..];
    }
    remaining.is_empty()
}

pub(crate) fn syntax_style(
    class: Option<SyntaxClass>,
    kind: DiffLineKind,
    theme: DiffTheme,
) -> Style {
    let mut style = line_style(kind, theme);
    if let Some(color) = class.and_then(|class| syntax_fg(class, theme)) {
        style = style.fg(color);
    }
    style
}

pub(crate) fn inline_style(style: Style, kind: DiffLineKind, theme: DiffTheme) -> Style {
    if theme.transparent_background || theme.diff.inline_background == DiffBackground::None {
        return match kind {
            DiffLineKind::Addition | DiffLineKind::Deletion => style.add_modifier(Modifier::BOLD),
            DiffLineKind::Context | DiffLineKind::Meta => style,
        };
    }

    match kind {
        DiffLineKind::Addition => style
            .bg(inline_bg(kind, theme))
            .add_modifier(Modifier::BOLD),
        DiffLineKind::Deletion => style
            .bg(inline_bg(kind, theme))
            .add_modifier(Modifier::BOLD),
        DiffLineKind::Context | DiffLineKind::Meta => style,
    }
}

pub(crate) fn inline_bg(kind: DiffLineKind, theme: DiffTheme) -> Color {
    match (theme.diff.inline_background, kind) {
        (DiffBackground::Subtle, DiffLineKind::Addition) => theme.addition_bg,
        (DiffBackground::Subtle, DiffLineKind::Deletion) => theme.deletion_bg,
        (_, DiffLineKind::Addition) => theme.addition_inline_bg,
        (_, DiffLineKind::Deletion) => theme.deletion_inline_bg,
        _ => Color::Reset,
    }
}

pub(crate) fn syntax_fg(class: SyntaxClass, theme: DiffTheme) -> Option<Color> {
    theme.syntax.color(class)
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SplitLineRender {
    pub(crate) file: usize,
    pub(crate) hunk: usize,
    pub(crate) left: Option<usize>,
    pub(crate) right: Option<usize>,
    pub(crate) row_index: usize,
    pub(crate) width: usize,
    pub(crate) focused: bool,
}

pub(crate) fn render_split_line_with_focus(
    app: &mut DiffApp,
    render: SplitLineRender,
) -> Line<'static> {
    let SplitLineRender {
        file,
        hunk,
        left,
        right,
        row_index,
        width,
        focused,
    } = render;
    if width == 0 {
        return Line::default();
    }
    let theme = app.theme;
    let horizontal_scroll = app.horizontal_scroll;

    let left_syntax = left.and_then(|index| app.syntax_line(file, hunk, index, DiffSide::Old));
    let right_syntax = right.and_then(|index| app.syntax_line(file, hunk, index, DiffSide::New));
    let left_inline = left
        .map(|index| app.inline_ranges(file, hunk, index))
        .unwrap_or_default();
    let right_inline = right
        .map(|index| app.inline_ranges(file, hunk, index))
        .unwrap_or_default();

    let left_width = width / 2;
    let right_width = width.saturating_sub(left_width);
    let lines = &app.changeset.files[file].hunks[hunk].lines;
    let left_line = left.and_then(|index| lines.get(index));
    let right_line = right.and_then(|index| lines.get(index));
    let mut spans = split_cell_spans_at_scroll_with_focus(
        left_line,
        left_syntax.as_ref(),
        &left_inline,
        SplitCellRender {
            side: SplitSide::Old,
            row_index,
            width: left_width,
            theme,
        },
        horizontal_scroll,
        focused,
    );
    spans.extend(split_cell_spans_at_scroll_with_focus(
        right_line,
        right_syntax.as_ref(),
        &right_inline,
        SplitCellRender {
            side: SplitSide::New,
            row_index,
            width: right_width,
            theme,
        },
        horizontal_scroll,
        focused,
    ));
    Line::from(spans)
}

pub(crate) fn render_split_line_wrapped_with_focus(
    app: &mut DiffApp,
    render: SplitLineRender,
) -> Vec<Line<'static>> {
    let SplitLineRender {
        file,
        hunk,
        left,
        right,
        row_index,
        width,
        focused,
    } = render;
    if width == 0 {
        return vec![Line::default()];
    }
    let theme = app.theme;

    let left_syntax = left.and_then(|index| app.syntax_line(file, hunk, index, DiffSide::Old));
    let right_syntax = right.and_then(|index| app.syntax_line(file, hunk, index, DiffSide::New));
    let left_inline = left
        .map(|index| app.inline_ranges(file, hunk, index))
        .unwrap_or_default();
    let right_inline = right
        .map(|index| app.inline_ranges(file, hunk, index))
        .unwrap_or_default();

    let left_width = width / 2;
    let right_width = width.saturating_sub(left_width);
    let lines = &app.changeset.files[file].hunks[hunk].lines;
    let left_line = left.and_then(|index| lines.get(index));
    let right_line = right.and_then(|index| lines.get(index));
    let left_content_width = split_cell_content_width(left_width);
    let right_content_width = split_cell_content_width(right_width);
    let left_scrolls = left_line
        .map(|line| wrapped_line_start_columns(&line.text, left_content_width))
        .unwrap_or_else(|| vec![0]);
    let right_scrolls = right_line
        .map(|line| wrapped_line_start_columns(&line.text, right_content_width))
        .unwrap_or_else(|| vec![0]);
    let left_text_width = left_line.map(|line| line.text.width()).unwrap_or(0);
    let right_text_width = right_line.map(|line| line.text.width()).unwrap_or(0);
    let rows = left_scrolls.len().max(right_scrolls.len()).max(1);
    let visual_row_start = app.wrapped_visual_scroll_for_model_row(row_index);
    let mut rendered_lines = Vec::with_capacity(rows);
    for wrap_index in 0..rows {
        let left_scroll = wrapped_segment_scroll(&left_scrolls, left_text_width, wrap_index);
        let right_scroll = wrapped_segment_scroll(&right_scrolls, right_text_width, wrap_index);
        let visual_row = visual_row_start.saturating_add(wrap_index);
        let mut spans = split_cell_spans_at_scroll_with_focus_and_continuation(
            left_line,
            left_syntax.as_ref(),
            &left_inline,
            SplitCellRender {
                side: SplitSide::Old,
                row_index: visual_row,
                width: left_width,
                theme,
            },
            left_scroll,
            focused,
            wrap_index > 0,
        );
        spans.extend(split_cell_spans_at_scroll_with_focus_and_continuation(
            right_line,
            right_syntax.as_ref(),
            &right_inline,
            SplitCellRender {
                side: SplitSide::New,
                row_index: visual_row,
                width: right_width,
                theme,
            },
            right_scroll,
            focused,
            wrap_index > 0,
        ));
        let line = Line::from(spans);
        rendered_lines.push(highlight_wrapped_split_grep_line(
            line,
            left_line,
            right_line,
            SplitGrepRender {
                query: &app.grep_filter,
                width,
                left_scroll,
                right_scroll,
                theme,
            },
        ));
    }
    rendered_lines
}

fn wrapped_segment_scroll(starts: &[usize], text_width: usize, wrap_index: usize) -> usize {
    starts.get(wrap_index).copied().unwrap_or(text_width)
}

#[derive(Debug, Clone, Copy)]
struct SplitGrepRender<'a> {
    query: &'a str,
    width: usize,
    left_scroll: usize,
    right_scroll: usize,
    theme: DiffTheme,
}

fn highlight_wrapped_split_grep_line(
    rendered: Line<'static>,
    left_line: Option<&DiffLine>,
    right_line: Option<&DiffLine>,
    render: SplitGrepRender<'_>,
) -> Line<'static> {
    let SplitGrepRender {
        query,
        width,
        left_scroll,
        right_scroll,
        theme,
    } = render;

    if query.is_empty() {
        return rendered;
    }

    let left_width = width / 2;
    let right_width = width.saturating_sub(left_width);
    let mut targets = Vec::with_capacity(2);
    if let Some(target) = left_line.and_then(|line| {
        split_diff_line_grep_highlight_target(line, &rendered.spans, 0, left_width, left_scroll)
    }) {
        targets.push(target);
    }
    if let Some(target) = right_line.and_then(|line| {
        split_diff_line_grep_highlight_target(
            line,
            &rendered.spans,
            left_width,
            right_width,
            right_scroll,
        )
    }) {
        targets.push(target);
    }

    highlighted_grep_text_line(rendered, query, targets, theme)
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum SplitSide {
    Old,
    New,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SplitCellRender {
    pub(crate) side: SplitSide,
    pub(crate) row_index: usize,
    pub(crate) width: usize,
    pub(crate) theme: DiffTheme,
}

pub(crate) fn split_cell_spans_at_scroll(
    line: Option<&DiffLine>,
    syntax: Option<&HighlightedLine>,
    inline: &[InlineRange],
    render: SplitCellRender,
    horizontal_scroll: usize,
) -> Vec<Span<'static>> {
    split_cell_spans_at_scroll_with_focus(line, syntax, inline, render, horizontal_scroll, false)
}

pub(crate) fn split_cell_spans_at_scroll_with_focus(
    line: Option<&DiffLine>,
    syntax: Option<&HighlightedLine>,
    inline: &[InlineRange],
    render: SplitCellRender,
    horizontal_scroll: usize,
    focused: bool,
) -> Vec<Span<'static>> {
    split_cell_spans_at_scroll_with_focus_and_continuation(
        line,
        syntax,
        inline,
        render,
        horizontal_scroll,
        focused,
        false,
    )
}

fn split_cell_spans_at_scroll_with_focus_and_continuation(
    line: Option<&DiffLine>,
    syntax: Option<&HighlightedLine>,
    inline: &[InlineRange],
    render: SplitCellRender,
    horizontal_scroll: usize,
    focused: bool,
    continuation: bool,
) -> Vec<Span<'static>> {
    let SplitCellRender {
        side,
        row_index,
        width,
        theme,
    } = render;

    if width == 0 {
        return Vec::new();
    }

    let Some(line) = line else {
        let empty_kind = DiffLineKind::Context;
        let indicator_width = 1.min(width);
        let gutter_width = GUTTER_WIDTH.min(width.saturating_sub(indicator_width));
        let content_width = split_cell_content_width(width);
        let mut spans = Vec::new();
        if indicator_width > 0 {
            spans.push(diff_indicator_span_for_focus(empty_kind, theme, focused));
        }
        if gutter_width > 0 {
            spans.push(Span::styled(
                spaces(gutter_width),
                Style::default().bg(line_gutter_bg(empty_kind, theme)),
            ));
        }
        if content_width > 0 {
            spans.push(Span::styled(
                empty_diff_fill_from(
                    content_width,
                    row_index,
                    indicator_width + gutter_width + horizontal_scroll,
                ),
                Style::default().fg(theme.empty_diff).bg(base_bg(theme)),
            ));
        }
        return spans;
    };

    let indicator_width = 1.min(width);
    let gutter_width = GUTTER_WIDTH.min(width.saturating_sub(indicator_width));
    let content_width = split_cell_content_width(width);
    let line_number = if continuation {
        None
    } else {
        match side {
            SplitSide::Old => line.old_line,
            SplitSide::New => line.new_line,
        }
    };
    let sign = if continuation {
        " "
    } else {
        match (side, line.kind) {
            (SplitSide::Old, DiffLineKind::Deletion) => "-",
            (SplitSide::New, DiffLineKind::Addition) => "+",
            _ => " ",
        }
    };

    let mut spans = Vec::new();
    if indicator_width > 0 {
        spans.push(diff_indicator_span_for_focus(line.kind, theme, focused));
    }
    if gutter_width > 0 {
        spans.extend(gutter_spans(
            &split_gutter_text(line_number),
            sign,
            gutter_width,
            line.kind,
            theme,
        ));
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
    spans
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
