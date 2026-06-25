use std::{collections::HashMap, fs};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mark_core::{MarkError, MarkResult};
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GlobalAction {
    Help,
    Reload,
    FileFilter,
    Grep,
    DiffMenu,
    OptionsMenu,
    FileBrowser,
    Quit,
    Layout,
    EditHunk,
    CopyErrorLog,
    NextDiffType,
    PreviousDiffType,
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
    leader: KeyPress,
    help: Vec<KeySequence>,
    reload: Vec<KeySequence>,
    file_filter: Vec<KeySequence>,
    grep: Vec<KeySequence>,
    diff_menu: Vec<KeySequence>,
    options_menu: Vec<KeySequence>,
    file_browser: Vec<KeySequence>,
    quit: Vec<KeySequence>,
    layout: Vec<KeySequence>,
    edit_hunk: Vec<KeySequence>,
    copy_error_log: Vec<KeySequence>,
    next_diff_type: Vec<KeySequence>,
    previous_diff_type: Vec<KeySequence>,
    menu_up: Vec<KeySequence>,
    menu_down: Vec<KeySequence>,
    menu_select: Vec<KeySequence>,
    menu_confirm: Vec<KeySequence>,
    menu_close: Vec<KeySequence>,
}

impl Default for Keymap {
    fn default() -> Self {
        let leader = KeyPress::new(KeyCode::Char(' '), KeyModifiers::NONE);
        Self {
            leader,
            help: key_sequences(&["?"]),
            reload: key_sequences(&["r"]),
            file_filter: key_sequences(&["f"]),
            grep: key_sequences(&["/"]),
            diff_menu: key_sequences(&["space m"]),
            options_menu: key_sequences(&["space o"]),
            file_browser: key_sequences(&["b"]),
            quit: key_sequences(&["q"]),
            layout: key_sequences(&["space s"]),
            edit_hunk: key_sequences(&["ctrl-g"]),
            copy_error_log: key_sequences(&["ctrl-shift-c"]),
            next_diff_type: key_sequences(&["tab"]),
            previous_diff_type: key_sequences(&["shift-tab"]),
            menu_up: key_sequences(&["up", "shift-tab", "ctrl-p"]),
            menu_down: key_sequences(&["down", "tab", "ctrl-n"]),
            menu_select: Vec::new(),
            menu_confirm: key_sequences(&["enter"]),
            menu_close: key_sequences(&["esc"]),
        }
    }
}

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

        if let Some(leader) = stored.global.leader {
            let new_leader = parse_key_press(&leader)?;
            keymap.replace_default_leader(new_leader);
            keymap.leader = new_leader;
        }

        set_sequences(&mut keymap.help, stored.global.help)?;
        set_sequences(&mut keymap.reload, stored.global.reload)?;
        set_sequences(&mut keymap.file_filter, stored.global.file_filter)?;
        set_sequences(&mut keymap.grep, stored.global.grep)?;
        set_sequences(&mut keymap.diff_menu, stored.global.diff_menu)?;
        set_sequences(&mut keymap.options_menu, stored.global.options_menu)?;
        set_sequences(&mut keymap.file_browser, stored.global.file_browser)?;
        set_sequences(&mut keymap.quit, stored.global.quit)?;
        set_sequences(&mut keymap.layout, stored.global.layout)?;
        set_sequences(&mut keymap.edit_hunk, stored.global.edit_hunk)?;
        set_sequences(&mut keymap.copy_error_log, stored.global.copy_error_log)?;
        set_sequences(&mut keymap.next_diff_type, stored.global.next_diff_type)?;
        set_sequences(
            &mut keymap.previous_diff_type,
            stored.global.previous_diff_type,
        )?;

        set_sequences(&mut keymap.menu_up, stored.menu.up)?;
        set_sequences(&mut keymap.menu_down, stored.menu.down)?;
        set_sequences(&mut keymap.menu_select, stored.menu.select)?;
        set_sequences(&mut keymap.menu_confirm, stored.menu.confirm)?;
        set_sequences(&mut keymap.menu_close, stored.menu.close)?;

        keymap.validate()?;
        Ok(keymap)
    }

    fn validate(&self) -> Result<(), String> {
        for (name, bindings) in [
            ("help", &self.help),
            ("reload", &self.reload),
            ("file_filter", &self.file_filter),
            ("grep", &self.grep),
            ("diff_menu", &self.diff_menu),
            ("options_menu", &self.options_menu),
            ("file_browser", &self.file_browser),
            ("quit", &self.quit),
            ("layout", &self.layout),
            ("edit_hunk", &self.edit_hunk),
            ("copy_error_log", &self.copy_error_log),
            ("next_diff_type", &self.next_diff_type),
            ("previous_diff_type", &self.previous_diff_type),
        ] {
            for sequence in bindings {
                if sequence.0.is_empty() || sequence.0.len() > 2 {
                    return Err(format!(
                        "keymap.global.{name} must be one key or a leader sequence"
                    ));
                }
                if sequence.0.len() == 2 && sequence.0.first() != Some(&self.leader) {
                    return Err(format!(
                        "keymap.global.{name} multi-key binding must start with leader"
                    ));
                }
                if sequence.0.as_slice() == [self.leader] {
                    return Err(format!(
                        "keymap.global.{name} single-key binding cannot use leader"
                    ));
                }
                if name == "edit_hunk" && sequence.0.len() != 1 {
                    return Err("keymap.global.edit_hunk must be a single key".to_owned());
                }
            }
        }

        for (name, bindings) in [
            ("up", &self.menu_up),
            ("down", &self.menu_down),
            ("select", &self.menu_select),
            ("confirm", &self.menu_confirm),
            ("close", &self.menu_close),
        ] {
            for sequence in bindings {
                if sequence.0.len() != 1 {
                    return Err(format!("keymap.menu.{name} must be a single key"));
                }
            }
        }

        self.validate_global_conflicts()?;
        self.validate_menu_conflicts()?;

        Ok(())
    }

    fn validate_global_conflicts(&self) -> Result<(), String> {
        let bindings = [
            ("help", &self.help),
            ("reload", &self.reload),
            ("file_filter", &self.file_filter),
            ("grep", &self.grep),
            ("diff_menu", &self.diff_menu),
            ("options_menu", &self.options_menu),
            ("file_browser", &self.file_browser),
            ("quit", &self.quit),
            ("layout", &self.layout),
            ("edit_hunk", &self.edit_hunk),
            ("copy_error_log", &self.copy_error_log),
            ("next_diff_type", &self.next_diff_type),
            ("previous_diff_type", &self.previous_diff_type),
        ];
        validate_conflicts("keymap.global", &bindings)
    }

    fn validate_menu_conflicts(&self) -> Result<(), String> {
        let bindings = [
            ("up", &self.menu_up),
            ("down", &self.menu_down),
            ("select", &self.menu_select),
            ("confirm", &self.menu_confirm),
            ("close", &self.menu_close),
        ];
        validate_conflicts("keymap.menu", &bindings)
    }

    fn replace_default_leader(&mut self, new_leader: KeyPress) {
        let old_leader = self.leader;
        for bindings in [
            &mut self.help,
            &mut self.reload,
            &mut self.file_filter,
            &mut self.grep,
            &mut self.diff_menu,
            &mut self.options_menu,
            &mut self.file_browser,
            &mut self.quit,
            &mut self.layout,
            &mut self.edit_hunk,
            &mut self.copy_error_log,
            &mut self.next_diff_type,
            &mut self.previous_diff_type,
        ] {
            for sequence in bindings {
                if sequence.0.first() == Some(&old_leader) {
                    sequence.0[0] = new_leader;
                }
            }
        }
    }

    pub(crate) fn is_leader(&self, key: KeyEvent) -> bool {
        KeyPress::from(key) == self.leader
    }

    pub(crate) fn matches_single(&self, action: GlobalAction, key: KeyEvent) -> bool {
        let key = KeyPress::from(key);
        self.global_sequences(action)
            .iter()
            .any(|sequence| sequence.0.as_slice() == [key])
    }

    pub(crate) fn matches_leader(&self, action: GlobalAction, key: KeyEvent) -> bool {
        let key = KeyPress::from(key);
        self.global_sequences(action)
            .iter()
            .any(|sequence| sequence.0.as_slice() == [self.leader, key])
    }

    pub(crate) fn leader_label(&self) -> String {
        key_display_label(&self.leader)
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

    fn global_sequences(&self, action: GlobalAction) -> &[KeySequence] {
        match action {
            GlobalAction::Help => &self.help,
            GlobalAction::Reload => &self.reload,
            GlobalAction::FileFilter => &self.file_filter,
            GlobalAction::Grep => &self.grep,
            GlobalAction::DiffMenu => &self.diff_menu,
            GlobalAction::OptionsMenu => &self.options_menu,
            GlobalAction::FileBrowser => &self.file_browser,
            GlobalAction::Quit => &self.quit,
            GlobalAction::Layout => &self.layout,
            GlobalAction::EditHunk => &self.edit_hunk,
            GlobalAction::CopyErrorLog => &self.copy_error_log,
            GlobalAction::NextDiffType => &self.next_diff_type,
            GlobalAction::PreviousDiffType => &self.previous_diff_type,
        }
    }

    fn menu_sequences(&self, action: MenuAction) -> &[KeySequence] {
        match action {
            MenuAction::Up => &self.menu_up,
            MenuAction::Down => &self.menu_down,
            MenuAction::Select => &self.menu_select,
            MenuAction::Confirm => &self.menu_confirm,
            MenuAction::Close => &self.menu_close,
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
pub(crate) struct KeySequence(Vec<KeyPress>);

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
    options_menu: Option<KeySpec>,
    file_browser: Option<KeySpec>,
    quit: Option<KeySpec>,
    layout: Option<KeySpec>,
    edit_hunk: Option<KeySpec>,
    copy_error_log: Option<KeySpec>,
    next_diff_type: Option<KeySpec>,
    #[serde(alias = "prev_diff_type")]
    previous_diff_type: Option<KeySpec>,
}

#[derive(Debug, Default, Deserialize)]
struct StoredMenuKeymap {
    up: Option<KeySpec>,
    down: Option<KeySpec>,
    select: Option<KeySpec>,
    confirm: Option<KeySpec>,
    close: Option<KeySpec>,
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

fn key_sequences(keys: &[&str]) -> Vec<KeySequence> {
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

fn sequence_label(sequence: &KeySequence) -> String {
    sequence
        .0
        .iter()
        .map(key_label)
        .collect::<Vec<_>>()
        .join(" ")
}

fn sequence_list_display_label(sequences: &[KeySequence]) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keymap_parses_configured_global_and_menu_bindings() {
        let keymap = Keymap::parse(
            r#"
            [keymap.global]
            leader = ","
            diff_menu = ", d"
            quit = ", x"
            file_filter = "ctrl-f"
            copy_error_log = "ctrl+shift+c"
            prev_diff_type = "shift-left"

            [keymap.menu]
            down = ["s", "down"]
            "#,
        )
        .expect("keymap should parse");

        assert!(keymap.is_leader(KeyEvent::new(KeyCode::Char(','), KeyModifiers::NONE)));
        assert!(keymap.matches_leader(
            GlobalAction::DiffMenu,
            KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE)
        ));
        assert!(keymap.matches_single(
            GlobalAction::FileFilter,
            KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL)
        ));
        assert!(keymap.matches_single(
            GlobalAction::CopyErrorLog,
            KeyEvent::new(
                KeyCode::Char('C'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT
            )
        ));
        assert_eq!(
            keymap.global_action_label(GlobalAction::CopyErrorLog),
            "Ctrl-Shift-C"
        );
        assert!(keymap.matches_menu(
            MenuAction::Down,
            KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE)
        ));
        assert!(keymap.matches_help_menu_scroll(
            MenuAction::Down,
            KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE)
        ));
        assert!(!keymap.matches_help_menu_scroll(
            MenuAction::Down,
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)
        ));
        assert!(!keymap.matches_help_menu_scroll(
            MenuAction::Up,
            KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT)
        ));
        assert!(keymap.matches_single(
            GlobalAction::PreviousDiffType,
            KeyEvent::new(KeyCode::Left, KeyModifiers::SHIFT)
        ));
    }

    #[test]
    fn keymap_preserves_shifted_character_bindings() {
        let keymap = Keymap::parse(
            r#"
            [keymap.global]
            quit = "shift-q"
            "#,
        )
        .expect("keymap should parse");

        assert!(keymap.matches_single(
            GlobalAction::Quit,
            KeyEvent::new(KeyCode::Char('Q'), KeyModifiers::SHIFT)
        ));
        assert!(!keymap.matches_single(
            GlobalAction::Quit,
            KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)
        ));
    }

    #[test]
    fn default_copy_error_log_matches_hunk_diff_binding() {
        let keymap = Keymap::default();

        assert!(keymap.matches_single(
            GlobalAction::CopyErrorLog,
            KeyEvent::new(
                KeyCode::Char('C'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT
            )
        ));
        assert!(keymap.matches_single(
            GlobalAction::CopyErrorLog,
            KeyEvent::new(
                KeyCode::Char('c'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT
            )
        ));
        assert!(!keymap.matches_single(
            GlobalAction::CopyErrorLog,
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)
        ));
        assert_eq!(
            keymap.global_action_label(GlobalAction::CopyErrorLog),
            "Ctrl-Shift-C"
        );
    }

    #[test]
    fn keymap_rejects_non_leader_multi_key_global_binding() {
        let error = Keymap::parse(
            r#"
            [keymap.global]
            diff_menu = "x d"
            "#,
        )
        .expect_err("invalid keymap should fail");

        assert!(error.contains("must start with leader"));
    }

    #[test]
    fn keymap_rejects_conflicting_bindings_in_same_context() {
        let error = Keymap::parse(
            r#"
            [keymap.global]
            help = "r"
            reload = "r"
            "#,
        )
        .expect_err("conflicting keymap should fail");

        assert!(error.contains("keymap.global conflict"));
    }

    #[test]
    fn keymap_rejects_multi_key_editor_binding() {
        let error = Keymap::parse(
            r#"
            [keymap.global]
            edit_hunk = "space e"
            "#,
        )
        .expect_err("multi-key editor binding should fail");

        assert!(error.contains("edit_hunk must be a single key"));
    }
}
