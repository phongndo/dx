use ratatui::{
    Frame,
    layout::Rect,
    prelude::{Line, Span, Style, Text},
    widgets::Paragraph,
};

use crate::{
    annotation::AnnotationKey,
    app::DiffApp,
    model::UiRow,
    render::{
        annotations::{
            append_annotation_add_button, render_annotation_compose_block,
            render_annotation_saved_block,
        },
        grep::{
            grep_highlight_targets_for_row, highlighted_grep_text_line,
            highlighted_mouse_diff_content_line,
        },
        headers::{
            file_header_line, file_separator_line, hunk_header_line, hunk_header_line_with_focus,
        },
        style::base_bg,
        text::fit_padded,
    },
    syntax::unified_syntax_side,
};

mod content;
mod context;
mod split;
mod unified;
pub(crate) use content::{
    content_spans_at_scroll, diff_indicator_span_for_focus, empty_diff_fill_from, gutter_spans,
};
#[cfg(test)]
pub(crate) use content::{inline_bg, syntax_fg};
use content::{split_gutter_text, unified_gutter_text};
#[cfg(test)]
pub(crate) use context::render_split_context_line_wrapped;
pub(crate) use context::{
    context_expand_marker, context_hide_line, context_hide_marker, context_show_line,
    render_context_line, render_context_line_wrapped,
};
#[cfg(test)]
pub(crate) use split::{SplitCellRender, SplitSide, split_cell_spans_at_scroll};
pub(crate) use split::{
    SplitLineRender, render_split_line_with_focus, render_split_line_wrapped_with_focus,
};
pub(crate) use unified::{
    line_style, render_unified_line_at_scroll_with_focus, render_unified_line_wrapped_with_focus,
};
#[cfg(test)]
pub(crate) use unified::{render_unified_line_at_scroll, row_bg};

pub(crate) fn draw_diff(frame: &mut Frame<'_>, app: &mut DiffApp, area: Rect) {
    if app.document.model.is_empty() {
        let message = if app.filters.active() && !app.document.base_changeset.files.is_empty() {
            "No files match filters."
        } else {
            "No changes."
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                message,
                Style::default().fg(app.config.theme.muted),
            )))
            .style(Style::default().bg(base_bg(app.config.theme))),
            area,
        );
        return;
    }

    let visible_rows = area.height as usize;
    app.prepare_syntax_for_viewport(visible_rows);
    let width = area.width as usize;
    let lines = build_diff_viewport_lines(app, width, visible_rows);
    frame.render_widget(
        Paragraph::new(Text::from(lines)).style(Style::default().bg(base_bg(app.config.theme))),
        area,
    );
}

pub(crate) fn build_diff_viewport_lines(
    app: &mut DiffApp,
    width: usize,
    visible_rows: usize,
) -> Vec<Line<'static>> {
    if app.viewport.line_wrapping {
        return build_wrapped_viewport_lines(app, width, visible_rows);
    }

    let mouse_highlight = mouse_highlight_for_viewport(app);
    let theme = app.config.theme;
    let layout = app.viewport.layout;
    let draft = app.annotations_state.annotation_draft.clone();
    let annotations = app.annotations_state.annotations.clone();
    let focused_hunk = app.focused_hunk_for_viewport(visible_rows);
    let mut lines = Vec::with_capacity(visible_rows);

    for offset in 0..visible_rows {
        if lines.len() >= visible_rows {
            break;
        }
        let visual_row = app.viewport.scroll.saturating_add(offset);
        let Some(row) = app.document.model.row(visual_row) else {
            break;
        };
        let mut line = render_row_with_focus(app, visual_row, row, width, focused_hunk);
        if mouse_highlight.is_some_and(|(_, highlight_row)| highlight_row == visual_row)
            && row_has_diff_code_content(row)
            && draft.is_none()
        {
            line = highlighted_mouse_diff_content_line(line, layout, width, theme);
            if row_has_annotation_target(app, row) {
                line = append_annotation_add_button(line, width, theme);
            }
        }
        lines.push(line);

        for key in AnnotationKey::candidates_from_ui_row(&app.document.changeset, row) {
            if let Some(draft) = draft
                .as_ref()
                .filter(|d| d.model_row_index == visual_row && d.key == key)
            {
                let label = app.annotation_label(&draft.key);
                push_annotation_block(
                    &mut lines,
                    render_annotation_compose_block(draft, width, theme, label.as_deref()),
                    visible_rows,
                );
            } else if let Some(text) = annotations.get(&key)
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

    lines.truncate(visible_rows);
    lines
}

fn build_wrapped_viewport_lines(
    app: &mut DiffApp,
    width: usize,
    visible_rows: usize,
) -> Vec<Line<'static>> {
    let mouse_highlight = mouse_highlight_for_viewport(app);
    let theme = app.config.theme;
    let layout = app.viewport.layout;
    let draft = app.annotations_state.annotation_draft.clone();
    let annotations = app.annotations_state.annotations.clone();
    let focused_hunk = app.focused_hunk_for_viewport(visible_rows);
    let mut lines = Vec::with_capacity(visible_rows);
    let Some((mut row_index, mut row_offset)) = app.model_row_at_scroll(app.viewport.scroll) else {
        return lines;
    };
    let mut visual_row = app.viewport.scroll;
    while lines.len() < visible_rows {
        let Some(row) = app.document.model.row(row_index) else {
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
            if mouse_highlight.is_some_and(|(_, highlight_row)| highlight_row == visual_row)
                && row_has_diff_code_content(row)
                && draft.is_none()
            {
                line = highlighted_mouse_diff_content_line(line, layout, width, theme);
                if visual_row == anchor_visual && row_has_annotation_target(app, row) {
                    line = append_annotation_add_button(line, width, theme);
                }
            }
            lines.push(line);
            visual_row = visual_row.saturating_add(1);
            if lines.len() >= visible_rows {
                break;
            }
            if is_last_wrap {
                for key in AnnotationKey::candidates_from_ui_row(&app.document.changeset, row) {
                    if let Some(draft) = draft
                        .as_ref()
                        .filter(|d| d.model_row_index == row_index && d.key == key)
                    {
                        let label = app.annotation_label(&draft.key);
                        push_annotation_block(
                            &mut lines,
                            render_annotation_compose_block(draft, width, theme, label.as_deref()),
                            visible_rows,
                        );
                    } else if let Some(text) = annotations.get(&key)
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

fn mouse_highlight_for_viewport(app: &DiffApp) -> Option<(u16, usize)> {
    if app.diff_modal_blocks_mouse_hover() {
        return None;
    }
    let (column, _) = app.viewport.mouse_hover?;
    app.diff_mouse_highlight_visual_row()
        .map(|visual_row| (column, visual_row))
}

fn row_has_annotation_target(app: &DiffApp, row: UiRow) -> bool {
    AnnotationKey::from_ui_row(&app.document.changeset, row).is_some()
}

fn row_has_diff_code_content(row: UiRow) -> bool {
    matches!(
        row,
        UiRow::UnifiedLine { .. } | UiRow::SplitLine { .. } | UiRow::ContextLine { .. }
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
    let theme = app.config.theme;
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
            let kind = app.document.changeset.files[file].hunks[hunk].lines[line].kind;
            let syntax =
                unified_syntax_side(kind).and_then(|side| app.syntax_line(file, hunk, line, side));
            let inline = app.inline_ranges(file, hunk, line);
            let diff_line = &app.document.changeset.files[file].hunks[hunk].lines[line];
            render_unified_line_wrapped_with_focus(
                diff_line,
                syntax.as_ref(),
                &inline,
                width,
                theme,
                hunk_focused,
                &app.filters.grep_filter,
            )
        }
        UiRow::MetaLine { file, hunk, line } => {
            let diff_line = &app.document.changeset.files[file].hunks[hunk].lines[line];
            render_unified_line_wrapped_with_focus(
                diff_line,
                None,
                &[],
                width,
                theme,
                hunk_focused,
                &app.filters.grep_filter,
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
    let theme = app.config.theme;
    let horizontal_scroll = app.viewport.horizontal_scroll;
    let hunk_focused = row
        .hunk_key()
        .is_some_and(|hunk_key| Some(hunk_key) == focused_hunk);
    let mut line = match row {
        UiRow::FileSeparator => file_separator_line(app.viewport.layout, width, theme),
        UiRow::FileHeader(file_index) => {
            let file = &app.document.changeset.files[file_index];
            file_header_line(file, width, theme)
        }
        UiRow::BinaryFile(file_index) => {
            let file = &app.document.changeset.files[file_index];
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
            hunk,
            lines,
            expanded,
            ..
        } => context_show_line(
            lines,
            expanded > 0,
            context_expand_marker(hunk),
            width,
            theme,
        ),
        UiRow::ContextLine {
            file,
            old_line,
            new_line,
        } => render_context_line(app, file, old_line, new_line, row_index, width),
        UiRow::ContextHide { hunk, lines, .. } => {
            context_hide_line(lines, context_hide_marker(hunk), width, theme)
        }
        UiRow::HunkHeader { file, hunk } => {
            let hunk = &app.document.changeset.files[file].hunks[hunk];
            if hunk_focused {
                hunk_header_line_with_focus(hunk, width, theme, true)
            } else {
                hunk_header_line(hunk, width, theme)
            }
        }
        UiRow::UnifiedLine { file, hunk, line } => {
            let kind = app.document.changeset.files[file].hunks[hunk].lines[line].kind;
            let syntax =
                unified_syntax_side(kind).and_then(|side| app.syntax_line(file, hunk, line, side));
            let inline = app.inline_ranges(file, hunk, line);
            let diff_line = &app.document.changeset.files[file].hunks[hunk].lines[line];
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
            let diff_line = &app.document.changeset.files[file].hunks[hunk].lines[line];
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

    if !app.filters.grep_filter.is_empty() {
        let targets = grep_highlight_targets_for_row(app, row, &line, width);
        line = highlighted_grep_text_line(line, &app.filters.grep_filter, targets, theme);
    }
    line
}
