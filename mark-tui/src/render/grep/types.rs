#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GrepHighlightTarget {
    pub(crate) text: String,
    pub(crate) spans: Vec<GrepHighlightSpan>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GrepHighlightSpan {
    pub(crate) span_index: usize,
    pub(crate) text_byte_start: usize,
    pub(crate) span_byte_start: usize,
    pub(crate) span_byte_end: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SpanColumnPosition {
    pub(crate) span_index: usize,
    pub(crate) byte_index: usize,
}
