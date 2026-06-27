mod commands;
mod options;
mod output;
mod table;

pub(crate) use commands::syntax;
pub(crate) use options::{
    diff_options, difftool_options, patch_options, review_options, show_options,
};
pub(crate) use output::{
    print_syntax_add_result, print_syntax_remove_result, print_syntax_statuses,
    print_syntax_update_result,
};
#[cfg(test)]
pub(crate) use table::{display_width, list_glyphs, render_syntax_statuses};

#[cfg(test)]
mod tests {
    use super::*;

    fn status(
        language: &str,
        enabled: bool,
        installed: bool,
        trusted: bool,
        has_highlights: bool,
    ) -> mark_command::SyntaxLanguageStatus {
        mark_command::SyntaxLanguageStatus {
            language: language.to_owned(),
            enabled,
            installed,
            trusted,
            has_highlights,
            version: installed.then(|| "1.9.0-rc.18".to_owned()),
            artifact: None,
            source: installed.then(|| "bundled".to_owned()),
        }
    }

    #[test]
    fn syntax_status_output_uses_compact_table() {
        let output = render_syntax_statuses(
            &[
                status("rust", true, true, true, true),
                status("typescript", true, true, true, false),
                status("elixir", false, true, true, true),
            ],
            false,
            list_glyphs(false),
            None,
        );

        assert!(output.contains("language"));
        assert!(output.contains("status"));
        assert!(output.contains("version"));
        let headers = output
            .lines()
            .next()
            .expect("header should render")
            .split_whitespace()
            .collect::<Vec<_>>();
        assert_eq!(headers, ["language", "status", "source", "version"]);
        assert!(output.contains("rust"));
        assert!(output.contains("ok"));
        assert!(output.contains("typescript"));
        assert!(output.contains("!"));
        assert!(output.contains("elixir"));
        assert!(output.contains("-"));
        assert!(!output.contains("enabled"));
        assert!(!output.contains("syntax"));
        assert!(!output.contains("trusted"));
    }

    #[test]
    fn syntax_status_output_centers_unicode_status() {
        let output = render_syntax_statuses(
            &[status("rust", true, true, true, true)],
            false,
            list_glyphs(true),
            None,
        );

        let rust_line = output
            .lines()
            .find(|line| line.starts_with("rust"))
            .expect("rust status row should render");

        assert!(rust_line.contains("  ✓   "));
    }

    #[test]
    fn syntax_status_output_truncates_to_terminal_width() {
        let output = render_syntax_statuses(
            &[status("very-long-language-name", true, true, true, true)],
            false,
            list_glyphs(false),
            Some(31),
        );

        for line in output.lines() {
            assert!(display_width(line) <= 31, "line too wide: {line}");
        }
    }
}
