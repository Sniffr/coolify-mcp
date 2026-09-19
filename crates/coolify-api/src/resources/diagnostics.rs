use crate::{CoolifyApiError, CoolifyClient};
use reqwest::Method;
use serde_json::Value;
impl CoolifyClient {
    pub async fn diagnose_application(&self, uuid: &str) -> Result<Value, CoolifyApiError> {
        self.request_json(Method::GET, &format!("/applications/{uuid}/diagnose"), None)
            .await
    }
    pub async fn diagnose_server(&self, uuid: &str) -> Result<Value, CoolifyApiError> {
        self.request_json(Method::GET, &format!("/servers/{uuid}/diagnose"), None)
            .await
    }
}
