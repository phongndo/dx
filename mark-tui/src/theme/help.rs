use crate::keymap::GlobalAction;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HelpMenuKey {
    Static(&'static str),
    Global(GlobalAction),
    GlobalPair(GlobalAction, GlobalAction),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HelpMenuRow {
    Section(&'static str),
    Binding(HelpMenuKey, &'static str),
}

pub(crate) const HELP_MENU_ROWS: &[HelpMenuRow] = &[
    HelpMenuRow::Section("Global"),
    HelpMenuRow::Binding(HelpMenuKey::Global(GlobalAction::Help), "open keybindings"),
    HelpMenuRow::Binding(HelpMenuKey::Global(GlobalAction::Quit), "quit"),
    HelpMenuRow::Binding(HelpMenuKey::Static("Ctrl-C"), "force quit"),
    HelpMenuRow::Binding(HelpMenuKey::Static("Esc"), "close"),
    HelpMenuRow::Section("Navigate"),
    HelpMenuRow::Binding(HelpMenuKey::Static("j/k, ↑/↓"), "scroll"),
    HelpMenuRow::Binding(HelpMenuKey::Static("d/Ctrl-D/PgDn, u/PgUp"), "page"),
    HelpMenuRow::Binding(HelpMenuKey::Static("g/G, Home/End"), "top / bottom"),
    HelpMenuRow::Binding(HelpMenuKey::Static("h/l, ←/→"), "horizontal"),
    HelpMenuRow::Binding(
        HelpMenuKey::GlobalPair(GlobalAction::PreviousFile, GlobalAction::NextFile),
        "file",
    ),
    HelpMenuRow::Binding(
        HelpMenuKey::GlobalPair(GlobalAction::PreviousHunk, GlobalAction::NextHunk),
        "hunk",
    ),
    HelpMenuRow::Binding(
        HelpMenuKey::GlobalPair(
            GlobalAction::ExpandContextUp,
            GlobalAction::ExpandContextDown,
        ),
        "expand context",
    ),
    HelpMenuRow::Binding(
        HelpMenuKey::Global(GlobalAction::CollapseContextAll),
        "collapse context",
    ),
    HelpMenuRow::Binding(
        HelpMenuKey::Global(GlobalAction::EditHunk),
        "edit focused hunk",
    ),
    HelpMenuRow::Binding(
        HelpMenuKey::GlobalPair(GlobalAction::NextDiffType, GlobalAction::PreviousDiffType),
        "cycle diff type",
    ),
    HelpMenuRow::Section("Actions"),
    HelpMenuRow::Binding(
        HelpMenuKey::Global(GlobalAction::FileFilter),
        "filter files",
    ),
    HelpMenuRow::Binding(HelpMenuKey::Global(GlobalAction::Grep), "grep diff"),
    HelpMenuRow::Binding(HelpMenuKey::Static("n/p"), "next / previous grep match"),
    HelpMenuRow::Binding(
        HelpMenuKey::Global(GlobalAction::CopyErrorLog),
        "copy error log",
    ),
    HelpMenuRow::Binding(HelpMenuKey::Global(GlobalAction::CopyMarks), "copy marks"),
    HelpMenuRow::Binding(
        HelpMenuKey::Global(GlobalAction::FileBrowser),
        "toggle file sidebar",
    ),
    HelpMenuRow::Binding(HelpMenuKey::Global(GlobalAction::Layout), "split / unified"),
    HelpMenuRow::Binding(HelpMenuKey::Global(GlobalAction::Reload), "reload diff"),
    HelpMenuRow::Binding(HelpMenuKey::Global(GlobalAction::DiffMenu), "diff selector"),
    HelpMenuRow::Binding(
        HelpMenuKey::Global(GlobalAction::HeadBranch),
        "select head branch",
    ),
    HelpMenuRow::Binding(
        HelpMenuKey::Global(GlobalAction::BaseBranch),
        "select base branch",
    ),
    HelpMenuRow::Binding(
        HelpMenuKey::Global(GlobalAction::CommitPicker),
        "select commit",
    ),
    HelpMenuRow::Binding(
        HelpMenuKey::Global(GlobalAction::OptionsMenu),
        "settings menu",
    ),
    HelpMenuRow::Binding(
        HelpMenuKey::Global(GlobalAction::ClearFilters),
        "clear filters",
    ),
    HelpMenuRow::Binding(
        HelpMenuKey::GlobalPair(
            GlobalAction::PreviousAnnotation,
            GlobalAction::NextAnnotation,
        ),
        "previous / next annotation",
    ),
    HelpMenuRow::Section("Annotations"),
    HelpMenuRow::Binding(HelpMenuKey::Static("hover [+]"), "add / edit annotation"),
    HelpMenuRow::Binding(HelpMenuKey::Global(GlobalAction::SaveMark), "save mark"),
    HelpMenuRow::Binding(HelpMenuKey::Global(GlobalAction::CancelMark), "cancel mark"),
    HelpMenuRow::Binding(HelpMenuKey::Static("Enter"), "new annotation line"),
    HelpMenuRow::Binding(HelpMenuKey::Static("Cmd-←/→, Ctrl-A/E"), "line start / end"),
    HelpMenuRow::Binding(HelpMenuKey::Static("Alt-←/→"), "word left / right"),
    HelpMenuRow::Binding(HelpMenuKey::Static("Cmd-Delete"), "delete to line start"),
    HelpMenuRow::Section("Keybindings menu"),
    HelpMenuRow::Binding(HelpMenuKey::Static("type"), "filter keybindings"),
    HelpMenuRow::Binding(HelpMenuKey::Static("Backspace"), "delete char"),
    HelpMenuRow::Binding(HelpMenuKey::Static("Ctrl-U"), "clear filter"),
    HelpMenuRow::Binding(HelpMenuKey::Static("↑/↓"), "scroll list"),
    HelpMenuRow::Binding(HelpMenuKey::Static("Ctrl-N/Ctrl-P"), "scroll list"),
    HelpMenuRow::Binding(HelpMenuKey::Static("PgUp/PgDn"), "page list"),
    HelpMenuRow::Binding(HelpMenuKey::Static("Home/End"), "top / bottom"),
    HelpMenuRow::Binding(HelpMenuKey::Static("Esc"), "close"),
    HelpMenuRow::Section("Filter input"),
    HelpMenuRow::Binding(HelpMenuKey::Static("Enter"), "keep filter"),
    HelpMenuRow::Binding(HelpMenuKey::Static("Esc"), "clear active filters"),
    HelpMenuRow::Binding(HelpMenuKey::Static("Backspace"), "delete char"),
    HelpMenuRow::Binding(HelpMenuKey::Static("Cmd-←/→, Ctrl-A/E"), "line start / end"),
    HelpMenuRow::Binding(HelpMenuKey::Static("Alt-←/→"), "word left / right"),
    HelpMenuRow::Binding(HelpMenuKey::Static("Ctrl-U"), "clear input"),
    HelpMenuRow::Section("Branch filter"),
    HelpMenuRow::Binding(HelpMenuKey::Static("type"), "filter branches"),
    HelpMenuRow::Binding(HelpMenuKey::Static("Enter"), "select branch"),
    HelpMenuRow::Binding(HelpMenuKey::Static("Tab/Shift-Tab"), "cycle matches"),
    HelpMenuRow::Binding(HelpMenuKey::Static("Ctrl-N/Ctrl-P"), "cycle matches"),
    HelpMenuRow::Binding(HelpMenuKey::Static("Backspace"), "delete char"),
    HelpMenuRow::Binding(HelpMenuKey::Static("Ctrl-U"), "clear filter"),
    HelpMenuRow::Binding(HelpMenuKey::Static("↑/↓, PgUp/PgDn"), "move"),
    HelpMenuRow::Binding(HelpMenuKey::Static("Home/End"), "first / last match"),
];
