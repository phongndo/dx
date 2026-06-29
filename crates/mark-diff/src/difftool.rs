use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

use mark_core::{MarkError, MarkResult};

use crate::{DiffOptions, git_io::git_error};

pub(super) fn difftool_workdir(options: &DiffOptions) -> MarkResult<PathBuf> {
    options.repo.clone().map_or_else(
        || env::current_dir().map_err(MarkError::Io),
        |repo| Ok(repo.into_path_buf()),
    )
}

pub(super) fn difftool_display_path(left: &Path, right: &Path, path: Option<&Path>) -> String {
    path.map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| {
            let fallback = if is_null_path(right) && !is_null_path(left) {
                left
            } else {
                right
            };
            fallback
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| fallback.to_string_lossy().into_owned())
        })
}

pub(super) fn difftool_patch_bytes(
    workdir: &Path,
    left: &Path,
    right: &Path,
    display_path: Option<&Path>,
) -> MarkResult<Vec<u8>> {
    reject_difftool_directory(workdir, left, "left")?;
    reject_difftool_directory(workdir, right, "right")?;

    let output = Command::new("git")
        .arg("-C")
        .arg(workdir)
        .args([
            "diff",
            "--no-index",
            "--binary",
            "--no-color",
            "--no-ext-diff",
            "--",
        ])
        .arg(left)
        .arg(right)
        .output()?;

    let status = output.status.code();
    let diff_succeeded = status == Some(0) || (status == Some(1) && !output.stdout.is_empty());
    if !diff_succeeded {
        return Err(git_error("git difftool pair diff failed", &output));
    }

    let display_path = difftool_display_path(left, right, display_path);
    Ok(rewrite_difftool_patch_paths(&output.stdout, &display_path))
}

fn reject_difftool_directory(workdir: &Path, path: &Path, side: &str) -> MarkResult<()> {
    if is_null_path(path) {
        return Ok(());
    }

    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        workdir.join(path)
    };

    if path.is_dir() {
        return Err(MarkError::Usage(format!(
            "mark difftool expects file paths, but the {side} path is a directory: {}",
            path.display()
        )));
    }

    Ok(())
}

fn is_null_path(path: &Path) -> bool {
    path == Path::new("/dev/null") || path == Path::new("NUL")
}

pub(super) fn rewrite_difftool_patch_paths(patch: &[u8], display_path: &str) -> Vec<u8> {
    let old_path = git_patch_path("a/", display_path).into_bytes();
    let new_path = git_patch_path("b/", display_path).into_bytes();
    let mut rewritten = Vec::with_capacity(patch.len());
    let mut section: Option<DifftoolPatchSection> = None;

    for line in patch.split_inclusive(|byte| *byte == b'\n') {
        let (header_line, line_ending) = patch_line_parts(line);

        if header_line.starts_with(b"diff --git ") {
            flush_difftool_patch_section(&mut rewritten, &mut section);
            let mut next = DifftoolPatchSection::default();
            next.bytes.extend_from_slice(b"diff --git ");
            next.bytes.extend_from_slice(&old_path);
            next.bytes.push(b' ');
            next.bytes.extend_from_slice(&new_path);
            next.bytes.extend_from_slice(line_ending);
            section = Some(next);
            continue;
        }

        let Some(section) = section.as_mut() else {
            rewritten.extend_from_slice(line);
            continue;
        };

        if !section.in_hunk {
            if is_difftool_temp_mode_line(header_line) {
                continue;
            }

            if let Some(path) = header_line.strip_prefix(b"--- ") {
                section.bytes.extend_from_slice(b"--- ");
                if path == b"/dev/null" {
                    section.bytes.extend_from_slice(b"/dev/null");
                } else {
                    section.bytes.extend_from_slice(&old_path);
                }
                section.bytes.extend_from_slice(line_ending);
                section.has_substantive_line = true;
                continue;
            }

            if let Some(path) = header_line.strip_prefix(b"+++ ") {
                section.bytes.extend_from_slice(b"+++ ");
                if path == b"/dev/null" {
                    section.bytes.extend_from_slice(b"/dev/null");
                } else {
                    section.bytes.extend_from_slice(&new_path);
                }
                section.bytes.extend_from_slice(line_ending);
                section.has_substantive_line = true;
                continue;
            }

            if header_line.starts_with(b"Binary files ") && header_line.ends_with(b" differ") {
                section.bytes.extend_from_slice(b"Binary files ");
                section.bytes.extend_from_slice(&old_path);
                section.bytes.extend_from_slice(b" and ");
                section.bytes.extend_from_slice(&new_path);
                section.bytes.extend_from_slice(b" differ");
                section.bytes.extend_from_slice(line_ending);
                section.has_substantive_line = true;
                continue;
            }
        }

        if header_line.starts_with(b"@@ ") {
            section.in_hunk = true;
        }
        if is_difftool_substantive_line(header_line) {
            section.has_substantive_line = true;
        }
        section.bytes.extend_from_slice(line);
    }

    flush_difftool_patch_section(&mut rewritten, &mut section);
    rewritten
}

#[derive(Debug, Default)]
struct DifftoolPatchSection {
    bytes: Vec<u8>,
    in_hunk: bool,
    has_substantive_line: bool,
}

fn flush_difftool_patch_section(
    rewritten: &mut Vec<u8>,
    section: &mut Option<DifftoolPatchSection>,
) {
    let Some(section) = section.take() else {
        return;
    };
    if section.has_substantive_line {
        rewritten.extend(section.bytes);
    }
}

pub(super) fn patch_line_parts(line: &[u8]) -> (&[u8], &[u8]) {
    if let Some(line) = line.strip_suffix(b"\n") {
        if let Some(line) = line.strip_suffix(b"\r") {
            (line, b"\r\n")
        } else {
            (line, b"\n")
        }
    } else if let Some(line) = line.strip_suffix(b"\r") {
        (line, b"\r")
    } else {
        (line, b"")
    }
}

fn is_difftool_temp_mode_line(line: &[u8]) -> bool {
    line.starts_with(b"old mode ") || line.starts_with(b"new mode ")
}

fn is_difftool_substantive_line(line: &[u8]) -> bool {
    !line.is_empty() && !line.starts_with(b"index ")
}

fn git_patch_path(prefix: &str, path: &str) -> String {
    quote_git_path(&format!("{prefix}{path}"))
}

fn quote_git_path(path: &str) -> String {
    if path
        .bytes()
        .all(|byte| byte.is_ascii_graphic() && !matches!(byte, b'"' | b'\\'))
    {
        return path.to_owned();
    }

    let mut quoted = String::with_capacity(path.len() + 2);
    quoted.push('"');
    for byte in path.bytes() {
        match byte {
            b'\n' => quoted.push_str("\\n"),
            b'\r' => quoted.push_str("\\r"),
            b'\t' => quoted.push_str("\\t"),
            b'\\' => quoted.push_str("\\\\"),
            b'"' => quoted.push_str("\\\""),
            byte if byte.is_ascii_graphic() || byte == b' ' => quoted.push(char::from(byte)),
            byte => quoted.push_str(&format!("\\{byte:03o}")),
        }
    }
    quoted.push('"');
    quoted
}
