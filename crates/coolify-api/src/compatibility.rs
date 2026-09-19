use crate::{CoolifyApiError, CoolifyClient};
use reqwest::Method;
use serde::de::DeserializeOwned;
use serde_json::Value;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LegacyEndpoint {
    ServersValidate,
    ApiEnable,
    ApiDisable,
}
impl LegacyEndpoint {
    fn key(self) -> &'static str {
        match self {
            Self::ServersValidate => "servers.validate",
            Self::ApiEnable => "api.enable",
            Self::ApiDisable => "api.disable",
        }
    }
}
pub(crate) async fn post_fallback<T: DeserializeOwned>(
    client: &CoolifyClient,
    key: LegacyEndpoint,
    path: &str,
    body: Option<Value>,
) -> Result<T, CoolifyApiError> {
    let cached = client
        .legacy_methods
        .lock()
        .expect("legacy lock")
        .get(key.key())
        .copied();
    if cached == Some(true) {
        match client.request_json(Method::GET, path, None).await {
            Ok(v) => return Ok(v),
            Err(e) if e.is_method_or_routing(path) => {
                client
                    .legacy_methods
                    .lock()
                    .expect("legacy lock")
                    .remove(key.key());
            }
            Err(e) => return Err(e),
        }
    }
    match client.request_json(Method::POST, path, body.clone()).await {
        Ok(v) => Ok(v),
        Err(post_err) if post_err.is_method_or_routing(path) => {
            match client.request_json(Method::GET, path, None).await {
                Ok(v) => {
                    client
                        .legacy_methods
                        .lock()
                        .expect("legacy lock")
                        .insert(key.key().into(), true);
                    Ok(v)
                }
                Err(get_err) => Err(get_err),
            }
        }
        Err(e) => Err(e),
    }
}
pub fn error_hint(status: u16, path: &str) -> Option<&'static str> {
    if status == 500 && path.contains("scheduled") {
        Some(
            "scheduled task commands longer than 255 characters are rejected by some Coolify versions",
        )
    } else if status == 405 {
        Some(
            "this endpoint changed method between Coolify versions; check the installed API version",
        )
    } else if matches!(status, 401 | 403) {
        Some("check token scopes and team member permissions")
    } else if status == 404
        && (path.contains("destinations") || path.contains("/tags") || path.contains("/move"))
    {
        Some("this route requires a newer Coolify version")
    } else {
        None
    }
}
pub fn error_hint_with_body(status: u16, path: &str, body: &str) -> Option<&'static str> {
    if status == 500 && (body.contains("scheduled") || body.contains("255")) {
        Some(
            "scheduled task commands longer than 255 characters are rejected by some Coolify versions",
        )
    } else if status == 404 && (body.contains("Resource not found") || body.contains("UUID")) {
        Some("verify the resource UUID belongs to this Coolify instance and team")
    } else {
        error_hint(status, path)
    }
}
