use transport::stdio::{FrameMode, decode_frames, encode_frame};

#[test]
fn decodes_ndjson_and_preserves_mode() {
    let (mode, messages) = decode_frames(
        br#"{"jsonrpc":"2.0","id":1,"method":"ping"}
{"jsonrpc":"2.0","method":"notifications/initialized"}
"#,
    )
    .unwrap();
    assert_eq!(mode, FrameMode::Ndjson);
    assert_eq!(messages.len(), 2);
}

#[test]
fn decodes_content_length_and_selects_it_for_session() {
    let body = br#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;
    let input = format!(
        "Content-Length: {}\r\n\r\n{}",
        body.len(),
        String::from_utf8_lossy(body)
    );
    let (mode, messages) = decode_frames(input.as_bytes()).unwrap();
    assert_eq!(mode, FrameMode::ContentLength);
    assert_eq!(messages[0]["method"], "ping");
    assert!(encode_frame(FrameMode::ContentLength, &messages[0]).starts_with("Content-Length:"));
}

#[test]
fn invalid_json_is_a_json_rpc_error_and_notifications_have_no_response() {
    let (mode, messages) = decode_frames(
        b"not-json\n{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n",
    )
    .unwrap();
    assert_eq!(mode, FrameMode::Ndjson);
    assert!(messages[0].get("error").is_some());
    assert!(messages[1].get("id").is_none());
}

#[test]
fn eof_and_flush_framing_are_explicit() {
    let value = serde_json::json!({"jsonrpc":"2.0","id":7,"result":{}});
    let encoded = encode_frame(FrameMode::Ndjson, &value);
    assert!(encoded.ends_with('\n'));
    assert!(encoded.contains("\"id\":7"));
}
