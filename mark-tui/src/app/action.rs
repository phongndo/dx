use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AppAction {
    Quit,
    ToggleHelp,
    Reload,
    OpenFileFilter,
    OpenGrepFilter,
    OpenDiffMenu,
    ToggleHeadBranchMenu,
    ToggleBaseBranchMenu,
    ToggleCommitMenu,
    OpenOptionsMenu,
    ToggleFileSidebar,
    PreviousFile,
    NextFile,
    PreviousHunk,
    NextHunk,
    ExpandContextUp,
    ExpandContextDown,
    CollapseContextAll,
    ToggleLayout,
    EditHunk,
    CopyMarks,
    CopyErrorLog,
    ClearFilters,
    NextDiffType,
    PreviousDiffType,
    NextAnnotation,
    PreviousAnnotation,
}

impl AppAction {
    pub(crate) fn from_global(action: GlobalAction) -> Option<Self> {
        Some(match action {
            GlobalAction::Quit => Self::Quit,
            GlobalAction::Help => Self::ToggleHelp,
            GlobalAction::Reload => Self::Reload,
            GlobalAction::FileFilter => Self::OpenFileFilter,
            GlobalAction::Grep => Self::OpenGrepFilter,
            GlobalAction::DiffMenu => Self::OpenDiffMenu,
            GlobalAction::HeadBranch => Self::ToggleHeadBranchMenu,
            GlobalAction::BaseBranch => Self::ToggleBaseBranchMenu,
            GlobalAction::CommitPicker => Self::ToggleCommitMenu,
            GlobalAction::OptionsMenu => Self::OpenOptionsMenu,
            GlobalAction::FileBrowser => Self::ToggleFileSidebar,
            GlobalAction::PreviousFile => Self::PreviousFile,
            GlobalAction::NextFile => Self::NextFile,
            GlobalAction::PreviousHunk => Self::PreviousHunk,
            GlobalAction::NextHunk => Self::NextHunk,
            GlobalAction::ExpandContextUp => Self::ExpandContextUp,
            GlobalAction::ExpandContextDown => Self::ExpandContextDown,
            GlobalAction::CollapseContextAll => Self::CollapseContextAll,
            GlobalAction::Layout => Self::ToggleLayout,
            GlobalAction::EditHunk => Self::EditHunk,
            GlobalAction::CopyMarks => Self::CopyMarks,
            GlobalAction::CopyErrorLog => Self::CopyErrorLog,
            GlobalAction::ClearFilters => Self::ClearFilters,
            GlobalAction::NextDiffType => Self::NextDiffType,
            GlobalAction::PreviousDiffType => Self::PreviousDiffType,
            GlobalAction::NextAnnotation => Self::NextAnnotation,
            GlobalAction::PreviousAnnotation => Self::PreviousAnnotation,
            GlobalAction::SaveMark | GlobalAction::CancelMark => return None,
        })
    }
}

impl DiffApp {
    pub(crate) fn perform_app_action(&mut self, action: AppAction) -> MarkResult<Option<bool>> {
        match action {
            AppAction::Quit => Ok(Some(true)),
            AppAction::ToggleHelp => {
                self.toggle_help_menu();
                Ok(Some(false))
            }
            AppAction::Reload => {
                self.reload()?;
                Ok(Some(false))
            }
            AppAction::OpenFileFilter => {
                self.open_filter_input(DiffFilterKind::File);
                Ok(Some(false))
            }
            AppAction::OpenGrepFilter => {
                self.open_filter_input(DiffFilterKind::Grep);
                Ok(Some(false))
            }
            AppAction::OpenDiffMenu => {
                self.open_diff_menu();
                Ok(Some(false))
            }
            AppAction::ToggleHeadBranchMenu => {
                self.toggle_branch_menu(BranchMenu::Head);
                Ok(Some(false))
            }
            AppAction::ToggleBaseBranchMenu => {
                self.toggle_branch_menu(BranchMenu::Base);
                Ok(Some(false))
            }
            AppAction::ToggleCommitMenu => {
                self.toggle_commit_menu();
                Ok(Some(false))
            }
            AppAction::OpenOptionsMenu => {
                self.open_options_menu();
                Ok(Some(false))
            }
            AppAction::ToggleFileSidebar => {
                self.toggle_file_sidebar();
                Ok(Some(false))
            }
            AppAction::PreviousFile => {
                self.move_file(-1);
                Ok(Some(false))
            }
            AppAction::NextFile => {
                self.move_file(1);
                Ok(Some(false))
            }
            AppAction::PreviousHunk => {
                self.previous_hunk();
                Ok(Some(false))
            }
            AppAction::NextHunk => {
                self.next_hunk();
                Ok(Some(false))
            }
            AppAction::ExpandContextUp => {
                self.expand_context_around_focused_hunk(-1);
                Ok(Some(false))
            }
            AppAction::ExpandContextDown => {
                self.expand_context_around_focused_hunk(1);
                Ok(Some(false))
            }
            AppAction::CollapseContextAll => {
                self.collapse_all_context();
                Ok(Some(false))
            }
            AppAction::ToggleLayout => {
                self.toggle_layout();
                Ok(Some(false))
            }
            AppAction::EditHunk => {
                self.open_focused_hunk_in_editor();
                Ok(Some(false))
            }
            AppAction::CopyMarks => {
                self.copy_marks_to_terminal_clipboard();
                Ok(Some(false))
            }
            AppAction::CopyErrorLog => {
                if self.notifications.error_log.is_none() {
                    return Ok(None);
                }
                self.copy_error_log_to_terminal_clipboard();
                Ok(Some(false))
            }
            AppAction::ClearFilters => {
                self.clear_all_filters();
                self.filters.filter_input = None;
                Ok(Some(false))
            }
            AppAction::NextDiffType => {
                self.cycle_diff_choice(1);
                Ok(Some(false))
            }
            AppAction::PreviousDiffType => {
                self.cycle_diff_choice(-1);
                Ok(Some(false))
            }
            AppAction::NextAnnotation => {
                self.move_annotation(1);
                Ok(Some(false))
            }
            AppAction::PreviousAnnotation => {
                self.move_annotation(-1);
                Ok(Some(false))
            }
        }
    }
}
