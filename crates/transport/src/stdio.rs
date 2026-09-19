use mcp_tools::McpApplication;
use serde_json::{Value, json};
use std::io::{self, Write};
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};

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

fn parse_message(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).unwrap_or_else(|_| json!({"jsonrpc":"2.0","id":Value::Null,"error":{"code":-32700,"message":"Parse error"}}))
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
                out.push(parse_message(line.strip_suffix(b"\r").unwrap_or(line)));
            }
        }
        FrameMode::ContentLength => {
            let mut rest = input;
            while !rest.is_empty() {
                let Some(pos) = rest.windows(4).position(|w| w == b"\r\n\r\n") else {
                    return Err(StdioError::Framing);
                };
                let len = rest[..pos]
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
                out.push(parse_message(&rest[..len]));
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
    run_stdio_with(app, tokio::io::stdin(), tokio::io::stdout()).await
}
pub async fn run_stdio_with<A, R, W>(app: A, reader: R, mut writer: W) -> Result<(), StdioError>
where
    A: McpApplication,
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut reader = BufReader::new(reader);
    let mode = if reader.fill_buf().await?.starts_with(b"Content-Length:")
        || reader.fill_buf().await?.starts_with(b"content-length:")
    {
        FrameMode::ContentLength
    } else {
        FrameMode::Ndjson
    };
    loop {
        let message = match mode {
            FrameMode::Ndjson => {
                let mut line = String::new();
                if reader.read_line(&mut line).await? == 0 {
                    break;
                }
                if line.trim().is_empty() {
                    continue;
                }
                parse_message(line.as_bytes())
            }
            FrameMode::ContentLength => {
                let mut headers = String::new();
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).await? == 0 {
                        return Ok(());
                    }
                    if line == "\r\n" {
                        break;
                    }
                    headers.push_str(&line);
                }
                let len = headers
                    .lines()
                    .find_map(|line| {
                        line.strip_prefix("Content-Length:")
                            .or_else(|| line.strip_prefix("content-length:"))
                            .and_then(|v| v.trim().parse().ok())
                    })
                    .ok_or(StdioError::Framing)?;
                let mut body = vec![0; len];
                reader.read_exact(&mut body).await?;
                parse_message(&body)
            }
        };
        if message.get("id").is_none() {
            continue;
        }
        let id = message["id"].clone();
        let response = match message.get("error") {
            Some(error) => json!({"jsonrpc":"2.0","id":id,"error":error}),
            None => dispatch(&app, &message).await,
        };
        writer
            .write_all(encode_frame(mode, &response).as_bytes())
            .await?;
        writer.flush().await?;
    }
    Ok(())
}
async fn dispatch<A: McpApplication>(app: &A, message: &Value) -> Value {
    let id = message["id"].clone();
    match message.get("method").and_then(Value::as_str) {
        Some("initialize") => {
            json!({"jsonrpc":"2.0","id":id,"result":{"protocolVersion":"2025-03-26","capabilities":{"tools":{}},"serverInfo":{"name":"coolify-mcp","version":env!("CARGO_PKG_VERSION")}}})
        }
        Some("tools/list") => json!({"jsonrpc":"2.0","id":id,"result":{"tools":app.tools()}}),
        Some("tools/call") => {
            let name = message
                .pointer("/params/name")
                .and_then(Value::as_str)
                .unwrap_or("");
            let args = message
                .pointer("/params/arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let r = app.call(name, args).await;
            json!({"jsonrpc":"2.0","id":id,"result":{"content":[{"type":"text","text":r.text}],"isError":r.is_error}})
        }
        Some(_) => {
            json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"Method not found"}})
        }
        None => {
            json!({"jsonrpc":"2.0","id":id,"error":{"code":-32600,"message":"Invalid Request"}})
        }
    }
}
pub fn write_diagnostic(message: &str) {
    let _ = writeln!(io::stderr(), "{message}");
}
