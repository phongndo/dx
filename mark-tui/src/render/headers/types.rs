use ratatui::prelude::Style;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HeaderSpanPart {
    pub(crate) text: String,
    pub(crate) style: Style,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeltaKind {
    Addition,
    Deletion,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeltaPart {
    pub(crate) text: String,
    pub(crate) kind: DeltaKind,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct HeaderStyles {
    pub(crate) prefix: Style,
    pub(crate) body: Style,
    pub(crate) fill: Style,
    pub(crate) addition: Style,
    pub(crate) deletion: Style,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FittedPrefixedParts {
    pub(crate) prefix: String,
    pub(crate) gap: bool,
    pub(crate) body: String,
}
