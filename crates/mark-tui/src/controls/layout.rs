use std::io;

use ratatui::{Terminal, backend::CrosstermBackend};

use crate::theme::MIN_SPLIT_WIDTH;

pub(crate) type CrosstermTerminal = Terminal<CrosstermBackend<io::Stdout>>;

pub(crate) const INPUT_CURSOR: &str = "█";

pub(crate) fn default_layout_for_width(width: u16) -> DiffLayoutMode {
    if width >= MIN_SPLIT_WIDTH {
        DiffLayoutMode::Split
    } else {
        DiffLayoutMode::Unified
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DiffLayoutMode {
    Split,
    Unified,
}

impl DiffLayoutMode {
    pub(crate) fn toggled(self) -> Self {
        match self {
            Self::Split => Self::Unified,
            Self::Unified => Self::Split,
        }
    }
}
