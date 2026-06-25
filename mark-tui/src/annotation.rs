use std::collections::HashMap;

use mark_diff::{Changeset, DiffFile, DiffLine};

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
    pub(crate) path: String,
    pub(crate) side: AnnotationSide,
    pub(crate) line: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum AnnotationSide {
    Old,
    New,
}

impl AnnotationSide {
    pub(crate) fn label(self) -> char {
        match self {
            Self::Old => 'L',
            Self::New => 'R',
        }
    }
}

impl AnnotationKey {
    pub(crate) fn from_ui_row(changeset: &Changeset, row: UiRow) -> Option<Self> {
        Self::candidates_from_ui_row(changeset, row)
            .into_iter()
            .next()
    }

    pub(crate) fn candidates_from_ui_row(changeset: &Changeset, row: UiRow) -> Vec<Self> {
        match row {
            UiRow::UnifiedLine { file, hunk, line } | UiRow::MetaLine { file, hunk, line } => {
                let Some(file) = changeset.files.get(file) else {
                    return Vec::new();
                };
                let Some(diff_line) = file.hunks.get(hunk).and_then(|hunk| hunk.lines.get(line))
                else {
                    return Vec::new();
                };
                Self::candidates_from_diff_line(file, diff_line)
            }
            UiRow::SplitLine {
                file,
                hunk,
                left,
                right,
            } => {
                let Some(file) = changeset.files.get(file) else {
                    return Vec::new();
                };
                let Some(hunk) = file.hunks.get(hunk) else {
                    return Vec::new();
                };
                let lines = &hunk.lines;
                let mut candidates = Vec::with_capacity(2);
                if let Some(index) = right {
                    if let Some(line) = lines.get(index).and_then(|line| line.new_line) {
                        Self::push_candidate(&mut candidates, file, AnnotationSide::New, line);
                    }
                }
                if let Some(index) = left
                    && let Some(line) = lines.get(index).and_then(|line| line.old_line)
                {
                    Self::push_candidate(&mut candidates, file, AnnotationSide::Old, line);
                }
                candidates
            }
            UiRow::ContextLine { file, new_line, .. } => {
                let Some(file) = changeset.files.get(file) else {
                    return Vec::new();
                };
                Self::path_for_side(file, AnnotationSide::New)
                    .map(|path| vec![Self::new(path, AnnotationSide::New, new_line)])
                    .unwrap_or_default()
            }
            _ => Vec::new(),
        }
    }

    fn candidates_from_diff_line(file: &DiffFile, line: &DiffLine) -> Vec<Self> {
        let mut candidates = Vec::with_capacity(2);
        if let Some(line) = line.new_line {
            Self::push_candidate(&mut candidates, file, AnnotationSide::New, line);
        }
        if let Some(line) = line.old_line {
            Self::push_candidate(&mut candidates, file, AnnotationSide::Old, line);
        }
        candidates
    }

    fn push_candidate(
        candidates: &mut Vec<Self>,
        file: &DiffFile,
        side: AnnotationSide,
        line: usize,
    ) {
        if let Some(path) = Self::path_for_side(file, side) {
            candidates.push(Self::new(path, side, line));
        }
    }

    pub(crate) fn path_for_side(file: &DiffFile, side: AnnotationSide) -> Option<&str> {
        match side {
            AnnotationSide::Old => file.old_path.as_deref().or(file.new_path.as_deref()),
            AnnotationSide::New => file.new_path.as_deref().or(file.old_path.as_deref()),
        }
    }

    fn new(path: &str, side: AnnotationSide, line: usize) -> Self {
        Self {
            path: path.to_owned(),
            side,
            line,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct AnnotationDraft {
    pub(crate) key: AnnotationKey,
    pub(crate) model_row_index: usize,
    pub(crate) input: String,
    pub(crate) cursor: usize,
}

pub(crate) type AnnotationStore = HashMap<AnnotationKey, String>;
