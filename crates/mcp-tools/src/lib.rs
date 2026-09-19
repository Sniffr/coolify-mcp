//! MCP tool registry and handler foundations.
mod annotations;
pub mod handlers;
mod registry;
pub mod schemas;
pub use annotations::ToolAnnotations;
use coolify_api::CoolifyApiError;
pub use registry::{
    AuditHook, DEFAULT_TOOL_ROSTER, InstanceRegistry, ToolContext, ToolSpec, registered_tools,
};
use safety::{Action, allows};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq)]
pub struct ToolResult {
    pub text: String,
    pub is_error: bool,
}
impl ToolResult {
    fn ok(data: Value) -> Self {
        Self {
            text: serde_json::to_string(&json!({"data":data}))
                .unwrap_or_else(|_| "{\"data\":null}".into()),
            is_error: false,
        }
    }
    fn err(e: impl Into<String>) -> Self {
        Self {
            text: format!("Error: {}", e.into()),
            is_error: true,
        }
    }
}
fn action_for(name: &str) -> Action {
    if [
        "deploy",
        "deployment",
        "redeploy_project",
        "restart_project_apps",
    ]
    .contains(&name)
    {
        Action::Deploy
    } else if [
        "get_version",
        "get_mcp_version",
        "list_applications",
        "list_databases",
        "list_servers",
        "list_services",
        "list_deployments",
        "get_application",
        "get_database",
        "get_server",
        "get_service",
        "application_logs",
        "logs",
        "diagnose_app",
        "diagnose_server",
        "find_issues",
        "search_docs",
        "teams",
        "get_infrastructure_overview",
        "server_resources",
        "server_domains",
        "list_destinations",
    ]
    .contains(&name)
    {
        Action::Read
    } else if [
        "stop_all_apps",
        "database",
        "application",
        "service",
        "projects",
        "environments",
        "private_keys",
        "cloud_tokens",
        "storages",
        "tags",
        "scheduled_tasks",
        "database_backups",
        "control",
        "bulk_env_update",
        "system",
        "env_vars",
    ]
    .contains(&name)
    {
        Action::Delete
    } else {
        Action::Write
    }
}
fn api_error(e: CoolifyApiError) -> ToolResult {
    ToolResult::err(e.to_string())
}
pub async fn call_tool(ctx: ToolContext, name: &str, args: Value) -> ToolResult {
    if !allows(ctx.policy, action_for(name)) {
        return ToolResult::err(format!(
            "tool '{name}' is not permitted by the active capability profile"
        ));
    }
    if let Some(instance) = args.get("instance").and_then(Value::as_str)
        && ctx.instance.as_deref() != Some(instance)
    {
        return ToolResult::err("unknown instance name");
    }
    let result = match name {
        "get_version" => ctx
            .client
            .get_version()
            .await
            .map(|v| ToolResult::ok(Value::String(v)))
            .map_err(api_error),
        "list_applications" => {
            let page = args.get("page").and_then(Value::as_u64).unwrap_or(1) as u32;
            let per = args.get("per_page").and_then(Value::as_u64).unwrap_or(50) as u32;
            ctx.client
                .list_applications(page, per)
                .await
                .map(|v| ToolResult::ok(json!(v)))
                .map_err(api_error)
        }
        "get_application" => match args.get("uuid").and_then(Value::as_str) {
            Some(u) => ctx
                .client
                .get_application(u)
                .await
                .map(|v| ToolResult::ok(json!(v)))
                .map_err(api_error),
            None => Ok(ToolResult::err("uuid is required")),
        },
        "application_logs" => match args.get("uuid").and_then(Value::as_str) {
            Some(u) => ctx
                .client
                .application_logs(
                    u,
                    args.get("lines").and_then(Value::as_u64).unwrap_or(200) as u32,
                )
                .await
                .map(|v| ToolResult::ok(json!(v)))
                .map_err(api_error),
            None => Ok(ToolResult::err("uuid is required")),
        },
        _ => Ok(ToolResult::ok(
            json!({"status":"not_implemented","tool":name,"arguments":args}),
        )),
    };
    let out = result.unwrap_or_else(|e| e);
    if let Some(a) = ctx.audit.as_ref() {
        a.record(name, if out.is_error { "error" } else { "ok" });
    }
    out
}
