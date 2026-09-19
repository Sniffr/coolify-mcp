//! MCP tool registry and typed Coolify handlers.
mod annotations;
pub mod docs_search;
pub mod handlers;
pub mod instances;
pub mod prompts;
mod registry;
pub mod resources;
pub mod schemas;
use crate::schemas::CommonInput;
pub use annotations::ToolAnnotations;
use coolify_api::{CoolifyApiError, CoolifyClient};
pub use instances::{Instance, InstanceRegistryError};
pub use registry::{
    AuditHook, DEFAULT_TOOL_ROSTER, InstanceRegistry, ToolContext, ToolSpec, registered_tools,
};
use reqwest::Method;
use safety::{Action, allows, frame_untrusted};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq)]
pub struct ToolResult {
    pub text: String,
    pub is_error: bool,
}
impl ToolResult {
    fn ok(v: Value) -> Self {
        let mut envelope = json!({"data":v,"_actions":[]});
        let encoded = serde_json::to_string(&envelope).unwrap_or_default();
        if encoded.len() > 200_000 {
            envelope["data"] = json!({"truncated":true,"preview":encoded.chars().take(199_000).collect::<String>()});
        }
        Self {
            text: serde_json::to_string(&envelope).unwrap(),
            is_error: false,
        }
    }
    fn err(s: impl Into<String>) -> Self {
        let message = s.into();
        Self { text:serde_json::to_string(&json!({"error":{"code":"MCP_TOOL_ERROR","message":message,"details":null},"is_error":true})).unwrap(), is_error:true }
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
fn typed_body(a: &Value) -> Option<Value> {
    let o = a.as_object()?;
    let mut out = serde_json::Map::new();
    for key in [
        "environment_name",
        "name",
        "type",
        "fqdn",
        "repository",
        "branch",
        "key",
        "value",
        "schedule",
        "command",
        "provider",
        "organization",
        "team_id",
    ] {
        if let Some(v) = o.get(key) {
            out.insert(key.into(), v.clone());
        }
    }
    (!out.is_empty()).then_some(Value::Object(out))
}
fn fixed_method(name: &str, action: Option<&str>) -> Method {
    match (name, action) {
        (
            "list_applications"
            | "list_servers"
            | "list_databases"
            | "list_services"
            | "list_deployments"
            | "get_application"
            | "get_database"
            | "get_service"
            | "get_server"
            | "application_logs"
            | "logs"
            | "diagnose_app"
            | "diagnose_server"
            | "search_docs"
            | "find_issues"
            | "get_infrastructure_overview"
            | "server_domains"
            | "server_resources"
            | "list_destinations"
            | "teams",
            _,
        ) => Method::GET,
        (_, Some("get" | "list" | "logs" | "search")) => Method::GET,
        (_, Some("delete" | "remove")) => Method::DELETE,
        (_, Some("start" | "stop" | "restart" | "cancel" | "deploy" | "create")) => Method::POST,
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
async fn fixed_request(
    c: &CoolifyClient,
    root: &str,
    a: &Value,
    tool: &str,
) -> Result<Value, ToolResult> {
    let suffix = a
        .get("uuid")
        .or_else(|| a.get("id"))
        .and_then(Value::as_str)
        .map(|x| format!("/{}", seg(x)))
        .unwrap_or_default();
    let path = format!("{root}{suffix}");
    c.request_value(
        fixed_method(tool, a.get("action").and_then(Value::as_str)),
        &path,
        typed_body(a),
    )
    .await
    .map_err(api)
}
async fn dispatch(c: &CoolifyClient, n: &str, a: &Value) -> Result<Value, ToolResult> {
    let page = a.get("page").and_then(Value::as_u64).unwrap_or(1) as u32;
    let per = a.get("per_page").and_then(Value::as_u64).unwrap_or(50) as u32;
    match n {
        "list_instances" => Ok(json!([])),
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
            .map(|v| json!({"logs":frame_untrusted(&v.chars().take(200000).collect::<String>(), "logs")}))
            .map_err(api),
        "application" => {
            let u = id(a, "uuid")?;
            match a.get("action").and_then(Value::as_str) {
                Some("get") => c.get_application(u).await.map(|v| json!(v)).map_err(api),
                Some(x) => c
                    .application_action(u, x, typed_body(a))
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
                fixed_method(n, a.get("action").and_then(Value::as_str)),
                typed_body(a),
            )
            .await
            .map(|v| json!(v))
            .map_err(api),
        "database_backups" => c
            .request_value(
                fixed_method(n, a.get("action").and_then(Value::as_str)),
                &format!(
                    "/databases/{}/backups{}",
                    seg(id(a, "uuid")?),
                    a.get("backup_uuid")
                        .and_then(Value::as_str)
                        .map(|x| format!("/{}", seg(x)))
                        .unwrap_or_default()
                ),
                typed_body(a),
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
                fixed_method(n, a.get("action").and_then(Value::as_str)),
                typed_body(a),
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
            fixed_request(
                c,
                &format!("/servers/{}/domains", seg(id(a, "uuid")?)),
                a,
                n,
            )
            .await
        }
        "server_resources" => {
            fixed_request(
                c,
                &format!("/servers/{}/resources", seg(id(a, "uuid")?)),
                a,
                n,
            )
            .await
        }
        "list_destinations" => fixed_request(c, "/destinations", a, n).await,
        "list_deployments" => c.list_deployments().await.map(|v| json!(v)).map_err(api),
        "deployment" => {
            let u = id(a, "uuid")?;
            match a.get("action").and_then(Value::as_str) {
                Some("logs") => c
                    .deployment_logs(u)
                    .await
                    .map(|v| json!({"logs":frame_untrusted(&v.chars().take(200000).collect::<String>(), "logs")}))
                    .map_err(api),
                Some("cancel") => c.cancel_deployment(u).await.map(|v| json!(v)).map_err(api),
                Some("wait") => {
                    let v = c
                        .poll_deployment(
                            u,
                            std::time::Duration::from_secs(5),
                            std::time::Duration::from_secs(
                                a.get("timeout_seconds")
                                    .and_then(Value::as_u64)
                                    .unwrap_or(300)
                                    .min(300),
                            ),
                        )
                        .await
                        .map_err(api)?;
                    let status = v.status.to_ascii_lowercase();
                    let terminal = matches!(status.as_str(), "finished" | "failed" | "cancelled");
                    let mut out = json!({"status":v.status,"deployment_uuid":v.deployment_uuid,"application_uuid":v.uuid,"timed_out":!terminal});
                    if status == "failed"
                        && let Ok(logs) = c.deployment_logs(u).await
                    {
                        let tail: String = logs
                            .chars()
                            .rev()
                            .take(20_000)
                            .collect::<String>()
                            .chars()
                            .rev()
                            .collect();
                        out["logs_tail"] = Value::String(frame_untrusted(&tail, "logs"));
                    }
                    if !terminal {
                        out["next_action"] =
                            json!({"tool":"deployment","args":{"uuid":u,"action":"wait"}});
                    }
                    Ok(out)
                }
                _ => c.get_deployment(u).await.map(|v| json!(v)).map_err(api),
            }
        }
        "deploy" => c
            .trigger_deployment(typed_body(a).unwrap_or_else(|| json!({})))
            .await
            .map(|v| json!(v))
            .map_err(api),
        "logs" => c
            .deployment_logs(id(a, "uuid")?)
            .await
            .map(|v| json!({"logs":frame_untrusted(&v.chars().take(200000).collect::<String>(), "logs")}))
            .map_err(api),
        "projects" => match a.get("action").and_then(Value::as_str) {
            Some("get") => c
                .get_project(id(a, "uuid")?)
                .await
                .map(|v| json!(v))
                .map_err(api),
            Some("create") => c
                .create_project(typed_body(a).unwrap_or_else(|| json!({})))
                .await
                .map(|v| json!(v))
                .map_err(api),
            Some("update") => c
                .update_project(id(a, "uuid")?, typed_body(a).unwrap_or_else(|| json!({})))
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
        "environments" => {
            let project = seg(id(a, "uuid")?);
            let suffix = a
                .get("environment_name")
                .and_then(Value::as_str)
                .map(|v| format!("/{}", seg(v)))
                .unwrap_or_default();
            c.request_value(
                fixed_method(n, a.get("action").and_then(Value::as_str)),
                &format!("/projects/{}/environments{}", project, suffix),
                typed_body(a),
            )
            .await
            .map_err(api)
        }
        "env_vars" => c
            .request_value(
                fixed_method(n, a.get("action").and_then(Value::as_str)),
                &format!("/applications/{}/envs", seg(id(a, "uuid")?)),
                typed_body(a),
            )
            .await
            .map_err(api),
        "storages" => c
            .application_storage(
                id(a, "uuid")?,
                a.get("storage_uuid").and_then(Value::as_str),
                fixed_method(n, a.get("action").and_then(Value::as_str)),
                typed_body(a),
            )
            .await
            .map(|v| json!(v))
            .map_err(api),
        "tags" => c
            .tags(
                a.get("tag_uuid").and_then(Value::as_str),
                fixed_method(n, a.get("action").and_then(Value::as_str)),
                typed_body(a),
            )
            .await
            .map(|v| json!(v))
            .map_err(api),
        "system" => c
            .system_action(
                a.get("action")
                    .and_then(Value::as_str)
                    .ok_or_else(|| ToolResult::err("action is required"))?,
                typed_body(a),
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
        "cloud_tokens" => fixed_request(c, "/cloud-tokens", a, n).await,
        "private_keys" => fixed_request(c, "/private-keys", a, n).await,
        "github_apps" => fixed_request(c, "/github-apps", a, n).await,
        "hetzner" => fixed_request(c, "/hetzner", a, n).await,
        "scheduled_tasks" => fixed_request(c, "/scheduled-tasks", a, n).await,
        "teams" => fixed_request(c, "/teams", a, n).await,
        "search_docs" => {
            let query = a.get("query").and_then(Value::as_str).unwrap_or("");
            let limit = a.get("per_page").and_then(Value::as_u64).unwrap_or(10).min(50) as usize;
            Ok(json!(crate::docs_search::DocsSearchEngine::embedded().search(query, limit)))
        },
        "get_infrastructure_overview" => fixed_request(c, "/resources", a, n).await,
        "find_issues" => fixed_request(c, "/diagnostics", a, n).await,
        "bulk_env_update" => c
            .request_value(Method::PATCH, "/applications/envs/bulk", typed_body(a))
            .await
            .map_err(api),
        "stop_all_apps" => c
            .request_value(Method::POST, "/applications/stop", typed_body(a))
            .await
            .map_err(api),
        "redeploy_project" => c
            .request_value(Method::POST, "/deploy", typed_body(a))
            .await
            .map_err(api),
        "restart_project_apps" => c
            .request_value(Method::POST, "/applications/restart", typed_body(a))
            .await
            .map_err(api),
        "control" => c
            .application_action(
                id(a, "uuid")?,
                a.get("action")
                    .and_then(Value::as_str)
                    .ok_or_else(|| ToolResult::err("action is required"))?,
                typed_body(a),
            )
            .await
            .map(|v| json!(v))
            .map_err(api),
        _ => Err(ToolResult::err("tool is not registered")),
    }
}
fn registered(name: &str) -> bool {
    DEFAULT_TOOL_ROSTER.iter().any(|t| t.name == name)
}
fn unsupported(message: &str) -> ToolResult {
    ToolResult { text: serde_json::to_string(&json!({"error":{"code":"MCP_UNSUPPORTED","message":message,"details":null},"is_error":true})).unwrap(), is_error: true }
}
fn validate_input(name: &str, args: &Value) -> Result<Value, ToolResult> {
    let typed: CommonInput = serde_json::from_value(args.clone())
        .map_err(|_| ToolResult::err("invalid typed arguments"))?;
    let Some(obj) = args.as_object() else {
        return Err(ToolResult::err("arguments must be an object"));
    };
    let common = [
        "uuid",
        "id",
        "action",
        "page",
        "per_page",
        "lines",
        "wait",
        "timeout_seconds",
        "storage_uuid",
        "tag_uuid",
        "backup_uuid",
        "query",
        "instance",
        "environment_name",
        "name",
        "type",
        "fqdn",
        "repository",
        "branch",
        "key",
        "value",
        "schedule",
        "command",
        "provider",
        "organization",
        "team_id",
    ];
    if let Some(k) = obj.keys().find(|k| !common.contains(&k.as_str())) {
        return Err(ToolResult::err(format!("unknown argument '{k}'")));
    }
    let action_tools = [
        "application",
        "database",
        "service",
        "deployment",
        "projects",
        "control",
        "system",
        "database_backups",
        "storages",
        "tags",
        "cloud_tokens",
        "private_keys",
        "github_apps",
        "hetzner",
        "scheduled_tasks",
        "environments",
        "env_vars",
        "bulk_env_update",
        "deploy",
        "stop_all_apps",
        "redeploy_project",
        "restart_project_apps",
    ];
    if action_tools.contains(&name) && obj.get("action").and_then(Value::as_str).is_none() {
        return Err(ToolResult::err("action is required"));
    }
    let allowed = match name {
        "application" => &[
            "get", "create", "update", "delete", "start", "stop", "restart", "move", "migrate",
            "rollback",
        ][..],
        "database" => &[
            "get", "create", "update", "delete", "start", "stop", "restart", "move", "migrate",
        ][..],
        "service" => &[
            "get", "create", "update", "delete", "start", "stop", "restart",
        ][..],
        "deployment" => &["get", "logs", "cancel", "wait"][..],
        "projects" => &["list", "get", "create", "update", "delete"][..],
        "control" => &["start", "stop", "restart"][..],
        "system" => &["enable", "disable", "restart"][..],
        "database_backups" => &["list", "get", "create", "update", "delete", "executions"][..],
        "storages" => &["list", "get", "create", "update", "delete"][..],
        "tags" => &["list", "create", "delete"][..],
        "cloud_tokens" | "private_keys" | "github_apps" | "hetzner" | "scheduled_tasks" => {
            &["list", "get", "create", "update", "delete"][..]
        }
        "environments" | "env_vars" => &["list", "get", "create", "update", "delete"][..],
        "bulk_env_update" => &["update"][..],
        "deploy" => &["deploy"][..],
        "stop_all_apps" => &["stop"][..],
        "redeploy_project" => &["redeploy"][..],
        "restart_project_apps" => &["restart"][..],
        _ => &[][..],
    };
    if let Some(a) = obj.get("action").and_then(Value::as_str)
        && !allowed.contains(&a)
    {
        return Err(ToolResult::err("unknown action for tool"));
    }
    if obj
        .get("per_page")
        .and_then(Value::as_u64)
        .is_some_and(|v| !(1..=100).contains(&v))
    {
        return Err(ToolResult::err("per_page must be between 1 and 100"));
    }
    serde_json::to_value(typed).map_err(|_| ToolResult::err("typed argument encoding failed"))
}
fn with_pagination(mut result: ToolResult, name: &str, page: u32, per_page: u32) -> ToolResult {
    if matches!(name, "list_applications" | "list_servers")
        && let Ok(mut v) = serde_json::from_str::<Value>(&result.text)
        && v["data"].is_array()
    {
        v["_pagination"] = json!({"page":page,"per_page":per_page});
        result.text = serde_json::to_string(&v).unwrap_or(result.text);
    }
    result
}
pub async fn call_tool(ctx: ToolContext, name: &str, args: Value) -> ToolResult {
    let finish = |result: ToolResult| {
        if let Some(a) = ctx.audit.as_ref() {
            a.record(name, if result.is_error { "error" } else { "ok" });
        }
        result
    };
    if name == "list_instances" {
        if ctx.instance_registry.as_ref().is_none_or(|r| !r.is_fleet()) {
            return finish(unsupported(
                "list_instances requires a fleet-enabled InstanceRegistry",
            ));
        }
        let registry = ctx.instance_registry.as_ref().expect("checked above");
        return finish(ToolResult::ok(json!(registry.projection())));
    }
    if !registered(name) {
        return finish(ToolResult::err("tool is not registered"));
    }
    let typed_args = match validate_input(name, &args) {
        Ok(v) => v,
        Err(e) => return finish(e),
    };
    let act = action(name, &typed_args);
    if !allows(ctx.policy, act) {
        return finish(ToolResult::err(
            "tool is not permitted by the active capability profile",
        ));
    }
    if matches!(act, Action::Write | Action::Delete | Action::Deploy)
        && ctx
            .request_metadata
            .get("confirmed")
            .and_then(Value::as_bool)
            != Some(true)
    {
        return finish(ToolResult::err("confirmation required for this action"));
    }
    let client = if let Some(i) = typed_args.get("instance").and_then(Value::as_str) {
        let Some(registry) = ctx.instance_registry.as_ref() else {
            return finish(unsupported(
                "instance routing requires a fleet-enabled InstanceRegistry",
            ));
        };
        match registry.select(i) {
            Ok(instance) => instance.client,
            Err(_) => return finish(ToolResult::err("unknown instance name")),
        }
    } else {
        if ctx
            .instance_registry
            .as_ref()
            .is_some_and(|registry| registry.is_fleet())
        {
            return finish(unsupported(
                "an instance selector is required in fleet mode",
            ));
        }
        ctx.client.clone()
    };
    let result = dispatch(&client, name, &typed_args)
        .await
        .map(ToolResult::ok)
        .unwrap_or_else(|e| e);
    if !result.is_error {
        return finish(with_pagination(
            result,
            name,
            typed_args.get("page").and_then(Value::as_u64).unwrap_or(1) as u32,
            typed_args
                .get("per_page")
                .and_then(Value::as_u64)
                .unwrap_or(50)
                .clamp(1, 100) as u32,
        ));
    }
    finish(result)
}
pub trait McpApplication: Send + Sync {
    fn tools(&self) -> Vec<ToolSpec>;
    fn call<'a>(
        &'a self,
        name: &'a str,
        args: Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ToolResult> + Send + 'a>>;
}
