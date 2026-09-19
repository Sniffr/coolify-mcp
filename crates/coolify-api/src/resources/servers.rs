use crate::{CoolifyApiError, CoolifyClient, ServerSummary, ValidationResult};
use reqwest::Method;
impl CoolifyClient {
    pub async fn list_servers(
        &self,
        page: u32,
        per_page: u32,
    ) -> Result<Vec<ServerSummary>, CoolifyApiError> {
        self.request_json(
            Method::GET,
            &format!("/servers?page={page}&per_page={per_page}"),
            None,
        )
        .await
    }
    pub async fn get_server(&self, uuid: &str) -> Result<ServerSummary, CoolifyApiError> {
        self.request_json(
            Method::GET,
            &format!("/servers/{}", crate::encode_segment(uuid)),
            None,
        )
        .await
    }
    pub async fn validate_server(&self, uuid: &str) -> Result<ValidationResult, CoolifyApiError> {
        self.post_with_legacy_get_fallback(
            crate::LegacyEndpoint::ServersValidate,
            &format!("/servers/{}/validate", crate::encode_segment(uuid)),
            None,
        )
        .await
    }
}
