use rand::Rng;
use regex::Regex;
use std::sync::OnceLock;

pub fn frame_untrusted(text: &str, supplied_nonce: &str) -> String {
    let nonce = if !supplied_nonce.is_empty()
        && supplied_nonce
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        supplied_nonce.to_owned()
    } else {
        let value: u128 = rand::rng().random();
        format!("{value:032x}")
    };
    let begin = format!("[BEGIN UNTRUSTED LOG OUTPUT:{nonce}]");
    let end = format!("[END UNTRUSTED LOG OUTPUT:{nonce}]");
    static BOUNDARY: OnceLock<Regex> = OnceLock::new();
    let boundary = BOUNDARY.get_or_init(|| {
        Regex::new(r"(?i)\[\s*(?:begin|end)\s+untrusted\s+log\s+output(?:\s*:\s*[^\]]+)?\s*\]")
            .expect("valid boundary regex")
    });
    let safe = boundary.replace_all(text, "[UNTRUSTED-BOUNDARY-REDACTED]");
    format!("{begin}\n{safe}\n{end}")
}
