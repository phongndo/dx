use super::*;

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FileFingerprint {
    len: u64,
    modified: Option<SystemTime>,
    #[cfg(unix)]
    changed: (i64, i64, u64),
}

impl FileFingerprint {
    pub(crate) fn read(path: &Path) -> Option<Self> {
        let metadata = fs::metadata(path).ok()?;
        Some(Self {
            len: metadata.len(),
            modified: metadata.modified().ok(),
            #[cfg(unix)]
            changed: (metadata.ctime(), metadata.ctime_nsec(), metadata.ino()),
        })
    }
}

pub(crate) fn file_changed_since(path: &Path, before: Option<FileFingerprint>) -> bool {
    let after = FileFingerprint::read(path);
    match (before, after) {
        (Some(before), Some(after)) => before != after,
        (None, None) => false,
        _ => true,
    }
}

impl DiffApp {
    pub(crate) fn focused_hunk_editor_target(&self) -> Option<EditorTarget> {
        if matches!(
            self.document.options.source,
            DiffSource::Patch(_) | DiffSource::Show(_) | DiffSource::Difftool { .. }
        ) {
            return None;
        }

        let (file, hunk) = self.focused_hunk_for_viewport(self.viewport.viewport_rows)?;
        let file_diff = self.document.changeset.files.get(file)?;
        let hunk_diff = file_diff.hunks.get(hunk)?;
        let path = file_diff.new_path.as_deref()?;
        let line = self
            .focused_hunk_editor_line(file, hunk)
            .unwrap_or_else(|| hunk_diff.new_start.max(1));

        Some(EditorTarget {
            path: repo_file_path(&self.document.changeset.repo, path),
            line,
        })
    }

    pub(crate) fn focused_hunk_editor_reload_request(&self) -> Option<EditorReloadRequest> {
        if matches!(
            self.document.options.source,
            DiffSource::Patch(_) | DiffSource::Show(_) | DiffSource::Difftool { .. }
        ) {
            return None;
        }

        let (file, _) = self.focused_hunk_for_viewport(self.viewport.viewport_rows)?;
        editor_reload_request_for_file(self.document.changeset.files.get(file)?)
    }

    pub(crate) fn focused_hunk_editor_line(&self, file: usize, hunk: usize) -> Option<usize> {
        let rendered_rows = self.rendered_diff_rows_for_viewport(self.viewport.viewport_rows);
        find_rendered_diff_row_outward(
            &rendered_rows,
            self.rendered_viewport_focus_row(self.viewport.viewport_rows),
            |rendered_row| self.editor_line_at_hunk_row(rendered_row.model_row, file, hunk),
        )
    }

    pub(crate) fn editor_line_at_hunk_row(
        &self,
        row_index: usize,
        file: usize,
        hunk: usize,
    ) -> Option<usize> {
        let hunk_diff = self.document.changeset.files.get(file)?.hunks.get(hunk)?;
        match self.document.model.row(row_index)? {
            UiRow::UnifiedLine {
                file: row_file,
                hunk: row_hunk,
                line,
            }
            | UiRow::MetaLine {
                file: row_file,
                hunk: row_hunk,
                line,
            } if row_file == file && row_hunk == hunk => {
                hunk_diff.lines.get(line)?.new_line.map(|line| line.max(1))
            }
            UiRow::SplitLine {
                file: row_file,
                hunk: row_hunk,
                left,
                right,
            } if row_file == file && row_hunk == hunk => right
                .or(left)
                .and_then(|line| hunk_diff.lines.get(line))
                .and_then(|line| line.new_line)
                .map(|line| line.max(1)),
            _ => None,
        }
    }

    pub(crate) fn open_focused_hunk_in_editor(&mut self) {
        if let Some(editor) = self.prepare_focused_hunk_editor() {
            self.open_prepared_hunk_in_editor(editor, None);
        }
    }

    pub(super) fn prepare_focused_hunk_editor(&mut self) -> Option<FocusedEditorLaunch> {
        self.prepare_focused_hunk_editor_with(configured_editor())
    }

    pub(super) fn prepare_focused_hunk_editor_with(
        &mut self,
        configured_editor: Option<String>,
    ) -> Option<FocusedEditorLaunch> {
        let Some(target) = self.focused_hunk_editor_target() else {
            self.set_blocked_notice("no editable focused hunk");
            return None;
        };
        let Some(editor) = configured_editor else {
            self.set_warning_notice("set $VISUAL, $GIT_EDITOR, or $EDITOR to edit focused hunk");
            return None;
        };
        Some(FocusedEditorLaunch { target, editor })
    }

    #[cfg(test)]
    pub(crate) fn prepare_focused_hunk_editor_for_test(
        &mut self,
        configured_editor: Option<String>,
    ) -> bool {
        self.prepare_focused_hunk_editor_with(configured_editor)
            .is_some()
    }

    pub(super) fn open_prepared_hunk_in_editor(
        &mut self,
        editor: FocusedEditorLaunch,
        mut live_diff: Option<&mut Option<LiveDiff>>,
    ) {
        let FocusedEditorLaunch { target, editor } = editor;
        self.overlays.diff_menu_open = false;
        self.overlays.diff_menu.reset_input();
        self.overlays.rendered_diff_menu_area = None;
        self.overlays.options_menu_open = false;
        self.close_color_scheme_picker();
        self.close_review_input();
        self.close_branch_menu();
        self.runtime.terminal_clear_requested = true;
        let mut paused_live_diff = false;
        if matches!(self.document.options.source, DiffSource::Worktree)
            && let Some(live_diff) = live_diff.as_mut().and_then(|live_diff| live_diff.as_mut())
        {
            live_diff.set_paused(true);
            paused_live_diff = true;
        }
        let scoped_reload = self.focused_hunk_editor_reload_request().or_else(|| {
            repo_relative_path(&self.document.changeset.repo, &target.path).map(|path| {
                let pathspecs = vec![path.clone()];
                EditorReloadRequest { path, pathspecs }
            })
        });
        let before = FileFingerprint::read(&target.path);
        let status_result = open_editor(&editor, &target);
        self.jobs.post_editor_quit_key_ignore_until =
            Some(Instant::now() + POST_EDITOR_QUIT_KEY_IGNORE);
        if paused_live_diff
            && let Some(live_diff) = live_diff.as_mut().and_then(|live_diff| live_diff.as_mut())
        {
            live_diff.set_paused(false);
        }

        match status_result {
            Ok(status) if status.success() => {
                let changed = file_changed_since(&target.path, before);
                match self.editor_reload_behavior(
                    changed,
                    scoped_reload.as_ref().map(|request| request.path.as_path()),
                ) {
                    EditorReloadBehavior::None => self.set_notice("editor closed"),
                    EditorReloadBehavior::ScopedAsync => {
                        let request = scoped_reload.expect("scoped reload requires a request");
                        self.queue_editor_scoped_reload(request);
                        self.set_notice("editor closed; refreshing edited file");
                    }
                    EditorReloadBehavior::Sync => match self.reload() {
                        Ok(()) => self.set_notice("editor closed; reloading"),
                        Err(error) => {
                            self.set_error_log(format!("editor closed; reload failed: {error}"))
                        }
                    },
                }
            }
            Ok(status) => {
                self.set_warning_notice(format!("editor exited with {status}"));
            }
            Err(error) => self.set_error_log(format!("editor failed: {error}")),
        }
    }

    pub(crate) fn editor_reload_behavior(
        &self,
        target_changed: bool,
        scoped_path: Option<&Path>,
    ) -> EditorReloadBehavior {
        if !target_changed
            || !matches!(
                self.document.options.source,
                DiffSource::Worktree | DiffSource::Base(_)
            )
        {
            return EditorReloadBehavior::None;
        }

        if scoped_path.is_some() {
            return EditorReloadBehavior::ScopedAsync;
        }

        EditorReloadBehavior::Sync
    }

    pub(crate) fn start_editor_scoped_reload(&mut self, request: EditorReloadRequest) {
        let options = self.document.options.clone();
        let path = request.path;
        let pathspecs = request.pathspecs;
        let (tx, rx) = oneshot::channel();
        runtime::spawn_detached_blocking(move || {
            let changeset = mark_diff::load_review_ref_paths(&options, &pathspecs);
            let _ = tx.send(EditorScopedReload { path, changeset });
        });
        self.jobs.editor_reload = Some(EditorReloadWorker {
            generation: self.document.generation,
            rx,
        });
    }

    pub(crate) fn queue_editor_scoped_reload(&mut self, request: EditorReloadRequest) {
        self.jobs.pending_editor_reload = Some(request);
        self.runtime.dirty = true;
    }

    pub(crate) fn start_pending_editor_reload(&mut self) {
        let Some(request) = self.jobs.pending_editor_reload.take() else {
            return;
        };

        self.start_editor_scoped_reload(request);
    }

    pub(crate) fn drain_editor_reload(&mut self) -> bool {
        let Some(mut worker) = self.jobs.editor_reload.take() else {
            return false;
        };

        match worker.rx.try_recv() {
            Ok(reload) => {
                if worker.generation != self.document.generation {
                    return false;
                }

                match reload.changeset {
                    Ok(changeset) => {
                        self.replace_path_changeset(&reload.path, changeset);
                        self.set_success_notice("edited file reloaded");
                    }
                    Err(error) => self.set_error_log(format!("edited file reload failed: {error}")),
                }
                true
            }
            Err(oneshot::error::TryRecvError::Empty) => {
                self.jobs.editor_reload = Some(worker);
                false
            }
            Err(oneshot::error::TryRecvError::Closed) => {
                self.set_error_log("edited file reload failed");
                true
            }
        }
    }
}
