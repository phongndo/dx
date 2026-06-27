use super::AnnotationScratchFile;
use mark_core::MarkResult;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::{fs, io, process};

pub(crate) fn normalize_annotation_editor_contents(contents: &str) -> String {
    contents
        .replace("\r\n", "\n")
        .trim_end_matches('\n')
        .to_owned()
}

pub(crate) fn create_annotation_scratch_file(contents: &str) -> MarkResult<AnnotationScratchFile> {
    let prefix = format!("mark-annotations-{}-", process::id());
    let dir = tempfile::Builder::new().prefix(&prefix).tempdir()?;
    #[cfg(unix)]
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700))?;

    let path = dir.path().join("annotation.md");
    write_annotation_scratch_file(&path, contents)?;

    Ok(AnnotationScratchFile { _dir: dir, path })
}

#[cfg(unix)]
fn write_annotation_scratch_file(path: &Path, contents: &str) -> io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(contents.as_bytes())?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn write_annotation_scratch_file(path: &Path, contents: &str) -> io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(contents.as_bytes())
}

#[cfg(test)]
mod annotation_editor_tests {
    use super::normalize_annotation_editor_contents;

    #[test]
    fn annotation_editor_contents_normalize_crlf_line_endings() {
        assert_eq!(
            normalize_annotation_editor_contents("first\r\nsecond\r\n"),
            "first\nsecond"
        );
        assert_eq!(
            normalize_annotation_editor_contents("first\r\nsecond"),
            "first\nsecond"
        );
        assert_eq!(
            normalize_annotation_editor_contents("first\r\n\r\nsecond\r\n"),
            "first\n\nsecond"
        );
        assert_eq!(
            normalize_annotation_editor_contents("trailing spaces  \r\n"),
            "trailing spaces  "
        );
    }
}

#[cfg(all(test, unix))]
mod annotation_scratch_tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt as _;

    use super::create_annotation_scratch_file;

    #[test]
    fn annotation_scratch_file_is_private_and_removed_on_drop() {
        let scratch = create_annotation_scratch_file("secret").expect("scratch file");
        let dir = scratch.path.parent().expect("scratch dir").to_path_buf();

        assert_eq!(
            fs::metadata(&dir)
                .expect("scratch dir metadata")
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&scratch.path)
                .expect("scratch file metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            fs::read_to_string(&scratch.path).expect("scratch contents"),
            "secret"
        );

        drop(scratch);

        assert!(!dir.exists());
    }
}
