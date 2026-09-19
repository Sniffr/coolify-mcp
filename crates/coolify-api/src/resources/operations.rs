use crate::{CoolifyApiError, CoolifyClient};
use reqwest::Method;
use serde_json::Value;
impl CoolifyClient {
    pub async fn application_envs(
        &self,
        uuid: &str,
    ) -> Result<Vec<crate::EnvironmentVariable>, CoolifyApiError> {
        self.request_json(
            Method::GET,
            &format!("/applications/{}/envs", crate::encode_segment(uuid)),
            None,
        )
        .await
    }
    pub async fn application_action(
        &self,
        uuid: &str,
        action: &str,
        body: Option<Value>,
    ) -> Result<crate::ActionResult, CoolifyApiError> {
        self.request_json(
            Method::POST,
            &format!(
                "/applications/{}/{}",
                crate::encode_segment(uuid),
                crate::encode_segment(action)
            ),
            body,
        )
        .await
    }
    pub async fn application_storage(
        &self,
        uuid: &str,
        storage_uuid: Option<&str>,
        method: Method,
        body: Option<Value>,
    ) -> Result<crate::ActionResult, CoolifyApiError> {
        let path = format!(
            "/applications/{}/storages{}",
            crate::encode_segment(uuid),
            storage_uuid
                .map(|x| format!("/{}", crate::encode_segment(x)))
                .unwrap_or_default()
        );
        self.request_json(method, &path, body).await
    }
    pub async fn application_tags(
        &self,
        uuid: &str,
        tag_uuid: Option<&str>,
        method: Method,
        body: Option<Value>,
    ) -> Result<crate::ActionResult, CoolifyApiError> {
        let path = format!(
            "/applications/{}/tags{}",
            crate::encode_segment(uuid),
            tag_uuid
                .map(|x| format!("/{}", crate::encode_segment(x)))
                .unwrap_or_default()
        );
        self.request_json(method, &path, body).await
    }
    pub async fn database_child(
        &self,
        uuid: &str,
        child: &str,
        child_uuid: Option<&str>,
        method: Method,
        body: Option<Value>,
    ) -> Result<crate::ActionResult, CoolifyApiError> {
        let path = format!(
            "/databases/{}/{}/{}",
            crate::encode_segment(uuid),
            crate::encode_segment(child),
            child_uuid.map(crate::encode_segment).unwrap_or_default()
        )
        .trim_end_matches('/')
        .to_owned();
        self.request_json(method, &path, body).await
    }
    pub async fn database_action(
        &self,
        uuid: &str,
        action: &str,
        method: Method,
        body: Option<Value>,
    ) -> Result<crate::ActionResult, CoolifyApiError> {
        self.request_json(
            method,
            &format!(
                "/databases/{}/{}",
                crate::encode_segment(uuid),
                crate::encode_segment(action)
            ),
            body,
        )
        .await
    }
    pub async fn service_child(
        &self,
        uuid: &str,
        child: &str,
        child_uuid: Option<&str>,
        method: Method,
        body: Option<Value>,
    ) -> Result<crate::ActionResult, CoolifyApiError> {
        let path = format!(
            "/services/{}/{}/{}",
            crate::encode_segment(uuid),
            crate::encode_segment(child),
            child_uuid.map(crate::encode_segment).unwrap_or_default()
        )
        .trim_end_matches('/')
        .to_owned();
        self.request_json(method, &path, body).await
    }
    pub async fn service_action(
        &self,
        uuid: &str,
        action: &str,
        method: Method,
        body: Option<Value>,
    ) -> Result<crate::ActionResult, CoolifyApiError> {
        self.request_json(
            method,
            &format!(
                "/services/{}/{}",
                crate::encode_segment(uuid),
                crate::encode_segment(action)
            ),
            body,
        )
        .await
    }
    pub async fn project_environments(
        &self,
        uuid: &str,
    ) -> Result<Vec<crate::EnvironmentSummary>, CoolifyApiError> {
        self.request_json(
            Method::GET,
            &format!("/projects/{}/environments", crate::encode_segment(uuid)),
            None,
        )
        .await
    }
    pub async fn system_action(
        &self,
        action: &str,
        body: Option<Value>,
    ) -> Result<crate::ActionResult, CoolifyApiError> {
        self.request_json(
            Method::POST,
            &format!("/system/{}", crate::encode_segment(action)),
            body,
        )
        .await
    }
    pub async fn s3_storage(
        &self,
        uuid: Option<&str>,
        method: Method,
        body: Option<Value>,
    ) -> Result<crate::ActionResult, CoolifyApiError> {
        let path = format!(
            "/s3{}",
            uuid.map(|x| format!("/{}", crate::encode_segment(x)))
                .unwrap_or_default()
        );
        self.request_json(method, &path, body).await
    }
    pub async fn tags(
        &self,
        uuid: Option<&str>,
        method: Method,
        body: Option<Value>,
    ) -> Result<crate::ActionResult, CoolifyApiError> {
        let path = format!(
            "/tags{}",
            uuid.map(|x| format!("/{}", crate::encode_segment(x)))
                .unwrap_or_default()
        );
        self.request_json(method, &path, body).await
    }
}
