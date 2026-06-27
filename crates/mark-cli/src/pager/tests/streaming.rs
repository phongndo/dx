use super::*;

#[test]
fn pager_streams_plain_text_after_classification_limit() {
    let mut input = std::io::Cursor::new(vec![b'x'; PAGER_CLASSIFICATION_LIMIT + 1]);

    let decision = read_pager_input(
        &mut input,
        true,
        &env(Some("xterm-256color"), None, None, false),
        true,
    )
    .unwrap();

    let PagerInput::Streaming { prefix, action } = decision else {
        panic!("expected streaming input");
    };
    assert_eq!(action, StreamingPagerAction::PlainTextPager);
    assert_eq!(prefix.len(), PAGER_CLASSIFICATION_LIMIT);
    assert_eq!(input.position(), PAGER_CLASSIFICATION_LIMIT as u64);
}

#[test]
fn plain_text_stream_fallback_replays_prefix_and_unread_input() {
    let prefix = b"buffered prefix\n";
    let rest = b"still unread\n".to_vec();
    let mut input = std::io::Cursor::new(rest.clone());
    let mut pager = FailingWriter::new(0);
    let mut fallback = StreamFallback::default();

    let error = stream_to_pager(prefix, &mut input, &mut pager, &mut fallback).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
    assert_eq!(input.position(), 0);

    let mut output = Vec::new();
    fallback
        .write_to_writer(prefix, &mut input, &mut output)
        .unwrap();

    let mut expected = prefix.to_vec();
    expected.extend_from_slice(&rest);
    assert_eq!(output, expected);
}

#[test]
fn plain_text_stream_fallback_replays_spooled_and_unread_input() {
    let prefix = b"buffered prefix\n";
    let rest = vec![b'x'; STREAM_BUFFER_SIZE + 1];
    let mut input = std::io::Cursor::new(rest.clone());
    let mut pager = FailingWriter::new(prefix.len() + 4);
    let mut fallback = StreamFallback::default();

    let error = stream_to_pager(prefix, &mut input, &mut pager, &mut fallback).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
    assert_eq!(input.position(), STREAM_BUFFER_SIZE as u64);

    let mut output = Vec::new();
    fallback
        .write_to_writer(prefix, &mut input, &mut output)
        .unwrap();

    let mut expected = prefix.to_vec();
    expected.extend_from_slice(&rest);
    assert_eq!(output, expected);
}

#[test]
fn plain_text_stream_fallback_replays_fully_spooled_input() {
    let prefix = b"buffered prefix\n";
    let rest = vec![b'x'; STREAM_BUFFER_SIZE + 1];
    let mut input = std::io::Cursor::new(rest.clone());
    let mut pager = Vec::new();
    let mut fallback = StreamFallback::default();

    stream_to_pager(prefix, &mut input, &mut pager, &mut fallback).unwrap();
    assert_eq!(input.position(), rest.len() as u64);

    let mut output = Vec::new();
    fallback
        .write_to_writer(prefix, &mut input, &mut output)
        .unwrap();

    let mut expected = prefix.to_vec();
    expected.extend_from_slice(&rest);
    assert_eq!(output, expected);
}

#[test]
fn pager_buffers_diff_after_detection() {
    let mut input_bytes = b"diff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -1 +1 @@\n-a\n+b\n".to_vec();
    input_bytes.extend(vec![b'x'; STREAM_BUFFER_SIZE * 2]);
    let expected_len = input_bytes.len();
    let mut input = std::io::Cursor::new(input_bytes);

    let decision = read_pager_input(
        &mut input,
        true,
        &env(Some("xterm-256color"), None, None, false),
        true,
    )
    .unwrap();

    let PagerInput::Buffered { input, action } = decision else {
        panic!("expected buffered input");
    };
    assert_eq!(action, PagerAction::InteractiveDiff);
    assert_eq!(input.len(), expected_len);
}

#[test]
fn pager_streams_without_classification_when_action_cannot_change() {
    let mut input = std::io::Cursor::new(vec![b'x'; PAGER_CLASSIFICATION_LIMIT + 1]);

    let decision = read_pager_input(
        &mut input,
        false,
        &env(Some("xterm-256color"), None, None, false),
        true,
    )
    .unwrap();

    let PagerInput::Streaming { prefix, action } = decision else {
        panic!("expected streaming input");
    };
    assert_eq!(action, StreamingPagerAction::Passthrough);
    assert!(prefix.is_empty());
    assert_eq!(input.position(), 0);
}

#[test]
fn plain_text_pager_replaces_self_referential_mark_pager() {
    assert_eq!(
        resolve_text_pager_command(Some("mark pager")),
        DEFAULT_TEXT_PAGER
    );
    assert_eq!(
        resolve_text_pager_command(Some("mark page")),
        DEFAULT_TEXT_PAGER
    );
    assert_eq!(
        resolve_text_pager_command(Some("/usr/local/bin/mark page --layout unified")),
        DEFAULT_TEXT_PAGER
    );
    assert_eq!(
        resolve_text_pager_command(Some("/usr/local/bin/mark pager --layout unified")),
        DEFAULT_TEXT_PAGER
    );
    assert_eq!(
        resolve_text_pager_command(Some("env TERM=xterm-256color mark pager")),
        DEFAULT_TEXT_PAGER
    );
    assert_eq!(
        resolve_text_pager_command(Some("command mark pager")),
        DEFAULT_TEXT_PAGER
    );
    assert_eq!(
        resolve_text_pager_command(Some("PAGER=cat exec mark pager")),
        DEFAULT_TEXT_PAGER
    );
}

#[test]
fn plain_text_pager_preserves_non_self_pager_commands() {
    assert_eq!(resolve_text_pager_command(None), DEFAULT_TEXT_PAGER);
    assert_eq!(resolve_text_pager_command(Some("")), DEFAULT_TEXT_PAGER);
    assert_eq!(resolve_text_pager_command(Some("less -FRX")), "less -FRX");
    assert_eq!(
        resolve_text_pager_command(Some("delta --paging=always")),
        "delta --paging=always"
    );
    assert_eq!(resolve_text_pager_command(Some("mark diff")), "mark diff");
}
