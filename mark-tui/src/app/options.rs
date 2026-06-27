use crate::controls::DiffLayoutMode;
use mark_core::{MarkError, MarkResult};
use mark_syntax::{
    DiffContextExpansion, LayoutSetting, NotificationMode, SyntaxThemeConfig, SyntaxThemeSource,
    ToastCorner,
};
use std::fs;
use std::path::Path;

fn cycle_choice<T: Copy + PartialEq>(choices: &[T], current: T, delta: isize) -> T {
    let current = choices
        .iter()
        .position(|candidate| *candidate == current)
        .unwrap_or_default();
    let next = choice_index_with_delta(current, choices.len(), delta);
    choices[next]
}

fn choice_index_with_delta(current: usize, len: usize, delta: isize) -> usize {
    let len = len as isize;
    (current as isize + delta.rem_euclid(len)).rem_euclid(len) as usize
}

fn cycle_ordered_choice<T: Copy + Ord>(choices: &[T], current: T, delta: isize) -> T {
    if choices.is_empty() || delta == 0 {
        return current;
    }

    if let Some(current) = choices.iter().position(|candidate| *candidate == current) {
        let next = choice_index_with_delta(current, choices.len(), delta);
        return choices[next];
    }

    // Numeric settings can have valid custom values that are not listed in the
    // menu. Snap those to the next choice in the requested direction, or to the
    // nearest boundary when the custom value is outside the choice range.
    let first_step = if delta > 0 {
        choices
            .iter()
            .position(|candidate| *candidate > current)
            .unwrap_or(choices.len() - 1)
    } else {
        choices
            .iter()
            .rposition(|candidate| *candidate < current)
            .unwrap_or_default()
    };
    let remaining_delta = delta - delta.signum();
    let next = choice_index_with_delta(first_step, choices.len(), remaining_delta);
    choices[next]
}

pub(crate) fn next_notification_mode(mode: NotificationMode) -> NotificationMode {
    match mode {
        NotificationMode::Default => NotificationMode::Debug,
        NotificationMode::Debug => NotificationMode::Default,
    }
}

pub(crate) fn next_toast_corner(corner: ToastCorner, delta: isize) -> ToastCorner {
    cycle_choice(TOAST_CORNER_CHOICES, corner, delta)
}

pub(crate) fn next_toast_timeout_ms(timeout_ms: u64, delta: isize) -> u64 {
    cycle_ordered_choice(TOAST_TIMEOUT_CHOICES_MS, timeout_ms, delta)
}

pub(crate) fn next_toast_max_visible(max_visible: usize, delta: isize) -> usize {
    cycle_ordered_choice(TOAST_MAX_VISIBLE_CHOICES, max_visible, delta)
}

pub(crate) fn context_expansion_label(expansion: DiffContextExpansion) -> String {
    match expansion {
        DiffContextExpansion::Lines(lines) => lines.to_string(),
        DiffContextExpansion::Full => "full".to_owned(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ColorSchemeChoice {
    Custom,
    System,
    CatppuccinLatte,
    CatppuccinFrappe,
    CatppuccinMacchiato,
    CatppuccinMocha,
    GruvboxDark,
    GruvboxLight,
    GithubDark,
    GithubDarkHighContrast,
    GithubLight,
    GithubLightHighContrast,
    Tokyonight,
}

pub(crate) const COLOR_SCHEME_CHOICES: &[ColorSchemeChoice] = &[
    ColorSchemeChoice::System,
    ColorSchemeChoice::CatppuccinLatte,
    ColorSchemeChoice::CatppuccinFrappe,
    ColorSchemeChoice::CatppuccinMacchiato,
    ColorSchemeChoice::CatppuccinMocha,
    ColorSchemeChoice::GruvboxDark,
    ColorSchemeChoice::GruvboxLight,
    ColorSchemeChoice::GithubDark,
    ColorSchemeChoice::GithubDarkHighContrast,
    ColorSchemeChoice::GithubLight,
    ColorSchemeChoice::GithubLightHighContrast,
    ColorSchemeChoice::Tokyonight,
];

pub(crate) fn color_scheme_label(choice: ColorSchemeChoice) -> &'static str {
    match choice {
        ColorSchemeChoice::Custom => "custom",
        ColorSchemeChoice::System => "system",
        ColorSchemeChoice::CatppuccinLatte => "catppuccin-latte",
        ColorSchemeChoice::CatppuccinFrappe => "catppuccin-frappe",
        ColorSchemeChoice::CatppuccinMacchiato => "catppuccin-macchiato",
        ColorSchemeChoice::CatppuccinMocha => "catppuccin-mocha",
        ColorSchemeChoice::GruvboxDark => "gruvbox-dark",
        ColorSchemeChoice::GruvboxLight => "gruvbox-light",
        ColorSchemeChoice::GithubDark => "github-dark",
        ColorSchemeChoice::GithubDarkHighContrast => "github-dark-high-contrast",
        ColorSchemeChoice::GithubLight => "github-light",
        ColorSchemeChoice::GithubLightHighContrast => "github-light-high-contrast",
        ColorSchemeChoice::Tokyonight => "tokyonight",
    }
}

pub(crate) fn color_scheme_from_config(config: &SyntaxThemeConfig) -> ColorSchemeChoice {
    match config.source {
        SyntaxThemeSource::Ansi | SyntaxThemeSource::Base16 => ColorSchemeChoice::Custom,
        SyntaxThemeSource::Builtin => color_scheme_from_name(config.name.as_deref()),
    }
}

pub(crate) fn color_scheme_from_name(name: Option<&str>) -> ColorSchemeChoice {
    match name
        .unwrap_or("system")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "system" | "default" | "" => ColorSchemeChoice::System,
        "catppuccin-latte" | "latte" => ColorSchemeChoice::CatppuccinLatte,
        "catppuccin-frappe" | "frappe" => ColorSchemeChoice::CatppuccinFrappe,
        "catppuccin-macchiato" | "macchiato" => ColorSchemeChoice::CatppuccinMacchiato,
        "catppuccin" | "catppuccin-mocha" | "mocha" => ColorSchemeChoice::CatppuccinMocha,
        "gruvbox" | "gruvbox-dark" => ColorSchemeChoice::GruvboxDark,
        "gruvbox-light" => ColorSchemeChoice::GruvboxLight,
        "github" | "github-dark" => ColorSchemeChoice::GithubDark,
        "github-dark-high-contrast" | "github-high-contrast" => {
            ColorSchemeChoice::GithubDarkHighContrast
        }
        "github-light" => ColorSchemeChoice::GithubLight,
        "github-light-high-contrast" => ColorSchemeChoice::GithubLightHighContrast,
        "tokyonight" | "tokyo-night" | "tokyonight-night" => ColorSchemeChoice::Tokyonight,
        _ => ColorSchemeChoice::Custom,
    }
}

pub(crate) fn color_scheme_config(choice: ColorSchemeChoice) -> Option<SyntaxThemeConfig> {
    match choice {
        ColorSchemeChoice::Custom => None,
        choice => Some(SyntaxThemeConfig {
            source: SyntaxThemeSource::Builtin,
            name: Some(color_scheme_label(choice).to_owned()),
            path: None,
        }),
    }
}

pub(crate) fn layout_override_from_setting(setting: LayoutSetting) -> Option<DiffLayoutMode> {
    match setting {
        LayoutSetting::Dynamic => None,
        LayoutSetting::Split => Some(DiffLayoutMode::Split),
        LayoutSetting::Unified => Some(DiffLayoutMode::Unified),
    }
}

pub(crate) fn layout_setting_from_override(
    layout_override: Option<DiffLayoutMode>,
) -> LayoutSetting {
    match layout_override {
        Some(DiffLayoutMode::Split) => LayoutSetting::Split,
        Some(DiffLayoutMode::Unified) => LayoutSetting::Unified,
        None => LayoutSetting::Dynamic,
    }
}

pub(crate) fn layout_setting_label(layout: LayoutSetting) -> &'static str {
    match layout {
        LayoutSetting::Dynamic => "dynamic",
        LayoutSetting::Split => "split",
        LayoutSetting::Unified => "unified",
    }
}

pub(crate) fn next_layout_setting(setting: LayoutSetting, delta: isize) -> LayoutSetting {
    let settings = [
        LayoutSetting::Dynamic,
        LayoutSetting::Split,
        LayoutSetting::Unified,
    ];
    let current = settings
        .iter()
        .position(|candidate| *candidate == setting)
        .unwrap_or_default();
    let next = (current as isize + delta).rem_euclid(settings.len() as isize) as usize;
    settings[next]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OptionsMenuItem {
    Layout,
    LiveReload,
    ContextExpansion,
    SyntaxHighlighting,
    LineWrapping,
    ColorScheme,
    NotificationMode,
    ToastCorner,
    ToastTimeout,
    ToastMaxVisible,
}

// Construct the legacy-only variant so unused-option linting stays meaningful
// while this hidden settings-persistence path remains available.
const _: OptionsMenuItem = OptionsMenuItem::ContextExpansion;

pub(crate) const COMMON_OPTIONS_MENU_ITEMS: &[OptionsMenuItem] = &[
    OptionsMenuItem::Layout,
    OptionsMenuItem::LiveReload,
    OptionsMenuItem::SyntaxHighlighting,
    OptionsMenuItem::LineWrapping,
    OptionsMenuItem::ColorScheme,
    OptionsMenuItem::NotificationMode,
    OptionsMenuItem::ToastCorner,
    OptionsMenuItem::ToastTimeout,
    OptionsMenuItem::ToastMaxVisible,
];

pub(crate) fn option_label(item: OptionsMenuItem) -> &'static str {
    match item {
        OptionsMenuItem::Layout => "Layout",
        OptionsMenuItem::LiveReload => "Live reload",
        OptionsMenuItem::ContextExpansion => "Context expand",
        OptionsMenuItem::SyntaxHighlighting => "Syntax highlighting",
        OptionsMenuItem::LineWrapping => "Line wrapping",
        OptionsMenuItem::ColorScheme => "Colorscheme",
        OptionsMenuItem::NotificationMode => "Notification mode",
        OptionsMenuItem::ToastCorner => "Toast corner",
        OptionsMenuItem::ToastTimeout => "Toast timeout",
        OptionsMenuItem::ToastMaxVisible => "Toast max visible",
    }
}

pub(crate) fn checkbox(enabled: bool) -> String {
    if enabled { "[x]" } else { "[ ]" }.to_owned()
}

pub(crate) fn on_off_search(enabled: bool) -> String {
    if enabled { "on" } else { "off" }.to_owned()
}

pub(crate) fn notification_mode_label(mode: NotificationMode) -> &'static str {
    match mode {
        NotificationMode::Default => "default",
        NotificationMode::Debug => "debug",
    }
}

fn notification_mode_config_value(mode: NotificationMode) -> toml::Value {
    toml::Value::String(notification_mode_label(mode).to_owned())
}

pub(crate) fn toast_corner_label(corner: ToastCorner) -> &'static str {
    match corner {
        ToastCorner::TopLeft => "top-left",
        ToastCorner::TopRight => "top-right",
        ToastCorner::BottomLeft => "bottom-left",
        ToastCorner::BottomRight => "bottom-right",
    }
}

fn toast_corner_config_value(corner: ToastCorner) -> toml::Value {
    toml::Value::String(toast_corner_label(corner).to_owned())
}

pub(crate) fn toast_timeout_label(timeout_ms: u64) -> String {
    if timeout_ms % 1_000 == 0 {
        format!("{}s", timeout_ms / 1_000)
    } else {
        format!("{timeout_ms}ms")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OptionsDraft {
    pub(crate) layout: LayoutSetting,
    pub(crate) live_updates_enabled: bool,
    pub(crate) context_expansion: DiffContextExpansion,
    pub(crate) syntax_enabled: bool,
    pub(crate) line_wrapping: bool,
    pub(crate) color_scheme: ColorSchemeChoice,
    pub(crate) notification_mode: NotificationMode,
    pub(crate) toast_corner: ToastCorner,
    pub(crate) toast_timeout_ms: u64,
    pub(crate) toast_max_visible: usize,
}

const TOAST_TIMEOUT_CHOICES_MS: &[u64] = &[500, 1_000, 1_500, 2_500, 5_000, 10_000];
const TOAST_MAX_VISIBLE_CHOICES: &[usize] = &[1, 2, 3, 4, 5];
const TOAST_CORNER_CHOICES: &[ToastCorner] = &[
    ToastCorner::TopRight,
    ToastCorner::BottomRight,
    ToastCorner::BottomLeft,
    ToastCorner::TopLeft,
];

pub(crate) fn persist_options_menu_draft_to_path(
    path: &Path,
    draft: OptionsDraft,
    changed_item: OptionsMenuItem,
) -> MarkResult<()> {
    let mut table = if path.exists() {
        let contents = fs::read_to_string(path)?;
        if contents.trim().is_empty() {
            toml::Table::new()
        } else {
            contents.parse::<toml::Table>().map_err(|error| {
                MarkError::Usage(format!("failed to parse {}: {error}", path.display()))
            })?
        }
    } else {
        toml::Table::new()
    };

    match changed_item {
        OptionsMenuItem::Layout => {
            table.insert(
                "layout".to_owned(),
                toml::Value::String(layout_setting_label(draft.layout).to_owned()),
            );
        }
        OptionsMenuItem::LiveReload => {
            table.insert(
                "live_reload".to_owned(),
                toml::Value::Boolean(draft.live_updates_enabled),
            );
        }
        OptionsMenuItem::ContextExpansion => {
            let mut diff = match table.remove("diff") {
                Some(toml::Value::Table(diff)) => diff,
                Some(_) => {
                    return Err(MarkError::Usage(format!(
                        "failed to update {}: diff must be a table",
                        path.display()
                    )));
                }
                None => toml::Table::new(),
            };
            diff.remove("context_expansion");
            diff.remove("context_lines");
            diff.remove("expand_context");
            diff.insert(
                "context_expand".to_owned(),
                context_expansion_config_value(draft.context_expansion),
            );
            table.insert("diff".to_owned(), toml::Value::Table(diff));
        }
        OptionsMenuItem::SyntaxHighlighting => {
            table.insert(
                "syntax_highlighting".to_owned(),
                toml::Value::Boolean(draft.syntax_enabled),
            );
        }
        OptionsMenuItem::LineWrapping => {
            table.remove("word_wrap");
            table.remove("wrap_lines");
            table.insert(
                "line_wrapping".to_owned(),
                toml::Value::Boolean(draft.line_wrapping),
            );
        }
        OptionsMenuItem::ColorScheme => {
            if let Some(config) = color_scheme_config(draft.color_scheme)
                && config.source == SyntaxThemeSource::Builtin
                && let Some(name) = config.name
            {
                table.insert("colorscheme".to_owned(), toml::Value::String(name));
            }
        }
        OptionsMenuItem::NotificationMode
        | OptionsMenuItem::ToastCorner
        | OptionsMenuItem::ToastTimeout
        | OptionsMenuItem::ToastMaxVisible => {
            let mut notifications = match table.remove("notifications") {
                Some(toml::Value::Table(notifications)) => notifications,
                Some(_) => {
                    return Err(MarkError::Usage(format!(
                        "failed to update {}: notifications must be a table",
                        path.display()
                    )));
                }
                None => toml::Table::new(),
            };
            match changed_item {
                OptionsMenuItem::NotificationMode => {
                    notifications.insert(
                        "mode".to_owned(),
                        notification_mode_config_value(draft.notification_mode),
                    );
                }
                OptionsMenuItem::ToastCorner => {
                    notifications.insert(
                        "corner".to_owned(),
                        toast_corner_config_value(draft.toast_corner),
                    );
                }
                OptionsMenuItem::ToastTimeout => {
                    notifications.insert(
                        "timeout_ms".to_owned(),
                        toml::Value::Integer(draft.toast_timeout_ms as i64),
                    );
                }
                OptionsMenuItem::ToastMaxVisible => {
                    notifications.insert(
                        "max_visible".to_owned(),
                        toml::Value::Integer(draft.toast_max_visible as i64),
                    );
                }
                _ => {}
            }
            table.insert(
                "notifications".to_owned(),
                toml::Value::Table(notifications),
            );
        }
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let contents = toml::to_string_pretty(&table)
        .map_err(|error| MarkError::Usage(format!("failed to serialize settings: {error}")))?;
    fs::write(path, contents)?;
    Ok(())
}

fn context_expansion_config_value(expansion: DiffContextExpansion) -> toml::Value {
    match expansion {
        DiffContextExpansion::Lines(lines) => toml::Value::Integer(lines as i64),
        DiffContextExpansion::Full => toml::Value::String("full".to_owned()),
    }
}
