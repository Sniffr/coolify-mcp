use crate::{CoolifyApiError, CoolifyClient};
use reqwest::Method;
impl CoolifyClient {
    pub async fn environment_variables(
        &self,
        kind: &str,
        uuid: &str,
    ) -> Result<Vec<crate::BoundedPayload>, CoolifyApiError> {
        self.request_json(Method::GET, &format!("/{kind}/{uuid}/envs"), None)
            .await
    }
    pub async fn system(&self) -> Result<crate::BoundedPayload, CoolifyApiError> {
        self.request_json(Method::GET, "/system", None).await
    }
}
