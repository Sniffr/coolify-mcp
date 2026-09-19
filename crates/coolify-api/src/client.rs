use crate::{CoolifyApiError, CoolifyConfig, HttpErrorDetails, MAX_BODY_BYTES};
use reqwest::{
    Client, Method,
    header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue},
};
use serde::de::DeserializeOwned;
use serde_json::Value;

pub struct CoolifyClient {
    client: Client,
    config: CoolifyConfig,
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
        let client = Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(|e| CoolifyApiError::Transport(e.to_string()))?;
        Ok(Self { client, config })
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
            return Err(CoolifyApiError::Decode("response was not JSON".into()));
        }
        let bytes = read_bounded(response)
            .await
            .map_err(|e| CoolifyApiError::Decode(e.to_string()))?;
        let value: Value =
            serde_json::from_slice(&bytes).map_err(|e| CoolifyApiError::Decode(e.to_string()))?;
        let sanitized = safety::sanitize_json(&value, false);
        serde_json::from_value(sanitized).map_err(|e| CoolifyApiError::Decode(e.to_string()))
    }
    pub async fn request_text(
        &self,
        method: Method,
        path: &str,
    ) -> Result<String, CoolifyApiError> {
        let bytes = read_bounded(self.send(method, path, None).await?)
            .await
            .map_err(|e| CoolifyApiError::Transport(e.to_string()))?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }
    pub async fn get_version(&self) -> Result<String, CoolifyApiError> {
        self.request_text(Method::GET, "/version")
            .await
            .map(|v| v.trim().to_owned())
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
        Err(CoolifyApiError::http(
            HttpErrorDetails::new(status, body, retry_after).redact_token(&token),
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
