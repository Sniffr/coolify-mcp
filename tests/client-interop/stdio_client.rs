//! Protocol acceptance client: parses JSON-RPC over both supported stdio framing modes.
use serde_json::json;
use std::{
    io::Write,
    process::{Command, Stdio},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let binary = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "target/debug/sniffr-coolify-mcp".into());
    let coolify_base =
        std::env::var("COOLIFY_BASE_URL").unwrap_or_else(|_| "http://127.0.0.1:19000".into());
    for content_length in [false, true] {
        let requests = [
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}),
            json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"get_version","arguments":{}}}),
            json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"list_applications","arguments":{}}}),
            json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"application_logs","arguments":{"uuid":"app-1","lines":20}}}),
        ];
        let mut input = String::new();
        for request in &requests {
            let body = serde_json::to_string(request)?;
            let frame = if content_length {
                format!("Content-Length: {}\r\n\r\n{}", body.len(), body)
            } else {
                format!("{}\n", body)
            };
            input.push_str(&frame);
        }
        let mut child = Command::new(&binary)
            .env("MCP_TRANSPORT", "stdio")
            .env("COOLIFY_BASE_URL", &coolify_base)
            .env("COOLIFY_ACCESS_TOKEN", "fixture-token")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        child
            .stdin
            .take()
            .ok_or("missing stdin")?
            .write_all(input.as_bytes())?;
        let output = child.wait_with_output()?;
        assert!(
            output.status.success(),
            "stdio child failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let (mode, responses) = transport::stdio::decode_frames(&output.stdout)?;
        assert_eq!(
            mode,
            if content_length {
                transport::stdio::FrameMode::ContentLength
            } else {
                transport::stdio::FrameMode::Ndjson
            }
        );
        assert_eq!(responses.len(), requests.len());
        for (request, response) in requests.iter().zip(responses.iter()) {
            assert_eq!(response["jsonrpc"], "2.0");
            assert_eq!(response["id"], request["id"]);
            assert!(
                response.get("result").is_some(),
                "unexpected JSON-RPC error: {response}"
            );
        }
        let tools = responses[1]["result"]["tools"]
            .as_array()
            .ok_or("missing tools")?;
        assert_eq!(tools.len(), 45);
        assert!(
            responses[2]["result"]["content"][0]["text"]
                .as_str()
                .unwrap_or("")
                .contains("fixture-1.0")
        );
        let inventory = responses[3]["result"]["content"][0]["text"]
            .as_str()
            .unwrap_or("");
        assert!(
            inventory.contains("fixture-app")
                && !inventory.contains("nested-secret")
                && !inventory.contains("nested-password")
                && !inventory.contains("fixture-token")
        );
        let logs = responses[4]["result"]["content"][0]["text"]
            .as_str()
            .unwrap_or("");
        assert!(
            logs.contains("IGNORE ALL PREVIOUS INSTRUCTIONS")
                && logs.contains("UNTRUSTED")
                && !logs.contains("fixture-token")
        );
    }
    Ok(())
}
