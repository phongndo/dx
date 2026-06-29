use std::{
    borrow::Cow,
    fs,
    io::{self, BufRead, BufReader},
};

use mark_core::{MarkError, MarkResult};

use crate::{
    DiffOptions, DiffSource, DiffStats, PatchSource, diff_patch_bytes,
    difftool::patch_line_parts,
    git_args::{git_diff_numstat_args, should_include_untracked, validate_options},
    git_io::{git_numstat_stats, git_numstat_stats_with_untracked},
    parser::{
        diff_git_paths, git_metadata_path, parse_hunk_header, strip_prefix_path,
        unified_header_path,
    },
};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct PatchStats {
    pub(super) files: Vec<PatchFileStat>,
    pub(super) totals: DiffStats,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct PatchFileStat {
    pub(super) old_path: Option<String>,
    pub(super) new_path: Option<String>,
    pub(super) additions: usize,
    pub(super) deletions: usize,
    pub(super) is_binary: bool,
}

impl PatchFileStat {
    pub(super) fn display_path(&self) -> &str {
        self.new_path
            .as_deref()
            .or(self.old_path.as_deref())
            .unwrap_or("/dev/null")
    }

    #[cfg(test)]
    pub(super) fn is_binary(&self) -> bool {
        self.is_binary
    }
}

pub(super) fn render_patch_stats(stats: &PatchStats) -> String {
    let mut output = String::new();
    for file in &stats.files {
        output.push_str(&format!(
            "{:>6} {:>6} {}\n",
            file.additions,
            file.deletions,
            terminal_safe_text(file.display_path())
        ));
    }
    output.push_str(&format!(
        "\n{} files changed, {} insertions(+), {} deletions(-)",
        stats.totals.files, stats.totals.additions, stats.totals.deletions
    ));
    if stats.totals.binary_files > 0 {
        output.push_str(&format!(", {} binary", stats.totals.binary_files));
    }
    output.push('\n');
    output
}

pub(super) fn terminal_safe_text(text: &str) -> Cow<'_, str> {
    if !text.chars().any(char::is_control) {
        return Cow::Borrowed(text);
    }

    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        if character.is_control() {
            escaped.extend(character.escape_default());
        } else {
            escaped.push(character);
        }
    }
    Cow::Owned(escaped)
}

pub(super) fn patch_stats(options: &DiffOptions) -> MarkResult<PatchStats> {
    if let DiffSource::Patch(source) = &options.source {
        validate_options(options)?;
        return patch_source_stats(source);
    }

    if matches!(options.source, DiffSource::Difftool { .. }) {
        let (_, patch) = diff_patch_bytes(options)?;
        return Ok(parse_patch_stats_lossy(patch.as_ref()));
    }

    let repo = mark_git::repository_root(options.repo.as_deref())?;
    validate_options(options)?;
    let args = git_diff_numstat_args(options, &repo)?;
    if should_include_untracked(options) {
        git_numstat_stats_with_untracked(&repo, &args)
    } else {
        git_numstat_stats(&repo, &args)
    }
}

fn patch_source_stats(source: &PatchSource) -> MarkResult<PatchStats> {
    match source {
        PatchSource::File(path) => {
            let file = fs::File::open(path)?;
            parse_patch_stats(BufReader::new(file)).map_err(MarkError::Io)
        }
        PatchSource::Stdin(patch) => {
            parse_patch_stats(BufReader::new(patch.as_ref())).map_err(MarkError::Io)
        }
        PatchSource::Text { patch, .. } | PatchSource::Review { patch, .. } => {
            parse_patch_stats(BufReader::new(patch.as_ref())).map_err(MarkError::Io)
        }
    }
}

pub(super) fn parse_patch_stats(mut reader: impl BufRead) -> io::Result<PatchStats> {
    let mut parser = PatchStatsParser::default();
    let mut line = Vec::new();

    loop {
        line.clear();
        if reader.read_until(b'\n', &mut line)? == 0 {
            break;
        }
        let (line, _) = patch_line_parts(&line);
        parser.push_line_bytes(line);
    }

    Ok(parser.finish())
}

pub(super) fn parse_patch_stats_lossy(patch: &[u8]) -> PatchStats {
    let mut parser = PatchStatsParser::default();

    for line in patch.split_inclusive(|byte| *byte == b'\n') {
        let (line, _) = patch_line_parts(line);
        parser.push_line_bytes(line);
    }

    parser.finish()
}

#[derive(Debug, Default)]
struct PatchStatsParser {
    stats: PatchStats,
    current: Option<PatchFileStatBuilder>,
    current_hunk: Option<PatchHunkStat>,
}

impl PatchStatsParser {
    fn push_line_bytes(&mut self, line: &[u8]) {
        if let Some(hunk) = self.current_hunk.as_mut() {
            hunk.push_line(line, self.current.as_mut());
            if hunk.is_complete() {
                self.current_hunk = None;
            }
            return;
        }

        let line = String::from_utf8_lossy(line);
        self.push_header_line(&line);
    }

    fn push_header_line(&mut self, line: &str) {
        if line.starts_with("diff --git ") {
            finish_patch_stat_file(&mut self.stats, &mut self.current);
            self.current = Some(PatchFileStatBuilder::from_diff_git(line));
            return;
        }

        if line.starts_with("--- ") {
            if self
                .current
                .as_ref()
                .is_some_and(PatchFileStatBuilder::has_seen_hunk)
            {
                finish_patch_stat_file(&mut self.stats, &mut self.current);
            }
            let file = self
                .current
                .get_or_insert_with(PatchFileStatBuilder::default);
            file.apply_header(line);
            return;
        }

        if let Some(file) = self.current.as_mut() {
            if line.starts_with("@@ ") {
                file.seen_hunk = true;
                self.current_hunk = Some(PatchHunkStat::from_header(line));
                return;
            }

            file.apply_header(line);
        }
    }

    fn finish(mut self) -> PatchStats {
        finish_patch_stat_file(&mut self.stats, &mut self.current);
        self.stats
    }
}

fn finish_patch_stat_file(stats: &mut PatchStats, file: &mut Option<PatchFileStatBuilder>) {
    let Some(file) = file.take() else {
        return;
    };
    let file = file.finish();
    stats.totals.files += 1;
    stats.totals.additions += file.additions;
    stats.totals.deletions += file.deletions;
    if file.is_binary {
        stats.totals.binary_files += 1;
    }
    stats.files.push(file);
}

#[derive(Debug, Default)]
struct PatchFileStatBuilder {
    old_path: Option<String>,
    new_path: Option<String>,
    additions: usize,
    deletions: usize,
    is_binary: bool,
    seen_hunk: bool,
}

impl PatchFileStatBuilder {
    fn from_diff_git(line: &str) -> Self {
        let (old_path, new_path) = diff_git_paths(line);
        Self {
            old_path,
            new_path,
            ..Self::default()
        }
    }

    fn has_seen_hunk(&self) -> bool {
        self.seen_hunk
    }

    fn apply_header(&mut self, line: &str) {
        if line.starts_with("Binary files ") || line == "GIT binary patch" {
            self.is_binary = true;
        } else if let Some(path) = line.strip_prefix("--- ") {
            let path = unified_header_path(path);
            if path.as_ref() != "/dev/null" {
                self.old_path = strip_prefix_path(path.as_ref(), "a/");
            } else {
                self.old_path = None;
            }
        } else if let Some(path) = line.strip_prefix("+++ ") {
            let path = unified_header_path(path);
            if path.as_ref() != "/dev/null" {
                self.new_path = strip_prefix_path(path.as_ref(), "b/");
            } else {
                self.new_path = None;
            }
        } else if let Some(path) = line.strip_prefix("rename from ") {
            self.old_path = Some(git_metadata_path(path));
        } else if let Some(path) = line.strip_prefix("rename to ") {
            self.new_path = Some(git_metadata_path(path));
        } else if let Some(path) = line.strip_prefix("copy from ") {
            self.old_path = Some(git_metadata_path(path));
        } else if let Some(path) = line.strip_prefix("copy to ") {
            self.new_path = Some(git_metadata_path(path));
        }
    }

    fn finish(self) -> PatchFileStat {
        PatchFileStat {
            old_path: self.old_path,
            new_path: self.new_path,
            additions: self.additions,
            deletions: self.deletions,
            is_binary: self.is_binary,
        }
    }
}

#[derive(Debug)]
struct PatchHunkStat {
    old_remaining: usize,
    new_remaining: usize,
}

impl PatchHunkStat {
    fn from_header(header: &str) -> Self {
        let (_, old_count, _, new_count) = parse_hunk_header(header);
        Self {
            old_remaining: old_count,
            new_remaining: new_count,
        }
    }

    fn push_line(&mut self, raw: &[u8], file: Option<&mut PatchFileStatBuilder>) {
        match raw.first().copied() {
            Some(b'+') => {
                self.new_remaining = self.new_remaining.saturating_sub(1);
                if let Some(file) = file {
                    file.additions += 1;
                }
            }
            Some(b'-') => {
                self.old_remaining = self.old_remaining.saturating_sub(1);
                if let Some(file) = file {
                    file.deletions += 1;
                }
            }
            Some(b'\\') => {}
            _ => {
                self.old_remaining = self.old_remaining.saturating_sub(1);
                self.new_remaining = self.new_remaining.saturating_sub(1);
            }
        }
    }

    fn is_complete(&self) -> bool {
        self.old_remaining == 0 && self.new_remaining == 0
    }
}
