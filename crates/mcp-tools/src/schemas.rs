use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct CommonInput {
    pub action: Option<String>,
    pub uuid: Option<String>,
    pub page: Option<u32>,
    pub per_page: Option<u32>,
    pub instance: Option<String>,
    pub wait: Option<bool>,
    pub lines: Option<u32>,
    pub args: Option<Value>,
}

pub fn schema_for(_name: &str, fleet: bool) -> Value {
    let mut properties = json!({"action":{"type":"string"},"uuid":{"type":"string"},"page":{"type":"integer","minimum":1},"per_page":{"type":"integer","minimum":1,"maximum":100},"wait":{"type":"boolean"},"lines":{"type":"integer","minimum":1,"maximum":10000}});
    if fleet {
        properties["instance"] = json!({"type":"string","description":"Instance name"});
    }
    json!({"type":"object","properties":properties,"additionalProperties":false})
}
