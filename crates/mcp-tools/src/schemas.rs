use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommonInput {
    pub action: Option<String>,
    pub uuid: Option<String>,
    pub id: Option<String>,
    pub page: Option<u32>,
    pub per_page: Option<u32>,
    pub lines: Option<u32>,
    pub wait: Option<bool>,
    pub timeout_seconds: Option<u32>,
    pub query: Option<String>,
    pub name: Option<String>,
    pub r#type: Option<String>,
    pub fqdn: Option<String>,
    pub repository: Option<String>,
    pub branch: Option<String>,
    pub key: Option<String>,
    pub value: Option<String>,
    pub schedule: Option<String>,
    pub command: Option<String>,
    pub provider: Option<String>,
    pub organization: Option<String>,
    pub team_id: Option<String>,
    pub storage_uuid: Option<String>,
    pub tag_uuid: Option<String>,
    pub backup_uuid: Option<String>,
    pub instance: Option<String>,
}
pub fn schema_for(name: &str, fleet: bool) -> Value {
    let mut properties = json!({"uuid":{"type":"string"},"id":{"type":"string"},"action":{"type":"string"},"page":{"type":"integer","minimum":1},"per_page":{"type":"integer","minimum":1,"maximum":100},"lines":{"type":"integer","minimum":1,"maximum":10000},"wait":{"type":"boolean"},"timeout_seconds":{"type":"integer","minimum":1,"maximum":300},"query":{"type":"string"},"name":{"type":"string"},"type":{"type":"string"},"fqdn":{"type":"string"},"repository":{"type":"string"},"branch":{"type":"string"},"key":{"type":"string"},"value":{"type":"string"},"schedule":{"type":"string"},"command":{"type":"string"},"provider":{"type":"string"},"organization":{"type":"string"},"team_id":{"type":"string"},"storage_uuid":{"type":"string"},"tag_uuid":{"type":"string"},"backup_uuid":{"type":"string"}});
    if fleet {
        properties["instance"] = json!({"type":"string","description":"Instance name"});
    }
    if matches!(
        name,
        "application" | "database" | "service" | "deployment" | "projects" | "control" | "system"
    ) {
        properties["action"] = json!({"type":"string","enum":match name {"application"=>json!(["get","create","update","delete","start","stop","restart","move","migrate","rollback"]),"database"=>json!(["get","create","update","delete","start","stop","restart","move","migrate"]),"service"=>json!(["get","create","update","delete","start","stop","restart"]),"deployment"=>json!(["get","logs","cancel","wait"]),"projects"=>json!(["list","get","create","update","delete"]),"control"=>json!(["start","stop","restart"]),_=>json!(["enable","disable","restart"])}});
    }
    json!({"type":"object","properties":properties,"additionalProperties":false})
}
