use std::collections::HashMap;

use crate::model::UiRow;

pub(crate) const ANNOTATION_ADD_BUTTON: &str = " [+]";
pub(crate) const ANNOTATION_ADD_BUTTON_WIDTH: usize = 4;
pub(crate) const ANNOTATION_CLOSE_BUTTON: &str = "[x]";
pub(crate) const ANNOTATION_CLOSE_BUTTON_WIDTH: usize = 3;
pub(crate) const ANNOTATION_SUBMIT_BUTTON: &str = "[✓]";
pub(crate) const ANNOTATION_SUBMIT_BUTTON_ASCII: &str = "[s]";
pub(crate) const ANNOTATION_SUBMIT_BUTTON_WIDTH: usize = 3;
pub(crate) const ANNOTATION_EDIT_BUTTON: &str = "[↻]";
pub(crate) const ANNOTATION_EDIT_BUTTON_ASCII: &str = "[e]";
pub(crate) const ANNOTATION_EDIT_BUTTON_WIDTH: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct AnnotationKey {
    pub(crate) file: usize,
    pub(crate) model_row_index: usize,
    pub(crate) kind: AnnotationLineKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum AnnotationLineKind {
    Unified {
        hunk: usize,
        line: usize,
    },
    Split {
        hunk: usize,
        left: Option<usize>,
        right: Option<usize>,
    },
    Context {
        old_line: usize,
        new_line: usize,
    },
}

impl AnnotationKey {
    pub(crate) fn from_ui_row(row: UiRow, row_index: usize) -> Option<Self> {
        match row {
            UiRow::UnifiedLine { file, hunk, line } | UiRow::MetaLine { file, hunk, line } => {
                Some(Self {
                    file,
                    model_row_index: row_index,
                    kind: AnnotationLineKind::Unified { hunk, line },
                })
            }
            UiRow::SplitLine {
                file,
                hunk,
                left,
                right,
            } => Some(Self {
                file,
                model_row_index: row_index,
                kind: AnnotationLineKind::Split { hunk, left, right },
            }),
            UiRow::ContextLine {
                file,
                old_line,
                new_line,
            } => Some(Self {
                file,
                model_row_index: row_index,
                kind: AnnotationLineKind::Context { old_line, new_line },
            }),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct AnnotationDraft {
    pub(crate) key: AnnotationKey,
    pub(crate) input: String,
    pub(crate) cursor: usize,
}

pub(crate) type AnnotationStore = HashMap<AnnotationKey, String>;
