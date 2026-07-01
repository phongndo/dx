use mark_diff::{DiffOptions, DiffSource, PatchSource};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DiffChoice {
    Branch,
    Review,
    Show,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DiffFilterKind {
    File,
    Grep,
}

impl DiffChoice {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Branch => "Branch",
            Self::Review => "Review",
            Self::Show => "Show",
            Self::All => "All changes",
        }
    }
}

pub(crate) fn is_review_options(options: &DiffOptions) -> bool {
    matches!(
        &options.source,
        DiffSource::Patch(PatchSource::Review { .. })
    )
}
