use std::{num::NonZeroU64, path::PathBuf, sync::Arc};

macro_rules! string_newtype {
    ($name:ident) => {
        #[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }

            pub fn is_empty(&self) -> bool {
                self.0.is_empty()
            }

            pub fn into_string(self) -> String {
                self.0
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self(value)
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self(value.to_owned())
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }

        impl std::borrow::Borrow<str> for $name {
            fn borrow(&self) -> &str {
                self.as_str()
            }
        }

        impl std::ops::Deref for $name {
            type Target = str;

            fn deref(&self) -> &Self::Target {
                self.as_str()
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str(self.as_str())
            }
        }
    };
}

string_newtype!(RevSpec);
string_newtype!(BranchName);
string_newtype!(CommitSha);
string_newtype!(RefName);
string_newtype!(ReviewId);
string_newtype!(RemoteName);
string_newtype!(DiffPath);
string_newtype!(RepoRelativePath);
string_newtype!(PatchLabel);

macro_rules! path_newtype {
    ($name:ident) => {
        #[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(PathBuf);

        impl $name {
            pub fn new(path: impl Into<PathBuf>) -> Self {
                Self(path.into())
            }

            pub fn as_path(&self) -> &std::path::Path {
                &self.0
            }

            pub fn into_path_buf(self) -> PathBuf {
                self.0
            }
        }

        impl From<PathBuf> for $name {
            fn from(path: PathBuf) -> Self {
                Self(path)
            }
        }

        impl From<&std::path::Path> for $name {
            fn from(path: &std::path::Path) -> Self {
                Self(path.to_path_buf())
            }
        }

        impl AsRef<std::path::Path> for $name {
            fn as_ref(&self) -> &std::path::Path {
                self.as_path()
            }
        }

        impl std::ops::Deref for $name {
            type Target = std::path::Path;

            fn deref(&self) -> &Self::Target {
                self.as_path()
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.as_path().display().fmt(formatter)
            }
        }
    };
}

path_newtype!(RepoArg);
path_newtype!(WorktreePath);
path_newtype!(DisplayPath);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PullRequestId(NonZeroU64);

impl PullRequestId {
    pub fn new(value: u64) -> Option<Self> {
        NonZeroU64::new(value).map(Self)
    }

    pub fn get(self) -> u64 {
        self.0.get()
    }
}

impl From<NonZeroU64> for PullRequestId {
    fn from(value: NonZeroU64) -> Self {
        Self(value)
    }
}

impl std::fmt::Display for PullRequestId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RepoRoot(PathBuf);

impl RepoRoot {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self(path.into())
    }

    pub fn as_path(&self) -> &std::path::Path {
        &self.0
    }

    pub fn into_path_buf(self) -> PathBuf {
        self.0
    }
}

impl From<PathBuf> for RepoRoot {
    fn from(path: PathBuf) -> Self {
        Self(path)
    }
}

impl AsRef<std::path::Path> for RepoRoot {
    fn as_ref(&self) -> &std::path::Path {
        self.as_path()
    }
}

impl std::ops::Deref for RepoRoot {
    type Target = std::path::Path;

    fn deref(&self) -> &Self::Target {
        self.as_path()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OldLineNumber(usize);

impl OldLineNumber {
    pub fn new(line: usize) -> Self {
        Self(line)
    }

    pub fn get(self) -> usize {
        self.0
    }
}

impl From<usize> for OldLineNumber {
    fn from(line: usize) -> Self {
        Self::new(line)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NewLineNumber(usize);

impl NewLineNumber {
    pub fn new(line: usize) -> Self {
        Self(line)
    }

    pub fn get(self) -> usize {
        self.0
    }
}

impl From<usize> for NewLineNumber {
    fn from(line: usize) -> Self {
        Self::new(line)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum DiffSource {
    #[default]
    Worktree,
    Show(RevSpec),
    Base(RevSpec),
    Branch {
        base: RevSpec,
        head: RevSpec,
    },
    Range {
        left: RevSpec,
        right: RevSpec,
    },
    Difftool {
        left: WorktreePath,
        right: WorktreePath,
        path: Option<DisplayPath>,
    },
    Patch(PatchSource),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatchSource {
    File(PathBuf),
    Stdin(Arc<[u8]>),
    Text { label: PatchLabel, patch: Arc<[u8]> },
    Review { label: PatchLabel, patch: Arc<[u8]> },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum UntrackedMode {
    #[default]
    Include,
    Exclude,
}

impl UntrackedMode {
    pub fn from_include(include: bool) -> Self {
        if include {
            Self::Include
        } else {
            Self::Exclude
        }
    }

    pub fn includes(self) -> bool {
        matches!(self, Self::Include)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DiffOutput {
    #[default]
    Patch,
    Stat,
}

impl DiffOutput {
    pub fn is_stat(self) -> bool {
        matches!(self, Self::Stat)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffOptions {
    pub repo: Option<RepoArg>,
    pub source: DiffSource,
    pub local_untracked: UntrackedMode,
    pub output: DiffOutput,
}

impl Default for DiffOptions {
    fn default() -> Self {
        Self {
            repo: None,
            source: DiffSource::default(),
            local_untracked: UntrackedMode::Include,
            output: DiffOutput::Patch,
        }
    }
}

impl DiffOptions {
    pub fn include_untracked(&self) -> bool {
        self.local_untracked.includes()
    }

    pub fn is_stat(&self) -> bool {
        self.output.is_stat()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Changeset {
    pub repo: RepoRoot,
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
            if file.is_binary() {
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
    pub change: FileChange,
    pub additions: usize,
    pub deletions: usize,
    pub body: DiffFileBody,
}

impl DiffFile {
    pub fn display_path(&self) -> &str {
        self.new_path().or(self.old_path()).unwrap_or("/dev/null")
    }

    pub fn old_path(&self) -> Option<&str> {
        self.change.old_path()
    }

    pub fn new_path(&self) -> Option<&str> {
        self.change.new_path()
    }

    pub fn status(&self) -> FileStatus {
        self.change.status()
    }

    pub fn is_binary(&self) -> bool {
        matches!(self.body, DiffFileBody::Binary)
    }

    pub fn has_textual_changes(&self) -> bool {
        matches!(&self.body, DiffFileBody::Text { hunks } if !hunks.is_empty())
    }

    pub fn has_no_textual_changes(&self) -> bool {
        matches!(&self.body, DiffFileBody::NoTextualChanges)
            || matches!(&self.body, DiffFileBody::Text { hunks } if hunks.is_empty())
    }

    pub fn hunks(&self) -> &[DiffHunk] {
        match &self.body {
            DiffFileBody::Text { hunks } => hunks,
            DiffFileBody::Binary | DiffFileBody::NoTextualChanges => &[],
        }
    }

    pub fn hunks_mut(&mut self) -> &mut Vec<DiffHunk> {
        if matches!(self.body, DiffFileBody::NoTextualChanges) {
            self.body = DiffFileBody::Text { hunks: Vec::new() };
        }
        match &mut self.body {
            DiffFileBody::Text { hunks } => hunks,
            DiffFileBody::Binary => panic!("binary diff files do not have text hunks"),
            DiffFileBody::NoTextualChanges => unreachable!(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileChange {
    Modified {
        old_path: DiffPath,
        new_path: DiffPath,
    },
    Added {
        path: DiffPath,
    },
    Deleted {
        path: DiffPath,
    },
    Renamed {
        old_path: DiffPath,
        new_path: DiffPath,
    },
    Copied {
        old_path: DiffPath,
        new_path: DiffPath,
    },
    TypeChanged {
        old_path: DiffPath,
        new_path: DiffPath,
    },
    Unknown {
        old_path: Option<DiffPath>,
        new_path: Option<DiffPath>,
    },
}

impl FileChange {
    pub fn modified(path: impl Into<DiffPath>) -> Self {
        let path = path.into();
        Self::Modified {
            old_path: path.clone(),
            new_path: path,
        }
    }

    pub fn from_status(
        status: FileStatus,
        old_path: Option<String>,
        new_path: Option<String>,
    ) -> Self {
        let old_path = old_path.map(DiffPath::from);
        let new_path = new_path.map(DiffPath::from);
        match status {
            FileStatus::Modified => match (old_path, new_path) {
                (Some(old_path), Some(new_path)) => Self::Modified { old_path, new_path },
                (Some(path), None) | (None, Some(path)) => Self::Modified {
                    old_path: path.clone(),
                    new_path: path,
                },
                (None, None) => Self::Unknown {
                    old_path: None,
                    new_path: None,
                },
            },
            FileStatus::Added => match new_path {
                Some(path) => Self::Added { path },
                None => Self::Unknown {
                    old_path,
                    new_path: None,
                },
            },
            FileStatus::Deleted => match old_path {
                Some(path) => Self::Deleted { path },
                None => Self::Unknown {
                    old_path: None,
                    new_path,
                },
            },
            FileStatus::Renamed => match (old_path, new_path) {
                (Some(old_path), Some(new_path)) => Self::Renamed { old_path, new_path },
                (old_path, new_path) => Self::Unknown { old_path, new_path },
            },
            FileStatus::Copied => match (old_path, new_path) {
                (Some(old_path), Some(new_path)) => Self::Copied { old_path, new_path },
                (old_path, new_path) => Self::Unknown { old_path, new_path },
            },
            FileStatus::TypeChanged => match (old_path, new_path) {
                (Some(old_path), Some(new_path)) => Self::TypeChanged { old_path, new_path },
                (Some(path), None) | (None, Some(path)) => Self::TypeChanged {
                    old_path: path.clone(),
                    new_path: path,
                },
                (None, None) => Self::Unknown {
                    old_path: None,
                    new_path: None,
                },
            },
            FileStatus::Unknown => Self::Unknown { old_path, new_path },
        }
    }

    pub fn old_path(&self) -> Option<&str> {
        match self {
            Self::Modified { old_path, .. } => Some(old_path.as_str()),
            Self::Deleted { path } => Some(path.as_str()),
            Self::Added { .. } => None,
            Self::Renamed { old_path, .. }
            | Self::Copied { old_path, .. }
            | Self::TypeChanged { old_path, .. } => Some(old_path.as_str()),
            Self::Unknown { old_path, .. } => old_path.as_ref().map(DiffPath::as_str),
        }
    }

    pub fn new_path(&self) -> Option<&str> {
        match self {
            Self::Modified { new_path, .. } => Some(new_path.as_str()),
            Self::Added { path } => Some(path.as_str()),
            Self::Deleted { .. } => None,
            Self::Renamed { new_path, .. }
            | Self::Copied { new_path, .. }
            | Self::TypeChanged { new_path, .. } => Some(new_path.as_str()),
            Self::Unknown { new_path, .. } => new_path.as_ref().map(DiffPath::as_str),
        }
    }

    pub fn status(&self) -> FileStatus {
        match self {
            Self::Modified { .. } => FileStatus::Modified,
            Self::Added { .. } => FileStatus::Added,
            Self::Deleted { .. } => FileStatus::Deleted,
            Self::Renamed { .. } => FileStatus::Renamed,
            Self::Copied { .. } => FileStatus::Copied,
            Self::TypeChanged { .. } => FileStatus::TypeChanged,
            Self::Unknown { .. } => FileStatus::Unknown,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffFileBody {
    Text { hunks: Vec<DiffHunk> },
    Binary,
    NoTextualChanges,
}

impl Default for DiffFileBody {
    fn default() -> Self {
        Self::Text { hunks: Vec::new() }
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
    pub ranges: HunkLineRanges,
    pub lines: Vec<DiffLine>,
}

impl DiffHunk {
    pub fn old_start(&self) -> usize {
        self.ranges.old_start()
    }

    pub fn old_count(&self) -> usize {
        self.ranges.old_count()
    }

    pub fn new_start(&self) -> usize {
        self.ranges.new_start()
    }

    pub fn new_count(&self) -> usize {
        self.ranges.new_count()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HunkLineRanges {
    old: LineSpan<OldLineNumber>,
    new: LineSpan<NewLineNumber>,
}

impl HunkLineRanges {
    pub fn new(old_start: usize, old_count: usize, new_start: usize, new_count: usize) -> Self {
        Self {
            old: LineSpan::new(OldLineNumber::new(old_start), old_count),
            new: LineSpan::new(NewLineNumber::new(new_start), new_count),
        }
    }

    pub fn old(&self) -> LineSpan<OldLineNumber> {
        self.old
    }

    pub fn new_lines(&self) -> LineSpan<NewLineNumber> {
        self.new
    }

    pub fn old_start(&self) -> usize {
        self.old.start().get()
    }

    pub fn old_count(&self) -> usize {
        self.old.count()
    }

    pub fn new_start(&self) -> usize {
        self.new.start().get()
    }

    pub fn new_count(&self) -> usize {
        self.new.count()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineSpan<N> {
    start: N,
    count: usize,
}

impl<N: Copy> LineSpan<N> {
    pub fn new(start: N, count: usize) -> Self {
        Self { start, count }
    }

    pub fn start(self) -> N {
        self.start
    }

    pub fn count(self) -> usize {
        self.count
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffLine {
    Context {
        old_line: OldLineNumber,
        new_line: NewLineNumber,
        text: String,
    },
    Addition {
        new_line: NewLineNumber,
        text: String,
    },
    Deletion {
        old_line: OldLineNumber,
        text: String,
    },
    Meta {
        text: String,
    },
}

impl DiffLine {
    pub fn context(old_line: usize, new_line: usize, text: impl Into<String>) -> Self {
        Self::Context {
            old_line: OldLineNumber::new(old_line),
            new_line: NewLineNumber::new(new_line),
            text: text.into(),
        }
    }

    pub fn addition(new_line: usize, text: impl Into<String>) -> Self {
        Self::Addition {
            new_line: NewLineNumber::new(new_line),
            text: text.into(),
        }
    }

    pub fn deletion(old_line: usize, text: impl Into<String>) -> Self {
        Self::Deletion {
            old_line: OldLineNumber::new(old_line),
            text: text.into(),
        }
    }

    pub fn meta(text: impl Into<String>) -> Self {
        Self::Meta { text: text.into() }
    }

    pub fn kind(&self) -> DiffLineKind {
        match self {
            Self::Context { .. } => DiffLineKind::Context,
            Self::Addition { .. } => DiffLineKind::Addition,
            Self::Deletion { .. } => DiffLineKind::Deletion,
            Self::Meta { .. } => DiffLineKind::Meta,
        }
    }

    pub fn old_line(&self) -> Option<usize> {
        match self {
            Self::Context { old_line, .. } | Self::Deletion { old_line, .. } => {
                Some(old_line.get())
            }
            Self::Addition { .. } | Self::Meta { .. } => None,
        }
    }

    pub fn new_line(&self) -> Option<usize> {
        match self {
            Self::Context { new_line, .. } | Self::Addition { new_line, .. } => {
                Some(new_line.get())
            }
            Self::Deletion { .. } | Self::Meta { .. } => None,
        }
    }

    pub fn text(&self) -> &str {
        match self {
            Self::Context { text, .. }
            | Self::Addition { text, .. }
            | Self::Deletion { text, .. }
            | Self::Meta { text } => text,
        }
    }

    pub fn text_mut(&mut self) -> &mut String {
        match self {
            Self::Context { text, .. }
            | Self::Addition { text, .. }
            | Self::Deletion { text, .. }
            | Self::Meta { text } => text,
        }
    }
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
    FileBodyNotice(usize),
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

            if file.is_binary() || file.has_no_textual_changes() {
                rows.push(DiffRowRef::FileBodyNotice(file_index));
                continue;
            }

            for (hunk_index, hunk) in file.hunks().iter().enumerate() {
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
