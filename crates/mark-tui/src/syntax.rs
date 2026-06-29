mod inline;
mod lru;
mod queue;
mod runtime;
mod runtime_queue;
mod runtime_results;
mod source;
mod types;

#[cfg(test)]
pub(crate) use inline::{
    InlineCharClass, compute_hunk_inline_emphasis, inline_ascii_class, inline_tokens,
};
pub(crate) use inline::{InlineHunkEmphasisCache, InlineHunkKey, InlineRange};
pub(crate) use lru::LruCache;
pub(crate) use queue::{SyntaxPriority, SyntaxQueueError, SyntaxWorkerQueue};
pub(crate) use runtime::SyntaxRuntime;
#[cfg(test)]
pub(crate) use source::{FullFileSource, FullFileSourceKind, HunkSource, git_blob, git_merge_base};
#[cfg(test)]
pub(crate) use source::{
    SyntaxJob, SyntaxJobFailure, SyntaxJobSource, SyntaxResult, build_full_file_line_map,
    build_hunk_source,
};
pub(crate) use source::{
    available_context_lines, full_file_source, load_full_file_source, split_context_source_lines,
    unified_syntax_side,
};
pub(crate) use types::{DiffSide, HighlightedSide, SyntaxKey, SyntaxPosition, SyntaxSkipReason};
#[cfg(test)]
pub(crate) use types::{SyntaxSourceId, SyntaxSourceKind};

#[cfg(test)]
mod tests;
