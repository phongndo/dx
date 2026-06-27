use crossterm::event::{KeyCode, KeyEvent};

mod bindings;

pub(crate) use bindings::KeyPress;
use bindings::{KeySequence, key_sequences, sequence_list_display_label};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GlobalAction {
    Help,
    Reload,
    FileFilter,
    Grep,
    DiffMenu,
    HeadBranch,
    BaseBranch,
    CommitPicker,
    OptionsMenu,
    FileBrowser,
    PreviousFile,
    NextFile,
    PreviousHunk,
    NextHunk,
    ExpandContextUp,
    ExpandContextDown,
    CollapseContextAll,
    Quit,
    Layout,
    EditHunk,
    SaveMark,
    CancelMark,
    CopyMarks,
    CopyErrorLog,
    ClearFilters,
    NextDiffType,
    PreviousDiffType,
    NextAnnotation,
    PreviousAnnotation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MenuAction {
    Up,
    Down,
    Select,
    Confirm,
    Close,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Keymap {
    global: Vec<Vec<KeySequence>>,
    menu: Vec<Vec<KeySequence>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GlobalConflictGroup {
    Normal,
    MarkDraft,
}

#[derive(Debug, Clone, Copy)]
struct GlobalActionSpec {
    action: GlobalAction,
    name: &'static str,
    defaults: &'static [&'static str],
    max_keys: usize,
    conflict_group: GlobalConflictGroup,
}

#[derive(Debug, Clone, Copy)]
struct MenuActionSpec {
    action: MenuAction,
    name: &'static str,
    defaults: &'static [&'static str],
}

macro_rules! global_action_spec {
    ($action:expr, $name:expr, [$($default:expr),* $(,)?]) => {
        GlobalActionSpec {
            action: $action,
            name: $name,
            defaults: &[$($default),*],
            max_keys: 2,
            conflict_group: GlobalConflictGroup::Normal,
        }
    };
    ($action:expr, $name:expr, [$($default:expr),* $(,)?], $max_keys:expr) => {
        GlobalActionSpec {
            action: $action,
            name: $name,
            defaults: &[$($default),*],
            max_keys: $max_keys,
            conflict_group: GlobalConflictGroup::Normal,
        }
    };
    ($action:expr, $name:expr, [$($default:expr),* $(,)?], $max_keys:expr, $conflict_group:expr) => {
        GlobalActionSpec {
            action: $action,
            name: $name,
            defaults: &[$($default),*],
            max_keys: $max_keys,
            conflict_group: $conflict_group,
        }
    };
}

macro_rules! menu_action_spec {
    ($action:expr, $name:expr, [$($default:expr),* $(,)?]) => {
        MenuActionSpec {
            action: $action,
            name: $name,
            defaults: &[$($default),*],
        }
    };
}

const GLOBAL_ACTION_SPECS: &[GlobalActionSpec] = &[
    global_action_spec!(GlobalAction::Help, "help", ["?"]),
    global_action_spec!(GlobalAction::Reload, "reload", ["r"]),
    global_action_spec!(GlobalAction::FileFilter, "file_filter", ["f"]),
    global_action_spec!(GlobalAction::Grep, "grep", ["/"]),
    global_action_spec!(GlobalAction::DiffMenu, "diff_menu", ["m m"]),
    global_action_spec!(GlobalAction::HeadBranch, "head_branch", ["m h"]),
    global_action_spec!(GlobalAction::BaseBranch, "base_branch", ["m b"]),
    global_action_spec!(GlobalAction::CommitPicker, "commit_picker", ["m c"]),
    global_action_spec!(GlobalAction::OptionsMenu, "options_menu", ["o"]),
    global_action_spec!(GlobalAction::FileBrowser, "file_browser", ["b"]),
    global_action_spec!(GlobalAction::PreviousFile, "previous_file", ["("]),
    global_action_spec!(GlobalAction::NextFile, "next_file", [")"]),
    global_action_spec!(GlobalAction::PreviousHunk, "previous_hunk", ["["]),
    global_action_spec!(GlobalAction::NextHunk, "next_hunk", ["]"]),
    global_action_spec!(GlobalAction::ExpandContextUp, "expand_context_up", [","]),
    global_action_spec!(
        GlobalAction::ExpandContextDown,
        "expand_context_down",
        ["."]
    ),
    global_action_spec!(
        GlobalAction::CollapseContextAll,
        "collapse_context_all",
        ["c"]
    ),
    global_action_spec!(GlobalAction::Quit, "quit", ["q"]),
    global_action_spec!(GlobalAction::Layout, "layout", ["s"]),
    global_action_spec!(GlobalAction::EditHunk, "edit_hunk", ["ctrl-g"], 1),
    global_action_spec!(
        GlobalAction::SaveMark,
        "save_mark",
        ["ctrl-s"],
        1,
        GlobalConflictGroup::MarkDraft
    ),
    global_action_spec!(
        GlobalAction::CancelMark,
        "cancel_mark",
        ["esc"],
        1,
        GlobalConflictGroup::MarkDraft
    ),
    global_action_spec!(GlobalAction::CopyMarks, "copy_marks", ["y"]),
    global_action_spec!(
        GlobalAction::CopyErrorLog,
        "copy_error_log",
        ["ctrl-shift-c"]
    ),
    global_action_spec!(GlobalAction::ClearFilters, "clear_filters", ["ctrl-u"]),
    global_action_spec!(GlobalAction::NextDiffType, "next_diff_type", ["tab"]),
    global_action_spec!(
        GlobalAction::PreviousDiffType,
        "previous_diff_type",
        ["shift-tab"]
    ),
    global_action_spec!(GlobalAction::NextAnnotation, "next_annotation", ["}"]),
    global_action_spec!(
        GlobalAction::PreviousAnnotation,
        "previous_annotation",
        ["{"]
    ),
];

const MENU_ACTION_SPECS: &[MenuActionSpec] = &[
    menu_action_spec!(MenuAction::Up, "up", ["up", "shift-tab", "ctrl-p"]),
    menu_action_spec!(MenuAction::Down, "down", ["down", "tab", "ctrl-n"]),
    menu_action_spec!(MenuAction::Select, "select", []),
    menu_action_spec!(MenuAction::Confirm, "confirm", ["enter"]),
    menu_action_spec!(MenuAction::Close, "close", ["esc"]),
];

impl Default for Keymap {
    fn default() -> Self {
        Self {
            global: GLOBAL_ACTION_SPECS
                .iter()
                .map(|spec| key_sequences(spec.defaults))
                .collect(),
            menu: MENU_ACTION_SPECS
                .iter()
                .map(|spec| key_sequences(spec.defaults))
                .collect(),
        }
    }
}

impl Keymap {
    fn has_sequence_starting_with(&self, prefix: KeyPress) -> bool {
        GLOBAL_ACTION_SPECS
            .iter()
            .map(|spec| self.global_sequences(spec.action))
            .any(|bindings| {
                bindings
                    .iter()
                    .any(|sequence| sequence.0.len() == 2 && sequence.0.first() == Some(&prefix))
            })
    }

    pub(crate) fn is_prefix(&self, key: KeyEvent) -> bool {
        self.has_sequence_starting_with(KeyPress::from(key))
    }

    pub(crate) fn matches_single(&self, action: GlobalAction, key: KeyEvent) -> bool {
        let key = KeyPress::from(key);
        self.global_sequences(action)
            .iter()
            .any(|sequence| sequence.0.as_slice() == [key])
    }

    pub(crate) fn matches_prefix(
        &self,
        action: GlobalAction,
        prefix: KeyPress,
        key: KeyEvent,
    ) -> bool {
        let key = KeyPress::from(key);
        self.global_sequences(action)
            .iter()
            .any(|sequence| sequence.0.as_slice() == [prefix, key])
    }

    pub(crate) fn global_action_label(&self, action: GlobalAction) -> String {
        sequence_list_display_label(self.global_sequences(action))
    }

    pub(crate) fn matches_menu(&self, action: MenuAction, key: KeyEvent) -> bool {
        let key = KeyPress::from(key);
        self.menu_sequences(action)
            .iter()
            .any(|sequence| sequence.0.as_slice() == [key])
    }

    /// Menu up/down for scrollable overlays that intentionally ignore Tab / Shift-Tab.
    pub(crate) fn matches_help_menu_scroll(&self, action: MenuAction, key: KeyEvent) -> bool {
        if matches!(key.code, KeyCode::Tab | KeyCode::BackTab) {
            return false;
        }
        self.matches_menu(action, key)
    }

    fn global_sequences(&self, action: GlobalAction) -> &Vec<KeySequence> {
        &self.global[global_action_index(action)]
    }

    fn global_sequences_mut(&mut self, action: GlobalAction) -> &mut Vec<KeySequence> {
        &mut self.global[global_action_index(action)]
    }

    fn menu_sequences(&self, action: MenuAction) -> &Vec<KeySequence> {
        &self.menu[menu_action_index(action)]
    }

    fn menu_sequences_mut(&mut self, action: MenuAction) -> &mut Vec<KeySequence> {
        &mut self.menu[menu_action_index(action)]
    }
}

fn global_action_index(action: GlobalAction) -> usize {
    GLOBAL_ACTION_SPECS
        .iter()
        .position(|spec| spec.action == action)
        .expect("global action should have a spec")
}

fn menu_action_index(action: MenuAction) -> usize {
    MENU_ACTION_SPECS
        .iter()
        .position(|spec| spec.action == action)
        .expect("menu action should have a spec")
}

#[cfg(test)]
mod tests;
