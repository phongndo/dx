use std::collections::HashMap;

use mark_diff::{Changeset, DiffFile, DiffLine, DiffLineKind};

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
                let Some(hunk) = file.hunks.get(hunk) else {
                    return Vec::new();
                };
                Self::candidates_from_hunk_line(file, &hunk.lines, line)
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
                let mut candidates = Vec::with_capacity(1);
                if let Some(index) = right {
                    // Prefer the right/current side when a split row has both sides;
                    // unpaired left-only rows remain old-side deletion marks.
                    if let Some(line) = lines.get(index).and_then(|line| line.new_line) {
                        Self::push_candidate(&mut candidates, file, AnnotationSide::New, line);
                    }
                    return candidates;
                }
                if let Some(index) = left {
                    Self::push_unpaired_deletion_candidate(&mut candidates, file, lines, index);
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

    fn candidates_from_hunk_line(
        file: &DiffFile,
        lines: &[DiffLine],
        line_index: usize,
    ) -> Vec<Self> {
        let Some(line) = lines.get(line_index) else {
            return Vec::new();
        };

        let mut candidates = Vec::with_capacity(1);
        match line.kind {
            DiffLineKind::Context => {
                if let Some(line) = line.new_line {
                    Self::push_candidate(&mut candidates, file, AnnotationSide::New, line);
                } else if let Some(line) = line.old_line {
                    Self::push_candidate(&mut candidates, file, AnnotationSide::Old, line);
                }
            }
            DiffLineKind::Addition => {
                if let Some(line) = line.new_line {
                    Self::push_candidate(&mut candidates, file, AnnotationSide::New, line);
                }
            }
            DiffLineKind::Deletion => {
                Self::push_unpaired_deletion_candidate(&mut candidates, file, lines, line_index);
            }
            DiffLineKind::Meta => {}
        }
        candidates
    }

    fn push_unpaired_deletion_candidate(
        candidates: &mut Vec<Self>,
        file: &DiffFile,
        lines: &[DiffLine],
        line_index: usize,
    ) {
        let Some(line) = lines.get(line_index) else {
            return;
        };
        if !matches!(line.kind, DiffLineKind::Deletion)
            || deletion_has_paired_addition(lines, line_index)
        {
            return;
        }
        if let Some(line) = line.old_line {
            Self::push_candidate(candidates, file, AnnotationSide::Old, line);
        }
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

pub(crate) fn paired_old_line_for_addition(
    lines: &[DiffLine],
    addition_index: usize,
) -> Option<usize> {
    let (deletions, additions) = change_block_line_indices(lines, addition_index)?;
    let pair_index = additions
        .iter()
        .position(|index| *index == addition_index)?;
    let deletion_index = *deletions.get(pair_index)?;
    lines.get(deletion_index)?.old_line
}

fn deletion_has_paired_addition(lines: &[DiffLine], deletion_index: usize) -> bool {
    let Some((deletions, additions)) = change_block_line_indices(lines, deletion_index) else {
        return false;
    };
    let Some(pair_index) = deletions.iter().position(|index| *index == deletion_index) else {
        return false;
    };
    pair_index < additions.len()
}

fn change_block_line_indices(lines: &[DiffLine], index: usize) -> Option<(Vec<usize>, Vec<usize>)> {
    if !lines.get(index).is_some_and(is_change_line) {
        return None;
    }

    let mut start = index;
    while start > 0 && lines.get(start - 1).is_some_and(is_change_block_line) {
        start -= 1;
    }

    let mut end = index + 1;
    while end < lines.len() && lines.get(end).is_some_and(is_change_block_line) {
        end += 1;
    }

    let mut deletions = Vec::new();
    let mut additions = Vec::new();
    for (offset, line) in lines[start..end].iter().enumerate() {
        let line_index = start + offset;
        match line.kind {
            DiffLineKind::Deletion => deletions.push(line_index),
            DiffLineKind::Addition => additions.push(line_index),
            DiffLineKind::Context | DiffLineKind::Meta => {}
        }
    }

    Some((deletions, additions))
}

fn is_change_line(line: &DiffLine) -> bool {
    matches!(line.kind, DiffLineKind::Deletion | DiffLineKind::Addition)
}

fn is_change_block_line(line: &DiffLine) -> bool {
    matches!(
        line.kind,
        DiffLineKind::Deletion | DiffLineKind::Addition | DiffLineKind::Meta
    )
}

#[derive(Debug, Clone)]
pub(crate) struct AnnotationDraft {
    pub(crate) key: AnnotationKey,
    pub(crate) model_row_index: usize,
    pub(crate) input: String,
    pub(crate) cursor: usize,
}

pub(crate) type AnnotationStore = HashMap<AnnotationKey, String>;
