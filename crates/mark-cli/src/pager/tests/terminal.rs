use super::*;

#[test]
fn sanitized_terminal_bytes_escapes_malformed_escapes() {
    let sanitized = sanitized_terminal_bytes(b"a\x1b]unterminated\nb\x1b[31\nc");

    assert_eq!(sanitized, b"a\\u{1b}]unterminated\nb\\u{1b}[31\nc");
}

#[test]
fn strip_terminal_escapes_removes_csi_and_osc_but_preserves_cr() {
    let stripped = strip_terminal_escapes(b"a\r\n\x1b[31mb\x1b[0mc\x1b]52;c;secret\x07d");

    assert_eq!(stripped, b"a\r\nbcd");
}

#[test]
fn sanitized_terminal_bytes_escapes_controls_after_stripping_sequences() {
    let sanitized = sanitized_terminal_bytes(b"a\r\x07\x1b[31mb\x1b[0m\n");

    assert_eq!(sanitized, b"a\\r\\u{7}b\n");
}
