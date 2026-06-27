use std::{path::PathBuf, sync::Arc};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DiffScope {
    #[default]
    All,
    Staged,
    Unstaged,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum DiffSource {
    #[default]
    Worktree,
    Show(String),
    Base(String),
    Branch {
        base: String,
        head: String,
    },
    Range {
        left: String,
        right: String,
    },
    Difftool {
        left: PathBuf,
        right: PathBuf,
        path: Option<PathBuf>,
    },
    Patch(PatchSource),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatchSource {
    File(PathBuf),
    Stdin(Arc<[u8]>),
    Text { label: String, patch: Arc<[u8]> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffOptions {
    pub repo: Option<PathBuf>,
    pub source: DiffSource,
    pub scope: DiffScope,
    pub include_untracked: bool,
    pub stat: bool,
}

impl Default for DiffOptions {
    fn default() -> Self {
        Self {
            repo: None,
            source: DiffSource::Worktree,
            scope: DiffScope::All,
            include_untracked: true,
            stat: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Changeset {
    pub repo: PathBuf,
    pub title: String,
    pub files: Vec<DiffFile>,
    pub raw_patch: Vec<u8>,
}

impl Changeset {
    pub fn stats(&self) -> DiffStats {
        let mut stats = DiffStats {
            files: self.files.len(),
            ..DiffStats::default()
        };
        for file in &self.files {
            stats.additions += file.additions;
            stats.deletions += file.deletions;
            if file.is_binary {
                stats.binary_files += 1;
            }
        }
        stats
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiffStats {
    pub files: usize,
    pub additions: usize,
    pub deletions: usize,
    pub binary_files: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffFile {
    pub old_path: Option<String>,
    pub new_path: Option<String>,
    pub status: FileStatus,
    pub hunks: Vec<DiffHunk>,
    pub additions: usize,
    pub deletions: usize,
    pub is_binary: bool,
}

impl DiffFile {
    pub fn display_path(&self) -> &str {
        self.new_path
            .as_deref()
            .or(self.old_path.as_deref())
            .unwrap_or("/dev/null")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
    Copied,
    TypeChanged,
    Unknown,
}

impl FileStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Modified => "modified",
            Self::Added => "added",
            Self::Deleted => "deleted",
            Self::Renamed => "renamed",
            Self::Copied => "copied",
            Self::TypeChanged => "type-changed",
            Self::Unknown => "changed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffHunk {
    pub header: String,
    pub old_start: usize,
    pub old_count: usize,
    pub new_start: usize,
    pub new_count: usize,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub old_line: Option<usize>,
    pub new_line: Option<usize>,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffLineKind {
    Context,
    Addition,
    Deletion,
    Meta,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffRowRef {
    FileHeader(usize),
    BinaryFile(usize),
    HunkHeader {
        file: usize,
        hunk: usize,
    },
    Line {
        file: usize,
        hunk: usize,
        line: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffViewModel {
    rows: Vec<DiffRowRef>,
    file_start_rows: Vec<usize>,
    hunk_start_rows: Vec<usize>,
}

impl DiffViewModel {
    pub fn new(changeset: &Changeset) -> Self {
        let mut rows = Vec::new();
        let mut file_start_rows = Vec::with_capacity(changeset.files.len());
        let mut hunk_start_rows = Vec::new();

        for (file_index, file) in changeset.files.iter().enumerate() {
            file_start_rows.push(rows.len());
            rows.push(DiffRowRef::FileHeader(file_index));

            if file.is_binary || file.hunks.is_empty() {
                rows.push(DiffRowRef::BinaryFile(file_index));
                continue;
            }

            for (hunk_index, hunk) in file.hunks.iter().enumerate() {
                hunk_start_rows.push(rows.len());
                rows.push(DiffRowRef::HunkHeader {
                    file: file_index,
                    hunk: hunk_index,
                });
                for line_index in 0..hunk.lines.len() {
                    rows.push(DiffRowRef::Line {
                        file: file_index,
                        hunk: hunk_index,
                        line: line_index,
                    });
                }
            }
        }

        Self {
            rows,
            file_start_rows,
            hunk_start_rows,
        }
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn row(&self, index: usize) -> Option<DiffRowRef> {
        self.rows.get(index).copied()
    }

    pub fn file_start_row(&self, file: usize) -> Option<usize> {
        self.file_start_rows.get(file).copied()
    }

    pub fn file_at_row(&self, row: usize) -> Option<usize> {
        if self.file_start_rows.is_empty() {
            return None;
        }
        match self.file_start_rows.binary_search(&row) {
            Ok(index) => Some(index),
            Err(0) => Some(0),
            Err(index) => Some(index - 1),
        }
    }

    pub fn next_hunk_row(&self, row: usize) -> Option<usize> {
        let index = self.hunk_start_rows.partition_point(|start| *start <= row);
        self.hunk_start_rows.get(index).copied()
    }

    pub fn previous_hunk_row(&self, row: usize) -> Option<usize> {
        let index = self.hunk_start_rows.partition_point(|start| *start < row);
        index
            .checked_sub(1)
            .and_then(|index| self.hunk_start_rows.get(index))
            .copied()
    }
}
