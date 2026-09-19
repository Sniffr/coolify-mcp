use serde::Serialize;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct ToolAnnotations {
    #[serde(rename = "readOnlyHint")]
    pub read_only_hint: bool,
    #[serde(rename = "destructiveHint")]
    pub destructive_hint: bool,
    #[serde(rename = "idempotentHint")]
    pub idempotent_hint: bool,
    #[serde(rename = "openWorldHint")]
    pub open_world_hint: bool,
}
impl ToolAnnotations {
    pub const fn read_only(open_world: bool) -> Self {
        Self {
            read_only_hint: true,
            destructive_hint: false,
            idempotent_hint: false,
            open_world_hint: open_world,
        }
    }
    pub const fn destructive() -> Self {
        Self {
            read_only_hint: false,
            destructive_hint: true,
            idempotent_hint: false,
            open_world_hint: true,
        }
    }
    pub const fn safe_write(idempotent: bool) -> Self {
        Self {
            read_only_hint: false,
            destructive_hint: false,
            idempotent_hint: idempotent,
            open_world_hint: true,
        }
    }
}
