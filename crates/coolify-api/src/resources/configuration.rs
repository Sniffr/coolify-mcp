use crate::{CoolifyApiError, CoolifyClient};
use reqwest::Method;
impl CoolifyClient {
    pub async fn environment_variables(
        &self,
        kind: &str,
        uuid: &str,
    ) -> Result<Vec<crate::EnvironmentVariable>, CoolifyApiError> {
        self.request_json(
            Method::GET,
            &format!(
                "/{}/{}/envs",
                crate::encode_segment(kind),
                crate::encode_segment(uuid)
            ),
            None,
        )
        .await
    }
    pub async fn system(&self) -> Result<crate::SystemSummary, CoolifyApiError> {
        self.request_json(Method::GET, "/system", None).await
    }
}
