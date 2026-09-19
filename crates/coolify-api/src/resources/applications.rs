use crate::{ApplicationSummary, CoolifyApiError, CoolifyClient};
use reqwest::Method;
use serde_json::Value;
impl CoolifyClient {
    pub async fn list_applications(
        &self,
        page: u32,
        per_page: u32,
    ) -> Result<Vec<ApplicationSummary>, CoolifyApiError> {
        self.request_json(
            Method::GET,
            &format!("/applications?page={page}&per_page={per_page}"),
            None,
        )
        .await
    }
    pub async fn get_application(&self, uuid: &str) -> Result<ApplicationSummary, CoolifyApiError> {
        self.request_json(
            Method::GET,
            &format!("/applications/{}", crate::encode_segment(uuid)),
            None,
        )
        .await
    }
    pub async fn create_application(
        &self,
        kind: &str,
        body: Value,
    ) -> Result<crate::ActionResult, CoolifyApiError> {
        self.request_json(
            Method::POST,
            &format!("/applications/{}", crate::encode_segment(kind)),
            Some(body),
        )
        .await
    }
    pub async fn update_application(
        &self,
        uuid: &str,
        body: Value,
    ) -> Result<crate::ActionResult, CoolifyApiError> {
        self.request_json(
            Method::PATCH,
            &format!("/applications/{}", crate::encode_segment(uuid)),
            Some(body),
        )
        .await
    }
    pub async fn delete_application(
        &self,
        uuid: &str,
    ) -> Result<crate::ActionResult, CoolifyApiError> {
        self.request_json(
            Method::DELETE,
            &format!("/applications/{}", crate::encode_segment(uuid)),
            Some(serde_json::json!({"delete_volumes":false})),
        )
        .await
    }
    pub async fn application_logs(
        &self,
        uuid: &str,
        lines: u32,
    ) -> Result<String, CoolifyApiError> {
        let v: Value = self
            .request_json(
                Method::GET,
                &format!(
                    "/applications/{}/logs?lines={lines}",
                    crate::encode_segment(uuid)
                ),
                None,
            )
            .await?;
        Ok(crate::unwrap_logs(v))
    }
}
