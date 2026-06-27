use super::*;

#[test]
fn syntax_settings_load_error_falls_back_with_visible_diagnostic() {
    let (settings, error_log) =
        syntax_settings_for_diff(Err(MarkError::Usage("bad syntax config".to_owned())));

    assert_eq!(settings, SyntaxSettings::default());
    let error_log = error_log.expect("settings error should be visible");
    assert!(error_log.contains("syntax settings ignored"));
    assert!(error_log.contains("bad syntax config"));
}

#[test]
fn syntax_runtime_start_error_disables_syntax_with_visible_diagnostic() {
    let mut error_log = Some("syntax settings ignored: bad theme".to_owned());

    let syntax = syntax_runtime_for_diff(
        Err(MarkError::Usage("bad tree-sitter config".to_owned())),
        &mut error_log,
    );

    assert!(syntax.is_none());
    assert_eq!(
        error_log.as_deref(),
        Some("syntax settings ignored: bad theme\nsyntax disabled: bad tree-sitter config")
    );
}

#[test]
fn highlight_queue_runs_visible_jobs_before_prefetch_jobs() {
    let queue = SyntaxWorkerQueue::new(8, 0);
    let prefetch = syntax_key(1);
    let visible = syntax_key(2);

    queue
        .try_push(syntax_job(prefetch), SyntaxPriority::Prefetch)
        .unwrap();
    queue
        .try_push(syntax_job(visible), SyntaxPriority::Visible)
        .unwrap();

    assert_eq!(queue.try_pop().map(|job| job.key), Some(visible));
    assert_eq!(queue.try_pop().map(|job| job.key), Some(prefetch));
}

#[test]
fn visible_highlight_job_can_evict_prefetch_when_queue_is_full() {
    let queue = SyntaxWorkerQueue::new(1, 0);
    let prefetch = syntax_key(1);
    let visible = syntax_key(2);

    queue
        .try_push(syntax_job(prefetch), SyntaxPriority::Prefetch)
        .unwrap();
    let pushed = queue
        .try_push(syntax_job(visible), SyntaxPriority::Visible)
        .unwrap();

    assert_eq!(pushed.dropped, Some(prefetch));
    assert_eq!(queue.len(), 1);
    assert_eq!(queue.try_pop().map(|job| job.key), Some(visible));
}

#[test]
fn stale_highlight_jobs_are_dropped_on_generation_change() {
    let queue = SyntaxWorkerQueue::new(8, 0);

    queue
        .try_push(syntax_job(syntax_key(1)), SyntaxPriority::Prefetch)
        .unwrap();
    queue.set_generation(1);

    assert_eq!(queue.len(), 0);
    assert_eq!(
        queue.try_push(syntax_job(syntax_key(2)), SyntaxPriority::Visible),
        Err(SyntaxQueueError::Stale)
    );

    let fresh = syntax_key_with_generation(1, 0);
    queue
        .try_push(syntax_job(fresh), SyntaxPriority::Visible)
        .unwrap();
    assert_eq!(queue.try_pop().map(|job| job.key), Some(fresh));
}
