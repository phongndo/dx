use super::{DiffApp, MarkExport, json_string};
use crate::annotation::{AnnotationKey, AnnotationSide, paired_old_line_for_addition};
use crate::model::{UiModel, UiRow};
use crate::syntax::{DiffSide, available_context_lines};
use std::collections::HashSet;

#[cfg(test)]
use super::write_osc52_clipboard;
#[cfg(test)]
use std::io::Write;

impl DiffApp {
    #[cfg(test)]
    pub(crate) fn copy_marks_to_writer<W: Write>(&mut self, writer: &mut W) {
        let Some(marks) = self.marks_clipboard_json() else {
            self.set_warning_notice("no marks to copy");
            return;
        };

        match write_osc52_clipboard(writer, &marks) {
            Ok(()) => self.set_success_notice("marks copied"),
            Err(error) => self.set_error_log(format!("marks copy failed: {error}")),
        }
    }

    pub(crate) fn marks_clipboard_json(&self) -> Option<String> {
        let mut marks = self.export_marks();
        if marks.is_empty() {
            return None;
        }
        marks.sort_by(|left, right| {
            (&left.path, left.old_line, left.new_line).cmp(&(
                &right.path,
                right.old_line,
                right.new_line,
            ))
        });

        let mut out = String::from("{\n  \"version\": 1,\n  \"marks\": [\n");
        for (index, mark) in marks.iter().enumerate() {
            if index > 0 {
                out.push_str(",\n");
            }
            out.push_str("    {\n");
            out.push_str("      \"path\": ");
            out.push_str(&json_string(&mark.path));
            if let Some(old_line) = mark.old_line {
                out.push_str(",\n      \"old_line\": ");
                out.push_str(&old_line.to_string());
            }
            if let Some(new_line) = mark.new_line {
                out.push_str(",\n      \"new_line\": ");
                out.push_str(&new_line.to_string());
            }
            out.push_str(",\n      \"body\": ");
            out.push_str(&json_string(&mark.body));
            out.push_str("\n    }");
        }
        out.push_str("\n  ]\n}");
        Some(out)
    }

    fn export_marks(&self) -> Vec<MarkExport> {
        // Copy marks for the current diff, not stale annotations whose path still
        // exists after a reload. Build an unfiltered model so active file/grep
        // filters do not hide otherwise-current marks from export.
        let export_model = UiModel::new(
            &self.document.changeset,
            self.viewport.layout,
            &self.document.context_expansions,
        );
        let exportable_keys = self.exportable_annotation_keys(&export_model);
        self.annotations_state
            .annotations
            .iter()
            .filter_map(|(key, body)| {
                if !exportable_keys.contains(key)
                    && !self.collapsed_context_contains_annotation_key(&export_model, key)
                {
                    return None;
                }
                self.export_mark(key, body)
            })
            .collect()
    }

    fn exportable_annotation_keys(&self, model: &UiModel) -> HashSet<AnnotationKey> {
        model
            .rows
            .iter()
            .copied()
            .flat_map(|row| AnnotationKey::candidates_from_ui_row(&self.document.changeset, row))
            .collect()
    }

    fn collapsed_context_contains_annotation_key(
        &self,
        model: &UiModel,
        key: &AnnotationKey,
    ) -> bool {
        if key.side != AnnotationSide::New {
            return false;
        }

        model.rows.iter().any(|row| {
            let UiRow::Collapsed {
                file,
                new_start,
                lines,
                ..
            } = *row
            else {
                return false;
            };
            let Some(file) = self.document.changeset.files.get(file.get()) else {
                return false;
            };
            if AnnotationKey::path_for_side(file, AnnotationSide::New) != Some(key.path.as_str()) {
                return false;
            }

            key.line >= new_start && key.line.saturating_sub(new_start) < lines
        }) || self.trailing_context_contains_annotation_key(key)
    }

    fn trailing_context_contains_annotation_key(&self, key: &AnnotationKey) -> bool {
        // Collapsed trailing context has no UiRow::Collapsed sentinel; derive
        // the hidden range from the final hunk and the available source lines.
        self.document
            .changeset
            .files
            .iter()
            .enumerate()
            .any(|(file_index, file)| {
                if AnnotationKey::path_for_side(file, AnnotationSide::New)
                    != Some(key.path.as_str())
                {
                    return false;
                }
                let Some(last_hunk) = file.hunks().last() else {
                    return false;
                };
                let old_start = last_hunk.old_start().saturating_add(last_hunk.old_count());
                let new_start = last_hunk.new_start().saturating_add(last_hunk.new_count());
                if key.line < new_start {
                    return false;
                }

                let Some((side, source_line_count)) = self.context_source_line_count(file_index)
                else {
                    return false;
                };
                let source_start = match side {
                    DiffSide::Old => old_start,
                    DiffSide::New => new_start,
                };
                let available =
                    available_context_lines(source_start, usize::MAX, source_line_count);

                key.line.saturating_sub(new_start) < available
            })
    }

    fn export_mark(&self, key: &AnnotationKey, body: &str) -> Option<MarkExport> {
        let (old_line, new_line) = self.annotation_key_lines(key)?;
        Some(MarkExport {
            path: key.path.clone(),
            old_line,
            new_line,
            body: body.to_owned(),
        })
    }

    fn annotation_key_lines(&self, key: &AnnotationKey) -> Option<(Option<usize>, Option<usize>)> {
        match key.side {
            AnnotationSide::Old => Some((Some(key.line), None)),
            AnnotationSide::New => {
                Some((self.paired_old_line_for_new_annotation(key), Some(key.line)))
            }
        }
    }

    fn paired_old_line_for_new_annotation(&self, key: &AnnotationKey) -> Option<usize> {
        self.document.changeset.files.iter().find_map(|file| {
            if AnnotationKey::path_for_side(file, AnnotationSide::New) != Some(key.path.as_str()) {
                return None;
            }

            file.hunks().iter().find_map(|hunk| {
                hunk.lines
                    .iter()
                    .enumerate()
                    .find_map(|(line_index, line)| {
                        if line.new_line() == Some(key.line) {
                            paired_old_line_for_addition(&hunk.lines, line_index)
                        } else {
                            None
                        }
                    })
            })
        })
    }
}
