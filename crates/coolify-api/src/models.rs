use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
pub struct ServerSummary {
    pub uuid: String,
    pub name: String,
    #[serde(default)]
    pub ip: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub is_reachable: Option<bool>,
    #[serde(skip)]
    pub extra: Value,
}
impl<'de> Deserialize<'de> for ServerSummary {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = Value::deserialize(d)?;
        Ok(Self {
            uuid: v
                .get("uuid")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .into(),
            name: v
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .into(),
            ip: v.get("ip").and_then(Value::as_str).map(str::to_owned),
            status: v.get("status").and_then(Value::as_str).map(str::to_owned),
            is_reachable: v.get("is_reachable").and_then(Value::as_bool),
            extra: Value::Object(Default::default()),
        })
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSummary {
    pub uuid: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicationSummary {
    pub uuid: String,
    pub name: String,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default, deserialize_with = "domain_de", alias = "domains")]
    pub fqdn: Option<String>,
    #[serde(default)]
    pub git_repository: Option<String>,
    #[serde(default)]
    pub git_branch: Option<String>,
}
fn domain_de<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    let v = Option::<Value>::deserialize(d)?;
    Ok(v.as_ref().and_then(|v| {
        v.as_str().map(str::to_owned).or_else(|| {
            v.as_array()
                .and_then(|a| a.first())
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
    }))
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseSummary {
    pub uuid: String,
    pub name: String,
    #[serde(rename = "database_type", alias = "type")]
    pub r#type: String,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub is_public: Option<bool>,
    #[serde(default)]
    pub environment_uuid: Option<String>,
    #[serde(default)]
    pub environment_name: Option<String>,
    #[serde(default)]
    pub environment_id: Option<i64>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceSummary {
    pub uuid: String,
    pub name: String,
    #[serde(default)]
    pub r#type: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub domains: Option<Vec<String>>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentSummary {
    pub uuid: String,
    pub deployment_uuid: String,
    #[serde(default)]
    pub application_name: Option<String>,
    pub status: String,
    #[serde(default)]
    pub created_at: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentVariable {
    pub uuid: String,
    pub key: String,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub is_buildtime: bool,
    #[serde(default)]
    pub is_runtime: bool,
    #[serde(default)]
    pub is_preview: bool,
}
