//! Protocol acceptance client: exercises both supported stdio framing modes.
use std::{
    io::Write,
    process::{Command, Stdio},
};
fn main() {
    let binary = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "target/debug/sniffr-coolify-mcp".into());
    for content_length in [false, true] {
        let requests = [
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#,
        ];
        let mut input = String::new();
        for request in requests {
            let frame = if content_length {
                format!("Content-Length: {}\r\n\r\n{}", request.len(), request)
            } else {
                format!("{}\n", request)
            };
            input.push_str(&frame);
        }
        let mut child = Command::new(&binary)
            .env("MCP_TRANSPORT", "stdio")
            .env("COOLIFY_BASE_URL", "http://127.0.0.1:19000")
            .env("COOLIFY_ACCESS_TOKEN", "fixture-token")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("launch MCP");
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        let output = child.wait_with_output().expect("stdio response");
        let text = String::from_utf8_lossy(&output.stdout);
        assert!(
            text.contains("tools") && text.contains("initialize"),
            "framing={content_length} response missing: {text}"
        );
    }
}
