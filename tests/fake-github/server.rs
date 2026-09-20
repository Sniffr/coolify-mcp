//! Deterministic, local-only GitHub OAuth fixture. It never prints request bodies or tokens.
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
};

const FIXTURE_TOKEN: &str = "github-fixture-token";

#[derive(Clone, Default)]
struct Counters {
    requests: Arc<AtomicUsize>,
    token_exchanges: Arc<AtomicUsize>,
    user_fetches: Arc<AtomicUsize>,
    unauthorized: Arc<AtomicUsize>,
    invalid_codes: Arc<AtomicUsize>,
}

fn response(status: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn read_request(stream: &mut TcpStream) -> String {
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 4096];
    let mut body_length = None;
    loop {
        let read = stream.read(&mut buffer).unwrap_or(0);
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..read]);
        if body_length.is_none()
            && let Some(header_end) = bytes.windows(4).position(|window| window == b"\r\n\r\n")
        {
            let headers = String::from_utf8_lossy(&bytes[..header_end]);
            body_length = headers.lines().find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().unwrap_or(0))
            });
        }
        if let Some(header_end) = bytes.windows(4).position(|window| window == b"\r\n\r\n")
            && bytes.len() >= header_end + 4 + body_length.unwrap_or(0)
        {
            break;
        }
        if bytes.len() >= 64 * 1024 {
            break;
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

fn body(request: &str) -> &str {
    request
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .unwrap_or_default()
}

fn parameter(request_body: &str, wanted: &str) -> Option<String> {
    request_body.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == wanted).then(|| decode(value))
    })
}

fn decode(value: &str) -> String {
    let value = value.replace('+', " ");
    let bytes = value.as_bytes();
    let mut output = String::with_capacity(value.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let (Ok(high), Ok(low)) = (
                u8::from_str_radix(&value[index + 1..index + 2], 16),
                u8::from_str_radix(&value[index + 2..index + 3], 16),
            )
        {
            output.push((high * 16 + low) as char);
            index += 3;
        } else {
            output.push(bytes[index] as char);
            index += 1;
        }
    }
    output
}

fn handle(mut stream: TcpStream, counters: Counters) {
    let request = read_request(&mut stream);
    let mut first_line = request
        .lines()
        .next()
        .unwrap_or_default()
        .split_whitespace();
    let method = first_line.next().unwrap_or_default();
    let target = first_line.next().unwrap_or_default();

    if target == "/__fixture__/stats" {
        let body = format!(
            r#"{{"requests":{},"token_exchanges":{},"user_fetches":{},"unauthorized":{},"invalid_codes":{}}}"#,
            counters.requests.load(Ordering::Relaxed),
            counters.token_exchanges.load(Ordering::Relaxed),
            counters.user_fetches.load(Ordering::Relaxed),
            counters.unauthorized.load(Ordering::Relaxed),
            counters.invalid_codes.load(Ordering::Relaxed),
        );
        let _ = stream.write_all(response("200 OK", &body).as_bytes());
        return;
    }

    counters.requests.fetch_add(1, Ordering::Relaxed);
    let response = match (method, target) {
        ("POST", "/login/oauth/access_token") => {
            counters.token_exchanges.fetch_add(1, Ordering::Relaxed);
            match parameter(body(&request), "code").as_deref() {
                Some("fixture-code-a") | Some("fixture-code-b") => response(
                    "200 OK",
                    r#"{"access_token":"github-fixture-token","token_type":"bearer","scope":"read:user user:email"}"#,
                ),
                _ => {
                    counters.invalid_codes.fetch_add(1, Ordering::Relaxed);
                    response("400 Bad Request", r#"{"error":"bad_verification_code"}"#)
                }
            }
        }
        ("GET", "/user") => {
            counters.user_fetches.fetch_add(1, Ordering::Relaxed);
            let authorized = request.lines().any(|line| {
                line.eq_ignore_ascii_case(&format!("Authorization: Bearer {FIXTURE_TOKEN}"))
            });
            if !authorized {
                counters.unauthorized.fetch_add(1, Ordering::Relaxed);
                response(
                    "401 Unauthorized",
                    r#"{"message":"Requires authentication"}"#,
                )
            } else {
                let user_number = counters.user_fetches.load(Ordering::Relaxed);
                if user_number % 2 == 1 {
                    response(
                        "200 OK",
                        r#"{"id":1001,"login":"fixture-user-a","name":"Fixture User A"}"#,
                    )
                } else {
                    response(
                        "200 OK",
                        r#"{"id":1002,"login":"fixture-user-b","name":"Fixture User B"}"#,
                    )
                }
            }
        }
        _ => response("404 Not Found", r#"{"message":"Not Found"}"#),
    };
    let _ = stream.write_all(response.as_bytes());
}

fn main() {
    let address = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:19001".to_owned());
    let listener = TcpListener::bind(address).expect("bind fake GitHub fixture");
    let counters = Counters::default();
    eprintln!("fake GitHub fixture listening");
    for stream in listener.incoming().flatten() {
        let counters = counters.clone();
        thread::spawn(move || handle(stream, counters));
    }
}
