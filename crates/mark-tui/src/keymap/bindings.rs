use std::{collections::HashMap, fs};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mark_core::{MarkError, MarkResult};
use serde::Deserialize;

use super::{
    GLOBAL_ACTION_SPECS, GlobalAction, GlobalConflictGroup, Keymap, MENU_ACTION_SPECS, MenuAction,
};

impl Keymap {
    pub(crate) fn load() -> MarkResult<Self> {
        let path = mark_syntax::settings_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }

        let contents = fs::read_to_string(&path)?;
        Self::parse(&contents).map_err(|error| {
            MarkError::Usage(format!(
                "failed to parse keymap in {}: {error}",
                path.display()
            ))
        })
    }

    pub(crate) fn parse(contents: &str) -> Result<Self, String> {
        let stored: StoredConfig = toml::from_str(contents).map_err(|error| error.to_string())?;
        Self::from_stored(stored.keymap.unwrap_or_default())
    }

    fn from_stored(stored: StoredKeymap) -> Result<Self, String> {
        let mut keymap = Self::default();
        let mut stored_global = stored.global;
        let mut stored_menu = stored.menu;
        let copy_marks_configured = stored_global.copy_marks.is_some();

        if let Some(leader) = stored_global.leader.take() {
            parse_key_press(&leader)?;
        }

        for spec in GLOBAL_ACTION_SPECS {
            let configured = stored_global.take(spec.action);
            set_sequences(keymap.global_sequences_mut(spec.action), configured)?;
        }
        if !copy_marks_configured {
            keymap.clear_default_copy_marks_on_conflict();
        }

        for spec in MENU_ACTION_SPECS {
            let configured = stored_menu.take(spec.action);
            set_sequences(keymap.menu_sequences_mut(spec.action), configured)?;
        }

        keymap.validate()?;
        Ok(keymap)
    }

    fn validate(&self) -> Result<(), String> {
        for spec in GLOBAL_ACTION_SPECS {
            for sequence in self.global_sequences(spec.action) {
                if sequence.0.is_empty() || sequence.0.len() > spec.max_keys {
                    let keys = if spec.max_keys == 1 {
                        "a single key"
                    } else {
                        "one or two keys"
                    };
                    return Err(format!("keymap.global.{} must be {keys}", spec.name));
                }
            }
        }

        for spec in MENU_ACTION_SPECS {
            for sequence in self.menu_sequences(spec.action) {
                if sequence.0.len() != 1 {
                    return Err(format!("keymap.menu.{} must be a single key", spec.name));
                }
            }
        }

        self.validate_global_conflicts()?;
        self.validate_mark_draft_conflicts()?;
        self.validate_menu_conflicts()?;

        Ok(())
    }

    fn validate_global_conflicts(&self) -> Result<(), String> {
        // Save/cancel are draft-only: they run before normal global actions only
        // while composing an annotation, so they may share keys with globals.
        let bindings = GLOBAL_ACTION_SPECS
            .iter()
            .filter(|spec| spec.conflict_group == GlobalConflictGroup::Normal)
            .filter(|spec| {
                !matches!(
                    spec.action,
                    GlobalAction::SaveMark | GlobalAction::CancelMark
                )
            })
            .map(|spec| (spec.name, self.global_sequences(spec.action)))
            .collect::<Vec<_>>();
        validate_conflicts("keymap.global", &bindings)?;
        validate_prefix_conflicts("keymap.global", &bindings)
    }

    fn validate_mark_draft_conflicts(&self) -> Result<(), String> {
        let bindings = GLOBAL_ACTION_SPECS
            .iter()
            .filter(|spec| spec.conflict_group == GlobalConflictGroup::MarkDraft)
            .map(|spec| (spec.name, self.global_sequences(spec.action)))
            .collect::<Vec<_>>();
        validate_conflicts("keymap.global", &bindings)
    }

    fn validate_menu_conflicts(&self) -> Result<(), String> {
        let bindings = MENU_ACTION_SPECS
            .iter()
            .map(|spec| (spec.name, self.menu_sequences(spec.action)))
            .collect::<Vec<_>>();
        validate_conflicts("keymap.menu", &bindings)
    }

    fn clear_default_copy_marks_on_conflict(&mut self) {
        let copy_marks = self.global_sequences(GlobalAction::CopyMarks);
        let conflicts = GLOBAL_ACTION_SPECS
            .iter()
            .filter(|spec| spec.action != GlobalAction::CopyMarks)
            .map(|spec| self.global_sequences(spec.action))
            .any(|bindings| {
                bindings.iter().any(|sequence| {
                    copy_marks
                        .iter()
                        .any(|copy| sequences_conflict(copy, sequence))
                })
            });
        if conflicts {
            self.global_sequences_mut(GlobalAction::CopyMarks).clear();
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct KeyPress {
    code: KeyCode,
    modifiers: KeyModifiers,
}

impl KeyPress {
    fn new(mut code: KeyCode, modifiers: KeyModifiers) -> Self {
        if modifiers.contains(KeyModifiers::SHIFT)
            && let KeyCode::Char(character) = code
            && character.is_ascii_alphabetic()
        {
            code = KeyCode::Char(character.to_ascii_uppercase());
        }
        Self {
            code,
            modifiers: normalize_modifiers(code, modifiers),
        }
    }
}

impl From<KeyEvent> for KeyPress {
    fn from(key: KeyEvent) -> Self {
        Self::new(key.code, key.modifiers)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::keymap) struct KeySequence(pub(in crate::keymap) Vec<KeyPress>);

#[derive(Debug, Default, Deserialize)]
struct StoredConfig {
    #[serde(default)]
    keymap: Option<StoredKeymap>,
}

#[derive(Debug, Default, Deserialize)]
struct StoredKeymap {
    #[serde(default)]
    global: StoredGlobalKeymap,
    #[serde(default)]
    menu: StoredMenuKeymap,
}

#[derive(Debug, Default, Deserialize)]
struct StoredGlobalKeymap {
    leader: Option<String>,
    help: Option<KeySpec>,
    reload: Option<KeySpec>,
    file_filter: Option<KeySpec>,
    grep: Option<KeySpec>,
    diff_menu: Option<KeySpec>,
    head_branch: Option<KeySpec>,
    base_branch: Option<KeySpec>,
    commit_picker: Option<KeySpec>,
    options_menu: Option<KeySpec>,
    file_browser: Option<KeySpec>,
    #[serde(alias = "prev_file")]
    previous_file: Option<KeySpec>,
    next_file: Option<KeySpec>,
    #[serde(alias = "prev_hunk")]
    previous_hunk: Option<KeySpec>,
    next_hunk: Option<KeySpec>,
    expand_context_up: Option<KeySpec>,
    expand_context_down: Option<KeySpec>,
    collapse_context_all: Option<KeySpec>,
    quit: Option<KeySpec>,
    layout: Option<KeySpec>,
    edit_hunk: Option<KeySpec>,
    save_mark: Option<KeySpec>,
    cancel_mark: Option<KeySpec>,
    copy_marks: Option<KeySpec>,
    copy_error_log: Option<KeySpec>,
    clear_filters: Option<KeySpec>,
    next_diff_type: Option<KeySpec>,
    #[serde(alias = "prev_diff_type")]
    previous_diff_type: Option<KeySpec>,
    next_annotation: Option<KeySpec>,
    previous_annotation: Option<KeySpec>,
}

impl StoredGlobalKeymap {
    fn take(&mut self, action: GlobalAction) -> Option<KeySpec> {
        match action {
            GlobalAction::Help => self.help.take(),
            GlobalAction::Reload => self.reload.take(),
            GlobalAction::FileFilter => self.file_filter.take(),
            GlobalAction::Grep => self.grep.take(),
            GlobalAction::DiffMenu => self.diff_menu.take(),
            GlobalAction::HeadBranch => self.head_branch.take(),
            GlobalAction::BaseBranch => self.base_branch.take(),
            GlobalAction::CommitPicker => self.commit_picker.take(),
            GlobalAction::OptionsMenu => self.options_menu.take(),
            GlobalAction::FileBrowser => self.file_browser.take(),
            GlobalAction::PreviousFile => self.previous_file.take(),
            GlobalAction::NextFile => self.next_file.take(),
            GlobalAction::PreviousHunk => self.previous_hunk.take(),
            GlobalAction::NextHunk => self.next_hunk.take(),
            GlobalAction::ExpandContextUp => self.expand_context_up.take(),
            GlobalAction::ExpandContextDown => self.expand_context_down.take(),
            GlobalAction::CollapseContextAll => self.collapse_context_all.take(),
            GlobalAction::Quit => self.quit.take(),
            GlobalAction::Layout => self.layout.take(),
            GlobalAction::EditHunk => self.edit_hunk.take(),
            GlobalAction::SaveMark => self.save_mark.take(),
            GlobalAction::CancelMark => self.cancel_mark.take(),
            GlobalAction::CopyMarks => self.copy_marks.take(),
            GlobalAction::CopyErrorLog => self.copy_error_log.take(),
            GlobalAction::ClearFilters => self.clear_filters.take(),
            GlobalAction::NextDiffType => self.next_diff_type.take(),
            GlobalAction::PreviousDiffType => self.previous_diff_type.take(),
            GlobalAction::NextAnnotation => self.next_annotation.take(),
            GlobalAction::PreviousAnnotation => self.previous_annotation.take(),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct StoredMenuKeymap {
    up: Option<KeySpec>,
    down: Option<KeySpec>,
    select: Option<KeySpec>,
    confirm: Option<KeySpec>,
    close: Option<KeySpec>,
}

impl StoredMenuKeymap {
    fn take(&mut self, action: MenuAction) -> Option<KeySpec> {
        match action {
            MenuAction::Up => self.up.take(),
            MenuAction::Down => self.down.take(),
            MenuAction::Select => self.select.take(),
            MenuAction::Confirm => self.confirm.take(),
            MenuAction::Close => self.close.take(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum KeySpec {
    One(String),
    Many(Vec<String>),
}

impl KeySpec {
    fn into_strings(self) -> Vec<String> {
        match self {
            Self::One(key) => vec![key],
            Self::Many(keys) => keys,
        }
    }
}

fn set_sequences(target: &mut Vec<KeySequence>, spec: Option<KeySpec>) -> Result<(), String> {
    if let Some(spec) = spec {
        *target = spec
            .into_strings()
            .into_iter()
            .map(|sequence| parse_key_sequence(&sequence))
            .collect::<Result<_, _>>()?;
    }
    Ok(())
}

pub(in crate::keymap) fn key_sequences(keys: &[&str]) -> Vec<KeySequence> {
    keys.iter()
        .map(|key| parse_key_sequence(key).expect("default keymap should parse"))
        .collect()
}

fn validate_conflicts(context: &str, bindings: &[(&str, &Vec<KeySequence>)]) -> Result<(), String> {
    let mut seen = HashMap::new();
    for (action, sequences) in bindings.iter().copied() {
        for sequence in sequences {
            let key = sequence_label(sequence);
            if let Some(previous) = seen.insert(key.clone(), action) {
                if previous != action {
                    return Err(format!(
                        "{context} conflict: `{key}` is bound to both {previous} and {action}"
                    ));
                }
            }
        }
    }
    Ok(())
}

fn sequences_conflict(first: &KeySequence, second: &KeySequence) -> bool {
    first == second
        || matches!(
            (first.0.as_slice(), second.0.as_slice()),
            ([single], [prefix, _]) | ([prefix, _], [single]) if single == prefix
        )
}

fn validate_prefix_conflicts(
    context: &str,
    bindings: &[(&str, &Vec<KeySequence>)],
) -> Result<(), String> {
    let mut singles = HashMap::new();
    let mut prefixes = HashMap::new();

    for (action, sequences) in bindings.iter().copied() {
        for sequence in sequences {
            match sequence.0.as_slice() {
                [key] => {
                    singles.insert(key_label(key), action);
                }
                [prefix, _] => {
                    prefixes.insert(key_label(prefix), action);
                }
                _ => {}
            }
        }
    }

    for (prefix, prefix_action) in prefixes {
        if let Some(single_action) = singles.get(&prefix) {
            return Err(format!(
                "{context} conflict: `{prefix}` is both a binding for {single_action} and a prefix for {prefix_action}"
            ));
        }
    }

    Ok(())
}

fn sequence_label(sequence: &KeySequence) -> String {
    sequence
        .0
        .iter()
        .map(key_label)
        .collect::<Vec<_>>()
        .join(" ")
}

pub(in crate::keymap) fn sequence_list_display_label(sequences: &[KeySequence]) -> String {
    if sequences.is_empty() {
        return "unbound".to_owned();
    }

    sequences
        .iter()
        .map(sequence_display_label)
        .collect::<Vec<_>>()
        .join(", ")
}

fn sequence_display_label(sequence: &KeySequence) -> String {
    sequence
        .0
        .iter()
        .map(key_display_label)
        .collect::<Vec<_>>()
        .join(" ")
}

fn key_display_label(key: &KeyPress) -> String {
    let mut parts = Vec::new();
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        parts.push("Ctrl".to_owned());
    }
    if key.modifiers.contains(KeyModifiers::ALT) {
        parts.push("Alt".to_owned());
    }
    let shifted_modified_char = matches!(
        key.code,
        KeyCode::Char(character)
            if character.is_ascii_uppercase()
                && (key.modifiers.contains(KeyModifiers::CONTROL)
                    || key.modifiers.contains(KeyModifiers::ALT))
    );
    if key.modifiers.contains(KeyModifiers::SHIFT) || shifted_modified_char {
        parts.push("Shift".to_owned());
    }
    let has_modifier = !parts.is_empty();
    let key_label = match key.code {
        KeyCode::Char(' ') => "Space".to_owned(),
        KeyCode::Char(character) if has_modifier && character.is_ascii_alphabetic() => {
            character.to_ascii_uppercase().to_string()
        }
        KeyCode::Char(character) => character.to_string(),
        KeyCode::Enter => "Enter".to_owned(),
        KeyCode::Esc => "Esc".to_owned(),
        KeyCode::Tab => "Tab".to_owned(),
        KeyCode::BackTab => "Shift-Tab".to_owned(),
        KeyCode::Up => "Up".to_owned(),
        KeyCode::Down => "Down".to_owned(),
        KeyCode::Left => "Left".to_owned(),
        KeyCode::Right => "Right".to_owned(),
        KeyCode::Home => "Home".to_owned(),
        KeyCode::End => "End".to_owned(),
        KeyCode::PageUp => "PgUp".to_owned(),
        KeyCode::PageDown => "PgDn".to_owned(),
        KeyCode::Backspace => "Backspace".to_owned(),
        _ => format!("{:?}", key.code),
    };
    parts.push(key_label);
    parts.join("-")
}

fn key_label(key: &KeyPress) -> String {
    let mut parts = Vec::new();
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        parts.push("ctrl".to_owned());
    }
    if key.modifiers.contains(KeyModifiers::ALT) {
        parts.push("alt".to_owned());
    }
    let shifted_modified_char = matches!(
        key.code,
        KeyCode::Char(character)
            if character.is_ascii_uppercase()
                && (key.modifiers.contains(KeyModifiers::CONTROL)
                    || key.modifiers.contains(KeyModifiers::ALT))
    );
    if key.modifiers.contains(KeyModifiers::SHIFT) || shifted_modified_char {
        parts.push("shift".to_owned());
    }
    parts.push(match key.code {
        KeyCode::Char(' ') => "space".to_owned(),
        KeyCode::Char(character) if shifted_modified_char => {
            character.to_ascii_lowercase().to_string()
        }
        KeyCode::Char(character) => character.to_string(),
        KeyCode::Enter => "enter".to_owned(),
        KeyCode::Esc => "esc".to_owned(),
        KeyCode::Tab => "tab".to_owned(),
        KeyCode::BackTab => "shift-tab".to_owned(),
        KeyCode::Up => "up".to_owned(),
        KeyCode::Down => "down".to_owned(),
        KeyCode::Left => "left".to_owned(),
        KeyCode::Right => "right".to_owned(),
        KeyCode::Home => "home".to_owned(),
        KeyCode::End => "end".to_owned(),
        KeyCode::PageUp => "pageup".to_owned(),
        KeyCode::PageDown => "pagedown".to_owned(),
        KeyCode::Backspace => "backspace".to_owned(),
        _ => format!("{:?}", key.code).to_ascii_lowercase(),
    });
    parts.join("-")
}

fn parse_key_sequence(sequence: &str) -> Result<KeySequence, String> {
    let keys = sequence
        .split_whitespace()
        .map(parse_key_press)
        .collect::<Result<Vec<_>, _>>()?;
    if keys.is_empty() {
        return Err("empty key binding".to_owned());
    }
    Ok(KeySequence(keys))
}

fn parse_key_press(input: &str) -> Result<KeyPress, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err("empty key".to_owned());
    }

    let normalized = input.to_ascii_lowercase();
    let mut modifiers = KeyModifiers::NONE;
    let mut key = normalized.as_str();
    loop {
        if let Some(rest) = key
            .strip_prefix("ctrl-")
            .or_else(|| key.strip_prefix("ctrl+"))
            .or_else(|| key.strip_prefix("c-"))
            .or_else(|| key.strip_prefix("c+"))
        {
            modifiers.insert(KeyModifiers::CONTROL);
            key = rest;
        } else if let Some(rest) = key
            .strip_prefix("alt-")
            .or_else(|| key.strip_prefix("alt+"))
            .or_else(|| key.strip_prefix("a-"))
            .or_else(|| key.strip_prefix("a+"))
        {
            modifiers.insert(KeyModifiers::ALT);
            key = rest;
        } else if let Some(rest) = key
            .strip_prefix("shift-")
            .or_else(|| key.strip_prefix("shift+"))
            .or_else(|| key.strip_prefix("s-"))
            .or_else(|| key.strip_prefix("s+"))
        {
            modifiers.insert(KeyModifiers::SHIFT);
            key = rest;
        } else {
            break;
        }
    }

    let code = match key {
        "space" => KeyCode::Char(' '),
        "enter" | "return" => KeyCode::Enter,
        "esc" | "escape" => KeyCode::Esc,
        "tab" if modifiers.contains(KeyModifiers::SHIFT) => {
            modifiers.remove(KeyModifiers::SHIFT);
            KeyCode::BackTab
        }
        "tab" => KeyCode::Tab,
        "backtab" => KeyCode::BackTab,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" | "page-up" | "pgup" => KeyCode::PageUp,
        "pagedown" | "page-down" | "pgdn" => KeyCode::PageDown,
        "backspace" | "bs" => KeyCode::Backspace,
        _ => {
            let character_source = if modifiers.is_empty() { input } else { key };
            let mut chars = character_source.chars();
            let Some(mut character) = chars.next() else {
                return Err("empty key".to_owned());
            };
            if chars.next().is_some() {
                return Err(format!("unknown key `{input}`"));
            }
            if modifiers.contains(KeyModifiers::SHIFT) && character.is_ascii_alphabetic() {
                character = character.to_ascii_uppercase();
            }
            KeyCode::Char(character)
        }
    };

    Ok(KeyPress::new(code, modifiers))
}

fn normalize_modifiers(code: KeyCode, mut modifiers: KeyModifiers) -> KeyModifiers {
    if matches!(code, KeyCode::Char(_)) {
        modifiers.remove(KeyModifiers::SHIFT);
    }
    if matches!(code, KeyCode::BackTab) {
        modifiers.remove(KeyModifiers::SHIFT);
    }
    modifiers
}
