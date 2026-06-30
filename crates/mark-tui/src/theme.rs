use mark_core::MarkResult;
use mark_diff::DiffLineKind;
use mark_syntax::{ColorOverrides, DiffGutterBackground, DiffSettings};
use ratatui::prelude::Color;

mod benchmark;
mod colorscheme;
mod constants;
mod help;
mod palettes;

pub use benchmark::{DiffBenchmarkOptions, DiffBenchmarkReport, SyntaxBenchmarkReport};
pub(crate) use colorscheme::{Base16Scheme, RgbColor, config_color, diff_theme_from_config};
#[cfg(test)]
pub(crate) use colorscheme::{builtin_diff_theme, parse_base16_scheme};
pub(crate) use constants::*;
pub(crate) use help::{HELP_MENU_ROWS, HelpMenuKey, HelpMenuRow};
pub(crate) use palettes::SyntaxPalette;

pub(crate) fn line_gutter_fg(kind: DiffLineKind, theme: DiffTheme) -> Color {
    match kind {
        DiffLineKind::Addition => theme.addition_fg,
        DiffLineKind::Deletion => theme.deletion_fg,
        DiffLineKind::Context | DiffLineKind::Meta => theme.foreground,
    }
}

pub(crate) fn line_gutter_bg(kind: DiffLineKind, theme: DiffTheme) -> Color {
    match (theme.diff.gutter_background, kind) {
        (DiffGutterBackground::Delta, DiffLineKind::Addition) => theme.addition_gutter_bg,
        (DiffGutterBackground::Delta, DiffLineKind::Deletion) => theme.deletion_gutter_bg,
        _ => theme.gutter_bg,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DiffTheme {
    pub(crate) foreground: Color,
    pub(crate) background: Color,
    pub(crate) header: Color,
    pub(crate) file: Color,
    pub(crate) hunk: Color,
    pub(crate) notice: Color,
    pub(crate) cursor: Color,
    pub(crate) cursor_line_bg: Color,
    pub(crate) muted: Color,
    pub(crate) gutter_bg: Color,
    pub(crate) empty_diff: Color,
    pub(crate) search_match_fg: Color,
    pub(crate) search_match_bg: Color,
    pub(crate) statusline_fg: Color,
    pub(crate) statusline_bg: Color,
    pub(crate) statusline_accent_fg: Color,
    pub(crate) statusline_accent_bg: Color,
    pub(crate) statusline_info_fg: Color,
    pub(crate) statusline_info_bg: Color,
    pub(crate) addition_fg: Color,
    pub(crate) addition_gutter_bg: Color,
    pub(crate) addition_bg: Color,
    pub(crate) addition_inline_bg: Color,
    pub(crate) deletion_fg: Color,
    pub(crate) deletion_gutter_bg: Color,
    pub(crate) deletion_bg: Color,
    pub(crate) deletion_inline_bg: Color,
    pub(crate) transparent_background: bool,
    pub(crate) diff: DiffSettings,
    pub(crate) syntax: SyntaxPalette,
}

impl Default for DiffTheme {
    fn default() -> Self {
        Self::system()
    }
}

impl DiffTheme {
    pub(crate) fn system() -> Self {
        let base = RgbColor::new(0x11, 0x13, 0x15);
        let green = RgbColor::new(0x88, 0xd3, 0x9b);
        let red = RgbColor::new(0xf0, 0xa0, 0xa0);
        Self {
            foreground: Color::Reset,
            background: Color::Reset,
            header: Color::Reset,
            file: Color::Reset,
            hunk: Color::Indexed(13),
            notice: green.color(),
            cursor: Color::Reset,
            cursor_line_bg: Color::Indexed(237),
            muted: Color::Rgb(0x7d, 0x87, 0x94),
            gutter_bg: Color::Indexed(0),
            empty_diff: Color::Rgb(0x3d, 0x42, 0x49),
            search_match_fg: Color::Indexed(0),
            search_match_bg: Color::Indexed(3),
            statusline_fg: Color::Reset,
            statusline_bg: Color::Reset,
            statusline_accent_fg: Color::Indexed(0),
            statusline_accent_bg: Color::Indexed(13),
            statusline_info_fg: Color::Reset,
            statusline_info_bg: Color::Indexed(0),
            addition_fg: green.color(),
            addition_gutter_bg: base.blend(green, 0.12).color(),
            addition_bg: Color::Rgb(0x1f, 0x30, 0x25),
            addition_inline_bg: base.blend(green, 0.28).color(),
            deletion_fg: red.color(),
            deletion_gutter_bg: base.blend(red, 0.12).color(),
            deletion_bg: Color::Rgb(0x37, 0x25, 0x26),
            deletion_inline_bg: base.blend(red, 0.28).color(),
            transparent_background: false,
            diff: DiffSettings::default(),
            syntax: SyntaxPalette::ansi(),
        }
    }

    pub(crate) fn ansi() -> Self {
        Self {
            foreground: Color::Reset,
            background: Color::Reset,
            header: Color::Indexed(15),
            file: Color::Indexed(15),
            hunk: Color::Indexed(13),
            notice: Color::Indexed(2),
            cursor: Color::Indexed(15),
            cursor_line_bg: Color::Indexed(237),
            muted: Color::Indexed(8),
            gutter_bg: Color::Indexed(0),
            empty_diff: Color::Indexed(8),
            search_match_fg: Color::Indexed(0),
            search_match_bg: Color::Indexed(3),
            statusline_fg: Color::Indexed(15),
            statusline_bg: Color::Indexed(0),
            statusline_accent_fg: Color::Indexed(0),
            statusline_accent_bg: Color::Indexed(13),
            statusline_info_fg: Color::Indexed(15),
            statusline_info_bg: Color::Indexed(8),
            addition_fg: Color::Indexed(2),
            addition_gutter_bg: Color::Indexed(0),
            addition_bg: Color::Reset,
            addition_inline_bg: Color::Indexed(22),
            deletion_fg: Color::Indexed(1),
            deletion_gutter_bg: Color::Indexed(0),
            deletion_bg: Color::Reset,
            deletion_inline_bg: Color::Indexed(52),
            transparent_background: false,
            diff: DiffSettings::default(),
            syntax: SyntaxPalette::ansi(),
        }
    }

    pub(crate) fn tokyonight() -> Self {
        let base = RgbColor::new(0x1a, 0x1b, 0x26);
        let green = RgbColor::new(0x9e, 0xce, 0x6a);
        let red = RgbColor::new(0xf7, 0x76, 0x8e);
        Self {
            foreground: Color::Rgb(0xc0, 0xca, 0xf5),
            background: base.color(),
            header: Color::Rgb(0xc0, 0xca, 0xf5),
            file: Color::Rgb(0xc0, 0xca, 0xf5),
            hunk: Color::Rgb(0xbb, 0x9a, 0xf7),
            notice: green.color(),
            cursor: Color::Rgb(0xc0, 0xca, 0xf5),
            cursor_line_bg: base.blend(RgbColor::new(0xbb, 0x9a, 0xf7), 0.12).color(),
            muted: Color::Rgb(0x56, 0x5f, 0x89),
            gutter_bg: base.blend(RgbColor::new(0, 0, 0), 0.22).color(),
            empty_diff: Color::Rgb(0x24, 0x28, 0x3b),
            search_match_fg: base.color(),
            search_match_bg: Color::Rgb(0xe0, 0xaf, 0x68),
            statusline_fg: Color::Rgb(0xc0, 0xca, 0xf5),
            statusline_bg: base.blend(RgbColor::new(0, 0, 0), 0.18).color(),
            statusline_accent_fg: base.color(),
            statusline_accent_bg: Color::Rgb(0xbb, 0x9a, 0xf7),
            statusline_info_fg: Color::Rgb(0xc0, 0xca, 0xf5),
            statusline_info_bg: Color::Rgb(0x24, 0x28, 0x3b),
            addition_fg: green.color(),
            addition_gutter_bg: base.blend(green, 0.035).color(),
            addition_bg: base.blend(green, 0.045).color(),
            addition_inline_bg: base.blend(green, 0.14).color(),
            deletion_fg: red.color(),
            deletion_gutter_bg: base.blend(red, 0.035).color(),
            deletion_bg: base.blend(red, 0.045).color(),
            deletion_inline_bg: base.blend(red, 0.14).color(),
            transparent_background: false,
            diff: DiffSettings::default(),
            syntax: SyntaxPalette::tokyonight(),
        }
    }

    pub(crate) fn base16(scheme: Base16Scheme) -> Self {
        Self {
            foreground: scheme.base05.color(),
            background: scheme.base00.color(),
            header: scheme.base06.color(),
            file: scheme.base05.color(),
            hunk: scheme.base0e.color(),
            notice: scheme.base0b.color(),
            cursor: scheme.base05.color(),
            cursor_line_bg: scheme.base00.blend(scheme.base0d, 0.12).color(),
            muted: scheme.base03.color(),
            gutter_bg: scheme.base00.blend(RgbColor::new(0, 0, 0), 0.18).color(),
            empty_diff: scheme.base01.color(),
            search_match_fg: scheme.base00.color(),
            search_match_bg: scheme.base0a.color(),
            statusline_fg: scheme.base05.color(),
            statusline_bg: scheme.base00.blend(RgbColor::new(0, 0, 0), 0.18).color(),
            statusline_accent_fg: scheme.base00.color(),
            statusline_accent_bg: scheme.base0e.color(),
            statusline_info_fg: scheme.base05.color(),
            statusline_info_bg: scheme.base01.color(),
            addition_fg: scheme.base0b.color(),
            addition_gutter_bg: scheme.base00.blend(scheme.base0b, 0.035).color(),
            addition_bg: scheme.base00.blend(scheme.base0b, 0.045).color(),
            addition_inline_bg: scheme.base00.blend(scheme.base0b, 0.14).color(),
            deletion_fg: scheme.base08.color(),
            deletion_gutter_bg: scheme.base00.blend(scheme.base08, 0.035).color(),
            deletion_bg: scheme.base00.blend(scheme.base08, 0.045).color(),
            deletion_inline_bg: scheme.base00.blend(scheme.base08, 0.14).color(),
            transparent_background: false,
            diff: DiffSettings::default(),
            syntax: SyntaxPalette::base16(scheme),
        }
    }

    pub(crate) fn with_transparent_background(mut self, transparent: bool) -> Self {
        self.transparent_background = transparent;
        self
    }

    pub(crate) fn with_diff_settings(mut self, diff: DiffSettings) -> Self {
        self.diff = diff;
        self
    }

    pub(crate) fn with_color_overrides(mut self, colors: &ColorOverrides) -> MarkResult<Self> {
        if let Some(color) = config_color(&colors.bg, "bg")? {
            self.background = color;
        }
        if let Some(color) = config_color(&colors.fg, "fg")? {
            self.foreground = color;
        }
        if let Some(color) = config_color(&colors.header, "header")? {
            self.header = color;
        }
        if let Some(color) = config_color(&colors.file, "file")? {
            self.file = color;
        }
        if let Some(color) = config_color(&colors.hunk, "hunk")? {
            self.hunk = color;
        }
        if let Some(color) = config_color(&colors.notice, "notice")? {
            self.notice = color;
        }
        if let Some(color) = config_color(&colors.cursor, "cursor")? {
            self.cursor = color;
        }
        if let Some(color) = config_color(&colors.cursor_line_bg, "cursor_line_bg")? {
            self.cursor_line_bg = color;
        }
        if let Some(color) = config_color(&colors.muted, "muted")? {
            self.muted = color;
        }
        if let Some(color) = config_color(&colors.gutter_bg, "gutter_bg")? {
            self.gutter_bg = color;
        }
        if let Some(color) = config_color(&colors.empty_diff, "empty_diff")? {
            self.empty_diff = color;
        }
        if let Some(color) = config_color(&colors.search_match_fg, "search_match_fg")? {
            self.search_match_fg = color;
        }
        if let Some(color) = config_color(&colors.search_match_bg, "search_match_bg")? {
            self.search_match_bg = color;
        }
        if let Some(color) = config_color(&colors.statusline_fg, "statusline_fg")? {
            self.statusline_fg = color;
        }
        if let Some(color) = config_color(&colors.statusline_bg, "statusline_bg")? {
            self.statusline_bg = color;
        }
        if let Some(color) = config_color(&colors.statusline_accent_fg, "statusline_accent_fg")? {
            self.statusline_accent_fg = color;
        }
        if let Some(color) = config_color(&colors.statusline_accent_bg, "statusline_accent_bg")? {
            self.statusline_accent_bg = color;
        }
        if let Some(color) = config_color(&colors.statusline_info_fg, "statusline_info_fg")? {
            self.statusline_info_fg = color;
        }
        if let Some(color) = config_color(&colors.statusline_info_bg, "statusline_info_bg")? {
            self.statusline_info_bg = color;
        }
        if let Some(color) = config_color(&colors.addition_fg, "addition_fg")? {
            self.addition_fg = color;
        }
        if let Some(color) = config_color(&colors.addition_gutter_bg, "addition_gutter_bg")? {
            self.addition_gutter_bg = color;
        }
        if let Some(color) = config_color(&colors.addition_bg, "addition_bg")? {
            self.addition_bg = color;
        }
        if let Some(color) = config_color(&colors.addition_inline_bg, "addition_inline_bg")? {
            self.addition_inline_bg = color;
        }
        if let Some(color) = config_color(&colors.deletion_fg, "deletion_fg")? {
            self.deletion_fg = color;
        }
        if let Some(color) = config_color(&colors.deletion_gutter_bg, "deletion_gutter_bg")? {
            self.deletion_gutter_bg = color;
        }
        if let Some(color) = config_color(&colors.deletion_bg, "deletion_bg")? {
            self.deletion_bg = color;
        }
        if let Some(color) = config_color(&colors.deletion_inline_bg, "deletion_inline_bg")? {
            self.deletion_inline_bg = color;
        }
        if let Some(color) = config_color(&colors.attribute, "attribute")? {
            self.syntax.attribute = Some(color);
        }
        if let Some(color) = config_color(&colors.comment, "comment")? {
            self.syntax.comment = Some(color);
        }
        if let Some(color) = config_color(&colors.constant, "constant")? {
            self.syntax.constant = Some(color);
        }
        if let Some(color) = config_color(&colors.constructor, "constructor")? {
            self.syntax.constructor = Some(color);
        }
        if let Some(color) = config_color(&colors.function, "function")? {
            self.syntax.function = Some(color);
        }
        if let Some(color) = config_color(&colors.keyword, "keyword")? {
            self.syntax.keyword = Some(color);
        }
        if let Some(color) = config_color(&colors.label, "label")? {
            self.syntax.label = Some(color);
        }
        if let Some(color) = config_color(&colors.module, "module")? {
            self.syntax.module = Some(color);
        }
        if let Some(color) = config_color(&colors.number, "number")? {
            self.syntax.number = Some(color);
        }
        if let Some(color) = config_color(&colors.operator, "operator")? {
            self.syntax.operator = Some(color);
        }
        if let Some(color) = config_color(&colors.property, "property")? {
            self.syntax.property = Some(color);
        }
        if let Some(color) = config_color(&colors.punctuation, "punctuation")? {
            self.syntax.punctuation = Some(color);
        }
        if let Some(color) = config_color(&colors.string, "string")? {
            self.syntax.string = Some(color);
        }
        if let Some(color) = config_color(&colors.tag, "tag")? {
            self.syntax.tag = Some(color);
        }
        if let Some(color) = config_color(&colors.r#type, "type")? {
            self.syntax.r#type = Some(color);
        }
        if let Some(color) = config_color(&colors.variable, "variable")? {
            self.syntax.variable = Some(color);
        }
        Ok(self)
    }
}
