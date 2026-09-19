use crate::{CoolifyApiError, CoolifyClient};
use reqwest::Method;
use serde_json::Value;
impl CoolifyClient {
    pub async fn environment_variables(
        &self,
        kind: &str,
        uuid: &str,
    ) -> Result<Vec<Value>, CoolifyApiError> {
        self.request_json(Method::GET, &format!("/{kind}/{uuid}/envs"), None)
            .await
    }
    pub async fn system(&self) -> Result<Value, CoolifyApiError> {
        self.request_json(Method::GET, "/system", None).await
    }
}
