use super::terminal::csi_escape_end;

pub(super) fn looks_like_patch_input(input: &[u8]) -> bool {
    let normalized = normalized_patch_input(input);
    let text = String::from_utf8_lossy(&normalized);
    parseable_patch_has_renderable_change(&text)
}

pub(super) fn patch_input_has_prelude(input: &[u8]) -> bool {
    let normalized = normalized_patch_input(input);
    !split_patch_prelude(&normalized).0.is_empty()
}

fn parseable_patch_has_renderable_change(patch: &str) -> bool {
    let has_git_header = patch_header_lines(patch).any(|line| line.starts_with("diff --git "));
    mark_diff::parse_patch(patch)
        .iter()
        .any(|file| diff_file_has_renderable_change(file, has_git_header))
}

fn diff_file_has_renderable_change(file: &mark_diff::DiffFile, input_has_git_header: bool) -> bool {
    file.has_textual_changes()
        || file.is_binary()
        || (input_has_git_header
            && matches!(
                file.status(),
                mark_diff::FileStatus::Added
                    | mark_diff::FileStatus::Deleted
                    | mark_diff::FileStatus::Renamed
                    | mark_diff::FileStatus::Copied
                    | mark_diff::FileStatus::TypeChanged
            ))
}

fn patch_header_lines(patch: &str) -> impl Iterator<Item = &str> {
    patch
        .split_inclusive('\n')
        .map(|line| line.strip_suffix('\n').unwrap_or(line))
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
}

pub(super) fn normalized_patch_input(input: &[u8]) -> Vec<u8> {
    let lines = input
        .split_inclusive(|byte| *byte == b'\n')
        .collect::<Vec<_>>();
    let strip_uncolored_context_resets = strip_uncolored_context_reset_lines(&lines);
    let mut output = Vec::with_capacity(input.len());
    for (line, strip_uncolored_context_reset) in
        lines.into_iter().zip(strip_uncolored_context_resets)
    {
        append_patch_line_without_color_wrappers(line, &mut output, strip_uncolored_context_reset);
    }
    output
}

fn strip_uncolored_context_reset_lines(lines: &[&[u8]]) -> Vec<bool> {
    // Git emits normal-color context lines as ` context\x1b[m` when the hunk
    // body is colored. Track that per hunk; colored headers alone do not prove
    // trailing resets on hunk payload lines are Git color wrappers.
    let mut strip_reset = vec![false; lines.len()];
    let mut hunk_start = None;
    let mut hunk_has_colored_body = false;

    for (index, line) in lines.iter().enumerate() {
        if hunk_header_line(line) {
            finish_hunk_color_scan(&mut strip_reset, hunk_start, index, hunk_has_colored_body);
            hunk_start = Some(index + 1);
            hunk_has_colored_body = false;
            continue;
        }

        if patch_structural_line(line) {
            finish_hunk_color_scan(&mut strip_reset, hunk_start, index, hunk_has_colored_body);
            hunk_start = None;
            hunk_has_colored_body = false;
            continue;
        }

        if hunk_start.is_some() && hunk_body_line_has_git_color(line) {
            hunk_has_colored_body = true;
        }
    }

    finish_hunk_color_scan(
        &mut strip_reset,
        hunk_start,
        lines.len(),
        hunk_has_colored_body,
    );
    strip_reset
}

fn finish_hunk_color_scan(
    strip_reset: &mut [bool],
    hunk_start: Option<usize>,
    hunk_end: usize,
    hunk_has_colored_body: bool,
) {
    if !hunk_has_colored_body {
        return;
    }

    let Some(hunk_start) = hunk_start else {
        return;
    };

    for strip_reset in &mut strip_reset[hunk_start..hunk_end] {
        *strip_reset = true;
    }
}

fn hunk_header_line(line: &[u8]) -> bool {
    let line = patch_line_without_leading_sgr(line);
    line.starts_with(b"@@ ") || {
        let mut stripped = Vec::with_capacity(line.len());
        append_without_sgr_escapes(line, &mut stripped);
        stripped.starts_with(b"@@ ")
    }
}

fn patch_structural_line(line: &[u8]) -> bool {
    diff_structural_line(patch_line_without_leading_sgr(line))
}

fn patch_line_without_leading_sgr(line: &[u8]) -> &[u8] {
    let line_break_start = line.strip_suffix(b"\n").map_or(line.len(), <[u8]>::len);
    let mut content_start = 0;
    while content_start < line_break_start {
        let Some(end) = sgr_escape_end(line, content_start) else {
            break;
        };
        content_start = end;
    }
    &line[content_start..line_break_start]
}

fn hunk_body_line_has_git_color(line: &[u8]) -> bool {
    let line_break_start = line.strip_suffix(b"\n").map_or(line.len(), <[u8]>::len);
    let mut content_start = 0;
    let mut has_leading_sgr = false;
    while content_start < line_break_start {
        let Some(end) = sgr_escape_end(line, content_start) else {
            break;
        };
        has_leading_sgr = true;
        content_start = end;
    }

    if !matches!(line.get(content_start), Some(b' ' | b'+' | b'-')) {
        return false;
    }

    if has_leading_sgr {
        return true;
    }

    content_start += 1;
    sgr_escape_end(line, content_start)
        .is_some_and(|end| sgr_escape_is_reset(&line[content_start..end]))
}

fn append_patch_line_without_color_wrappers(
    line: &[u8],
    output: &mut Vec<u8>,
    strip_uncolored_context_reset: bool,
) {
    let line_break_start = line.strip_suffix(b"\n").map_or(line.len(), <[u8]>::len);
    let mut content_start = 0;
    let mut stripped_git_color = false;
    while content_start < line_break_start {
        let Some(end) = sgr_escape_end(line, content_start) else {
            break;
        };
        stripped_git_color = true;
        content_start = end;
    }
    let leading_sgr_end = content_start;

    let structural_content = &line[content_start..line_break_start];
    if diff_structural_line(structural_content) {
        append_without_sgr_escapes(&line[content_start..line_break_start], output);
        output.extend_from_slice(&line[line_break_start..]);
        return;
    }

    let diff_prefix = line.get(content_start).copied();
    if matches!(diff_prefix, Some(b' ' | b'+' | b'-')) {
        output.push(line[content_start]);
        content_start += 1;
        let mut stripped_prefix_reset = false;
        while content_start < line_break_start {
            let Some(end) = sgr_escape_end(line, content_start) else {
                break;
            };
            if !sgr_escape_is_reset(&line[content_start..end]) {
                break;
            }
            stripped_git_color = true;
            stripped_prefix_reset = true;
            content_start = end;
        }
        // Git can reset after the +/- marker and reapply the line color before
        // the payload. Strip one copy only; an identical following SGR can be
        // literal file content.
        if stripped_prefix_reset
            && strip_reapplied_git_line_color(line, leading_sgr_end, &mut content_start)
        {
            stripped_git_color = true;
        }
    }

    let strip_trailing_reset =
        stripped_git_color || (strip_uncolored_context_reset && matches!(diff_prefix, Some(b' ')));
    let content_end = if strip_trailing_reset {
        trailing_sgr_reset_start(line, content_start, line_break_start).unwrap_or(line_break_start)
    } else {
        line_break_start
    };

    output.extend_from_slice(&line[content_start..content_end]);
    output.extend_from_slice(&line[line_break_start..]);
}

fn strip_reapplied_git_line_color(
    line: &[u8],
    leading_sgr_end: usize,
    content_start: &mut usize,
) -> bool {
    if leading_sgr_end == 0 {
        return false;
    }

    let leading_sgr = &line[..leading_sgr_end];
    if !line[*content_start..].starts_with(leading_sgr) {
        return false;
    }

    *content_start += leading_sgr.len();
    true
}

fn diff_structural_line(line: &[u8]) -> bool {
    if diff_structural_line_without_sgr(line) {
        return true;
    }

    let mut stripped = Vec::with_capacity(line.len());
    append_without_sgr_escapes(line, &mut stripped);
    stripped.len() != line.len() && diff_structural_line_without_sgr(&stripped)
}

fn diff_structural_line_without_sgr(line: &[u8]) -> bool {
    line.starts_with(b"diff --git ")
        || line.starts_with(b"index ")
        || line.starts_with(b"old mode ")
        || line.starts_with(b"new mode ")
        || line.starts_with(b"deleted file mode ")
        || line.starts_with(b"new file mode ")
        || line.starts_with(b"similarity index ")
        || line.starts_with(b"dissimilarity index ")
        || line.starts_with(b"rename from ")
        || line.starts_with(b"rename to ")
        || line.starts_with(b"copy from ")
        || line.starts_with(b"copy to ")
        || line.starts_with(b"@@ ")
        || line.starts_with(b"Binary files ")
        || line.starts_with(b"GIT binary patch")
}

fn append_without_sgr_escapes(input: &[u8], output: &mut Vec<u8>) {
    let mut index = 0;
    while index < input.len() {
        if let Some(end) = sgr_escape_end(input, index) {
            index = end;
        } else {
            output.push(input[index]);
            index += 1;
        }
    }
}

pub(super) fn split_patch_prelude(input: &[u8]) -> (&[u8], &[u8]) {
    let Some(patch_start) = first_git_diff_line_start(input) else {
        return (&[], input);
    };
    input.split_at(patch_start)
}

fn first_git_diff_line_start(input: &[u8]) -> Option<usize> {
    let mut offset = 0;
    for line in input.split_inclusive(|byte| *byte == b'\n') {
        let line_without_lf = line.strip_suffix(b"\n").unwrap_or(line);
        let line_without_crlf = line_without_lf
            .strip_suffix(b"\r")
            .unwrap_or(line_without_lf);
        if line_without_crlf.starts_with(b"diff --git ") {
            return Some(offset);
        }
        offset += line.len();
    }
    None
}

fn trailing_sgr_reset_start(input: &[u8], start: usize, end: usize) -> Option<usize> {
    let mut index = start;
    while index < end {
        if let Some(escape_end) = sgr_escape_end(input, index) {
            if escape_end == end && sgr_escape_is_reset(&input[index..escape_end]) {
                return Some(index);
            }
            index = escape_end;
        } else {
            index += 1;
        }
    }
    None
}

fn sgr_escape_end(input: &[u8], index: usize) -> Option<usize> {
    if input.get(index) != Some(&0x1b) || input.get(index + 1) != Some(&b'[') {
        return None;
    }

    csi_escape_end(input, index + 2).filter(|end| input.get(end - 1) == Some(&b'm'))
}

fn sgr_escape_is_reset(escape: &[u8]) -> bool {
    let Some(parameters) = escape
        .strip_prefix(b"\x1b[")
        .and_then(|escape| escape.strip_suffix(b"m"))
    else {
        return false;
    };

    parameters.is_empty()
        || parameters
            .split(|byte| *byte == b';')
            .all(|parameter| parameter.is_empty() || parameter.iter().all(|byte| *byte == b'0'))
}
