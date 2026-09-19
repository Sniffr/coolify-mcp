use rand::Rng;
use regex::Regex;

pub fn frame_untrusted(text: &str, nonce: &str) -> String {
    let nonce = if nonce.is_empty() {
        let value: u128 = rand::rng().random();
        format!("{value:032x}")
    } else {
        nonce.to_owned()
    };
    let begin = format!("[BEGIN UNTRUSTED LOG OUTPUT:{nonce}]");
    let end = format!("[END UNTRUSTED LOG OUTPUT:{nonce}]");
    let boundary =
        Regex::new(r"(?i)\[\s*(?:begin|end)\s+untrusted\s+log\s+output(?:\s*:\s*[^\]]+)?\s*\]")
            .expect("valid boundary regex");
    let safe = boundary.replace_all(text, "[UNTRUSTED-BOUNDARY-REDACTED]");
    format!("{begin}\n{safe}\n{end}")
}
