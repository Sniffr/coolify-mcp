use crate::{CoolifyApiError, CoolifyClient};
use reqwest::Method;
impl CoolifyClient {
    pub async fn diagnose_application(
        &self,
        uuid: &str,
    ) -> Result<crate::DiagnosticSummary, CoolifyApiError> {
        self.request_json(
            Method::GET,
            &format!("/applications/{}/diagnose", crate::encode_segment(uuid)),
            None,
        )
        .await
    }
    pub async fn diagnose_server(
        &self,
        uuid: &str,
    ) -> Result<crate::DiagnosticSummary, CoolifyApiError> {
        self.request_json(
            Method::GET,
            &format!("/servers/{}/diagnose", crate::encode_segment(uuid)),
            None,
        )
        .await
    }
}
