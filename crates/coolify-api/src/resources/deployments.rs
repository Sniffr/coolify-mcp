use crate::{CoolifyApiError, CoolifyClient, DeploymentSummary};
use reqwest::Method;
use serde_json::Value;
impl CoolifyClient {
    pub async fn list_deployments(&self) -> Result<Vec<DeploymentSummary>, CoolifyApiError> {
        self.request_json(Method::GET, "/deployments", None).await
    }
    pub async fn list_application_deployments(
        &self,
        uuid: &str,
        skip: u32,
        take: u32,
    ) -> Result<Vec<DeploymentSummary>, CoolifyApiError> {
        self.request_json(
            Method::GET,
            &format!("/deployments/applications/{uuid}?skip={skip}&take={take}"),
            None,
        )
        .await
    }
    pub async fn get_deployment(&self, uuid: &str) -> Result<DeploymentSummary, CoolifyApiError> {
        self.request_json(Method::GET, &format!("/deployments/{uuid}"), None)
            .await
    }
    pub async fn deployment_logs(&self, uuid: &str) -> Result<String, CoolifyApiError> {
        let value: Value = self
            .request_json(Method::GET, &format!("/deployments/{uuid}/logs"), None)
            .await?;
        Ok(crate::unwrap_logs(value))
    }
    pub async fn trigger_deployment(
        &self,
        body: Value,
    ) -> Result<DeploymentSummary, CoolifyApiError> {
        self.request_json(Method::POST, "/deploy", Some(body)).await
    }
    pub async fn cancel_deployment(&self, uuid: &str) -> Result<Value, CoolifyApiError> {
        self.request_json(Method::POST, &format!("/deployments/{uuid}/cancel"), None)
            .await
    }
}
