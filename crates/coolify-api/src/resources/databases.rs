use crate::{CoolifyApiError, CoolifyClient, DatabaseSummary};
use reqwest::Method;
use serde_json::Value;
impl CoolifyClient {
    pub async fn list_databases(&self) -> Result<Vec<DatabaseSummary>, CoolifyApiError> {
        self.request_list(Method::GET, "/databases", None).await
    }
    pub async fn get_database(&self, uuid: &str) -> Result<DatabaseSummary, CoolifyApiError> {
        self.request_json(
            Method::GET,
            &format!("/databases/{}", crate::encode_segment(uuid)),
            None,
        )
        .await
    }
    pub async fn create_database(
        &self,
        kind: &str,
        body: Value,
    ) -> Result<crate::ActionResult, CoolifyApiError> {
        self.request_json(
            Method::POST,
            &format!("/databases/{}", crate::encode_segment(kind)),
            Some(body),
        )
        .await
    }
    pub async fn database_logs(&self, uuid: &str) -> Result<String, CoolifyApiError> {
        self.request_json::<Value>(
            Method::GET,
            &format!("/databases/{}/logs", crate::encode_segment(uuid)),
            None,
        )
        .await
        .map(crate::unwrap_logs)
    }
}
