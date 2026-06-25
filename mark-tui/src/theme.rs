use std::{
    env,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use mark_core::{MarkError, MarkResult};
use mark_diff::DiffLineKind;
use mark_syntax::{
    ColorOverrides, DiffGutterBackground, DiffSettings, SyntaxClass, SyntaxThemeConfig,
    SyntaxThemeSource,
};
use ratatui::prelude::Color;

use crate::keymap::GlobalAction;

pub(crate) const EVENT_POLL: Duration = Duration::from_millis(120);
pub(crate) const NOTICE_TTL: Duration = Duration::from_millis(1_500);
pub(crate) const LIVE_RELOAD_DEBOUNCE: Duration = Duration::from_millis(200);
pub(crate) const MAX_READY_EVENTS_PER_FRAME: usize = 64;
pub(crate) const MOUSE_SCROLL_HISTORY_SIZE: usize = 3;
pub(crate) const MOUSE_SCROLL_STREAK_TIMEOUT: Duration = Duration::from_millis(150);
pub(crate) const MOUSE_SCROLL_MIN_TICK_INTERVAL: Duration = Duration::from_millis(6);
pub(crate) const MOUSE_SCROLL_ACCEL_A: f64 = 0.4;
pub(crate) const MOUSE_SCROLL_ACCEL_TAU: f64 = 4.0;
pub(crate) const MOUSE_SCROLL_MAX_MULTIPLIER: f64 = 3.0;
pub(crate) const MOUSE_SCROLL_REFERENCE_INTERVAL_MS: f64 = 100.0;
pub(crate) const HORIZONTAL_SCROLL_STEP: usize = 8;
pub(crate) const MIN_SPLIT_WIDTH: u16 = 120;
pub(crate) const GUTTER_WIDTH: usize = 7;
pub(crate) const UNIFIED_GUTTER_WIDTH: usize = 13;
pub(crate) const DIFF_INDICATOR: &str = "▌";
pub(crate) const EMPTY_DIFF_FILL: char = '╱';
pub(crate) const EMPTY_DIFF_FILL_SPACING: usize = 3;
pub(crate) const MAX_SYNTAX_RESULTS_PER_FRAME: usize = 64;
pub(crate) const SYNTAX_THEME_ID: u64 = 0;
pub(crate) const MAX_INLINE_DIFF_LINE_BYTES: usize = 4 * 1024;
pub(crate) const MAX_INLINE_DIFF_TOKENS: usize = 256;
pub(crate) const MAX_INLINE_DIFF_CACHE_ENTRIES: usize = 512;
pub(crate) const MAX_BRANCH_MENU_ROWS: usize = 10;
pub(crate) const FILE_SIDEBAR_MIN_WIDTH: u16 = 20;
pub(crate) const FILE_SIDEBAR_MAX_WIDTH: u16 = 40;
pub(crate) const FILE_SIDEBAR_MIN_DIFF_WIDTH: u16 = 30;
pub(crate) const BRANCH_COMPARISON_SEPARATOR: &str = " → ";
pub(crate) const CURRENT_BRANCH_MARKER: &str = "●";
pub(crate) const BASE_BRANCH_MARKER: &str = "⌂";
pub(crate) const STATUSLINE_SELECTOR_GAP: &str = " ";
pub(crate) const HELP_MENU_WIDTH: u16 = 90;
pub(crate) const HELP_KEY_COLUMN_WIDTH: usize = 17;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HelpMenuKey {
    Static(&'static str),
    Leader,
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
    HelpMenuRow::Binding(HelpMenuKey::Leader, "leader"),
    HelpMenuRow::Binding(HelpMenuKey::Global(GlobalAction::Quit), "quit"),
    HelpMenuRow::Binding(HelpMenuKey::Static("Ctrl-C"), "force quit"),
    HelpMenuRow::Binding(HelpMenuKey::Static("Esc"), "close"),
    HelpMenuRow::Section("Navigate"),
    HelpMenuRow::Binding(HelpMenuKey::Static("j/k, ↑/↓"), "scroll"),
    HelpMenuRow::Binding(HelpMenuKey::Static("d/u, PgDn/PgUp"), "page"),
    HelpMenuRow::Binding(HelpMenuKey::Static("g/G, Home/End"), "top / bottom"),
    HelpMenuRow::Binding(HelpMenuKey::Static("h/l, ←/→"), "horizontal"),
    HelpMenuRow::Binding(HelpMenuKey::Static("J/K"), "file"),
    HelpMenuRow::Binding(HelpMenuKey::Static("]/["), "hunk"),
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
        HelpMenuKey::Global(GlobalAction::OptionsMenu),
        "settings menu",
    ),
    HelpMenuRow::Section("Annotations"),
    HelpMenuRow::Binding(HelpMenuKey::Static("hover [+]"), "add / edit annotation"),
    HelpMenuRow::Binding(HelpMenuKey::Global(GlobalAction::SaveMark), "save mark"),
    HelpMenuRow::Binding(HelpMenuKey::Global(GlobalAction::CancelMark), "cancel mark"),
    HelpMenuRow::Binding(
        HelpMenuKey::Global(GlobalAction::EditHunk),
        "edit active mark in editor",
    ),
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

pub(crate) fn line_gutter_fg(kind: DiffLineKind, theme: DiffTheme) -> Color {
    match kind {
        DiffLineKind::Addition => theme.addition_fg,
        DiffLineKind::Deletion => theme.deletion_fg,
        DiffLineKind::Context | DiffLineKind::Meta => theme.foreground,
    }
}

pub(crate) fn line_gutter_bg(kind: DiffLineKind, theme: DiffTheme) -> Color {
    if theme.transparent_background {
        return Color::Reset;
    }

    match (theme.diff.gutter_background, kind) {
        (DiffGutterBackground::Delta, DiffLineKind::Addition) => theme.addition_gutter_bg,
        (DiffGutterBackground::Delta, DiffLineKind::Deletion) => theme.deletion_gutter_bg,
        _ => theme.gutter_bg,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiffBenchmarkOptions {
    pub width: usize,
    pub viewport_rows: usize,
    pub scroll_step: usize,
    pub max_scroll_steps: usize,
}

impl Default for DiffBenchmarkOptions {
    fn default() -> Self {
        Self {
            width: 160,
            viewport_rows: 40,
            scroll_step: 20,
            max_scroll_steps: 200,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffBenchmarkReport {
    pub syntax_enabled: bool,
    pub row_count: usize,
    pub file_count: usize,
    pub hunk_count: usize,
    pub open_micros: u128,
    pub file_filter_micros: u128,
    pub legacy_file_filter_micros: u128,
    pub grep_filter_micros: u128,
    pub legacy_grep_filter_micros: u128,
    pub file_filter_apply_micros: u128,
    pub grep_filter_apply_micros: u128,
    pub hunk_navigation_steps: usize,
    pub hunk_navigation_total_micros: u128,
    pub hunk_navigation_max_micros: u128,
    pub initial_render_micros: u128,
    pub cold_scroll_steps: usize,
    pub cold_scroll_total_micros: u128,
    pub cold_scroll_max_micros: u128,
    pub syntax_settle_micros: Option<u128>,
    pub warm_scroll_steps: usize,
    pub warm_scroll_total_micros: u128,
    pub warm_scroll_max_micros: u128,
    pub warm_cache_hits: u64,
    pub warm_cache_misses: u64,
    pub syntax: SyntaxBenchmarkReport,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyntaxBenchmarkReport {
    pub queue_requests: u64,
    pub jobs_queued: u64,
    pub jobs_completed: u64,
    pub jobs_failed: u64,
    pub jobs_skipped: u64,
    pub jobs_rejected: u64,
    pub jobs_evicted: u64,
    pub stale_results: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub cache_entries_peak: usize,
    pub queue_depth_peak: usize,
    pub source_bytes_queued: u64,
    pub source_lines_queued: u64,
    pub estimated_memory_peak_bytes: u64,
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

    pub(crate) fn catppuccin_mocha() -> Self {
        Self::catppuccin(CatppuccinPalette::MOCHA)
    }

    pub(crate) fn catppuccin_macchiato() -> Self {
        Self::catppuccin(CatppuccinPalette::MACCHIATO)
    }

    pub(crate) fn catppuccin_frappe() -> Self {
        Self::catppuccin(CatppuccinPalette::FRAPPE)
    }

    pub(crate) fn catppuccin_latte() -> Self {
        Self::catppuccin(CatppuccinPalette::LATTE)
    }

    fn catppuccin(palette: CatppuccinPalette) -> Self {
        Self {
            foreground: palette.text.color(),
            background: palette.base.color(),
            header: palette.lavender.color(),
            file: palette.text.color(),
            hunk: palette.mauve.color(),
            notice: palette.green.color(),
            cursor: palette.rosewater.color(),
            cursor_line_bg: palette.base.blend(palette.rosewater, 0.12).color(),
            muted: palette.overlay0.color(),
            gutter_bg: palette.mantle.color(),
            empty_diff: palette.surface0.color(),
            search_match_fg: palette.base.color(),
            search_match_bg: palette.yellow.color(),
            statusline_fg: palette.text.color(),
            statusline_bg: palette.mantle.color(),
            statusline_accent_fg: palette.base.color(),
            statusline_accent_bg: palette.mauve.color(),
            statusline_info_fg: palette.text.color(),
            statusline_info_bg: palette.surface0.color(),
            addition_fg: palette.green.color(),
            addition_gutter_bg: palette.base.blend(palette.green, 0.035).color(),
            addition_bg: palette.base.blend(palette.green, 0.045).color(),
            addition_inline_bg: palette.base.blend(palette.green, 0.14).color(),
            deletion_fg: palette.red.color(),
            deletion_gutter_bg: palette.base.blend(palette.red, 0.035).color(),
            deletion_bg: palette.base.blend(palette.red, 0.045).color(),
            deletion_inline_bg: palette.base.blend(palette.red, 0.14).color(),
            transparent_background: false,
            diff: DiffSettings::default(),
            syntax: SyntaxPalette::catppuccin(palette),
        }
    }

    pub(crate) fn gruvbox_dark() -> Self {
        Self::gruvbox(GruvboxPalette::DARK)
    }

    pub(crate) fn gruvbox_light() -> Self {
        Self::gruvbox(GruvboxPalette::LIGHT)
    }

    fn gruvbox(palette: GruvboxPalette) -> Self {
        let addition = palette.bright_green;
        let deletion = palette.bright_red;
        Self {
            foreground: palette.fg1.color(),
            background: palette.bg0.color(),
            header: palette.fg0.color(),
            file: palette.fg1.color(),
            hunk: palette.bright_purple.color(),
            notice: addition.color(),
            cursor: palette.fg0.color(),
            cursor_line_bg: palette.bg0.blend(palette.fg0, 0.10).color(),
            muted: palette.gray.color(),
            gutter_bg: palette.bg0_h.color(),
            empty_diff: palette.bg1.color(),
            search_match_fg: palette.bg0.color(),
            search_match_bg: palette.bright_yellow.color(),
            statusline_fg: palette.fg1.color(),
            statusline_bg: palette.bg0_h.color(),
            statusline_accent_fg: palette.bg0.color(),
            statusline_accent_bg: palette.bright_purple.color(),
            statusline_info_fg: palette.fg1.color(),
            statusline_info_bg: palette.bg1.color(),
            addition_fg: addition.color(),
            addition_gutter_bg: palette.bg0.blend(addition, 0.035).color(),
            addition_bg: palette.bg0.blend(addition, 0.045).color(),
            addition_inline_bg: palette.bg0.blend(addition, 0.14).color(),
            deletion_fg: deletion.color(),
            deletion_gutter_bg: palette.bg0.blend(deletion, 0.035).color(),
            deletion_bg: palette.bg0.blend(deletion, 0.045).color(),
            deletion_inline_bg: palette.bg0.blend(deletion, 0.14).color(),
            transparent_background: false,
            diff: DiffSettings::default(),
            syntax: SyntaxPalette::gruvbox(palette),
        }
    }

    pub(crate) fn github_dark() -> Self {
        Self::github(GithubPalette::DARK)
    }

    pub(crate) fn github_dark_high_contrast() -> Self {
        Self::github(GithubPalette::DARK_HIGH_CONTRAST)
    }

    pub(crate) fn github_light() -> Self {
        Self::github(GithubPalette::LIGHT)
    }

    pub(crate) fn github_light_high_contrast() -> Self {
        Self::github(GithubPalette::LIGHT_HIGH_CONTRAST)
    }

    fn github(palette: GithubPalette) -> Self {
        Self {
            foreground: palette.fg_default.color(),
            background: palette.canvas_default.color(),
            header: palette.fg_default.color(),
            file: palette.fg_default.color(),
            hunk: palette.done_fg.color(),
            notice: palette.success_fg.color(),
            cursor: palette.fg_default.color(),
            cursor_line_bg: palette
                .canvas_default
                .blend(palette.accent_fg, 0.10)
                .color(),
            muted: palette.fg_muted.color(),
            gutter_bg: palette.canvas_subtle.color(),
            empty_diff: palette.canvas_inset.color(),
            search_match_fg: palette.canvas_default.color(),
            search_match_bg: palette.attention_fg.color(),
            statusline_fg: palette.fg_default.color(),
            statusline_bg: palette.canvas_subtle.color(),
            statusline_accent_fg: palette.canvas_default.color(),
            statusline_accent_bg: palette.accent_fg.color(),
            statusline_info_fg: palette.fg_default.color(),
            statusline_info_bg: palette.canvas_inset.color(),
            addition_fg: palette.success_fg.color(),
            addition_gutter_bg: palette
                .canvas_default
                .blend(palette.success_fg, 0.05)
                .color(),
            addition_bg: palette
                .canvas_default
                .blend(palette.success_fg, 0.06)
                .color(),
            addition_inline_bg: palette
                .canvas_default
                .blend(palette.success_fg, 0.16)
                .color(),
            deletion_fg: palette.danger_fg.color(),
            deletion_gutter_bg: palette
                .canvas_default
                .blend(palette.danger_fg, 0.05)
                .color(),
            deletion_bg: palette
                .canvas_default
                .blend(palette.danger_fg, 0.06)
                .color(),
            deletion_inline_bg: palette
                .canvas_default
                .blend(palette.danger_fg, 0.16)
                .color(),
            transparent_background: false,
            diff: DiffSettings::default(),
            syntax: SyntaxPalette::github(palette),
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

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CatppuccinPalette {
    rosewater: RgbColor,
    flamingo: RgbColor,
    pink: RgbColor,
    mauve: RgbColor,
    red: RgbColor,
    maroon: RgbColor,
    peach: RgbColor,
    yellow: RgbColor,
    green: RgbColor,
    teal: RgbColor,
    sky: RgbColor,
    sapphire: RgbColor,
    blue: RgbColor,
    lavender: RgbColor,
    text: RgbColor,
    subtext1: RgbColor,
    subtext0: RgbColor,
    overlay2: RgbColor,
    overlay1: RgbColor,
    overlay0: RgbColor,
    surface2: RgbColor,
    surface1: RgbColor,
    surface0: RgbColor,
    base: RgbColor,
    mantle: RgbColor,
    crust: RgbColor,
}

impl CatppuccinPalette {
    const LATTE: Self = Self {
        rosewater: RgbColor::new(0xdc, 0x8a, 0x78),
        flamingo: RgbColor::new(0xdd, 0x78, 0x78),
        pink: RgbColor::new(0xea, 0x76, 0xcb),
        mauve: RgbColor::new(0x88, 0x39, 0xef),
        red: RgbColor::new(0xd2, 0x0f, 0x39),
        maroon: RgbColor::new(0xe6, 0x45, 0x53),
        peach: RgbColor::new(0xfe, 0x64, 0x0b),
        yellow: RgbColor::new(0xdf, 0x8e, 0x1d),
        green: RgbColor::new(0x40, 0xa0, 0x2b),
        teal: RgbColor::new(0x17, 0x92, 0x99),
        sky: RgbColor::new(0x04, 0xa5, 0xe5),
        sapphire: RgbColor::new(0x20, 0x9f, 0xb5),
        blue: RgbColor::new(0x1e, 0x66, 0xf5),
        lavender: RgbColor::new(0x72, 0x87, 0xfd),
        text: RgbColor::new(0x4c, 0x4f, 0x69),
        subtext1: RgbColor::new(0x5c, 0x5f, 0x77),
        subtext0: RgbColor::new(0x6c, 0x6f, 0x85),
        overlay2: RgbColor::new(0x7c, 0x7f, 0x93),
        overlay1: RgbColor::new(0x8c, 0x8f, 0xa1),
        overlay0: RgbColor::new(0x9c, 0xa0, 0xb0),
        surface2: RgbColor::new(0xac, 0xb0, 0xbe),
        surface1: RgbColor::new(0xbc, 0xc0, 0xcc),
        surface0: RgbColor::new(0xcc, 0xd0, 0xda),
        base: RgbColor::new(0xef, 0xf1, 0xf5),
        mantle: RgbColor::new(0xe6, 0xe9, 0xef),
        crust: RgbColor::new(0xdc, 0xe0, 0xe8),
    };

    const FRAPPE: Self = Self {
        rosewater: RgbColor::new(0xf2, 0xd5, 0xcf),
        flamingo: RgbColor::new(0xee, 0xbe, 0xbe),
        pink: RgbColor::new(0xf4, 0xb8, 0xe4),
        mauve: RgbColor::new(0xca, 0x9e, 0xe6),
        red: RgbColor::new(0xe7, 0x82, 0x84),
        maroon: RgbColor::new(0xea, 0x99, 0x9c),
        peach: RgbColor::new(0xef, 0x9f, 0x76),
        yellow: RgbColor::new(0xe5, 0xc8, 0x90),
        green: RgbColor::new(0xa6, 0xd1, 0x89),
        teal: RgbColor::new(0x81, 0xc8, 0xbe),
        sky: RgbColor::new(0x99, 0xd1, 0xdb),
        sapphire: RgbColor::new(0x85, 0xc1, 0xdc),
        blue: RgbColor::new(0x8c, 0xaa, 0xee),
        lavender: RgbColor::new(0xba, 0xbb, 0xf1),
        text: RgbColor::new(0xc6, 0xd0, 0xf5),
        subtext1: RgbColor::new(0xb5, 0xbf, 0xe2),
        subtext0: RgbColor::new(0xa5, 0xad, 0xce),
        overlay2: RgbColor::new(0x94, 0x9c, 0xbb),
        overlay1: RgbColor::new(0x83, 0x8b, 0xa7),
        overlay0: RgbColor::new(0x73, 0x79, 0x94),
        surface2: RgbColor::new(0x62, 0x68, 0x80),
        surface1: RgbColor::new(0x51, 0x57, 0x6d),
        surface0: RgbColor::new(0x41, 0x45, 0x59),
        base: RgbColor::new(0x30, 0x34, 0x46),
        mantle: RgbColor::new(0x29, 0x2c, 0x3c),
        crust: RgbColor::new(0x23, 0x26, 0x34),
    };

    const MACCHIATO: Self = Self {
        rosewater: RgbColor::new(0xf4, 0xdb, 0xd6),
        flamingo: RgbColor::new(0xf0, 0xc6, 0xc6),
        pink: RgbColor::new(0xf5, 0xbd, 0xe6),
        mauve: RgbColor::new(0xc6, 0xa0, 0xf6),
        red: RgbColor::new(0xed, 0x87, 0x96),
        maroon: RgbColor::new(0xee, 0x99, 0xa0),
        peach: RgbColor::new(0xf5, 0xa9, 0x7f),
        yellow: RgbColor::new(0xee, 0xd4, 0x9f),
        green: RgbColor::new(0xa6, 0xda, 0x95),
        teal: RgbColor::new(0x8b, 0xd5, 0xca),
        sky: RgbColor::new(0x91, 0xd7, 0xe3),
        sapphire: RgbColor::new(0x7d, 0xc4, 0xe4),
        blue: RgbColor::new(0x8a, 0xad, 0xf4),
        lavender: RgbColor::new(0xb7, 0xbd, 0xf8),
        text: RgbColor::new(0xca, 0xd3, 0xf5),
        subtext1: RgbColor::new(0xb8, 0xc0, 0xe0),
        subtext0: RgbColor::new(0xa5, 0xad, 0xcb),
        overlay2: RgbColor::new(0x93, 0x9a, 0xb7),
        overlay1: RgbColor::new(0x80, 0x87, 0xa2),
        overlay0: RgbColor::new(0x6e, 0x73, 0x8d),
        surface2: RgbColor::new(0x5b, 0x60, 0x78),
        surface1: RgbColor::new(0x49, 0x4d, 0x64),
        surface0: RgbColor::new(0x36, 0x3a, 0x4f),
        base: RgbColor::new(0x24, 0x27, 0x3a),
        mantle: RgbColor::new(0x1e, 0x20, 0x30),
        crust: RgbColor::new(0x18, 0x19, 0x26),
    };

    const MOCHA: Self = Self {
        rosewater: RgbColor::new(0xf5, 0xe0, 0xdc),
        flamingo: RgbColor::new(0xf2, 0xcd, 0xcd),
        pink: RgbColor::new(0xf5, 0xc2, 0xe7),
        mauve: RgbColor::new(0xcb, 0xa6, 0xf7),
        red: RgbColor::new(0xf3, 0x8b, 0xa8),
        maroon: RgbColor::new(0xeb, 0xa0, 0xac),
        peach: RgbColor::new(0xfa, 0xb3, 0x87),
        yellow: RgbColor::new(0xf9, 0xe2, 0xaf),
        green: RgbColor::new(0xa6, 0xe3, 0xa1),
        teal: RgbColor::new(0x94, 0xe2, 0xd5),
        sky: RgbColor::new(0x89, 0xdc, 0xeb),
        sapphire: RgbColor::new(0x74, 0xc7, 0xec),
        blue: RgbColor::new(0x89, 0xb4, 0xfa),
        lavender: RgbColor::new(0xb4, 0xbe, 0xfe),
        text: RgbColor::new(0xcd, 0xd6, 0xf4),
        subtext1: RgbColor::new(0xba, 0xc2, 0xde),
        subtext0: RgbColor::new(0xa6, 0xad, 0xc8),
        overlay2: RgbColor::new(0x93, 0x99, 0xb2),
        overlay1: RgbColor::new(0x7f, 0x84, 0x9c),
        overlay0: RgbColor::new(0x6c, 0x70, 0x86),
        surface2: RgbColor::new(0x58, 0x5b, 0x70),
        surface1: RgbColor::new(0x45, 0x47, 0x5a),
        surface0: RgbColor::new(0x31, 0x32, 0x44),
        base: RgbColor::new(0x1e, 0x1e, 0x2e),
        mantle: RgbColor::new(0x18, 0x18, 0x25),
        crust: RgbColor::new(0x11, 0x11, 0x1b),
    };
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GruvboxPalette {
    bg0_h: RgbColor,
    bg0: RgbColor,
    bg0_s: RgbColor,
    bg1: RgbColor,
    bg2: RgbColor,
    bg3: RgbColor,
    bg4: RgbColor,
    fg0: RgbColor,
    fg1: RgbColor,
    fg2: RgbColor,
    fg3: RgbColor,
    fg4: RgbColor,
    red: RgbColor,
    green: RgbColor,
    yellow: RgbColor,
    blue: RgbColor,
    purple: RgbColor,
    aqua: RgbColor,
    gray: RgbColor,
    orange: RgbColor,
    bright_red: RgbColor,
    bright_green: RgbColor,
    bright_yellow: RgbColor,
    bright_blue: RgbColor,
    bright_purple: RgbColor,
    bright_aqua: RgbColor,
    bright_gray: RgbColor,
    bright_orange: RgbColor,
}

impl GruvboxPalette {
    const DARK: Self = Self {
        bg0_h: RgbColor::new(0x1d, 0x20, 0x21),
        bg0: RgbColor::new(0x28, 0x28, 0x28),
        bg0_s: RgbColor::new(0x32, 0x30, 0x2f),
        bg1: RgbColor::new(0x3c, 0x38, 0x36),
        bg2: RgbColor::new(0x50, 0x49, 0x45),
        bg3: RgbColor::new(0x66, 0x5c, 0x54),
        bg4: RgbColor::new(0x7c, 0x6f, 0x64),
        fg0: RgbColor::new(0xfb, 0xf1, 0xc7),
        fg1: RgbColor::new(0xeb, 0xdb, 0xb2),
        fg2: RgbColor::new(0xd5, 0xc4, 0xa1),
        fg3: RgbColor::new(0xbd, 0xae, 0x93),
        fg4: RgbColor::new(0xa8, 0x99, 0x84),
        red: RgbColor::new(0xcc, 0x24, 0x1d),
        green: RgbColor::new(0x98, 0x97, 0x1a),
        yellow: RgbColor::new(0xd7, 0x99, 0x21),
        blue: RgbColor::new(0x45, 0x85, 0x88),
        purple: RgbColor::new(0xb1, 0x62, 0x86),
        aqua: RgbColor::new(0x68, 0x9d, 0x6a),
        gray: RgbColor::new(0x92, 0x83, 0x74),
        orange: RgbColor::new(0xd6, 0x5d, 0x0e),
        bright_red: RgbColor::new(0xfb, 0x49, 0x34),
        bright_green: RgbColor::new(0xb8, 0xbb, 0x26),
        bright_yellow: RgbColor::new(0xfa, 0xbd, 0x2f),
        bright_blue: RgbColor::new(0x83, 0xa5, 0x98),
        bright_purple: RgbColor::new(0xd3, 0x86, 0x9b),
        bright_aqua: RgbColor::new(0x8e, 0xc0, 0x7c),
        bright_gray: RgbColor::new(0xa8, 0x99, 0x84),
        bright_orange: RgbColor::new(0xfe, 0x80, 0x19),
    };

    const LIGHT: Self = Self {
        bg0_h: RgbColor::new(0xf9, 0xf5, 0xd7),
        bg0: RgbColor::new(0xfb, 0xf1, 0xc7),
        bg0_s: RgbColor::new(0xf2, 0xe5, 0xbc),
        bg1: RgbColor::new(0xeb, 0xdb, 0xb2),
        bg2: RgbColor::new(0xd5, 0xc4, 0xa1),
        bg3: RgbColor::new(0xbd, 0xae, 0x93),
        bg4: RgbColor::new(0xa8, 0x99, 0x84),
        fg0: RgbColor::new(0x28, 0x28, 0x28),
        fg1: RgbColor::new(0x3c, 0x38, 0x36),
        fg2: RgbColor::new(0x50, 0x49, 0x45),
        fg3: RgbColor::new(0x66, 0x5c, 0x54),
        fg4: RgbColor::new(0x7c, 0x6f, 0x64),
        red: RgbColor::new(0xcc, 0x24, 0x1d),
        green: RgbColor::new(0x98, 0x97, 0x1a),
        yellow: RgbColor::new(0xd7, 0x99, 0x21),
        blue: RgbColor::new(0x45, 0x85, 0x88),
        purple: RgbColor::new(0xb1, 0x62, 0x86),
        aqua: RgbColor::new(0x68, 0x9d, 0x6a),
        gray: RgbColor::new(0x92, 0x83, 0x74),
        orange: RgbColor::new(0xd6, 0x5d, 0x0e),
        bright_red: RgbColor::new(0x9d, 0x00, 0x06),
        bright_green: RgbColor::new(0x79, 0x74, 0x0e),
        bright_yellow: RgbColor::new(0xb5, 0x76, 0x14),
        bright_blue: RgbColor::new(0x07, 0x66, 0x78),
        bright_purple: RgbColor::new(0x8f, 0x3f, 0x71),
        bright_aqua: RgbColor::new(0x42, 0x7b, 0x58),
        bright_gray: RgbColor::new(0x92, 0x83, 0x74),
        bright_orange: RgbColor::new(0xaf, 0x3a, 0x03),
    };
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GithubPalette {
    fg_default: RgbColor,
    fg_muted: RgbColor,
    fg_subtle: RgbColor,
    canvas_default: RgbColor,
    canvas_subtle: RgbColor,
    canvas_inset: RgbColor,
    border_default: RgbColor,
    accent_fg: RgbColor,
    success_fg: RgbColor,
    attention_fg: RgbColor,
    severe_fg: RgbColor,
    danger_fg: RgbColor,
    done_fg: RgbColor,
    syntax_comment: RgbColor,
    syntax_constant: RgbColor,
    syntax_entity: RgbColor,
    syntax_storage: RgbColor,
    syntax_string: RgbColor,
    syntax_variable: RgbColor,
    syntax_regexp: RgbColor,
}

impl GithubPalette {
    const LIGHT: Self = Self {
        fg_default: RgbColor::new(0x24, 0x29, 0x2f),
        fg_muted: RgbColor::new(0x57, 0x60, 0x6a),
        fg_subtle: RgbColor::new(0x6e, 0x77, 0x81),
        canvas_default: RgbColor::new(0xff, 0xff, 0xff),
        canvas_subtle: RgbColor::new(0xf6, 0xf8, 0xfa),
        canvas_inset: RgbColor::new(0xea, 0xee, 0xf2),
        border_default: RgbColor::new(0xd0, 0xd7, 0xde),
        accent_fg: RgbColor::new(0x09, 0x69, 0xda),
        success_fg: RgbColor::new(0x1a, 0x7f, 0x37),
        attention_fg: RgbColor::new(0x9a, 0x67, 0x00),
        severe_fg: RgbColor::new(0xbc, 0x4c, 0x00),
        danger_fg: RgbColor::new(0xcf, 0x22, 0x2e),
        done_fg: RgbColor::new(0x82, 0x50, 0xdf),
        syntax_comment: RgbColor::new(0x6e, 0x77, 0x81),
        syntax_constant: RgbColor::new(0x05, 0x50, 0xae),
        syntax_entity: RgbColor::new(0x82, 0x50, 0xdf),
        syntax_storage: RgbColor::new(0xcf, 0x22, 0x2e),
        syntax_string: RgbColor::new(0x0a, 0x30, 0x69),
        syntax_variable: RgbColor::new(0x95, 0x38, 0x00),
        syntax_regexp: RgbColor::new(0x11, 0x63, 0x29),
    };

    const DARK: Self = Self {
        fg_default: RgbColor::new(0xc9, 0xd1, 0xd9),
        fg_muted: RgbColor::new(0x8b, 0x94, 0x9e),
        fg_subtle: RgbColor::new(0x6e, 0x76, 0x81),
        canvas_default: RgbColor::new(0x0d, 0x11, 0x17),
        canvas_subtle: RgbColor::new(0x16, 0x1b, 0x22),
        canvas_inset: RgbColor::new(0x01, 0x04, 0x09),
        border_default: RgbColor::new(0x30, 0x36, 0x3d),
        accent_fg: RgbColor::new(0x58, 0xa6, 0xff),
        success_fg: RgbColor::new(0x3f, 0xb9, 0x50),
        attention_fg: RgbColor::new(0xd2, 0x99, 0x22),
        severe_fg: RgbColor::new(0xdb, 0x6d, 0x28),
        danger_fg: RgbColor::new(0xf8, 0x51, 0x49),
        done_fg: RgbColor::new(0xa3, 0x71, 0xf7),
        syntax_comment: RgbColor::new(0x8b, 0x94, 0x9e),
        syntax_constant: RgbColor::new(0x79, 0xc0, 0xff),
        syntax_entity: RgbColor::new(0xd2, 0xa8, 0xff),
        syntax_storage: RgbColor::new(0xff, 0x7b, 0x72),
        syntax_string: RgbColor::new(0xa5, 0xd6, 0xff),
        syntax_variable: RgbColor::new(0xff, 0xa6, 0x57),
        syntax_regexp: RgbColor::new(0x7e, 0xe7, 0x87),
    };

    const LIGHT_HIGH_CONTRAST: Self = Self {
        fg_default: RgbColor::new(0x0e, 0x11, 0x16),
        fg_muted: RgbColor::new(0x4b, 0x53, 0x5d),
        fg_subtle: RgbColor::new(0x59, 0x63, 0x6e),
        canvas_default: RgbColor::new(0xff, 0xff, 0xff),
        canvas_subtle: RgbColor::new(0xf6, 0xf8, 0xfa),
        canvas_inset: RgbColor::new(0xea, 0xee, 0xf2),
        border_default: RgbColor::new(0x85, 0x8f, 0x99),
        accent_fg: RgbColor::new(0x03, 0x49, 0xb4),
        success_fg: RgbColor::new(0x00, 0x6d, 0x32),
        attention_fg: RgbColor::new(0x7d, 0x4e, 0x00),
        severe_fg: RgbColor::new(0xa0, 0x41, 0x00),
        danger_fg: RgbColor::new(0xa4, 0x0e, 0x26),
        done_fg: RgbColor::new(0x62, 0x2c, 0xb8),
        syntax_comment: RgbColor::new(0x66, 0x70, 0x7b),
        syntax_constant: RgbColor::new(0x02, 0x3b, 0x95),
        syntax_entity: RgbColor::new(0x62, 0x2c, 0xbc),
        syntax_storage: RgbColor::new(0xa0, 0x11, 0x1f),
        syntax_string: RgbColor::new(0x03, 0x25, 0x63),
        syntax_variable: RgbColor::new(0x70, 0x2c, 0x00),
        syntax_regexp: RgbColor::new(0x02, 0x4c, 0x1a),
    };

    const DARK_HIGH_CONTRAST: Self = Self {
        fg_default: RgbColor::new(0xf0, 0xf3, 0xf6),
        fg_muted: RgbColor::new(0xbd, 0xc4, 0xcc),
        fg_subtle: RgbColor::new(0x9e, 0xa7, 0xb3),
        canvas_default: RgbColor::new(0x0a, 0x0c, 0x10),
        canvas_subtle: RgbColor::new(0x27, 0x2b, 0x33),
        canvas_inset: RgbColor::new(0x01, 0x04, 0x09),
        border_default: RgbColor::new(0x7a, 0x82, 0x8e),
        accent_fg: RgbColor::new(0x71, 0xb7, 0xff),
        success_fg: RgbColor::new(0x26, 0xcd, 0x4d),
        attention_fg: RgbColor::new(0xf0, 0xb7, 0x2f),
        severe_fg: RgbColor::new(0xe7, 0x81, 0x1d),
        danger_fg: RgbColor::new(0xff, 0x94, 0x92),
        done_fg: RgbColor::new(0xcb, 0x9e, 0xff),
        syntax_comment: RgbColor::new(0xbd, 0xc4, 0xcc),
        syntax_constant: RgbColor::new(0x91, 0xcb, 0xff),
        syntax_entity: RgbColor::new(0xdb, 0xb7, 0xff),
        syntax_storage: RgbColor::new(0xff, 0x94, 0x92),
        syntax_string: RgbColor::new(0xad, 0xdc, 0xff),
        syntax_variable: RgbColor::new(0xff, 0xb7, 0x57),
        syntax_regexp: RgbColor::new(0x72, 0xf0, 0x88),
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SyntaxPalette {
    pub(crate) attribute: Option<Color>,
    pub(crate) comment: Option<Color>,
    pub(crate) constant: Option<Color>,
    pub(crate) constructor: Option<Color>,
    pub(crate) function: Option<Color>,
    pub(crate) keyword: Option<Color>,
    pub(crate) label: Option<Color>,
    pub(crate) module: Option<Color>,
    pub(crate) number: Option<Color>,
    pub(crate) operator: Option<Color>,
    pub(crate) property: Option<Color>,
    pub(crate) punctuation: Option<Color>,
    pub(crate) string: Option<Color>,
    pub(crate) tag: Option<Color>,
    pub(crate) r#type: Option<Color>,
    pub(crate) variable: Option<Color>,
}

impl SyntaxPalette {
    pub(crate) fn ansi() -> Self {
        Self {
            attribute: Some(Color::Indexed(12)),
            comment: Some(Color::Indexed(8)),
            constant: Some(Color::Indexed(11)),
            constructor: Some(Color::Indexed(14)),
            function: Some(Color::Indexed(12)),
            keyword: Some(Color::Indexed(13)),
            label: Some(Color::Indexed(12)),
            module: Some(Color::Indexed(12)),
            number: Some(Color::Indexed(11)),
            operator: Some(Color::Indexed(13)),
            property: Some(Color::Indexed(12)),
            punctuation: Some(Color::Indexed(8)),
            string: Some(Color::Indexed(10)),
            tag: Some(Color::Indexed(9)),
            r#type: Some(Color::Indexed(14)),
            variable: None,
        }
    }

    fn catppuccin(palette: CatppuccinPalette) -> Self {
        Self {
            attribute: Some(palette.yellow.color()),
            comment: Some(palette.overlay2.color()),
            constant: Some(palette.peach.color()),
            constructor: Some(palette.yellow.color()),
            function: Some(palette.blue.color()),
            keyword: Some(palette.mauve.color()),
            label: Some(palette.yellow.color()),
            module: Some(palette.yellow.color()),
            number: Some(palette.peach.color()),
            operator: Some(palette.teal.color()),
            property: Some(palette.teal.color()),
            punctuation: Some(palette.overlay2.color()),
            string: Some(palette.green.color()),
            tag: Some(palette.blue.color()),
            r#type: Some(palette.yellow.color()),
            variable: None,
        }
    }

    fn gruvbox(palette: GruvboxPalette) -> Self {
        Self {
            attribute: Some(palette.bright_yellow.color()),
            comment: Some(palette.gray.color()),
            constant: Some(palette.bright_purple.color()),
            constructor: Some(palette.bright_yellow.color()),
            function: Some(palette.bright_yellow.color()),
            keyword: Some(palette.bright_red.color()),
            label: Some(palette.bright_yellow.color()),
            module: Some(palette.bright_yellow.color()),
            number: Some(palette.bright_purple.color()),
            operator: Some(palette.bright_aqua.color()),
            property: Some(palette.aqua.color()),
            punctuation: Some(palette.fg4.color()),
            string: Some(palette.bright_green.color()),
            tag: Some(palette.bright_aqua.color()),
            r#type: Some(palette.bright_yellow.color()),
            variable: Some(palette.bright_blue.color()),
        }
    }

    fn github(palette: GithubPalette) -> Self {
        Self {
            attribute: None,
            comment: Some(palette.syntax_comment.color()),
            constant: Some(palette.syntax_constant.color()),
            constructor: Some(palette.syntax_variable.color()),
            function: Some(palette.syntax_entity.color()),
            keyword: Some(palette.syntax_storage.color()),
            label: Some(palette.syntax_variable.color()),
            module: Some(palette.syntax_constant.color()),
            number: Some(palette.syntax_constant.color()),
            operator: Some(palette.syntax_storage.color()),
            property: Some(palette.syntax_constant.color()),
            punctuation: None,
            string: Some(palette.syntax_string.color()),
            tag: Some(palette.syntax_regexp.color()),
            r#type: Some(palette.syntax_variable.color()),
            variable: Some(palette.syntax_variable.color()),
        }
    }

    pub(crate) fn tokyonight() -> Self {
        Self {
            attribute: Some(Color::Rgb(0xbb, 0x9a, 0xf7)),
            comment: Some(Color::Rgb(0x51, 0x59, 0x7d)),
            constant: Some(Color::Rgb(0xff, 0x9e, 0x64)),
            constructor: Some(Color::Rgb(0x0d, 0xb9, 0xd7)),
            function: Some(Color::Rgb(0x7a, 0xa2, 0xf7)),
            keyword: Some(Color::Rgb(0xbb, 0x9a, 0xf7)),
            label: Some(Color::Rgb(0x7a, 0xa2, 0xf7)),
            module: Some(Color::Rgb(0x0d, 0xb9, 0xd7)),
            number: Some(Color::Rgb(0xff, 0x9e, 0x64)),
            operator: Some(Color::Rgb(0x89, 0xdd, 0xff)),
            property: Some(Color::Rgb(0x7d, 0xcf, 0xff)),
            punctuation: Some(Color::Rgb(0x89, 0xdd, 0xff)),
            string: Some(Color::Rgb(0x9e, 0xce, 0x6a)),
            tag: Some(Color::Rgb(0xf7, 0x76, 0x8e)),
            r#type: Some(Color::Rgb(0x0d, 0xb9, 0xd7)),
            variable: None,
        }
    }

    pub(crate) fn base16(scheme: Base16Scheme) -> Self {
        Self {
            attribute: Some(scheme.base0c.color()),
            comment: Some(scheme.base03.color()),
            constant: Some(scheme.base09.color()),
            constructor: Some(scheme.base0a.color()),
            function: Some(scheme.base0d.color()),
            keyword: Some(scheme.base0e.color()),
            label: Some(scheme.base0d.color()),
            module: Some(scheme.base0d.color()),
            number: Some(scheme.base09.color()),
            operator: Some(scheme.base0e.color()),
            property: Some(scheme.base0c.color()),
            punctuation: Some(scheme.base04.color()),
            string: Some(scheme.base0b.color()),
            tag: Some(scheme.base08.color()),
            r#type: Some(scheme.base0a.color()),
            variable: None,
        }
    }

    pub(crate) fn color(self, class: SyntaxClass) -> Option<Color> {
        match class {
            SyntaxClass::Attribute => self.attribute,
            SyntaxClass::Comment => self.comment,
            SyntaxClass::Constant => self.constant,
            SyntaxClass::Constructor => self.constructor,
            SyntaxClass::Function => self.function,
            SyntaxClass::Keyword => self.keyword,
            SyntaxClass::Label => self.label,
            SyntaxClass::Module => self.module,
            SyntaxClass::Number => self.number,
            SyntaxClass::Operator => self.operator,
            SyntaxClass::Property => self.property,
            SyntaxClass::Punctuation => self.punctuation,
            SyntaxClass::String => self.string,
            SyntaxClass::Tag => self.tag,
            SyntaxClass::Type => self.r#type,
            SyntaxClass::Variable => self.variable,
        }
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
    const fn new(red: u8, green: u8, blue: u8) -> Self {
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
