use super::*;

pub(crate) async fn run_loop(
    terminal: &mut CrosstermTerminal,
    app: &mut DiffApp,
    live_updates: bool,
    live_diff: &mut Option<LiveDiff>,
) -> MarkResult<()> {
    let mut events = TerminalEventReader::start("mark-diff-events")?;

    loop {
        app.expire_toasts(Instant::now());
        drain_live_diff_invalidation(app, live_diff.as_ref());
        sync_live_diff(live_diff, app, live_updates);
        drain_live_reloads(
            app,
            live_diff.as_mut().map(|live_diff| &mut live_diff.reload_rx),
        );
        app.drain_pending_diff_load();
        app.drain_diff_prefetch();
        app.start_due_filter_apply();
        app.drain_filter_worker();
        app.drain_syntax();
        if app.runtime.dirty {
            if app.runtime.terminal_clear_requested {
                terminal.clear()?;
                app.runtime.terminal_clear_requested = false;
            }
            terminal.draw(|frame| draw(frame, app))?;
            app.runtime.dirty = false;
            app.start_diff_prefetches();
        }
        app.start_pending_editor_reload();
        if app.drain_editor_reload() {
            continue;
        }

        if let Some(event) = events.read_timeout(app.event_poll()).await?
            && handle_ready_events(app, live_diff, event, &mut events)?
        {
            break;
        }
    }

    Ok(())
}

fn handle_ready_events(
    app: &mut DiffApp,
    live_diff: &mut Option<LiveDiff>,
    first_event: Event,
    events: &mut TerminalEventReader,
) -> MarkResult<bool> {
    if handle_event(app, first_event, live_diff, events)? {
        return Ok(true);
    }

    for _ in 1..MAX_READY_EVENTS_PER_FRAME {
        let Some(event) = events.try_read()? else {
            break;
        };
        if handle_event(app, event, live_diff, events)? {
            return Ok(true);
        }
    }

    Ok(false)
}

pub(crate) fn drain_live_diff_invalidation(app: &mut DiffApp, live_diff: Option<&LiveDiff>) {
    if live_diff.is_some_and(|live_diff| live_diff.take_invalidated()) {
        app.mark_live_reload_invalidated();
    }
}

pub(crate) fn sync_live_diff(
    live_diff: &mut Option<LiveDiff>,
    app: &mut DiffApp,
    live_updates: bool,
) {
    if !live_updates
        || !app.jobs.live_updates_allowed
        || !app.jobs.live_updates_enabled
        || !live_diff_supported(&app.document.options)
    {
        *live_diff = None;
        app.jobs.live_diff_failed_options = None;
        app.jobs.live_reload_invalidated = false;
        app.jobs.live_reload_pending = false;
        app.clear_cached_diff_choices();
        return;
    }

    if live_diff
        .as_ref()
        .is_some_and(|live_diff| live_diff.options == app.document.options)
    {
        return;
    }
    if app.jobs.live_diff_failed_options.as_ref() == Some(&app.document.options) {
        return;
    }

    match LiveDiff::start(app.document.options.clone(), &app.document.changeset.repo) {
        Ok(next_live_diff) => {
            app.jobs.live_diff_failed_options = None;
            app.jobs.live_reload_invalidated = false;
            app.jobs.live_reload_pending = false;
            *live_diff = Some(next_live_diff);
        }
        Err(error) => {
            *live_diff = None;
            app.jobs.live_diff_failed_options = Some(app.document.options.clone());
            app.jobs.live_reload_invalidated = false;
            app.jobs.live_reload_pending = false;
            app.clear_cached_diff_choices();
            app.set_error_log(format!("live reload unavailable: {error}"));
        }
    }
}

pub(crate) fn drain_live_reloads(
    app: &mut DiffApp,
    live_reload_rx: Option<&mut Receiver<LiveDiffReload>>,
) {
    let Some(live_reload_rx) = live_reload_rx else {
        return;
    };

    while let Ok(reload) = live_reload_rx.try_recv() {
        match reload {
            LiveDiffReload::Started => {
                if !app.jobs.live_reload_pending {
                    app.mark_live_reload_pending();
                }
            }
            LiveDiffReload::Loaded(Ok(changeset)) => app.replace_changeset(changeset),
            LiveDiffReload::Loaded(Err(error)) => {
                app.jobs.live_reload_invalidated = false;
                app.jobs.live_reload_pending = false;
                app.set_error_log(format!("live reload failed: {error}"));
            }
        }
    }
}

pub(crate) fn handle_event(
    app: &mut DiffApp,
    event: Event,
    live_diff: &mut Option<LiveDiff>,
    events: &mut TerminalEventReader,
) -> MarkResult<bool> {
    drain_live_diff_invalidation(app, live_diff.as_ref());
    if app.debug_notifications_enabled() {
        app.set_debug_notice(format!("event: {}", event_label(&event)));
    }

    match event {
        Event::Key(key) if app.ignore_post_editor_quit_key(key, Instant::now()) => Ok(false),
        Event::Key(key) if app.handle_annotation_save_or_cancel_key(key) => Ok(false),
        Event::Key(key) if is_quit_key(key) => Ok(true),
        Event::Key(key)
            if app
                .config
                .keymap
                .matches_single(GlobalAction::EditHunk, key)
                && app.editor_shortcut_available() =>
        {
            if app.annotations_state.annotation_draft.is_some() {
                let paused_events = events.pause();
                app.open_annotation_draft_in_editor();
                paused_events.resume()?;
            } else if let Some(editor) = app.prepare_focused_hunk_editor() {
                let paused_events = events.pause();
                app.open_prepared_hunk_in_editor(editor, Some(live_diff));
                paused_events.resume()?;
            }
            Ok(false)
        }
        Event::Key(key) if app.handle_key(key)? => Ok(true),
        Event::Mouse(mouse) => {
            app.handle_mouse(mouse)?;
            Ok(false)
        }
        Event::FocusLost => {
            app.clear_diff_mouse_hover();
            Ok(false)
        }
        Event::Resize(width, height) => {
            app.clear_diff_mouse_hover();
            app.set_terminal_area(Rect {
                x: 0,
                y: 0,
                width,
                height,
            });
            app.apply_responsive_layout(width);
            Ok(false)
        }
        _ => Ok(false),
    }
}

fn event_label(event: &Event) -> String {
    match event {
        Event::Key(key) => format!("key {:?}", key.code),
        Event::Mouse(mouse) => format!("mouse {:?}", mouse.kind),
        Event::Resize(width, height) => format!("resize {width}x{height}"),
        Event::FocusGained => "focus gained".to_owned(),
        Event::FocusLost => "focus lost".to_owned(),
        Event::Paste(text) => format!("paste {} bytes", text.len()),
    }
}

pub(crate) fn is_quit_key(key: KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL)
        && !key.modifiers.contains(KeyModifiers::SHIFT)
        && key.code == KeyCode::Char('c')
}
