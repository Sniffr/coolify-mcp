//! MCP tool registry and typed Coolify handlers.
mod annotations;
pub mod handlers;
mod registry;
pub mod schemas;
pub use annotations::ToolAnnotations;
use coolify_api::{CoolifyApiError, CoolifyClient};
pub use registry::{
    AuditHook, DEFAULT_TOOL_ROSTER, InstanceRegistry, ToolContext, ToolSpec, registered_tools,
};
use reqwest::Method;
use safety::{Action, allows};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq)]
pub struct ToolResult {
    pub text: String,
    pub is_error: bool,
}
impl ToolResult {
    fn ok(v: Value) -> Self {
        Self {
            text: serde_json::to_string(&json!({"data":v})).unwrap(),
            is_error: false,
        }
    }
    fn err(s: impl Into<String>) -> Self {
        Self {
            text: format!("Error: {}", s.into()),
            is_error: true,
        }
    }
}
fn api(e: CoolifyApiError) -> ToolResult {
    ToolResult::err(e.to_string())
}
fn id<'a>(a: &'a Value, n: &str) -> Result<&'a str, ToolResult> {
    a.get(n)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ToolResult::err(format!("{n} is required")))
}
fn body(a: &Value) -> Option<Value> {
    a.get("body").cloned().or_else(|| a.get("input").cloned())
}
fn method(a: &Value) -> Method {
    match a.get("method").and_then(Value::as_str).unwrap_or("PATCH") {
        "GET" => Method::GET,
        "POST" => Method::POST,
        "DELETE" => Method::DELETE,
        _ => Method::PATCH,
    }
}
fn seg(s: &str) -> String {
    percent_encoding::utf8_percent_encode(s, percent_encoding::NON_ALPHANUMERIC).to_string()
}
fn action(n: &str, a: &Value) -> Action {
    if [
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
    .contains(&n)
    {
        Action::Read
    } else if [
        "deploy",
        "deployment",
        "redeploy_project",
        "restart_project_apps",
    ]
    .contains(&n)
    {
        Action::Deploy
    } else if a
        .get("action")
        .and_then(Value::as_str)
        .is_some_and(|x| matches!(x, "get" | "list" | "logs" | "search"))
    {
        Action::Read
    } else if a
        .get("action")
        .and_then(Value::as_str)
        .is_some_and(|x| matches!(x, "delete" | "remove" | "stop" | "cancel"))
    {
        Action::Delete
    } else {
        Action::Write
    }
}
async fn generic(c: &CoolifyClient, root: &str, a: &Value) -> Result<Value, ToolResult> {
    let suffix = a
        .get("uuid")
        .or_else(|| a.get("id"))
        .and_then(Value::as_str)
        .map(|x| format!("/{}", seg(x)))
        .unwrap_or_default();
    let path = format!("{root}{suffix}");
    c.request_value(method(a), &path, body(a))
        .await
        .map_err(api)
}
async fn dispatch(c: &CoolifyClient, n: &str, a: &Value) -> Result<Value, ToolResult> {
    let page = a.get("page").and_then(Value::as_u64).unwrap_or(1) as u32;
    let per = a.get("per_page").and_then(Value::as_u64).unwrap_or(50) as u32;
    match n {
        "get_version" => c.get_version().await.map(Value::String).map_err(api),
        "get_mcp_version" => Ok(json!({"version":env!("CARGO_PKG_VERSION")})),
        "list_applications" => c
            .list_applications(page, per)
            .await
            .map(|v| json!(v))
            .map_err(api),
        "get_application" => c
            .get_application(id(a, "uuid")?)
            .await
            .map(|v| json!(v))
            .map_err(api),
        "application_logs" => c
            .application_logs(
                id(a, "uuid")?,
                a.get("lines")
                    .and_then(Value::as_u64)
                    .unwrap_or(200)
                    .min(10000) as u32,
            )
            .await
            .map(|v| json!({"logs":v.chars().take(200000).collect::<String>()}))
            .map_err(api),
        "application" => {
            let u = id(a, "uuid")?;
            match a.get("action").and_then(Value::as_str) {
                Some("get") => c.get_application(u).await.map(|v| json!(v)).map_err(api),
                Some(x) => c
                    .application_action(u, x, body(a))
                    .await
                    .map(|v| json!(v))
                    .map_err(api),
                None => Err(ToolResult::err("action is required")),
            }
        }
        "list_databases" => c.list_databases().await.map(|v| json!(v)).map_err(api),
        "get_database" => c
            .get_database(id(a, "uuid")?)
            .await
            .map(|v| json!(v))
            .map_err(api),
        "database" => c
            .database_action(
                id(a, "uuid")?,
                a.get("action")
                    .and_then(Value::as_str)
                    .ok_or_else(|| ToolResult::err("action is required"))?,
                method(a),
                body(a),
            )
            .await
            .map(|v| json!(v))
            .map_err(api),
        "database_backups" => c
            .request_value(
                method(a),
                &format!(
                    "/databases/{}/backups{}",
                    seg(id(a, "uuid")?),
                    a.get("backup_uuid")
                        .and_then(Value::as_str)
                        .map(|x| format!("/{}", seg(x)))
                        .unwrap_or_default()
                ),
                body(a),
            )
            .await
            .map_err(api),
        "list_services" => c.list_services().await.map(|v| json!(v)).map_err(api),
        "get_service" => c
            .get_service(id(a, "uuid")?)
            .await
            .map(|v| json!(v))
            .map_err(api),
        "service" => c
            .service_action(
                id(a, "uuid")?,
                a.get("action")
                    .and_then(Value::as_str)
                    .ok_or_else(|| ToolResult::err("action is required"))?,
                method(a),
                body(a),
            )
            .await
            .map(|v| json!(v))
            .map_err(api),
        "list_servers" => c
            .list_servers(page, per)
            .await
            .map(|v| json!(v))
            .map_err(api),
        "get_server" => c
            .get_server(id(a, "uuid")?)
            .await
            .map(|v| json!(v))
            .map_err(api),
        "validate_server" => c
            .validate_server(id(a, "uuid")?)
            .await
            .map(|v| json!(v))
            .map_err(api),
        "server_domains" => {
            generic(c, &format!("/servers/{}/domains", seg(id(a, "uuid")?)), a).await
        }
        "server_resources" => {
            generic(c, &format!("/servers/{}/resources", seg(id(a, "uuid")?)), a).await
        }
        "list_destinations" => generic(c, "/destinations", a).await,
        "list_deployments" => c.list_deployments().await.map(|v| json!(v)).map_err(api),
        "deployment" => {
            let u = id(a, "uuid")?;
            match a.get("action").and_then(Value::as_str) {
                Some("logs") => c
                    .deployment_logs(u)
                    .await
                    .map(|v| json!({"logs":v.chars().take(200000).collect::<String>()}))
                    .map_err(api),
                Some("cancel") => c.cancel_deployment(u).await.map(|v| json!(v)).map_err(api),
                Some("wait") => c
                    .poll_deployment(
                        u,
                        std::time::Duration::from_secs(5),
                        std::time::Duration::from_secs(
                            a.get("timeout_seconds")
                                .and_then(Value::as_u64)
                                .unwrap_or(300),
                        ),
                    )
                    .await
                    .map(|v| json!(v))
                    .map_err(api),
                _ => c.get_deployment(u).await.map(|v| json!(v)).map_err(api),
            }
        }
        "deploy" => c
            .trigger_deployment(body(a).unwrap_or_else(|| json!({})))
            .await
            .map(|v| json!(v))
            .map_err(api),
        "logs" => c
            .deployment_logs(id(a, "uuid")?)
            .await
            .map(|v| json!({"logs":v.chars().take(200000).collect::<String>()}))
            .map_err(api),
        "projects" => match a.get("action").and_then(Value::as_str) {
            Some("get") => c
                .get_project(id(a, "uuid")?)
                .await
                .map(|v| json!(v))
                .map_err(api),
            Some("create") => c
                .create_project(body(a).unwrap_or_else(|| json!({})))
                .await
                .map(|v| json!(v))
                .map_err(api),
            Some("update") => c
                .update_project(id(a, "uuid")?, body(a).unwrap_or_else(|| json!({})))
                .await
                .map(|v| json!(v))
                .map_err(api),
            Some("delete") => c
                .delete_project(id(a, "uuid")?)
                .await
                .map(|v| json!(v))
                .map_err(api),
            _ => c.list_projects().await.map(|v| json!(v)).map_err(api),
        },
        "environments" => c
            .project_environments(id(a, "uuid")?)
            .await
            .map(|v| json!(v))
            .map_err(api),
        "env_vars" => c
            .application_envs(id(a, "uuid")?)
            .await
            .map(|v| json!(v))
            .map_err(api),
        "storages" => c
            .application_storage(
                id(a, "uuid")?,
                a.get("storage_uuid").and_then(Value::as_str),
                method(a),
                body(a),
            )
            .await
            .map(|v| json!(v))
            .map_err(api),
        "tags" => c
            .tags(
                a.get("tag_uuid").and_then(Value::as_str),
                method(a),
                body(a),
            )
            .await
            .map(|v| json!(v))
            .map_err(api),
        "system" => c
            .system_action(
                a.get("action")
                    .and_then(Value::as_str)
                    .ok_or_else(|| ToolResult::err("action is required"))?,
                body(a),
            )
            .await
            .map(|v| json!(v))
            .map_err(api),
        "diagnose_app" => c
            .diagnose_application(id(a, "uuid")?)
            .await
            .map(|v| json!(v))
            .map_err(api),
        "diagnose_server" => c
            .diagnose_server(id(a, "uuid")?)
            .await
            .map(|v| json!(v))
            .map_err(api),
        "cloud_tokens" => generic(c, "/cloud-tokens", a).await,
        "private_keys" => generic(c, "/private-keys", a).await,
        "github_apps" => generic(c, "/github-apps", a).await,
        "hetzner" => generic(c, "/hetzner", a).await,
        "scheduled_tasks" => generic(c, "/scheduled-tasks", a).await,
        "teams" => generic(c, "/teams", a).await,
        "search_docs" => generic(c, "/docs", a).await,
        "get_infrastructure_overview" => generic(c, "/resources", a).await,
        "find_issues" => generic(c, "/diagnostics", a).await,
        "bulk_env_update" => c
            .request_value(Method::PATCH, "/applications/envs/bulk", body(a))
            .await
            .map_err(api),
        "stop_all_apps" => c
            .request_value(Method::POST, "/applications/stop", body(a))
            .await
            .map_err(api),
        "redeploy_project" => c
            .request_value(Method::POST, "/deploy", body(a))
            .await
            .map_err(api),
        "restart_project_apps" => c
            .request_value(Method::POST, "/applications/restart", body(a))
            .await
            .map_err(api),
        "control" => c
            .application_action(
                id(a, "uuid")?,
                a.get("action")
                    .and_then(Value::as_str)
                    .ok_or_else(|| ToolResult::err("action is required"))?,
                body(a),
            )
            .await
            .map(|v| json!(v))
            .map_err(api),
        _ => generic(c, &format!("/{}", n.replace('_', "-")), a).await,
    }
}
pub async fn call_tool(ctx: ToolContext, name: &str, args: Value) -> ToolResult {
    let act = action(name, &args);
    if !allows(ctx.policy, act) {
        return ToolResult::err("tool is not permitted by the active capability profile");
    }
    if matches!(act, Action::Write | Action::Delete | Action::Deploy)
        && ctx
            .request_metadata
            .get("confirmed")
            .and_then(Value::as_bool)
            != Some(true)
    {
        return ToolResult::err("confirmation required for this action");
    }
    if let Some(i) = args.get("instance").and_then(Value::as_str)
        && ctx.instance.as_deref() != Some(i)
    {
        return ToolResult::err("unknown instance name");
    }
    let r = dispatch(&ctx.client, name, &args)
        .await
        .map(ToolResult::ok)
        .unwrap_or_else(|e| e);
    if let Some(a) = ctx.audit {
        a.record(name, if r.is_error { "error" } else { "ok" })
    }
    r
}
pub trait McpApplication: Send + Sync {
    fn tools(&self) -> Vec<ToolSpec>;
    fn call<'a>(
        &'a self,
        name: &'a str,
        args: Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ToolResult> + Send + 'a>>;
}
