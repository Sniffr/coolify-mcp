use crate::network_policy::resolve_hosted_base_url;
use crate::{CoolifyApiError, CoolifyConfig, HttpErrorDetails, MAX_BODY_BYTES};
use reqwest::{
    Client, Method,
    header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue},
};
use safety::sanitize_text;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;

/// Sanitized metadata from one side-effect-free GET probe. The body is bounded
/// to [`MAX_BODY_BYTES`] with any configured token value redacted; `location`
/// carries the redirect Location header so proxy redirects stay detectable.
#[derive(Clone, Debug)]
pub struct ProbeOutcome {
    pub status: u16,
    pub content_type: Option<String>,
    pub body: String,
    pub redirected: bool,
    pub location: Option<String>,
}

pub struct CoolifyClient {
    pub(crate) client: Client,
    pub(crate) config: CoolifyConfig,
    pub(crate) legacy_methods: Mutex<HashMap<String, bool>>,
}
impl std::fmt::Debug for CoolifyClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CoolifyClient")
            .field("config", &self.config)
            .finish()
    }
}
impl CoolifyClient {
    pub fn new(config: CoolifyConfig) -> Result<Self, CoolifyApiError> {
        Self::build(config, false, false)
    }

    /// Construct a client for hosted tenant traffic. Validation runs before the
    /// token source is read, and redirects are disabled so a response cannot
    /// move bearer credentials to an unvalidated destination.
    pub fn new_hosted(config: CoolifyConfig) -> Result<Self, CoolifyApiError> {
        Self::new_hosted_with_local_escape(config, false)
    }

    /// The escape hatch is intentionally explicit and is only wired by the
    /// debug/test local transport configuration.
    pub fn new_hosted_with_local_escape(
        config: CoolifyConfig,
        allow_insecure_local_targets: bool,
    ) -> Result<Self, CoolifyApiError> {
        Self::build(config, true, allow_insecure_local_targets)
    }

    fn build(
        config: CoolifyConfig,
        hosted: bool,
        allow_insecure_local_targets: bool,
    ) -> Result<Self, CoolifyApiError> {
        let mut builder = Client::builder()
            .timeout(config.timeout)
            .redirect(reqwest::redirect::Policy::none());
        if hosted && !allow_insecure_local_targets {
            let addresses =
                resolve_hosted_base_url(&config.base_url).map_err(CoolifyApiError::Config)?;
            // Keep the validated DNS answer for the lifetime of this client;
            // otherwise request-time resolution permits DNS rebinding.
            builder = builder.resolve_to_addrs(
                config
                    .base_url
                    .host_str()
                    .ok_or_else(|| CoolifyApiError::Config("missing destination host".into()))?,
                &addresses,
            );
        }
        let client = builder
            .build()
            .map_err(|e| CoolifyApiError::Transport(e.to_string()))?;
        Ok(Self {
            client,
            config,
            legacy_methods: Mutex::new(HashMap::new()),
        })
    }
    pub async fn post_with_legacy_get_fallback<T: DeserializeOwned>(
        &self,
        key: crate::LegacyEndpoint,
        path: &str,
        body: Option<Value>,
    ) -> Result<T, CoolifyApiError> {
        crate::compatibility::post_fallback(self, key, path, body).await
    }
    /// Typed escape hatch for endpoint families whose response projection is action-specific.
    /// URL construction, auth, status handling, bounds, and sanitization remain centralized here.
    pub async fn request_value(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<Value, CoolifyApiError> {
        let response = self.send(method, path, body).await?;
        let bytes = read_bounded(response)
            .await
            .map_err(|e| CoolifyApiError::Decode(e.to_string()))?;
        let value: Value =
            serde_json::from_slice(&bytes).map_err(|e| CoolifyApiError::Decode(e.to_string()))?;
        Ok(safety::sanitize_json(&value, false))
    }

    pub async fn request_json<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<T, CoolifyApiError> {
        let response = self.send(method, path, body).await?;
        let is_json = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.to_ascii_lowercase().contains("application/json"))
            .unwrap_or(false);
        if !is_json {
            return Err(CoolifyApiError::Decode(format!(
                "{path}: response was not JSON"
            )));
        }
        let bytes = read_bounded(response)
            .await
            .map_err(|e| CoolifyApiError::Decode(format!("{path}: {e}")))?;
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|e| CoolifyApiError::Decode(format!("{path}: {e}")))?;
        let sanitized = safety::sanitize_json(&value, false);
        serde_json::from_value(sanitized)
            .map_err(|e| CoolifyApiError::Decode(format!("{path}: {e}")))
    }

    /// List helper tolerant to Coolify shape drift. Real servers return a bare
    /// array on some versions and a Laravel-style pagination envelope
    /// (`{"data": [...]}` or `{"data": {"data": [...]}}`) on others. Strict
    /// `Vec<T>` decoding turns the envelope into an opaque decode error, so
    /// normalize to an array first and include the path in failures.
    pub async fn request_list<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<Vec<T>, CoolifyApiError> {
        let value = self.request_value(method, path, body).await?;
        let items = extract_list_array(&value).ok_or_else(|| {
            CoolifyApiError::Decode(format!(
                "{}: expected JSON array or object with data array, got {}",
                path,
                value_kind(&value),
            ))
        })?;
        serde_json::from_value(Value::Array(items))
            .map_err(|e| CoolifyApiError::Decode(format!("{path}: {e}")))
    }
    pub async fn request_text(
        &self,
        method: Method,
        path: &str,
    ) -> Result<String, CoolifyApiError> {
        let bytes = read_bounded(self.send(method, path, None).await?)
            .await
            .map_err(|e| CoolifyApiError::Transport(e.to_string()))?;
        Ok(sanitize_text(&String::from_utf8_lossy(&bytes)))
    }
    pub async fn get_version(&self) -> Result<String, CoolifyApiError> {
        self.request_text(Method::GET, "/version")
            .await
            .map(|v| v.trim().to_owned())
    }
    /// Side-effect-free GET probe that preserves real response metadata.
    /// Automatic redirects are disabled so Cloudflare/proxy redirects surface as
    /// 3xx responses with their Location header instead of being followed.
    /// Every received response (any status) is returned as `Ok`; only transport
    /// failures and timeouts are `Err`, with no secret values included.
    pub async fn probe_get(&self, path: &str) -> Result<ProbeOutcome, String> {
        use reqwest::header::{CONTENT_TYPE, LOCATION};
        let url = self
            .config
            .base_url
            .join("api/v1/")
            .map_err(|e| e.to_string())?
            .join(path.trim_start_matches('/'))
            .map_err(|e| e.to_string())?;
        let token = self
            .config
            .token_source
            .current()
            .map_err(|_| "token unavailable".to_owned())?;
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        let auth = format!("Bearer {token}");
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&auth).map_err(|_| "invalid token".to_owned())?,
        );
        for (key, value) in &self.config.custom_headers {
            if key != AUTHORIZATION && key != CONTENT_TYPE {
                headers.insert(key.clone(), value.clone());
            }
        }
        let response = self
            .client
            .request(Method::GET, url)
            .headers(headers)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    "probe timed out after ten seconds".to_owned()
                } else {
                    "transport error".to_owned()
                }
            })?;
        let status = response.status().as_u16();
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let location = response
            .headers()
            .get(LOCATION)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let redirected = (300..400).contains(&status) || location.is_some();
        let mut bytes = Vec::new();
        let mut response = response;
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| "transport error".to_owned())?
        {
            let remaining = MAX_BODY_BYTES.saturating_sub(bytes.len());
            bytes.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
            if bytes.len() == MAX_BODY_BYTES {
                break;
            }
        }
        let mut body = String::from_utf8_lossy(&bytes).into_owned();
        if !token.is_empty() {
            body = body.replace(&token, "[redacted]");
        }
        Ok(ProbeOutcome {
            status,
            content_type,
            body: sanitize_text(&body),
            redirected,
            location,
        })
    }
    async fn send(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<reqwest::Response, CoolifyApiError> {
        let url = self
            .config
            .base_url
            .join("api/v1/")
            .map_err(|e| CoolifyApiError::Config(e.to_string()))?
            .join(path.trim_start_matches('/'))
            .map_err(|e| CoolifyApiError::Config(e.to_string()))?;
        let token = self
            .config
            .token_source
            .current()
            .map_err(|_| CoolifyApiError::Config("token unavailable".into()))?;
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        let auth = format!("Bearer {token}");
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&auth)
                .map_err(|_| CoolifyApiError::Config("invalid token".into()))?,
        );
        for (key, value) in &self.config.custom_headers {
            if key != AUTHORIZATION && key != CONTENT_TYPE {
                headers.insert(key.clone(), value.clone());
            }
        }
        let request = self.client.request(method, url).headers(headers);
        let response = match body {
            Some(body) => request.json(&body).send().await,
            None => request.send().await,
        }
        .map_err(|e| CoolifyApiError::Transport(e.to_string()))?;
        if response.status().is_success() {
            return Ok(response);
        }
        let status = response.status().as_u16();
        let retry_after = response
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let body = read_bounded(response).await.unwrap_or_default();
        let body = String::from_utf8_lossy(&body).into_owned();
        Err(CoolifyApiError::http_at_path(
            HttpErrorDetails::new(status, body, retry_after).redact_token(&token),
            path,
        ))
    }
}

async fn read_bounded(mut response: reqwest::Response) -> Result<Vec<u8>, reqwest::Error> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        let remaining = MAX_BODY_BYTES.saturating_sub(bytes.len());
        bytes.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
        if bytes.len() == MAX_BODY_BYTES {
            break;
        }
    }
    while !bytes.is_empty() && std::str::from_utf8(&bytes).is_err() {
        bytes.pop();
    }
    Ok(bytes)
}

/// Normalize a list response to its items. Accepts a bare array, an object
/// with a `data` array, or a nested `{"data": {"data": [...]}}` envelope.
fn extract_list_array(value: &Value) -> Option<Vec<Value>> {
    match value {
        Value::Array(items) => Some(items.clone()),
        Value::Object(map) => {
            // Try common envelope keys before giving up.
            for key in ["data", "applications", "servers", "items", "results"] {
                match map.get(key) {
                    Some(Value::Array(items)) => return Some(items.clone()),
                    Some(Value::Object(inner)) => {
                        if let Some(Value::Array(items)) = inner.get("data") {
                            return Some(items.clone());
                        }
                    }
                    _ => {}
                }
            }
            None
        }
        _ => None,
    }
}

fn value_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}
