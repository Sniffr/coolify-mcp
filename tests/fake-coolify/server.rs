//! Deterministic local Coolify fixture for hosted two-user acceptance.
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
};

#[derive(Clone, Default)]
struct Counters {
    requests: Arc<AtomicUsize>,
    unauthorized: Arc<AtomicUsize>,
    unknown_paths: Arc<AtomicUsize>,
    calls: Arc<Mutex<Vec<String>>>,
    poisoned: Arc<AtomicUsize>,
}
fn response(status: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}
fn escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}
fn decode(value: &str) -> String {
    let mut out = String::new();
    let bytes = value.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Ok(a), Ok(b)) = (
                u8::from_str_radix(&value[i + 1..i + 2], 16),
                u8::from_str_radix(&value[i + 2..i + 3], 16),
            )
        {
            out.push((a * 16 + b) as char);
            i += 3;
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}
fn handle(mut stream: std::net::TcpStream, c: Counters) {
    let mut buf = [0u8; 32768];
    let n = stream.read(&mut buf).unwrap_or(0);
    let request = String::from_utf8_lossy(&buf[..n]);
    let mut parts = request.lines().next().unwrap_or("").split_whitespace();
    let method = parts.next().unwrap_or("?");
    let target = parts.next().unwrap_or("/");
    if target == "/__fixture__/stats" {
        let calls = c.calls.lock().unwrap();
        let entries = calls
            .iter()
            .map(|x| format!("\"{}\"", escape(x)))
            .collect::<Vec<_>>()
            .join(",");
        let body = format!(
            r#"{{"requests":{},"unauthorized":{},"unknown_paths":{},"poisoned_logs":{},"calls":[{}]}}"#,
            c.requests.load(Ordering::Relaxed),
            c.unauthorized.load(Ordering::Relaxed),
            c.unknown_paths.load(Ordering::Relaxed),
            c.poisoned.load(Ordering::Relaxed),
            entries
        );
        let _ = stream.write_all(response("200 OK", &body).as_bytes());
        return;
    }
    c.requests.fetch_add(1, Ordering::Relaxed);
    let decoded = decode(target);
    let auth = request.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        if !name.eq_ignore_ascii_case("authorization") {
            return None;
        }
        match value.trim() {
            "Bearer coolify-fixture-token-a" => Some('a'),
            "Bearer coolify-fixture-token-b" => Some('b'),
            _ => None,
        }
    });
    c.calls
        .lock()
        .unwrap()
        .push(format!("{method} {} user_{}", decoded, auth.unwrap_or('x')));
    let Some(user) = auth else {
        c.unauthorized.fetch_add(1, Ordering::Relaxed);
        let _ = stream
            .write_all(response("401 Unauthorized", r#"{"error":"unauthorized"}"#).as_bytes());
        return;
    };
    let path = decoded.split('?').next().unwrap_or(&decoded);
    let known = path == "/api/v1/version" || path == "/api/v1/applications";
    if !known {
        c.unknown_paths.fetch_add(1, Ordering::Relaxed);
        let _ =
            stream.write_all(response("404 Not Found", r#"{"error":"unknown path"}"#).as_bytes());
        return;
    }
    let body = if path == "/api/v1/version" {
        if user == 'a' {
            r#"{"version":"fixture-a-1.0"}"#
        } else {
            r#"{"version":"fixture-b-1.0"}"#
        }
    } else if user == 'a' {
        r#"[{"uuid":"app-a","name":"fixture-app-a","environment":{"secrets":{"DATABASE_URL":"nested-secret-a","password":"nested-password-a"}}}]"#
    } else {
        r#"[{"uuid":"app-b","name":"fixture-app-b","environment":{"secrets":{"DATABASE_URL":"nested-secret-b","password":"nested-password-b"}}}]"#
    };
    if body.contains("nested") {
        c.poisoned.fetch_add(1, Ordering::Relaxed);
    }
    let _ = stream.write_all(response("200 OK", body).as_bytes());
}
fn main() {
    let address = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:19000".into());
    let listener = TcpListener::bind(address).expect("bind fixture");
    let counters = Counters::default();
    for stream in listener.incoming().flatten() {
        let c = counters.clone();
        thread::spawn(move || handle(stream, c));
    }
}
