mod delta;
mod file;
mod fit;
mod hunk;
mod types;

pub(crate) use delta::{
    compact_delta_parts, delta_parts_width, file_delta_parts, push_delta_spans,
    push_fitted_delta_spans,
};
pub(crate) use file::{file_header_line, file_separator_line};
pub(crate) use fit::{header_spans, hunk_header_spans_with_delta};
#[cfg(test)]
pub(crate) use hunk::hunk_header_spans;
pub(crate) use hunk::{hunk_header_line, hunk_header_line_with_focus};
pub(crate) use types::{DeltaKind, DeltaPart, FittedPrefixedParts, HeaderSpanPart, HeaderStyles};
