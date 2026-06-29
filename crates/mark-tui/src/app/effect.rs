use super::{
    DiffApp, OptionsDraft, OptionsMenuItem, persist_options_menu_draft_to_path,
    write_osc52_clipboard,
};
use crate::render::compositor::ComponentEventResult;
use crate::toast::ToastLevel;
use mark_core::MarkResult;
use std::io;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AppEffect {
    Quit,
    Reload,
    OpenEditorShortcut,
    OpenFocusedHunkInEditor,
    Toast(ToastLevel, String),
    CopyToClipboard {
        text: String,
        success_message: String,
        error_prefix: String,
    },
    PersistOptionsMenuDraft {
        draft: OptionsDraft,
        changed_item: OptionsMenuItem,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) enum ActionOutcome {
    #[default]
    Ignored,
    Consumed {
        effects: Vec<AppEffect>,
    },
}

impl ActionOutcome {
    pub(crate) fn ignored() -> Self {
        Self::Ignored
    }

    pub(crate) fn consumed() -> Self {
        Self::Consumed {
            effects: Vec::new(),
        }
    }

    pub(crate) fn effect(effect: AppEffect) -> Self {
        Self::Consumed {
            effects: vec![effect],
        }
    }

    pub(crate) fn from_component_event_result(result: ComponentEventResult) -> Self {
        match result {
            ComponentEventResult::Ignored => Self::ignored(),
            ComponentEventResult::Consumed => Self::consumed(),
            ComponentEventResult::Effect(effect) => Self::effect(effect),
            ComponentEventResult::Quit => Self::effect(AppEffect::Quit),
        }
    }

    pub(crate) fn into_legacy_quit(self) -> Option<bool> {
        match self {
            Self::Ignored => None,
            Self::Consumed { effects } => Some(
                effects
                    .iter()
                    .any(|effect| matches!(effect, AppEffect::Quit)),
            ),
        }
    }

    pub(crate) fn extend_effects(&mut self, next_effects: Vec<AppEffect>) {
        if next_effects.is_empty() {
            return;
        }

        match self {
            Self::Ignored => {
                *self = Self::Consumed {
                    effects: next_effects,
                }
            }
            Self::Consumed { effects } => effects.extend(next_effects),
        }
    }

    pub(crate) fn into_effects(self) -> Vec<AppEffect> {
        match self {
            Self::Ignored => Vec::new(),
            Self::Consumed { effects } => effects,
        }
    }
}

impl DiffApp {
    pub(crate) fn queue_effect(&mut self, effect: AppEffect) {
        self.runtime.push_effect(effect);
    }

    pub(crate) fn take_queued_effects(&mut self) -> Vec<AppEffect> {
        self.runtime.take_effects()
    }

    pub(crate) fn run_effects(&mut self, effects: Vec<AppEffect>) -> MarkResult<()> {
        for effect in effects {
            self.run_effect(effect)?;
        }
        Ok(())
    }

    pub(crate) fn run_effect(&mut self, effect: AppEffect) -> MarkResult<()> {
        match effect {
            AppEffect::Quit => Ok(()),
            AppEffect::Reload => self.reload(),
            AppEffect::OpenEditorShortcut => {
                self.open_editor_shortcut(None);
                Ok(())
            }
            AppEffect::OpenFocusedHunkInEditor => {
                self.open_focused_hunk_in_editor();
                Ok(())
            }
            AppEffect::Toast(level, text) => {
                if self.notifications.push_toast(level, text) {
                    self.runtime.mark_dirty();
                }
                Ok(())
            }
            AppEffect::CopyToClipboard {
                text,
                success_message,
                error_prefix,
            } => {
                let mut stdout = io::stdout().lock();
                match write_osc52_clipboard(&mut stdout, &text) {
                    Ok(()) => self.set_success_notice(success_message),
                    Err(error) => self.set_error_log(format!("{error_prefix}: {error}")),
                }
                Ok(())
            }
            AppEffect::PersistOptionsMenuDraft {
                draft,
                changed_item,
            } => {
                #[cfg(test)]
                {
                    self.config.last_persisted_options_menu_draft = Some((draft, changed_item));
                }

                if !self.config.settings_persistence_enabled {
                    return Ok(());
                }

                let result = mark_syntax::settings_write_path().and_then(|path| {
                    persist_options_menu_draft_to_path(&path, draft, changed_item)
                });
                if let Err(error) = result {
                    self.set_error_log(format!("settings not saved: {error}"));
                }
                Ok(())
            }
        }
    }
}
