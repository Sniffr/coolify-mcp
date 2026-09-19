use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommonInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uuid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub per_page: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lines: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wait: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fqdn: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schedule: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organization: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage_uuid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag_uuid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup_uuid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,
}
pub fn schema_for(name: &str, fleet: bool) -> Value {
    let mut properties = json!({"uuid":{"type":"string"},"id":{"type":"string"},"action":{"type":"string"},"page":{"type":"integer","minimum":1},"per_page":{"type":"integer","minimum":1,"maximum":100},"lines":{"type":"integer","minimum":1,"maximum":10000},"wait":{"type":"boolean"},"timeout_seconds":{"type":"integer","minimum":1,"maximum":300},"query":{"type":"string"},"name":{"type":"string"},"environment_name":{"type":"string"},"type":{"type":"string"},"fqdn":{"type":"string"},"repository":{"type":"string"},"branch":{"type":"string"},"key":{"type":"string"},"value":{"type":"string"},"schedule":{"type":"string"},"command":{"type":"string"},"provider":{"type":"string"},"organization":{"type":"string"},"team_id":{"type":"string"},"storage_uuid":{"type":"string"},"tag_uuid":{"type":"string"},"backup_uuid":{"type":"string"}});
    if fleet {
        properties["instance"] = json!({"type":"string","description":"Instance name"});
    }
    let actions = match name {
        "application" => Some(json!([
            "get", "create", "update", "delete", "start", "stop", "restart", "move", "migrate",
            "rollback"
        ])),
        "database" => Some(json!([
            "get", "create", "update", "delete", "start", "stop", "restart", "move", "migrate"
        ])),
        "service" => Some(json!([
            "get", "create", "update", "delete", "start", "stop", "restart"
        ])),
        "deployment" => Some(json!(["get", "logs", "cancel", "wait"])),
        "projects" => Some(json!(["list", "get", "create", "update", "delete"])),
        "control" => Some(json!(["start", "stop", "restart"])),
        "system" => Some(json!(["enable", "disable", "restart"])),
        "database_backups" => Some(json!([
            "list",
            "get",
            "create",
            "update",
            "delete",
            "executions"
        ])),
        "storages" => Some(json!(["list", "get", "create", "update", "delete"])),
        "tags" => Some(json!(["list", "create", "delete"])),
        "cloud_tokens" | "private_keys" | "github_apps" | "hetzner" | "scheduled_tasks" => {
            Some(json!(["list", "get", "create", "update", "delete"]))
        }
        "environments" | "env_vars" => Some(json!(["list", "get", "create", "update", "delete"])),
        "bulk_env_update" => Some(json!(["update"])),
        "deploy" => Some(json!(["deploy"])),
        "stop_all_apps" => Some(json!(["stop"])),
        "redeploy_project" => Some(json!(["redeploy"])),
        "restart_project_apps" => Some(json!(["restart"])),
        _ => None,
    };
    let mut out = json!({"type":"object","properties":properties,"additionalProperties":false});
    if let Some(values) = actions {
        out["properties"]["action"] = json!({"type":"string","enum":values});
        out["required"] = json!(["action"]);
    }
    out
}
