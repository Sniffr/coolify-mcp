use std::fmt;

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
            body: truncate_utf8(body, MAX_BODY_BYTES),
            retry_after,
        }
    }

    pub(crate) fn redact_token(mut self, token: &str) -> Self {
        if !token.is_empty() {
            self.body = self.body.replace(token, "[redacted]");
        }
        self
    }
}

fn truncate_utf8(value: String, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value;
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
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

pub enum CoolifyApiError {
    Config(String),
    Transport(String),
    Http {
        status: u16,
        body: String,
        retry_after: Option<String>,
    },
    Decode(String),
}

impl fmt::Display for CoolifyApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(_) => f.write_str("configuration error"),
            Self::Transport(_) => f.write_str("transport error"),
            Self::Http {
                status,
                retry_after,
                ..
            } => {
                write!(f, "HTTP {status}")?;
                if let Some(retry_after) = retry_after {
                    write!(f, " (Retry-After: {retry_after})")?;
                }
                Ok(())
            }
            Self::Decode(_) => f.write_str("response decode error"),
        }
    }
}
impl std::error::Error for CoolifyApiError {}
impl fmt::Debug for CoolifyApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(_) => f.debug_tuple("Config").field(&"[redacted]").finish(),
            Self::Transport(_) => f.debug_tuple("Transport").field(&"[redacted]").finish(),
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
            Self::Decode(_) => f.debug_tuple("Decode").field(&"[redacted]").finish(),
        }
    }
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
    pub(crate) fn is_method_or_routing(&self, _path: &str) -> bool {
        match self {
            Self::Http { status, body, .. } => {
                *status == 405 || crate::api_shape::is_routing_catch_all(*status, body)
            }
            _ => false,
        }
    }
    pub fn hint(&self) -> Option<&'static str> {
        match self {
            Self::Http { status, body, .. } => crate::error_hint_with_body(*status, "", body),
            _ => None,
        }
    }
    pub fn status(&self) -> Option<u16> {
        match self {
            Self::Http { status, .. } => Some(*status),
            _ => None,
        }
    }
    pub fn retry_after(&self) -> Option<&str> {
        match self {
            Self::Http { retry_after, .. } => retry_after.as_deref(),
            _ => None,
        }
    }
}
