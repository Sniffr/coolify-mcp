use crate::{CoolifyApiError, CoolifyClient, ServiceSummary};
use reqwest::Method;
use serde_json::Value;
impl CoolifyClient {
    pub async fn list_services(&self) -> Result<Vec<ServiceSummary>, CoolifyApiError> {
        self.request_json(Method::GET, "/services", None).await
    }
    pub async fn get_service(&self, uuid: &str) -> Result<ServiceSummary, CoolifyApiError> {
        self.request_json(Method::GET, &format!("/services/{uuid}"), None)
            .await
    }
    pub async fn create_service(&self, body: Value) -> Result<ServiceSummary, CoolifyApiError> {
        self.request_json(Method::POST, "/services", Some(body))
            .await
    }
}
