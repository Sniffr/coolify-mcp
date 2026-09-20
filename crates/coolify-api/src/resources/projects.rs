use crate::{CoolifyApiError, CoolifyClient, ProjectSummary};
use reqwest::Method;
use serde_json::Value;
impl CoolifyClient {
    pub async fn list_projects(&self) -> Result<Vec<ProjectSummary>, CoolifyApiError> {
        self.request_list(Method::GET, "/projects", None).await
    }
    pub async fn get_project(&self, uuid: &str) -> Result<ProjectSummary, CoolifyApiError> {
        self.request_json(
            Method::GET,
            &format!("/projects/{}", crate::encode_segment(uuid)),
            None,
        )
        .await
    }
    pub async fn create_project(&self, body: Value) -> Result<ProjectSummary, CoolifyApiError> {
        self.request_json(Method::POST, "/projects", Some(body))
            .await
    }
    pub async fn update_project(
        &self,
        uuid: &str,
        body: Value,
    ) -> Result<ProjectSummary, CoolifyApiError> {
        self.request_json(
            Method::PATCH,
            &format!("/projects/{}", crate::encode_segment(uuid)),
            Some(body),
        )
        .await
    }
    pub async fn delete_project(&self, uuid: &str) -> Result<crate::ActionResult, CoolifyApiError> {
        self.request_json(
            Method::DELETE,
            &format!("/projects/{}", crate::encode_segment(uuid)),
            Some(serde_json::json!({"delete_volumes":false})),
        )
        .await
    }
}
