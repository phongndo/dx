use mark_syntax::{DiffSettings, SyntaxClass};
use ratatui::prelude::Color;

use super::{Base16Scheme, DiffTheme, RgbColor};

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

impl DiffTheme {
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
            cursor: palette.text.color(),
            cursor_line_bg: palette.base.blend(palette.text, 0.10).color(),
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
}
