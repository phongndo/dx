use super::{ActionOutcome, AppEffect, DiffApp};
use crate::controls::{BranchMenu, DiffFilterKind};
use crate::keymap::GlobalAction;
use crate::toast::ToastLevel;
use mark_core::MarkResult;

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
    OpenAnnotationMenu,
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
            GlobalAction::AnnotationMenu => Self::OpenAnnotationMenu,
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
        let outcome = self.perform_app_action_with_effects(action)?;
        let legacy = outcome.clone().into_legacy_quit();
        self.run_effects(outcome.into_effects())?;
        Ok(legacy)
    }

    pub(crate) fn perform_app_action_with_effects(
        &mut self,
        action: AppAction,
    ) -> MarkResult<ActionOutcome> {
        match action {
            AppAction::Quit => Ok(ActionOutcome::effect(AppEffect::Quit)),
            AppAction::ToggleHelp => {
                self.toggle_help_menu();
                Ok(ActionOutcome::consumed())
            }
            AppAction::Reload => Ok(ActionOutcome::effect(AppEffect::Reload)),
            AppAction::OpenFileFilter => {
                self.open_filter_input(DiffFilterKind::File);
                Ok(ActionOutcome::consumed())
            }
            AppAction::OpenGrepFilter => {
                self.open_filter_input(DiffFilterKind::Grep);
                Ok(ActionOutcome::consumed())
            }
            AppAction::OpenDiffMenu => {
                self.open_diff_menu();
                Ok(ActionOutcome::consumed())
            }
            AppAction::ToggleHeadBranchMenu => {
                self.toggle_branch_menu(BranchMenu::Head);
                Ok(ActionOutcome::consumed())
            }
            AppAction::ToggleBaseBranchMenu => {
                self.toggle_branch_menu(BranchMenu::Base);
                Ok(ActionOutcome::consumed())
            }
            AppAction::ToggleCommitMenu => {
                self.toggle_commit_menu();
                Ok(ActionOutcome::consumed())
            }
            AppAction::OpenOptionsMenu => {
                self.open_options_menu();
                Ok(ActionOutcome::consumed())
            }
            AppAction::OpenAnnotationMenu => {
                self.open_annotation_menu();
                Ok(ActionOutcome::consumed())
            }
            AppAction::ToggleFileSidebar => {
                self.toggle_file_sidebar();
                Ok(ActionOutcome::consumed())
            }
            AppAction::PreviousFile => {
                self.move_file(-1);
                Ok(ActionOutcome::consumed())
            }
            AppAction::NextFile => {
                self.move_file(1);
                Ok(ActionOutcome::consumed())
            }
            AppAction::PreviousHunk => {
                self.previous_hunk();
                Ok(ActionOutcome::consumed())
            }
            AppAction::NextHunk => {
                self.next_hunk();
                Ok(ActionOutcome::consumed())
            }
            AppAction::ExpandContextUp => {
                self.expand_context_around_focused_hunk(-1);
                Ok(ActionOutcome::consumed())
            }
            AppAction::ExpandContextDown => {
                self.expand_context_around_focused_hunk(1);
                Ok(ActionOutcome::consumed())
            }
            AppAction::CollapseContextAll => {
                self.collapse_all_context();
                Ok(ActionOutcome::consumed())
            }
            AppAction::ToggleLayout => {
                self.toggle_layout();
                Ok(ActionOutcome::consumed())
            }
            AppAction::EditHunk => Ok(ActionOutcome::effect(AppEffect::OpenFocusedHunkInEditor)),
            AppAction::CopyMarks => {
                let Some(text) = self.marks_clipboard_json() else {
                    return Ok(ActionOutcome::effect(AppEffect::Toast(
                        ToastLevel::Warning,
                        "no marks to copy".to_owned(),
                    )));
                };
                Ok(ActionOutcome::effect(AppEffect::CopyToClipboard {
                    text,
                    success_message: "marks copied".to_owned(),
                    error_prefix: "marks copy failed".to_owned(),
                }))
            }
            AppAction::CopyErrorLog => {
                let Some(text) = self.notifications.error_log.clone() else {
                    return Ok(ActionOutcome::ignored());
                };
                Ok(ActionOutcome::effect(AppEffect::CopyToClipboard {
                    text,
                    success_message: "error log copied".to_owned(),
                    error_prefix: "error log copy failed".to_owned(),
                }))
            }
            AppAction::ClearFilters => {
                self.clear_all_filters();
                self.filters.filter_input = None;
                Ok(ActionOutcome::consumed())
            }
            AppAction::NextDiffType => {
                self.cycle_diff_choice(1);
                Ok(ActionOutcome::consumed())
            }
            AppAction::PreviousDiffType => {
                self.cycle_diff_choice(-1);
                Ok(ActionOutcome::consumed())
            }
            AppAction::NextAnnotation => {
                self.move_annotation(1);
                Ok(ActionOutcome::consumed())
            }
            AppAction::PreviousAnnotation => {
                self.move_annotation(-1);
                Ok(ActionOutcome::consumed())
            }
        }
    }
}
