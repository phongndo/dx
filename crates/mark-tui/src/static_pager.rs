use std::{borrow::Cow, io, thread, time::Duration};

use mark_core::MarkResult;
use mark_diff::{Changeset, DiffOptions};
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
) -> MarkResult<String> {
    let changeset = mark_diff::load_review_ref(&diff_options)?;
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
    let mut app = static_app(diff_options, changeset, pager_options, width);
    let mut output = String::new();
    for row_index in 0..app.document.model.len() {
        let Some(row) = app.document.model.row(row_index) else {
            continue;
        };
        let line = render_row(&mut app, row_index, row, width);
        push_ansi_line(&mut output, line, pager_options.color);
    }
    output
}

pub fn render_static_changeset_to_writer(
    diff_options: DiffOptions,
    changeset: Changeset,
    pager_options: StaticPagerOptions,
    writer: impl io::Write,
) -> io::Result<bool> {
    write_static_changeset(diff_options, changeset, pager_options, writer)
}

fn write_static_changeset(
    diff_options: DiffOptions,
    changeset: Changeset,
    pager_options: StaticPagerOptions,
    mut writer: impl io::Write,
) -> io::Result<bool> {
    if changeset.files.is_empty() {
        return Ok(false);
    }

    let width = pager_options.width.max(MIN_STATIC_WIDTH);
    let mut app = static_app(diff_options, changeset, pager_options, width);
    let mut wrote = false;
    for row_index in 0..app.document.model.len() {
        let Some(row) = app.document.model.row(row_index) else {
            continue;
        };
        let line = render_row(&mut app, row_index, row, width);
        write_ansi_line(&mut writer, line, pager_options.color)?;
        wrote = true;
    }
    Ok(wrote)
}

fn static_app(
    diff_options: DiffOptions,
    changeset: Changeset,
    pager_options: StaticPagerOptions,
    width: usize,
) -> DiffApp {
    let layout = resolve_static_layout(pager_options.layout, width);
    let syntax_mode = if pager_options.syntax {
        SyntaxStartupMode::Config
    } else {
        SyntaxStartupMode::Disabled
    };
    let mut app = match pager_options.layout {
        StaticPagerLayout::Auto => {
            DiffApp::new_static_with_syntax(diff_options, changeset, layout, syntax_mode)
        }
        StaticPagerLayout::Split | StaticPagerLayout::Unified => {
            DiffApp::new_static_with_explicit_layout(diff_options, changeset, layout, syntax_mode)
        }
    };
    if pager_options.layout == StaticPagerLayout::Auto {
        apply_static_auto_layout(&mut app, width);
    }
    configure_static_app(&mut app, width);
    settle_static_syntax(&mut app, pager_options.syntax_timeout);
    app
}

fn configure_static_app(app: &mut DiffApp, width: usize) {
    app.viewport.line_wrapping = false;
    app.set_viewport_width(width);
    app.set_viewport_rows(app.document.model.len().max(1));
}

fn apply_static_auto_layout(app: &mut DiffApp, width: usize) {
    app.apply_responsive_layout(static_width_for_layout(width));
}

fn resolve_static_layout(layout: StaticPagerLayout, width: usize) -> DiffLayoutMode {
    match layout {
        StaticPagerLayout::Auto => default_layout_for_width(static_width_for_layout(width)),
        StaticPagerLayout::Split => DiffLayoutMode::Split,
        StaticPagerLayout::Unified => DiffLayoutMode::Unified,
    }
}

fn static_width_for_layout(width: usize) -> u16 {
    width.min(usize::from(u16::MAX)) as u16
}

fn settle_static_syntax(app: &mut DiffApp, timeout: Duration) {
    if app.config.syntax.is_none() {
        return;
    }

    app.prepare_syntax_for_viewport(app.document.model.len().max(1));
    let start = std::time::Instant::now();
    loop {
        app.drain_syntax();
        let idle = app
            .config
            .syntax
            .as_ref()
            .is_none_or(SyntaxRuntime::is_idle);
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
        if text.as_ref().is_empty() {
            continue;
        }
        if color {
            let style = line.style.patch(span.style);
            let start = ansi_style_start(style);
            if start.is_empty() {
                output.push_str(text.as_ref());
            } else {
                output.push_str(&start);
                output.push_str(text.as_ref());
                output.push_str("\x1b[0m");
            }
        } else {
            output.push_str(text.as_ref());
        }
    }
    output.push('\n');
}

fn write_ansi_line(mut writer: impl io::Write, line: Line<'_>, color: bool) -> io::Result<()> {
    for span in line.spans {
        let text = sanitize_terminal_fragment(&span.content);
        if text.as_ref().is_empty() {
            continue;
        }
        if color {
            let style = line.style.patch(span.style);
            let start = ansi_style_start(style);
            if start.is_empty() {
                writer.write_all(text.as_ref().as_bytes())?;
            } else {
                writer.write_all(start.as_bytes())?;
                writer.write_all(text.as_ref().as_bytes())?;
                writer.write_all(b"\x1b[0m")?;
            }
        } else {
            writer.write_all(text.as_ref().as_bytes())?;
        }
    }
    writer.write_all(b"\n")
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

fn sanitize_terminal_fragment(text: &str) -> Cow<'_, str> {
    let mut chars = text.char_indices();
    while let Some((index, character)) = chars.next() {
        if character == '\t' || !character.is_control() {
            continue;
        }

        let mut output = String::with_capacity(text.len());
        output.push_str(&text[..index]);
        output.extend(character.escape_default());
        for (_, character) in chars {
            if character == '\t' {
                output.push('\t');
            } else if character.is_control() {
                output.extend(character.escape_default());
            } else {
                output.push(character);
            }
        }
        return Cow::Owned(output);
    }
    Cow::Borrowed(text)
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use mark_diff::{DiffFile, DiffHunk, DiffLine, FileChange, FileStatus, HunkLineRanges};
    use mark_syntax::{SyntaxLanguageSet, SyntaxLimits};
    use tokio::sync::mpsc;

    use super::*;
    use crate::{
        syntax::{LruCache, SyntaxRuntime, SyntaxWorkerQueue},
        theme::{MIN_SPLIT_WIDTH, SyntaxBenchmarkReport},
    };

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
    fn static_auto_layout_respects_saved_split_preference_when_narrow() {
        let mut app = DiffApp::new_with_syntax(
            DiffOptions::default(),
            fixture_changeset(),
            DiffLayoutMode::Split,
            SyntaxStartupMode::Disabled,
        );
        app.viewport.layout_override = Some(DiffLayoutMode::Split);

        apply_static_auto_layout(&mut app, usize::from(MIN_SPLIT_WIDTH - 1));

        assert_eq!(app.viewport.layout, DiffLayoutMode::Split);
        assert_eq!(app.viewport.layout_override, Some(DiffLayoutMode::Split));

        apply_static_auto_layout(&mut app, usize::from(MIN_SPLIT_WIDTH));

        assert_eq!(app.viewport.layout, DiffLayoutMode::Split);
    }

    #[test]
    fn static_pager_syntax_settle_ignores_interactive_line_wrapping() {
        let queue = SyntaxWorkerQueue::new(8, 0);
        let mut app = DiffApp::new_with_syntax(
            DiffOptions::default(),
            wrapping_static_changeset(),
            DiffLayoutMode::Unified,
            SyntaxStartupMode::Disabled,
        );
        app.viewport.line_wrapping = true;
        app.config.syntax = Some(syntax_runtime_with_rust_queue(queue.clone()));

        configure_static_app(&mut app, 20);
        settle_static_syntax(&mut app, Duration::ZERO);

        let mut files = Vec::new();
        while let Some(job) = queue.try_pop() {
            files.push(job.key.source.file);
        }
        files.sort_unstable();
        files.dedup();

        assert!(!app.viewport.line_wrapping);
        assert_eq!(files, vec![0, 1]);
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
        assert!(output.contains("bad\\u{1b}]52;c;secret\\u{7}.txt"));
        assert!(output.contains("safe\\u{1b}[2J"));
    }

    #[test]
    fn static_pager_preserves_tabs_in_diff_lines() {
        let mut changeset = fixture_changeset();
        *changeset.files[0].hunks_mut()[0].lines[0].text_mut() = "\told".to_owned();
        *changeset.files[0].hunks_mut()[0].lines[1].text_mut() = "\tnew".to_owned();

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
        *changeset.files[0].hunks_mut()[0].lines[0].text_mut() = "old\r".to_owned();
        *changeset.files[0].hunks_mut()[0].lines[1].text_mut() = "old".to_owned();

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
        *changeset.files[0].hunks_mut()[0].lines[1].text_mut() = "safe\u{009b}31m".to_owned();

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
    fn static_pager_escapes_malformed_terminal_sequences() {
        let mut changeset = fixture_changeset();
        *changeset.files[0].hunks_mut()[0].lines[1].text_mut() =
            "safe\x1b]unterminated\x1b[31".to_owned();

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

        assert!(output.contains("safe\\u{1b}]unterminated\\u{1b}[31"));
    }

    #[test]
    fn static_pager_escapes_complete_terminal_sequences() {
        let mut changeset = fixture_changeset();
        *changeset.files[0].hunks_mut()[0].lines[1].text_mut() = "\x1b[31mred\x1b[0m".to_owned();

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

        assert!(!output.contains('\x1b'));
        assert!(output.contains("\\u{1b}[31mred\\u{1b}[0m"));
    }

    #[test]
    fn static_pager_returns_empty_for_malformed_non_patch_input() {
        let output = render_static_pager(
            DiffOptions {
                source: mark_diff::DiffSource::Patch(mark_diff::PatchSource::Stdin(
                    std::sync::Arc::from(b"not a patch".as_slice()),
                )),
                local_untracked: mark_diff::UntrackedMode::Exclude,
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
            repo: std::path::PathBuf::new().into(),
            title: "test".to_owned(),
            raw_patch: Vec::new(),
            files: vec![DiffFile {
                change: FileChange::from_status(
                    FileStatus::Modified,
                    Some("file.rs".to_owned()),
                    Some("file.rs".to_owned()),
                ),
                additions: 1,
                deletions: 1,
                body: mark_diff::DiffFileBody::Text {
                    hunks: vec![DiffHunk {
                        header: "@@ -1 +1 @@".to_owned(),
                        ranges: HunkLineRanges::new(1, 1, 1, 1),
                        lines: vec![
                            DiffLine::deletion(1, "old".to_owned()),
                            DiffLine::addition(1, "new".to_owned()),
                        ],
                    }],
                },
            }],
        }
    }

    fn wrapping_static_changeset() -> Changeset {
        let mut changeset = fixture_changeset();
        *changeset.files[0].hunks_mut()[0].lines[0].text_mut() = "a".repeat(160);
        *changeset.files[0].hunks_mut()[0].lines[1].text_mut() = "b".repeat(160);
        let mut second = changeset.files[0].clone();
        second.change = FileChange::modified("second.rs");
        changeset.files.push(second);
        changeset
    }

    fn syntax_runtime_with_rust_queue(queue: SyntaxWorkerQueue) -> SyntaxRuntime {
        let (_result_tx, result_rx) = mpsc::channel(1);
        SyntaxRuntime {
            languages: SyntaxLanguageSet::from_enabled_languages(&["rust".to_owned()]),
            limits: SyntaxLimits::default(),
            result_rx,
            queue,
            cache: LruCache::new(8),
            pending: HashSet::new(),
            source_keys: HashMap::new(),
            position_keys: HashMap::new(),
            line_maps: HashMap::new(),
            skipped: HashMap::new(),
            skipped_sources: HashSet::new(),
            unavailable_full_files: HashSet::new(),
            failed: HashSet::new(),
            stats: SyntaxBenchmarkReport::default(),
            worker: None,
        }
    }

    fn unsafe_changeset() -> Changeset {
        let mut changeset = fixture_changeset();
        changeset.files[0].change = FileChange::modified("bad\x1b]52;c;secret\x07.txt");
        changeset.files[0].hunks_mut()[0].header = "@@ -1 +1 @@\x1b[2J".to_owned();
        *changeset.files[0].hunks_mut()[0].lines[1].text_mut() = "safe\x1b[2J".to_owned();
        changeset
    }
}
