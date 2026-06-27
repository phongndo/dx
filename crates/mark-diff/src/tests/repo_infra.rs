use super::*;

#[test]
fn temp_index_paths_are_adjacent_to_source_index() {
    let index = PathBuf::from("/repo/.git/worktrees/feature/index");
    let temp = temp_index_path(&index, 0).expect("temp index path should resolve");

    assert_eq!(temp.parent(), index.parent());
    assert!(
        temp.file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with(".mark-diff-index-")
    );
}

#[test]
fn stderr_capture_temp_file_is_private() {
    use std::os::unix::fs::PermissionsExt;

    let stderr = StderrCapture::new().expect("stderr capture should be created");
    let path = stderr.path.clone();
    let mode = fs::metadata(&path)
        .expect("stderr capture should exist")
        .permissions()
        .mode()
        & 0o777;

    assert_eq!(mode, 0o600);
    stderr.discard();
    assert!(!path.exists());
}

#[test]
fn stderr_capture_drop_removes_temp_file() {
    let stderr = StderrCapture::new().expect("stderr capture should be created");
    let path = stderr.path.clone();

    assert!(path.exists());
    drop(stderr);
    assert!(!path.exists());
}
