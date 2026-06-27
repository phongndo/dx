use mark_syntax::HighlightedLine;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum DiffSide {
    Old,
    New,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct SyntaxPosition {
    pub(crate) generation: u64,
    pub(crate) file: usize,
    pub(crate) hunk: usize,
    pub(crate) side: DiffSide,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum SyntaxSourceKind {
    HunkSide { hunk: usize },
    FullFile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct SyntaxSourceId {
    pub(crate) generation: u64,
    pub(crate) file: usize,
    pub(crate) side: DiffSide,
    pub(crate) kind: SyntaxSourceKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct SyntaxKey {
    pub(crate) source: SyntaxSourceId,
    pub(crate) language_hash: u64,
    pub(crate) theme_id: u64,
}

impl SyntaxKey {
    pub(crate) fn generation(self) -> u64 {
        self.source.generation
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SyntaxSkipReason {
    InvalidPosition,
    NoPath,
    NoLanguage,
    NoSource,
    TooLarge,
    QueueClosed,
    HighlightError,
}

#[derive(Debug, Clone)]
pub(crate) struct HighlightedSide {
    pub(crate) lines: Vec<HighlightedLine>,
}

impl HighlightedSide {
    pub(crate) fn memory_bytes(&self) -> usize {
        self.lines
            .iter()
            .flat_map(|line| line.segments.iter())
            .map(|segment| segment.text.len())
            .sum::<usize>()
            .saturating_add(self.lines.len() * std::mem::size_of::<HighlightedLine>())
    }
}
