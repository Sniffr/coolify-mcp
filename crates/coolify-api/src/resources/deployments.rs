use crate::{CoolifyApiError, CoolifyClient, DeploymentSummary};
use reqwest::Method;
use serde_json::Value;
use std::time::{Duration, Instant};
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
            &format!(
                "/deployments/applications/{}?skip={skip}&take={take}",
                crate::encode_segment(uuid)
            ),
            None,
        )
        .await
    }
    pub async fn get_deployment(&self, uuid: &str) -> Result<DeploymentSummary, CoolifyApiError> {
        self.request_json(
            Method::GET,
            &format!("/deployments/{}", crate::encode_segment(uuid)),
            None,
        )
        .await
    }
    pub async fn deployment_logs(&self, uuid: &str) -> Result<String, CoolifyApiError> {
        let value: Value = self
            .request_json(
                Method::GET,
                &format!("/deployments/{}/logs", crate::encode_segment(uuid)),
                None,
            )
            .await?;
        Ok(crate::unwrap_logs(value))
    }
    pub async fn trigger_deployment(
        &self,
        body: Value,
    ) -> Result<DeploymentSummary, CoolifyApiError> {
        self.request_json(Method::POST, "/deploy", Some(body)).await
    }
    pub async fn poll_deployment(
        &self,
        uuid: &str,
        interval: Duration,
        timeout: Duration,
    ) -> Result<DeploymentSummary, CoolifyApiError> {
        let interval = interval.max(Duration::from_millis(1));
        let deadline = Instant::now() + timeout.min(Duration::from_secs(300));
        loop {
            let deployment = self.get_deployment(uuid).await?;
            if matches!(
                deployment.status.to_ascii_lowercase().as_str(),
                "finished" | "failed" | "cancelled"
            ) || Instant::now() >= deadline
            {
                return Ok(deployment);
            }
            tokio::time::sleep(interval).await;
        }
    }
    pub async fn cancel_deployment(
        &self,
        uuid: &str,
    ) -> Result<crate::ActionResult, CoolifyApiError> {
        self.request_json(
            Method::POST,
            &format!("/deployments/{}/cancel", crate::encode_segment(uuid)),
            None,
        )
        .await
    }
}
