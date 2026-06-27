use std::{
    env,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};

use mark_core::{MarkError, MarkResult};
use mark_syntax::{SyntaxThemeConfig, SyntaxThemeSource};
use ratatui::prelude::Color;

use super::DiffTheme;

pub(crate) fn config_color(value: &Option<String>, name: &str) -> MarkResult<Option<Color>> {
    value
        .as_deref()
        .map(|value| parse_config_color(value, name))
        .transpose()
}

pub(crate) fn parse_config_color(value: &str, name: &str) -> MarkResult<Color> {
    let trimmed = value.trim();
    let lower = trimmed.to_ascii_lowercase();

    if matches!(lower.as_str(), "default" | "reset" | "none") {
        return Ok(Color::Reset);
    }

    if let Some(color) = parse_config_hex_color(trimmed) {
        return Ok(color.color());
    }

    if let Some(index) = parse_ansi_index(&lower) {
        return Ok(Color::Indexed(index));
    }

    if let Some(color) = parse_named_color(&lower) {
        return Ok(color);
    }

    Err(MarkError::Usage(format!(
        "invalid color for {name}: {value}; expected #rrggbb, ansi-N, or a named color"
    )))
}

pub(crate) fn parse_config_hex_color(value: &str) -> Option<RgbColor> {
    let value = value
        .trim()
        .trim_matches(['\'', '"'])
        .strip_prefix('#')
        .or_else(|| value.trim().strip_prefix("0x"))
        .unwrap_or_else(|| value.trim().trim_matches(['\'', '"']));
    parse_hex_digits(value)
}

pub(crate) fn parse_ansi_index(value: &str) -> Option<u8> {
    let index = value
        .strip_prefix("ansi-")
        .or_else(|| value.strip_prefix("ansi:"))
        .or_else(|| value.strip_prefix("indexed-"))
        .or_else(|| value.strip_prefix("indexed:"))
        .unwrap_or(value);
    index.parse::<u8>().ok()
}

pub(crate) fn parse_named_color(value: &str) -> Option<Color> {
    match value.replace('_', "-").as_str() {
        "black" => Some(Color::Black),
        "red" => Some(Color::Red),
        "green" => Some(Color::Green),
        "yellow" => Some(Color::Yellow),
        "blue" => Some(Color::Blue),
        "magenta" | "purple" => Some(Color::Magenta),
        "cyan" => Some(Color::Cyan),
        "gray" | "grey" => Some(Color::Gray),
        "dark-gray" | "dark-grey" | "bright-black" => Some(Color::DarkGray),
        "white" | "bright-white" => Some(Color::White),
        "bright-red" | "light-red" => Some(Color::LightRed),
        "bright-green" | "light-green" => Some(Color::LightGreen),
        "bright-yellow" | "light-yellow" => Some(Color::LightYellow),
        "bright-blue" | "light-blue" => Some(Color::LightBlue),
        "bright-magenta" | "light-magenta" | "bright-purple" | "light-purple" => {
            Some(Color::LightMagenta)
        }
        "bright-cyan" | "light-cyan" => Some(Color::LightCyan),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Base16Scheme {
    pub(crate) base00: RgbColor,
    pub(crate) base01: RgbColor,
    pub(crate) base03: RgbColor,
    pub(crate) base04: RgbColor,
    pub(crate) base05: RgbColor,
    pub(crate) base06: RgbColor,
    pub(crate) base08: RgbColor,
    pub(crate) base09: RgbColor,
    pub(crate) base0a: RgbColor,
    pub(crate) base0b: RgbColor,
    pub(crate) base0c: RgbColor,
    pub(crate) base0d: RgbColor,
    pub(crate) base0e: RgbColor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RgbColor {
    pub(crate) red: u8,
    pub(crate) green: u8,
    pub(crate) blue: u8,
}

impl RgbColor {
    pub(crate) const fn new(red: u8, green: u8, blue: u8) -> Self {
        Self { red, green, blue }
    }

    pub(crate) fn color(self) -> Color {
        Color::Rgb(self.red, self.green, self.blue)
    }

    pub(crate) fn blend(self, other: Self, amount: f32) -> Self {
        let amount = amount.clamp(0.0, 1.0);
        let mix = |a: u8, b: u8| -> u8 {
            ((f32::from(a) * (1.0 - amount)) + (f32::from(b) * amount)).round() as u8
        };
        Self {
            red: mix(self.red, other.red),
            green: mix(self.green, other.green),
            blue: mix(self.blue, other.blue),
        }
    }
}

pub(crate) fn diff_theme_from_config(config: &SyntaxThemeConfig) -> MarkResult<DiffTheme> {
    match config.source {
        SyntaxThemeSource::Builtin => {
            let name = config.name.as_deref();
            match builtin_diff_theme(name) {
                Ok(theme) => Ok(theme),
                Err(error) => {
                    if let Some(name) = name {
                        if let Some(theme) = load_named_colorscheme(name)? {
                            return Ok(theme);
                        }
                    }
                    Err(error)
                }
            }
        }
        SyntaxThemeSource::Ansi => Ok(DiffTheme::ansi()),
        SyntaxThemeSource::Base16 => {
            let path = config.path.as_ref().ok_or_else(|| {
                MarkError::Usage("base16 colorscheme requires colorscheme.path".to_owned())
            })?;
            Ok(DiffTheme::base16(load_base16_scheme(path)?))
        }
    }
}

pub(crate) fn load_named_colorscheme(name: &str) -> MarkResult<Option<DiffTheme>> {
    let name = name.trim();
    if name.is_empty() || Path::new(name).file_name().and_then(OsStr::to_str) != Some(name) {
        return Ok(None);
    }

    let colorscheme_dir = mark_syntax::colorscheme_dir()?;
    for path in colorscheme_paths(&colorscheme_dir, name) {
        if path.exists() {
            return Ok(Some(DiffTheme::base16(load_base16_scheme(&path)?)));
        }
    }
    Ok(None)
}

pub(crate) fn colorscheme_paths(dir: &Path, name: &str) -> Vec<PathBuf> {
    let path = dir.join(name);
    if Path::new(name).extension().is_some() {
        return vec![path];
    }

    ["toml", "yaml", "yml"]
        .into_iter()
        .map(|extension| path.with_extension(extension))
        .collect()
}

pub(crate) fn builtin_diff_theme(name: Option<&str>) -> MarkResult<DiffTheme> {
    let name = name.unwrap_or("system").trim().to_ascii_lowercase();
    match name.as_str() {
        "system" | "default" | "" => Ok(DiffTheme::system()),
        "catppuccin-latte" | "latte" => Ok(DiffTheme::catppuccin_latte()),
        "catppuccin-frappe" | "frappe" => Ok(DiffTheme::catppuccin_frappe()),
        "catppuccin-macchiato" | "macchiato" => Ok(DiffTheme::catppuccin_macchiato()),
        "catppuccin" | "catppuccin-mocha" | "mocha" => Ok(DiffTheme::catppuccin_mocha()),
        "gruvbox" | "gruvbox-dark" => Ok(DiffTheme::gruvbox_dark()),
        "gruvbox-light" => Ok(DiffTheme::gruvbox_light()),
        "github" | "github-dark" => Ok(DiffTheme::github_dark()),
        "github-dark-high-contrast" | "github-high-contrast" => {
            Ok(DiffTheme::github_dark_high_contrast())
        }
        "github-light" => Ok(DiffTheme::github_light()),
        "github-light-high-contrast" => Ok(DiffTheme::github_light_high_contrast()),
        "tokyonight" | "tokyo-night" | "tokyonight-night" => Ok(DiffTheme::tokyonight()),
        name => Err(MarkError::Usage(format!("unknown colorscheme '{name}'"))),
    }
}

pub(crate) fn load_base16_scheme(path: &Path) -> MarkResult<Base16Scheme> {
    let path = expand_user_path(path);
    let contents = fs::read_to_string(&path)?;
    parse_base16_scheme(&contents).ok_or_else(|| {
        MarkError::Usage(format!(
            "failed to parse base16 colorscheme at {}; expected base00 through base0F",
            path.display()
        ))
    })
}

pub(crate) fn expand_user_path(path: &Path) -> PathBuf {
    let path_text = path.to_string_lossy();
    if path_text == "~" {
        return env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| path.to_path_buf());
    }
    if let Some(rest) = path_text.strip_prefix("~/") {
        if let Some(home) = env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    path.to_path_buf()
}

pub(crate) fn parse_base16_scheme(contents: &str) -> Option<Base16Scheme> {
    let mut colors: [Option<RgbColor>; 16] = [None; 16];
    for line in contents.lines() {
        let Some((index, color)) = parse_base16_line(line) else {
            continue;
        };
        colors[index] = Some(color);
    }

    if colors.iter().any(Option::is_none) {
        return None;
    }

    Some(Base16Scheme {
        base00: colors[0]?,
        base01: colors[1]?,
        base03: colors[3]?,
        base04: colors[4]?,
        base05: colors[5]?,
        base06: colors[6]?,
        base08: colors[8]?,
        base09: colors[9]?,
        base0a: colors[10]?,
        base0b: colors[11]?,
        base0c: colors[12]?,
        base0d: colors[13]?,
        base0e: colors[14]?,
    })
}

pub(crate) fn parse_base16_line(line: &str) -> Option<(usize, RgbColor)> {
    let line = line.trim();
    let (key, value) = line.split_once(':').or_else(|| line.split_once('='))?;
    let key = key.trim().trim_matches(['\'', '"']).to_ascii_lowercase();
    let index = base16_index(&key)?;
    let color = parse_hex_color(value)?;
    Some((index, color))
}

pub(crate) fn base16_index(key: &str) -> Option<usize> {
    let suffix = key.strip_prefix("base")?;
    if suffix.len() != 2 || !suffix.starts_with('0') {
        return None;
    }
    usize::from_str_radix(suffix, 16)
        .ok()
        .filter(|index| *index < 16)
}

pub(crate) fn parse_hex_color(value: &str) -> Option<RgbColor> {
    let value = value.trim();
    if let Some(hash) = value.find('#') {
        return parse_hex_digits(value.get(hash + 1..hash + 7)?);
    }

    let token = value
        .trim_matches(['\'', '"', ',', ' '])
        .split_whitespace()
        .next()?;
    parse_hex_digits(token.trim_matches(['\'', '"', ',']))
}

pub(crate) fn parse_hex_digits(digits: &str) -> Option<RgbColor> {
    if digits.len() < 6
        || !digits.as_bytes()[..6]
            .iter()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return None;
    }
    Some(RgbColor {
        red: u8::from_str_radix(&digits[0..2], 16).ok()?,
        green: u8::from_str_radix(&digits[2..4], 16).ok()?,
        blue: u8::from_str_radix(&digits[4..6], 16).ok()?,
    })
}
