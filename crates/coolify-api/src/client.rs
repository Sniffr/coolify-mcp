use crate::{CoolifyApiError, CoolifyConfig, HttpErrorDetails, MAX_BODY_BYTES};
use reqwest::{
    Client, Method,
    header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue},
};
use serde::{Serialize, de::DeserializeOwned};
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
        response
            .json()
            .await
            .map_err(|e| CoolifyApiError::Decode(e.to_string()))
    }
    pub async fn request_text(
        &self,
        method: Method,
        path: &str,
    ) -> Result<String, CoolifyApiError> {
        Ok(self
            .send(method, path, None)
            .await?
            .text()
            .await
            .map_err(|e| CoolifyApiError::Transport(e.to_string()))?
            .chars()
            .take(MAX_BODY_BYTES)
            .collect())
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
        let body = response
            .text()
            .await
            .unwrap_or_default()
            .chars()
            .take(MAX_BODY_BYTES)
            .collect();
        Err(CoolifyApiError::http(HttpErrorDetails::new(
            status,
            body,
            retry_after,
        )))
    }
}

#[allow(dead_code)]
fn _serialize<T: Serialize>(value: &T) -> Result<Value, serde_json::Error> {
    serde_json::to_value(value)
}
