use mcp_tools::McpApplication;
use serde_json::{Value, json};
use std::io::{self, Write};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameMode {
    Ndjson,
    ContentLength,
}

#[derive(Debug, Error)]
pub enum StdioError {
    #[error("stdio I/O error")]
    Io(#[from] io::Error),
    #[error("invalid Content-Length framing")]
    Framing,
}

pub fn decode_frames(input: &[u8]) -> Result<(FrameMode, Vec<Value>), StdioError> {
    let mode = if input.starts_with(b"Content-Length:") || input.starts_with(b"content-length:") {
        FrameMode::ContentLength
    } else {
        FrameMode::Ndjson
    };
    let mut out = Vec::new();
    match mode {
        FrameMode::Ndjson => {
            for line in input.split(|b| *b == b'\n').filter(|x| !x.is_empty()) {
                out.push(serde_json::from_slice(line).unwrap_or_else(|_| json!({"jsonrpc":"2.0","id":Value::Null,"error":{"code":-32700,"message":"Parse error"}})));
            }
        }
        FrameMode::ContentLength => {
            let mut rest = input;
            while !rest.is_empty() {
                let Some(pos) = rest.windows(4).position(|w| w == b"\r\n\r\n") else {
                    return Err(StdioError::Framing);
                };
                let headers = &rest[..pos];
                let len = headers
                    .split(|b| *b == b'\n')
                    .find_map(|line| {
                        let line = line.strip_suffix(b"\r").unwrap_or(line);
                        let (name, value) = line.split_at(line.iter().position(|b| *b == b':')?);
                        (name.eq_ignore_ascii_case(b"content-length"))
                            .then(|| std::str::from_utf8(&value[1..]).ok()?.trim().parse().ok())
                    })
                    .flatten()
                    .ok_or(StdioError::Framing)?;
                rest = &rest[pos + 4..];
                if rest.len() < len {
                    return Err(StdioError::Framing);
                }
                out.push(serde_json::from_slice(&rest[..len]).unwrap_or_else(|_| json!({"jsonrpc":"2.0","id":Value::Null,"error":{"code":-32700,"message":"Parse error"}})));
                rest = &rest[len..];
            }
        }
    }
    Ok((mode, out))
}

pub fn encode_frame(mode: FrameMode, value: &Value) -> String {
    let body = serde_json::to_string(value).unwrap_or_else(|_| "{}".into());
    match mode {
        FrameMode::Ndjson => format!("{body}\n"),
        FrameMode::ContentLength => format!("Content-Length: {}\r\n\r\n{body}", body.len()),
    }
}

pub async fn run_stdio<A: McpApplication + 'static>(app: A) -> Result<(), StdioError> {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    run_stdio_with(app, stdin, stdout).await
}

pub async fn run_stdio_with<A, R, W>(app: A, reader: R, mut writer: W) -> Result<(), StdioError>
where
    A: McpApplication,
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut reader = BufReader::new(reader);
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).await?;
    let (mode, messages) = decode_frames(&bytes)?;
    for message in messages {
        if message.get("id").is_none() {
            continue;
        }
        let response = if let Some(method) = message.get("method").and_then(Value::as_str) {
            match method {
                "initialize" => {
                    json!({"jsonrpc":"2.0","id":message["id"],"result":{"protocolVersion":"2025-03-26","capabilities":{"tools":{}},"serverInfo":{"name":"coolify-mcp","version":env!("CARGO_PKG_VERSION")}}})
                }
                "tools/list" => {
                    json!({"jsonrpc":"2.0","id":message["id"],"result":{"tools":app.tools()}})
                }
                "tools/call" => {
                    let name = message
                        .pointer("/params/name")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    let args = message
                        .pointer("/params/arguments")
                        .cloned()
                        .unwrap_or_else(|| json!({}));
                    let result = app.call(name, args).await;
                    json!({"jsonrpc":"2.0","id":message["id"],"result":{"content":[{"type":"text","text":result.text}],"isError":result.is_error}})
                }
                _ => {
                    json!({"jsonrpc":"2.0","id":message["id"],"error":{"code":-32601,"message":"Method not found"}})
                }
            }
        } else {
            json!({"jsonrpc":"2.0","id":message["id"],"error":{"code":-32600,"message":"Invalid Request"}})
        };
        writer
            .write_all(encode_frame(mode, &response).as_bytes())
            .await?;
        writer.flush().await?;
    }
    Ok(())
}

pub fn write_diagnostic(message: &str) {
    let _ = writeln!(io::stderr(), "{message}");
}
