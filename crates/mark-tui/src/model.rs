use std::{collections::HashMap, ops::Range, sync::Arc};

use mark_diff::{Changeset, DiffLine, DiffLineKind};

use crate::{controls::DiffLayoutMode, syntax::DiffSide};

macro_rules! typed_index {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub(crate) struct $name(usize);

        impl $name {
            pub(crate) const fn new(index: usize) -> Self {
                Self(index)
            }

            pub(crate) const fn get(self) -> usize {
                self.0
            }
        }

        impl From<usize> for $name {
            fn from(index: usize) -> Self {
                Self::new(index)
            }
        }

        impl From<$name> for usize {
            fn from(index: $name) -> Self {
                index.get()
            }
        }

        impl<T> std::ops::Index<$name> for [T] {
            type Output = T;

            fn index(&self, index: $name) -> &Self::Output {
                &self[index.get()]
            }
        }

        impl<T> std::ops::IndexMut<$name> for [T] {
            fn index_mut(&mut self, index: $name) -> &mut Self::Output {
                &mut self[index.get()]
            }
        }

        impl<T> std::ops::Index<$name> for Vec<T> {
            type Output = T;

            fn index(&self, index: $name) -> &Self::Output {
                &self[index.get()]
            }
        }

        impl<T> std::ops::IndexMut<$name> for Vec<T> {
            fn index_mut(&mut self, index: $name) -> &mut Self::Output {
                &mut self[index.get()]
            }
        }
    };
}

typed_index!(FileIndex);
typed_index!(HunkIndex);
typed_index!(DiffLineIndex);
typed_index!(ModelRow);
typed_index!(VisualRow);
typed_index!(ScrollOffset);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UiRow {
    FileSeparator,
    FileHeader(FileIndex),
    FileBodyNotice(FileIndex),
    Collapsed {
        file: FileIndex,
        hunk: HunkIndex,
        old_start: usize,
        new_start: usize,
        lines: usize,
        expanded: usize,
    },
    ContextLine {
        file: FileIndex,
        old_line: usize,
        new_line: usize,
    },
    ContextHide {
        file: FileIndex,
        hunk: HunkIndex,
        lines: usize,
    },
    HunkHeader {
        file: FileIndex,
        hunk: HunkIndex,
    },
    UnifiedLine {
        file: FileIndex,
        hunk: HunkIndex,
        line: DiffLineIndex,
    },
    SplitLine {
        file: FileIndex,
        hunk: HunkIndex,
        left: Option<DiffLineIndex>,
        right: Option<DiffLineIndex>,
    },
    MetaLine {
        file: FileIndex,
        hunk: HunkIndex,
        line: DiffLineIndex,
    },
}

impl UiRow {
    pub(crate) fn typed_hunk_key(self) -> Option<(FileIndex, HunkIndex)> {
        match self {
            Self::HunkHeader { file, hunk }
            | Self::UnifiedLine { file, hunk, .. }
            | Self::SplitLine { file, hunk, .. }
            | Self::MetaLine { file, hunk, .. } => Some((file, hunk)),
            Self::FileSeparator
            | Self::FileHeader(_)
            | Self::FileBodyNotice(_)
            | Self::Collapsed { .. }
            | Self::ContextLine { .. }
            | Self::ContextHide { .. } => None,
        }
    }

    pub(crate) fn hunk_key(self) -> Option<(usize, usize)> {
        self.typed_hunk_key()
            .map(|(file, hunk)| (file.get(), hunk.get()))
    }

    pub(crate) fn is_hunk_row(self, file: usize, hunk: usize) -> bool {
        self.hunk_key() == Some((file, hunk))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ContextKey {
    pub(crate) file: FileIndex,
    /// The hunk whose surrounding context is expanded. A value one past the
    /// final hunk is used for trailing context after that final hunk.
    pub(crate) hunk: HunkIndex,
}

pub(crate) fn context_expands_up(hunk: HunkIndex) -> bool {
    hunk.get() == 0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ContextSourceKey {
    pub(crate) file: FileIndex,
    pub(crate) side: DiffSide,
}

#[derive(Debug, Clone)]
pub(crate) enum ContextSourceEntry {
    Lines(Arc<Vec<String>>),
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UiModel {
    pub(crate) rows: Vec<UiRow>,
    pub(crate) file_start_rows: Vec<Option<ModelRow>>,
    pub(crate) file_row_starts: Vec<(FileIndex, ModelRow)>,
    pub(crate) visible_files: Vec<FileIndex>,
    pub(crate) hunk_start_rows: Vec<ModelRow>,
    pub(crate) hunk_row_starts: Vec<((FileIndex, HunkIndex), ModelRow)>,
}

impl UiModel {
    pub(crate) fn new(
        changeset: &Changeset,
        layout: DiffLayoutMode,
        context_expansions: &HashMap<ContextKey, usize>,
    ) -> Self {
        let visible_files: Vec<_> = (0..changeset.files.len()).map(FileIndex::new).collect();
        Self::new_filtered(changeset, layout, context_expansions, &visible_files)
    }

    pub(crate) fn new_filtered(
        changeset: &Changeset,
        layout: DiffLayoutMode,
        context_expansions: &HashMap<ContextKey, usize>,
        visible_files: &[FileIndex],
    ) -> Self {
        let total_hunks = changeset
            .files
            .iter()
            .map(|file| file.hunks().len())
            .sum::<usize>();
        let total_hunk_lines = changeset
            .files
            .iter()
            .flat_map(|file| file.hunks().iter())
            .map(|hunk| hunk.lines.len())
            .sum::<usize>();
        let binary_or_empty_rows = changeset
            .files
            .iter()
            .filter(|file| file.is_binary() || file.has_no_textual_changes())
            .count();
        let file_separator_rows = changeset.files.len().saturating_sub(1);
        let expanded_context_rows = context_expansions.values().copied().sum::<usize>();
        let expanded_context_controls = context_expansions
            .values()
            .filter(|expanded| **expanded > 0)
            .count();
        let mut rows = Vec::with_capacity(
            changeset
                .files
                .len()
                .saturating_add(file_separator_rows)
                .saturating_add(binary_or_empty_rows)
                .saturating_add(total_hunks.saturating_mul(2))
                .saturating_add(total_hunk_lines)
                .saturating_add(expanded_context_rows)
                .saturating_add(expanded_context_controls),
        );
        let mut file_start_rows = vec![None; changeset.files.len()];
        let mut file_row_starts = Vec::with_capacity(visible_files.len());
        let mut hunk_start_rows = Vec::with_capacity(total_hunks);
        let mut hunk_row_starts = Vec::with_capacity(total_hunks);

        for (visible_index, file_index) in visible_files.iter().copied().enumerate() {
            let Some(file) = changeset.files.get(file_index.get()) else {
                continue;
            };
            if visible_index > 0 {
                rows.push(UiRow::FileSeparator);
            }
            file_start_rows[file_index] = Some(ModelRow::new(rows.len()));
            file_row_starts.push((file_index, ModelRow::new(rows.len())));
            rows.push(UiRow::FileHeader(file_index));

            if file.is_binary() || file.has_no_textual_changes() {
                rows.push(UiRow::FileBodyNotice(file_index));
                continue;
            }

            let mut next_old_line = 1;
            let mut next_new_line = 1;
            for (hunk_index, hunk) in file.hunks().iter().enumerate() {
                let hunk_index = HunkIndex::new(hunk_index);
                let collapsed_lines = hunk
                    .old_start()
                    .saturating_sub(next_old_line)
                    .min(hunk.new_start().saturating_sub(next_new_line));
                if collapsed_lines > 0 {
                    let key = ContextKey {
                        file: file_index,
                        hunk: hunk_index,
                    };
                    let expanded = context_expansions
                        .get(&key)
                        .copied()
                        .unwrap_or_default()
                        .min(collapsed_lines);
                    let remaining = collapsed_lines.saturating_sub(expanded);

                    if context_expands_up(hunk_index) {
                        if remaining > 0 {
                            rows.push(UiRow::Collapsed {
                                file: file_index,
                                hunk: hunk_index,
                                old_start: next_old_line,
                                new_start: next_new_line,
                                lines: remaining,
                                expanded,
                            });
                        }

                        if expanded > 0 {
                            let old_start = next_old_line.saturating_add(remaining);
                            let new_start = next_new_line.saturating_add(remaining);
                            for offset in 0..expanded {
                                rows.push(UiRow::ContextLine {
                                    file: file_index,
                                    old_line: old_start + offset,
                                    new_line: new_start + offset,
                                });
                            }
                            rows.push(UiRow::ContextHide {
                                file: file_index,
                                hunk: hunk_index,
                                lines: expanded,
                            });
                        }
                    } else {
                        if expanded > 0 {
                            rows.push(UiRow::ContextHide {
                                file: file_index,
                                hunk: hunk_index,
                                lines: expanded,
                            });
                            for offset in 0..expanded {
                                rows.push(UiRow::ContextLine {
                                    file: file_index,
                                    old_line: next_old_line + offset,
                                    new_line: next_new_line + offset,
                                });
                            }
                        }

                        if remaining > 0 {
                            rows.push(UiRow::Collapsed {
                                file: file_index,
                                hunk: hunk_index,
                                old_start: next_old_line.saturating_add(expanded),
                                new_start: next_new_line.saturating_add(expanded),
                                lines: remaining,
                                expanded,
                            });
                        }
                    }
                }

                let hunk_start_row = rows.len();
                let hunk_start_row = ModelRow::new(hunk_start_row);
                hunk_start_rows.push(hunk_start_row);
                hunk_row_starts.push(((file_index, hunk_index), hunk_start_row));
                rows.push(UiRow::HunkHeader {
                    file: file_index,
                    hunk: hunk_index,
                });

                match layout {
                    DiffLayoutMode::Unified => {
                        for line_index in 0..hunk.lines.len() {
                            rows.push(UiRow::UnifiedLine {
                                file: file_index,
                                hunk: hunk_index,
                                line: DiffLineIndex::new(line_index),
                            });
                        }
                    }
                    DiffLayoutMode::Split => push_split_hunk_rows(
                        &mut rows,
                        file_index,
                        hunk_index,
                        hunk.lines.as_slice(),
                    ),
                }

                next_old_line = hunk.old_start().saturating_add(hunk.old_count());
                next_new_line = hunk.new_start().saturating_add(hunk.new_count());
            }

            let trailing_context_key = ContextKey {
                file: file_index,
                hunk: HunkIndex::new(file.hunks().len()),
            };
            let expanded = context_expansions
                .get(&trailing_context_key)
                .copied()
                .unwrap_or_default();
            if expanded > 0 {
                rows.push(UiRow::ContextHide {
                    file: file_index,
                    hunk: trailing_context_key.hunk,
                    lines: expanded,
                });
                for offset in 0..expanded {
                    rows.push(UiRow::ContextLine {
                        file: file_index,
                        old_line: next_old_line.saturating_add(offset),
                        new_line: next_new_line.saturating_add(offset),
                    });
                }
            }
        }

        Self {
            rows,
            file_start_rows,
            file_row_starts,
            visible_files: visible_files.to_vec(),
            hunk_start_rows,
            hunk_row_starts,
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.rows.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub(crate) fn row(&self, index: usize) -> Option<UiRow> {
        self.rows.get(index).copied()
    }

    pub(crate) fn file_start_row(&self, file: usize) -> Option<usize> {
        self.file_start_rows
            .get(file)
            .copied()
            .flatten()
            .map(ModelRow::get)
    }

    pub(crate) fn file_at_row(&self, row: usize) -> Option<usize> {
        self.typed_file_at_row(ModelRow::new(row))
            .map(FileIndex::get)
    }

    pub(crate) fn typed_file_at_row(&self, row: ModelRow) -> Option<FileIndex> {
        if self.file_row_starts.is_empty() {
            return None;
        }
        match self
            .file_row_starts
            .binary_search_by_key(&row, |(_, start)| *start)
        {
            Ok(index) => self.file_row_starts.get(index).map(|(file, _)| *file),
            Err(0) => self.file_row_starts.first().map(|(file, _)| *file),
            Err(index) => self.file_row_starts.get(index - 1).map(|(file, _)| *file),
        }
    }

    pub(crate) fn visible_files(&self) -> &[FileIndex] {
        &self.visible_files
    }

    pub(crate) fn visible_file_position(&self, file: usize) -> Option<usize> {
        self.visible_files.binary_search(&FileIndex::new(file)).ok()
    }

    pub(crate) fn next_hunk_row(&self, row: usize) -> Option<usize> {
        self.typed_next_hunk_row(ModelRow::new(row))
            .map(ModelRow::get)
    }

    pub(crate) fn typed_next_hunk_row(&self, row: ModelRow) -> Option<ModelRow> {
        let index = self.hunk_start_rows.partition_point(|start| *start <= row);
        self.hunk_start_rows.get(index).copied()
    }

    pub(crate) fn previous_hunk_row(&self, row: usize) -> Option<usize> {
        self.typed_previous_hunk_row(ModelRow::new(row))
            .map(ModelRow::get)
    }

    pub(crate) fn typed_previous_hunk_row(&self, row: ModelRow) -> Option<ModelRow> {
        let index = self.hunk_start_rows.partition_point(|start| *start < row);
        index
            .checked_sub(1)
            .and_then(|index| self.hunk_start_rows.get(index))
            .copied()
    }

    pub(crate) fn hunk_start_row(&self, file: usize, hunk: usize) -> Option<usize> {
        self.typed_hunk_start_row(FileIndex::new(file), HunkIndex::new(hunk))
            .map(ModelRow::get)
    }

    pub(crate) fn typed_hunk_start_row(
        &self,
        file: FileIndex,
        hunk: HunkIndex,
    ) -> Option<ModelRow> {
        self.hunk_row_starts
            .binary_search_by_key(&(file, hunk), |(key, _)| *key)
            .ok()
            .and_then(|index| self.hunk_row_starts.get(index))
            .map(|(_, row)| *row)
    }

    pub(crate) fn hunk_row_range(&self, file: usize, hunk: usize) -> Option<Range<usize>> {
        let file = FileIndex::new(file);
        let hunk = HunkIndex::new(hunk);
        let start = self.typed_hunk_start_row(file, hunk)?.get();
        let end = (start + 1..self.rows.len())
            .find(|row| {
                self.row(*row)
                    .map(|row| !row.is_hunk_row(file.get(), hunk.get()))
                    .unwrap_or(true)
            })
            .unwrap_or(self.rows.len());
        Some(start..end)
    }
}

pub(crate) fn push_split_hunk_rows(
    rows: &mut Vec<UiRow>,
    file_index: FileIndex,
    hunk_index: HunkIndex,
    lines: &[DiffLine],
) {
    let mut index = 0;
    while index < lines.len() {
        match lines[index].kind() {
            DiffLineKind::Context => {
                rows.push(UiRow::SplitLine {
                    file: file_index,
                    hunk: hunk_index,
                    left: Some(DiffLineIndex::new(index)),
                    right: Some(DiffLineIndex::new(index)),
                });
                index += 1;
            }
            DiffLineKind::Meta => {
                rows.push(UiRow::MetaLine {
                    file: file_index,
                    hunk: hunk_index,
                    line: DiffLineIndex::new(index),
                });
                index += 1;
            }
            DiffLineKind::Deletion | DiffLineKind::Addition => {
                let mut deletions = Vec::new();
                let mut additions = Vec::new();
                while index < lines.len()
                    && matches!(
                        lines[index].kind(),
                        DiffLineKind::Deletion | DiffLineKind::Addition
                    )
                {
                    match lines[index].kind() {
                        DiffLineKind::Deletion => deletions.push(DiffLineIndex::new(index)),
                        DiffLineKind::Addition => additions.push(DiffLineIndex::new(index)),
                        DiffLineKind::Context | DiffLineKind::Meta => {}
                    }
                    index += 1;
                }

                let paired_rows = deletions.len().max(additions.len());
                for pair_index in 0..paired_rows {
                    rows.push(UiRow::SplitLine {
                        file: file_index,
                        hunk: hunk_index,
                        left: deletions.get(pair_index).copied(),
                        right: additions.get(pair_index).copied(),
                    });
                }
            }
        }
    }
}
