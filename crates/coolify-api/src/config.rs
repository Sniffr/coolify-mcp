use crate::{TokenSource, TokenSourceError};
use reqwest::header::HeaderMap;
use std::{collections::HashMap, time::Duration};
use thiserror::Error;
use url::Url;

pub struct CoolifyConfig {
    pub base_url: Url,
    pub token_source: TokenSource,
    pub custom_headers: HeaderMap,
    pub timeout: Duration,
}

impl std::fmt::Debug for CoolifyConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CoolifyConfig")
            .field("base_url", &self.base_url)
            .field("token_source", &self.token_source)
            .field("custom_headers", &self.custom_headers)
            .field("timeout", &self.timeout)
            .finish()
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("missing COOLIFY_BASE_URL or COOLIFY_URL")]
    MissingUrl,
    #[error("invalid Coolify base URL")]
    InvalidUrl,
    #[error("Coolify base URL must use http or https")]
    InvalidScheme,
    #[error("missing COOLIFY_ACCESS_TOKEN or COOLIFY_TOKEN")]
    MissingToken,
    #[error("token configuration error")]
    Token(#[source] TokenSourceError),
}

pub fn config_from_env(
    env: &HashMap<String, String>,
    _http_mode: bool,
) -> Result<CoolifyConfig, ConfigError> {
    let raw = env
        .get("COOLIFY_BASE_URL")
        .filter(|v| !v.trim().is_empty())
        .or_else(|| env.get("COOLIFY_URL"))
        .ok_or(ConfigError::MissingUrl)?;
    let mut base_url = Url::parse(raw).map_err(|_| ConfigError::InvalidUrl)?;
    if !matches!(base_url.scheme(), "http" | "https") {
        return Err(ConfigError::InvalidScheme);
    }
    while base_url.path().ends_with('/') && base_url.path() != "/" {
        let p = base_url.path().trim_end_matches('/').to_string();
        base_url.set_path(&p);
    }
    let token_source = TokenSource::from_env(env).map_err(|e| match e {
        TokenSourceError::Missing => ConfigError::MissingToken,
        other => ConfigError::Token(other),
    })?;
    Ok(CoolifyConfig {
        base_url,
        token_source,
        custom_headers: HeaderMap::new(),
        timeout: Duration::from_secs(45),
    })
}
