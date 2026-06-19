use std::{thread, time::Duration};

use dx_core::DxResult;
use dx_diff::{Changeset, DiffOptions};
use ratatui::prelude::{Color, Line, Modifier, Style};

use crate::{
    app::{DiffApp, SyntaxStartupMode},
    controls::{DiffLayoutMode, default_layout_for_width},
    render::diff::render_row,
    syntax::SyntaxRuntime,
};

const DEFAULT_STATIC_WIDTH: usize = 120;
const MIN_STATIC_WIDTH: usize = 20;
const STATIC_SYNTAX_SETTLE_TIMEOUT: Duration = Duration::from_millis(1500);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaticPagerLayout {
    Auto,
    Split,
    Unified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StaticPagerOptions {
    pub width: usize,
    pub layout: StaticPagerLayout,
    pub color: bool,
    pub syntax: bool,
    pub syntax_timeout: Duration,
}

impl Default for StaticPagerOptions {
    fn default() -> Self {
        Self {
            width: DEFAULT_STATIC_WIDTH,
            layout: StaticPagerLayout::Auto,
            color: true,
            syntax: true,
            syntax_timeout: STATIC_SYNTAX_SETTLE_TIMEOUT,
        }
    }
}

pub fn render_static_pager(
    diff_options: DiffOptions,
    pager_options: StaticPagerOptions,
) -> DxResult<String> {
    let changeset = dx_diff::load_review_ref(&diff_options)?;
    Ok(render_static_changeset(
        diff_options,
        changeset,
        pager_options,
    ))
}

pub fn render_static_changeset(
    diff_options: DiffOptions,
    changeset: Changeset,
    pager_options: StaticPagerOptions,
) -> String {
    if changeset.files.is_empty() {
        return String::new();
    }

    let width = pager_options.width.max(MIN_STATIC_WIDTH);
    let layout = resolve_static_layout(pager_options.layout, width);
    let syntax_mode = if pager_options.syntax {
        SyntaxStartupMode::Config
    } else {
        SyntaxStartupMode::Disabled
    };
    let mut app = DiffApp::new_with_syntax(diff_options, changeset, layout, syntax_mode);
    app.set_viewport_width(width);
    app.set_viewport_rows(app.model.len().max(1));
    settle_static_syntax(&mut app, pager_options.syntax_timeout);

    let mut output = String::new();
    for row_index in 0..app.model.len() {
        let Some(row) = app.model.row(row_index) else {
            continue;
        };
        let line = render_row(&mut app, row_index, row, width);
        push_ansi_line(&mut output, line, pager_options.color);
    }
    output
}

fn resolve_static_layout(layout: StaticPagerLayout, width: usize) -> DiffLayoutMode {
    match layout {
        StaticPagerLayout::Auto => {
            default_layout_for_width(width.min(usize::from(u16::MAX)) as u16)
        }
        StaticPagerLayout::Split => DiffLayoutMode::Split,
        StaticPagerLayout::Unified => DiffLayoutMode::Unified,
    }
}

fn settle_static_syntax(app: &mut DiffApp, timeout: Duration) {
    if app.syntax.is_none() {
        return;
    }

    app.prepare_syntax_for_viewport(app.model.len().max(1));
    let start = std::time::Instant::now();
    loop {
        app.drain_syntax();
        let idle = app.syntax.as_ref().is_none_or(SyntaxRuntime::is_idle);
        if idle || start.elapsed() >= timeout {
            break;
        }
        thread::sleep(Duration::from_millis(1));
    }
    app.drain_syntax();
}

fn push_ansi_line(output: &mut String, line: Line<'_>, color: bool) {
    for span in line.spans {
        let text = sanitize_terminal_fragment(&span.content);
        if text.is_empty() {
            continue;
        }
        if color {
            let style = line.style.patch(span.style);
            let start = ansi_style_start(style);
            if start.is_empty() {
                output.push_str(&text);
            } else {
                output.push_str(&start);
                output.push_str(&text);
                output.push_str("\x1b[0m");
            }
        } else {
            output.push_str(&text);
        }
    }
    output.push('\n');
}

fn ansi_style_start(style: Style) -> String {
    let mut codes = Vec::new();
    if style.add_modifier.contains(Modifier::BOLD) {
        codes.push("1".to_owned());
    }
    if style.add_modifier.contains(Modifier::DIM) {
        codes.push("2".to_owned());
    }
    if style.add_modifier.contains(Modifier::ITALIC) {
        codes.push("3".to_owned());
    }
    if style.add_modifier.contains(Modifier::UNDERLINED) {
        codes.push("4".to_owned());
    }
    if style.add_modifier.contains(Modifier::REVERSED) {
        codes.push("7".to_owned());
    }
    if style.add_modifier.contains(Modifier::CROSSED_OUT) {
        codes.push("9".to_owned());
    }
    if let Some(color) = style.fg.and_then(|color| ansi_color_code("38", color)) {
        codes.push(color);
    }
    if let Some(color) = style.bg.and_then(|color| ansi_color_code("48", color)) {
        codes.push(color);
    }

    if codes.is_empty() {
        String::new()
    } else {
        format!("\x1b[{}m", codes.join(";"))
    }
}

fn ansi_color_code(prefix: &str, color: Color) -> Option<String> {
    match color {
        Color::Reset => None,
        Color::Black => Some(basic_color(prefix, 0)),
        Color::Red => Some(basic_color(prefix, 1)),
        Color::Green => Some(basic_color(prefix, 2)),
        Color::Yellow => Some(basic_color(prefix, 3)),
        Color::Blue => Some(basic_color(prefix, 4)),
        Color::Magenta => Some(basic_color(prefix, 5)),
        Color::Cyan => Some(basic_color(prefix, 6)),
        Color::Gray => Some(basic_color(prefix, 7)),
        Color::DarkGray => Some(indexed_color(prefix, 8)),
        Color::LightRed => Some(indexed_color(prefix, 9)),
        Color::LightGreen => Some(indexed_color(prefix, 10)),
        Color::LightYellow => Some(indexed_color(prefix, 11)),
        Color::LightBlue => Some(indexed_color(prefix, 12)),
        Color::LightMagenta => Some(indexed_color(prefix, 13)),
        Color::LightCyan => Some(indexed_color(prefix, 14)),
        Color::White => Some(indexed_color(prefix, 15)),
        Color::Indexed(index) => Some(indexed_color(prefix, index)),
        Color::Rgb(red, green, blue) => Some(format!("{prefix};2;{red};{green};{blue}")),
    }
}

fn basic_color(prefix: &str, offset: u8) -> String {
    if prefix == "38" {
        (30 + offset).to_string()
    } else {
        (40 + offset).to_string()
    }
}

fn indexed_color(prefix: &str, index: u8) -> String {
    format!("{prefix};5;{index}")
}

fn sanitize_terminal_fragment(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut output = String::with_capacity(text.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == 0x1b {
            skip_escape(bytes, &mut index);
            continue;
        }

        let Some(character) = text[index..].chars().next() else {
            break;
        };
        if character == '\t' {
            output.push('\t');
        } else if character.is_control() {
            output.extend(character.escape_default());
        } else {
            output.push(character);
        }
        index += character.len_utf8();
    }
    output
}

fn skip_escape(input: &[u8], index: &mut usize) {
    *index += 1;
    let Some(introducer) = input.get(*index).copied() else {
        return;
    };
    *index += 1;

    match introducer {
        b'[' => skip_csi(input, index),
        b']' | b'P' | b'^' | b'_' | b'X' => skip_string_escape(input, index),
        0x20..=0x2f if *index < input.len() => *index += 1,
        _ => {}
    }
}

fn skip_csi(input: &[u8], index: &mut usize) {
    while let Some(byte) = input.get(*index).copied() {
        *index += 1;
        if (0x40..=0x7e).contains(&byte) {
            break;
        }
    }
}

fn skip_string_escape(input: &[u8], index: &mut usize) {
    while let Some(byte) = input.get(*index).copied() {
        *index += 1;
        if byte == 0x07 {
            break;
        }
        if byte == 0x1b && input.get(*index) == Some(&b'\\') {
            *index += 1;
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use dx_diff::{DiffFile, DiffHunk, DiffLine, DiffLineKind, FileStatus};

    use super::*;

    #[test]
    fn static_pager_renders_unified_diff_with_line_numbers() {
        let output = render_static_changeset(
            DiffOptions::default(),
            fixture_changeset(),
            StaticPagerOptions {
                width: 80,
                layout: StaticPagerLayout::Unified,
                color: false,
                syntax: false,
                ..StaticPagerOptions::default()
            },
        );

        assert!(output.contains("file.rs"));
        assert!(output.contains("@@ -1 +1 @@"));
        assert!(output.contains("1       -old"));
        assert!(output.contains("1 +new"));
    }

    #[test]
    fn static_pager_supports_split_and_unified_layouts() {
        let unified = render_static_changeset(
            DiffOptions::default(),
            fixture_changeset(),
            StaticPagerOptions {
                width: 80,
                layout: StaticPagerLayout::Unified,
                color: false,
                syntax: false,
                ..StaticPagerOptions::default()
            },
        );
        let split = render_static_changeset(
            DiffOptions::default(),
            fixture_changeset(),
            StaticPagerOptions {
                width: 80,
                layout: StaticPagerLayout::Split,
                color: false,
                syntax: false,
                ..StaticPagerOptions::default()
            },
        );

        assert_ne!(unified, split);
        assert!(split.contains("old"));
        assert!(split.contains("new"));
    }

    #[test]
    fn static_pager_serializes_theme_styles_as_ansi() {
        let output = render_static_changeset(
            DiffOptions::default(),
            fixture_changeset(),
            StaticPagerOptions {
                width: 80,
                layout: StaticPagerLayout::Unified,
                color: true,
                syntax: false,
                ..StaticPagerOptions::default()
            },
        );

        assert!(output.contains("\x1b["));
        assert!(output.contains("file.rs"));
    }

    #[test]
    fn static_pager_sanitizes_terminal_controls() {
        let output = render_static_changeset(
            DiffOptions::default(),
            unsafe_changeset(),
            StaticPagerOptions {
                width: 80,
                layout: StaticPagerLayout::Unified,
                color: false,
                syntax: false,
                ..StaticPagerOptions::default()
            },
        );

        assert!(!output.contains('\x1b'));
        assert!(!output.contains('\x07'));
        assert!(output.contains("bad.txt"));
        assert!(output.contains("safe"));
    }

    #[test]
    fn static_pager_preserves_tabs_in_diff_lines() {
        let mut changeset = fixture_changeset();
        changeset.files[0].hunks[0].lines[0].text = "\told".to_owned();
        changeset.files[0].hunks[0].lines[1].text = "\tnew".to_owned();

        let output = render_static_changeset(
            DiffOptions::default(),
            changeset,
            StaticPagerOptions {
                width: 80,
                layout: StaticPagerLayout::Unified,
                color: false,
                syntax: false,
                ..StaticPagerOptions::default()
            },
        );

        assert!(output.contains("\told"));
        assert!(output.contains("\tnew"));
    }

    #[test]
    fn static_pager_escapes_cr_payloads_in_diff_lines() {
        let mut changeset = fixture_changeset();
        changeset.files[0].hunks[0].lines[0].text = "old\r".to_owned();
        changeset.files[0].hunks[0].lines[1].text = "old".to_owned();

        let output = render_static_changeset(
            DiffOptions::default(),
            changeset,
            StaticPagerOptions {
                width: 80,
                layout: StaticPagerLayout::Unified,
                color: false,
                syntax: false,
                ..StaticPagerOptions::default()
            },
        );

        assert!(output.contains("old\\r"));
        assert!(output.contains("+old"));
    }

    #[test]
    fn static_pager_escapes_utf8_c1_controls() {
        let mut changeset = fixture_changeset();
        changeset.files[0].hunks[0].lines[1].text = "safe\u{009b}31m".to_owned();

        let output = render_static_changeset(
            DiffOptions::default(),
            changeset,
            StaticPagerOptions {
                width: 80,
                layout: StaticPagerLayout::Unified,
                color: false,
                syntax: false,
                ..StaticPagerOptions::default()
            },
        );

        assert!(!output.contains('\u{009b}'));
        assert!(output.contains("safe\\u{9b}31m"));
    }

    #[test]
    fn static_pager_returns_empty_for_malformed_non_patch_input() {
        let output = render_static_pager(
            DiffOptions {
                source: dx_diff::DiffSource::Patch(dx_diff::PatchSource::Stdin(
                    std::sync::Arc::from(b"not a patch".as_slice()),
                )),
                include_untracked: false,
                ..DiffOptions::default()
            },
            StaticPagerOptions {
                color: false,
                syntax: false,
                ..StaticPagerOptions::default()
            },
        )
        .unwrap();

        assert!(output.is_empty());
    }

    fn fixture_changeset() -> Changeset {
        Changeset {
            repo: std::path::PathBuf::new(),
            title: "test".to_owned(),
            raw_patch: Vec::new(),
            files: vec![DiffFile {
                old_path: Some("file.rs".to_owned()),
                new_path: Some("file.rs".to_owned()),
                status: FileStatus::Modified,
                hunks: vec![DiffHunk {
                    header: "@@ -1 +1 @@".to_owned(),
                    old_start: 1,
                    old_count: 1,
                    new_start: 1,
                    new_count: 1,
                    lines: vec![
                        DiffLine {
                            kind: DiffLineKind::Deletion,
                            old_line: Some(1),
                            new_line: None,
                            text: "old".to_owned(),
                        },
                        DiffLine {
                            kind: DiffLineKind::Addition,
                            old_line: None,
                            new_line: Some(1),
                            text: "new".to_owned(),
                        },
                    ],
                }],
                additions: 1,
                deletions: 1,
                is_binary: false,
            }],
        }
    }

    fn unsafe_changeset() -> Changeset {
        let mut changeset = fixture_changeset();
        changeset.files[0].new_path = Some("bad\x1b]52;c;secret\x07.txt".to_owned());
        changeset.files[0].hunks[0].header = "@@ -1 +1 @@\x1b[2J".to_owned();
        changeset.files[0].hunks[0].lines[1].text = "safe\x1b[2J".to_owned();
        changeset
    }
}
