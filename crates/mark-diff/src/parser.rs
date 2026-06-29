use std::borrow::Cow;

use crate::{DiffFile, DiffFileBody, DiffHunk, DiffLine, FileChange, FileStatus, HunkLineRanges};

pub fn parse_patch(patch: &str) -> Vec<DiffFile> {
    let mut files = Vec::new();
    let mut current: Option<DiffFileBuilder> = None;
    let mut current_hunk: Option<DiffHunkBuilder> = None;
    let mut lines = patch_lines(patch).peekable();

    while let Some(line) = lines.next() {
        let header_line = patch_header_line(line);
        if header_line.starts_with("diff --git ") {
            finish_hunk(&mut current, &mut current_hunk);
            finish_file(&mut files, &mut current);
            current = Some(DiffFileBuilder::from_diff_git(header_line));
            continue;
        }

        if header_line.starts_with("--- ")
            && (current.is_none()
                || current_hunk
                    .as_ref()
                    .is_some_and(DiffHunkBuilder::is_complete))
            && let Some(new_header) = lines
                .peek()
                .copied()
                .map(patch_header_line)
                .filter(|line| line.starts_with("+++ "))
        {
            finish_hunk(&mut current, &mut current_hunk);
            finish_file(&mut files, &mut current);
            let new_header = new_header.to_owned();
            let _ = lines.next();
            current = Some(DiffFileBuilder::from_unified_headers(
                header_line,
                &new_header,
            ));
            continue;
        }

        let Some(file) = current.as_mut() else {
            continue;
        };

        if header_line.starts_with("@@ ") {
            finish_hunk(&mut current, &mut current_hunk);
            current_hunk = Some(DiffHunkBuilder::from_header(header_line));
            continue;
        }

        if let Some(hunk) = current_hunk.as_mut() {
            hunk.push_line(line);
            continue;
        }

        file.apply_header(header_line);
    }

    finish_hunk(&mut current, &mut current_hunk);
    finish_file(&mut files, &mut current);
    files
}

fn patch_lines(patch: &str) -> impl Iterator<Item = &str> {
    patch
        .split_inclusive('\n')
        .map(|line| line.strip_suffix('\n').unwrap_or(line))
}

fn patch_header_line(line: &str) -> &str {
    line.strip_suffix('\r').unwrap_or(line)
}

fn is_diff_no_newline_marker(raw: &str) -> bool {
    raw.starts_with("\\ No newline at end of file")
}

fn finish_hunk(file: &mut Option<DiffFileBuilder>, hunk: &mut Option<DiffHunkBuilder>) {
    if let (Some(file), Some(hunk)) = (file.as_mut(), hunk.take()) {
        file.additions += hunk.additions;
        file.deletions += hunk.deletions;
        file.hunks.push(hunk.finish());
    }
}

fn finish_file(files: &mut Vec<DiffFile>, file: &mut Option<DiffFileBuilder>) {
    if let Some(file) = file.take() {
        files.push(file.finish());
    }
}

#[derive(Debug)]
struct DiffFileBuilder {
    old_path: Option<String>,
    new_path: Option<String>,
    status: FileStatus,
    hunks: Vec<DiffHunk>,
    additions: usize,
    deletions: usize,
    body: DiffFileBody,
}

impl DiffFileBuilder {
    fn from_diff_git(line: &str) -> Self {
        let (old_path, new_path) = diff_git_paths(line);

        Self {
            old_path,
            new_path,
            status: FileStatus::Modified,
            hunks: Vec::new(),
            additions: 0,
            deletions: 0,
            body: DiffFileBody::default(),
        }
    }

    fn from_unified_headers(old_header: &str, new_header: &str) -> Self {
        let mut builder = Self {
            old_path: None,
            new_path: None,
            status: FileStatus::Modified,
            hunks: Vec::new(),
            additions: 0,
            deletions: 0,
            body: DiffFileBody::default(),
        };
        builder.apply_header(old_header);
        builder.apply_header(new_header);
        builder
    }

    fn apply_header(&mut self, line: &str) {
        if line.starts_with("new file mode ") {
            self.status = FileStatus::Added;
        } else if line.starts_with("deleted file mode ") {
            self.status = FileStatus::Deleted;
        } else if line.starts_with("rename from ") {
            self.status = FileStatus::Renamed;
            self.old_path = Some(git_metadata_path(line.trim_start_matches("rename from ")));
        } else if line.starts_with("rename to ") {
            self.status = FileStatus::Renamed;
            self.new_path = Some(git_metadata_path(line.trim_start_matches("rename to ")));
        } else if line.starts_with("copy from ") {
            self.status = FileStatus::Copied;
            self.old_path = Some(git_metadata_path(line.trim_start_matches("copy from ")));
        } else if line.starts_with("copy to ") {
            self.status = FileStatus::Copied;
            self.new_path = Some(git_metadata_path(line.trim_start_matches("copy to ")));
        } else if line.starts_with("old mode ") || line.starts_with("new mode ") {
            if !matches!(self.status, FileStatus::Renamed | FileStatus::Copied) {
                self.status = FileStatus::TypeChanged;
            }
        } else if line.starts_with("Binary files ") || line == "GIT binary patch" {
            self.body = DiffFileBody::Binary;
        } else if let Some(path) = line.strip_prefix("--- ") {
            let path = unified_header_path(path);
            if path.as_ref() != "/dev/null" {
                self.old_path = strip_prefix_path(path.as_ref(), "a/");
            } else {
                self.status = FileStatus::Added;
                self.old_path = None;
            }
        } else if let Some(path) = line.strip_prefix("+++ ") {
            let path = unified_header_path(path);
            if path.as_ref() != "/dev/null" {
                self.new_path = strip_prefix_path(path.as_ref(), "b/");
            } else {
                self.status = FileStatus::Deleted;
                self.new_path = None;
            }
        }
    }

    fn finish(self) -> DiffFile {
        let body = if matches!(self.body, DiffFileBody::Binary) {
            DiffFileBody::Binary
        } else if self.hunks.is_empty() {
            DiffFileBody::NoTextualChanges
        } else {
            DiffFileBody::Text { hunks: self.hunks }
        };
        DiffFile {
            change: FileChange::from_status(self.status, self.old_path, self.new_path),
            additions: self.additions,
            deletions: self.deletions,
            body,
        }
    }
}

pub(super) fn diff_git_paths(line: &str) -> (Option<String>, Option<String>) {
    let Some(paths) = line.strip_prefix("diff --git ") else {
        return (None, None);
    };

    if paths.starts_with('"')
        && let Some((old, rest)) = parse_quoted_git_path_token(paths)
        && let Some((new, trailing)) = parse_quoted_git_path_token(rest.trim_start())
        && trailing.trim().is_empty()
    {
        return (strip_prefix_path(&old, "a/"), strip_prefix_path(&new, "b/"));
    }

    split_diff_git_paths(paths)
        .map(|(old, new)| (strip_prefix_path(old, "a/"), strip_prefix_path(new, "b/")))
        .unwrap_or((None, None))
}

fn split_diff_git_paths(paths: &str) -> Option<(&str, &str)> {
    let mut fallback = None;
    for (separator, _) in paths.match_indices(" b/") {
        let old = &paths[..separator];
        let new = &paths[separator + 1..];
        if !old.starts_with("a/") || !new.starts_with("b/") {
            continue;
        }

        let old_path = old.strip_prefix("a/").unwrap_or(old);
        let new_path = new.strip_prefix("b/").unwrap_or(new);
        if old_path == new_path {
            return Some((old, new));
        }

        fallback = Some((old, new));
    }

    fallback
}

pub(super) fn strip_prefix_path(path: &str, prefix: &str) -> Option<String> {
    Some(path.strip_prefix(prefix).unwrap_or(path).to_owned())
}

pub(super) fn unified_header_path(path: &str) -> Cow<'_, str> {
    if path.starts_with('"')
        && let Some((path, _)) = parse_quoted_git_path_token(path)
    {
        return Cow::Owned(path);
    }

    Cow::Borrowed(path.split_once('\t').map_or(path, |(path, _)| path))
}

pub(super) fn git_metadata_path(path: &str) -> String {
    if path.starts_with('"')
        && let Some((path, trailing)) = parse_quoted_git_path_token(path)
        && trailing.trim().is_empty()
    {
        return path;
    }

    path.to_owned()
}

fn parse_quoted_git_path_token(input: &str) -> Option<(String, &str)> {
    let input = input.strip_prefix('"')?;
    let mut output = Vec::new();
    let mut index = 0;
    let bytes = input.as_bytes();
    while let Some(byte) = bytes.get(index).copied() {
        match byte {
            b'"' => {
                return Some((
                    String::from_utf8_lossy(&output).into_owned(),
                    &input[index + 1..],
                ));
            }
            b'\\' => {
                index += 1;
                parse_git_path_escape(input, &mut index, &mut output)?;
            }
            byte => {
                output.push(byte);
                index += 1;
            }
        }
    }
    None
}

fn parse_git_path_escape(input: &str, index: &mut usize, output: &mut Vec<u8>) -> Option<()> {
    let bytes = input.as_bytes();
    let escaped = *bytes.get(*index)?;
    match escaped {
        b'a' => push_escaped_byte(index, output, b'\x07'),
        b'b' => push_escaped_byte(index, output, b'\x08'),
        b'f' => push_escaped_byte(index, output, b'\x0c'),
        b'n' => push_escaped_byte(index, output, b'\n'),
        b'r' => push_escaped_byte(index, output, b'\r'),
        b't' => push_escaped_byte(index, output, b'\t'),
        b'v' => push_escaped_byte(index, output, b'\x0b'),
        b'\\' => push_escaped_byte(index, output, b'\\'),
        b'"' => push_escaped_byte(index, output, b'"'),
        b'0'..=b'7' => push_octal_escape(bytes, index, output),
        byte if byte.is_ascii() => push_escaped_byte(index, output, byte),
        _ => {
            let character = input[*index..].chars().next()?;
            let mut buffer = [0; 4];
            output.extend_from_slice(character.encode_utf8(&mut buffer).as_bytes());
            *index += character.len_utf8();
        }
    }
    Some(())
}

fn push_escaped_byte(index: &mut usize, output: &mut Vec<u8>, byte: u8) {
    output.push(byte);
    *index += 1;
}

fn push_octal_escape(bytes: &[u8], index: &mut usize, output: &mut Vec<u8>) {
    let mut value = 0u32;
    for _ in 0..3 {
        let Some(byte) = bytes.get(*index).copied() else {
            break;
        };
        if !(b'0'..=b'7').contains(&byte) {
            break;
        }
        value = value * 8 + u32::from(byte - b'0');
        *index += 1;
    }
    if let Ok(byte) = u8::try_from(value) {
        output.push(byte);
    } else {
        output.extend_from_slice("\u{FFFD}".as_bytes());
    }
}

#[derive(Debug)]
struct DiffHunkBuilder {
    header: String,
    ranges: HunkLineRanges,
    old_line: usize,
    new_line: usize,
    additions: usize,
    deletions: usize,
    lines: Vec<DiffLine>,
}

impl DiffHunkBuilder {
    fn from_header(header: &str) -> Self {
        let (old_start, old_count, new_start, new_count) = parse_hunk_header(header);
        Self {
            header: header.to_owned(),
            ranges: HunkLineRanges::new(old_start, old_count, new_start, new_count),
            old_line: old_start,
            new_line: new_start,
            additions: 0,
            deletions: 0,
            lines: Vec::with_capacity(old_count.saturating_add(new_count)),
        }
    }

    fn push_line(&mut self, raw: &str) {
        let Some(prefix) = raw.as_bytes().first().copied() else {
            self.push_context("");
            return;
        };

        match prefix {
            b'+' => {
                let new_line = self.new_line;
                self.new_line += 1;
                self.additions += 1;
                self.lines.push(DiffLine::addition(
                    new_line,
                    raw.get(1..).unwrap_or_default(),
                ));
            }
            b'-' => {
                let old_line = self.old_line;
                self.old_line += 1;
                self.deletions += 1;
                self.lines.push(DiffLine::deletion(
                    old_line,
                    raw.get(1..).unwrap_or_default(),
                ));
            }
            b' ' => self.push_context_owned(raw.get(1..).unwrap_or_default().to_owned()),
            b'\\' => {
                if !is_diff_no_newline_marker(raw) {
                    self.lines.push(DiffLine::meta(raw));
                }
            }
            _ => self.push_context(raw),
        }
    }

    fn is_complete(&self) -> bool {
        self.old_line.saturating_sub(self.ranges.old_start()) >= self.ranges.old_count()
            && self.new_line.saturating_sub(self.ranges.new_start()) >= self.ranges.new_count()
    }

    fn push_context(&mut self, text: &str) {
        self.push_context_owned(text.to_owned());
    }

    fn push_context_owned(&mut self, text: String) {
        let old_line = self.old_line;
        let new_line = self.new_line;
        self.old_line += 1;
        self.new_line += 1;
        self.lines.push(DiffLine::context(old_line, new_line, text));
    }

    fn finish(self) -> DiffHunk {
        DiffHunk {
            header: self.header,
            ranges: self.ranges,
            lines: self.lines,
        }
    }
}

pub(super) fn parse_hunk_header(header: &str) -> (usize, usize, usize, usize) {
    let mut parts = header.split_whitespace();
    let _ = parts.next();
    let old = parts.next().unwrap_or("-0,0");
    let new = parts.next().unwrap_or("+0,0");
    let (old_start, old_count) = parse_hunk_range(old.trim_start_matches('-'));
    let (new_start, new_count) = parse_hunk_range(new.trim_start_matches('+'));
    (old_start, old_count, new_start, new_count)
}

fn parse_hunk_range(range: &str) -> (usize, usize) {
    let mut parts = range.splitn(2, ',');
    let start = parts.next().unwrap_or("0").parse().unwrap_or(0);
    let count = parts.next().map_or(1, |count| count.parse().unwrap_or(1));
    (start, count)
}
