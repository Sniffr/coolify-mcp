use std::fmt;
use thiserror::Error;

pub const MAX_BODY_BYTES: usize = 10_000;

#[derive(Clone)]
pub struct HttpErrorDetails {
    pub status: u16,
    pub body: String,
    pub retry_after: Option<String>,
}
impl HttpErrorDetails {
    pub fn new(status: u16, body: String, retry_after: Option<String>) -> Self {
        Self {
            status,
            body: body.chars().take(MAX_BODY_BYTES).collect(),
            retry_after,
        }
    }
}
impl fmt::Debug for HttpErrorDetails {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HttpErrorDetails")
            .field("status", &self.status)
            .field("body", &"[redacted]")
            .field("retry_after", &self.retry_after)
            .finish()
    }
}

#[derive(Error)]
pub enum CoolifyApiError {
    #[error("configuration error: {0}")]
    Config(String),
    #[error("transport error: {0}")]
    Transport(String),
    #[error("HTTP {status}: {body}{retry}", retry = retry_suffix(.retry_after))]
    Http {
        status: u16,
        body: String,
        retry_after: Option<String>,
    },
    #[error("response decode error: {0}")]
    Decode(String),
}
impl fmt::Debug for CoolifyApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(message) => f.debug_tuple("Config").field(message).finish(),
            Self::Transport(message) => f.debug_tuple("Transport").field(message).finish(),
            Self::Http {
                status,
                retry_after,
                ..
            } => f
                .debug_struct("Http")
                .field("status", status)
                .field("body", &"[redacted]")
                .field("retry_after", retry_after)
                .finish(),
            Self::Decode(message) => f.debug_tuple("Decode").field(message).finish(),
        }
    }
}
fn retry_suffix(value: &Option<String>) -> String {
    value
        .as_ref()
        .map(|v| format!(" (Retry-After: {v})"))
        .unwrap_or_default()
}
impl CoolifyApiError {
    pub fn config(message: impl Into<String>) -> Self {
        Self::Config(message.into())
    }
    pub fn http(details: HttpErrorDetails) -> Self {
        Self::Http {
            status: details.status,
            body: details.body,
            retry_after: details.retry_after,
        }
    }
    pub fn retry_after(&self) -> Option<&str> {
        match self {
            Self::Http { retry_after, .. } => retry_after.as_deref(),
            _ => None,
        }
    }
}
