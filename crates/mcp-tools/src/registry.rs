pub use crate::instances::InstanceRegistry;
use crate::{annotations::ToolAnnotations, schemas::schema_for};
use coolify_api::CoolifyClient;
use safety::CapabilityProfile;
use serde::Serialize;
use std::sync::{Arc, LazyLock};
use tenant::UserId;

const NAMES: [&str; 45] = [
    "application",
    "application_logs",
    "bulk_env_update",
    "cloud_tokens",
    "control",
    "database",
    "database_backups",
    "deploy",
    "deployment",
    "diagnose_app",
    "diagnose_server",
    "env_vars",
    "environments",
    "find_issues",
    "get_application",
    "get_database",
    "get_infrastructure_overview",
    "get_mcp_version",
    "get_server",
    "get_service",
    "get_version",
    "github_apps",
    "hetzner",
    "list_applications",
    "list_databases",
    "list_deployments",
    "list_destinations",
    "list_servers",
    "list_services",
    "logs",
    "private_keys",
    "projects",
    "redeploy_project",
    "restart_project_apps",
    "scheduled_tasks",
    "search_docs",
    "server_domains",
    "server_resources",
    "service",
    "stop_all_apps",
    "storages",
    "system",
    "tags",
    "teams",
    "validate_server",
];
const TITLES: [&str; 45] = [
    "Manage application",
    "Application logs",
    "Bulk update env var",
    "Cloud provider tokens",
    "Start, stop or restart",
    "Manage database",
    "Database backups",
    "Deploy",
    "Manage deployment",
    "Diagnose application",
    "Diagnose server",
    "Environment variables",
    "Manage environments",
    "Find estate issues",
    "Application details",
    "Database details",
    "Infrastructure overview",
    "MCP server version",
    "Server details",
    "Service details",
    "Coolify version",
    "GitHub Apps",
    "Hetzner cloud",
    "List applications",
    "List databases",
    "List deployments",
    "List destinations",
    "List servers",
    "List services",
    "Container logs",
    "SSH private keys",
    "Manage projects",
    "Redeploy project",
    "Restart project apps",
    "Scheduled tasks",
    "Search Coolify docs",
    "Server domains",
    "Server resources",
    "Manage service",
    "Emergency stop all apps",
    "Manage storages",
    "System and API access",
    "Manage tags",
    "Teams",
    "Validate server",
];
const READS: [&str; 23] = [
    "get_version",
    "get_mcp_version",
    "list_instances",
    "get_infrastructure_overview",
    "list_servers",
    "list_applications",
    "list_databases",
    "list_services",
    "list_deployments",
    "get_server",
    "get_application",
    "get_database",
    "get_service",
    "server_resources",
    "server_domains",
    "list_destinations",
    "diagnose_app",
    "diagnose_server",
    "find_issues",
    "search_docs",
    "application_logs",
    "logs",
    "teams",
];
#[derive(Clone, Debug, Serialize)]
pub struct ToolSpec {
    pub name: String,
    pub title: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
    pub annotations: ToolAnnotations,
    pub safety: String,
}
impl ToolSpec {
    pub fn is_read_only(&self) -> bool {
        self.annotations.read_only_hint
    }
}
pub static DEFAULT_TOOL_ROSTER: LazyLock<Vec<ToolSpec>> = LazyLock::new(|| {
    NAMES
        .iter()
        .enumerate()
        .map(|(i, name)| ToolSpec {
            name: (*name).into(),
            title: TITLES[i].into(),
            description: format!(
                "{}; actions are validated and routed through the typed Coolify API.",
                TITLES[i]
            ),
            input_schema: schema_for(name, false),
            annotations: if READS.contains(name) {
                ToolAnnotations::read_only(*name == "get_version" || *name == "get_mcp_version")
            } else if *name == "hetzner" {
                ToolAnnotations::safe_write(false)
            } else if *name == "validate_server" {
                ToolAnnotations::safe_write(true)
            } else {
                ToolAnnotations::destructive()
            },
            safety: if READS.contains(name) {
                "read-only".into()
            } else if *name == "hetzner" || *name == "validate_server" {
                "non-destructive-write".into()
            } else {
                "destructive-write".into()
            },
        })
        .collect()
});
pub fn registered_tools(
    profile: CapabilityProfile,
    fleet: Option<&InstanceRegistry>,
) -> Vec<ToolSpec> {
    let fleet_mode = fleet.is_some_and(InstanceRegistry::is_fleet);
    let mut names: Vec<&str> = NAMES.into_iter().collect();
    if fleet_mode || profile == CapabilityProfile::ReadOnly {
        names.push("list_instances");
    }
    names
        .into_iter()
        .filter(|n| profile != CapabilityProfile::ReadOnly || READS.contains(n))
        .map(|name| {
            let read = READS.contains(&name);
            let annotations = if read {
                ToolAnnotations::read_only(name == "get_version" || name == "get_mcp_version")
            } else if name == "hetzner" {
                ToolAnnotations::safe_write(false)
            } else if name == "validate_server" {
                ToolAnnotations::safe_write(true)
            } else {
                ToolAnnotations::destructive()
            };
            let i = NAMES.iter().position(|x| *x == name);
            ToolSpec {
                name: name.into(),
                title: i
                    .map(|x| TITLES[x])
                    .unwrap_or("List Coolify instances")
                    .into(),
                description: format!("{} action", name),
                input_schema: schema_for(name, fleet_mode),
                annotations,
                safety: if read { "read".into() } else { "write".into() },
            }
        })
        .collect()
}

pub trait AuditHook: Send + Sync {
    fn record(&self, tool: &str, outcome: &str);
}
#[derive(Clone)]
pub struct ToolContext {
    pub client: Arc<CoolifyClient>,
    pub policy: CapabilityProfile,
    pub audit: Option<Arc<dyn AuditHook>>,
    pub instance: Option<String>,
    pub instance_registry: Option<Arc<InstanceRegistry>>,
    pub request_metadata: serde_json::Map<String, serde_json::Value>,
}

/// Identity and capability policy resolved by the authenticated transport.
/// The profile is server-owned: tool arguments cannot replace it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TenantRequestContext {
    pub user_id: UserId,
    pub profile: CapabilityProfile,
}

/// Request-scoped Coolify client and the tenant context it was derived from.
/// This type intentionally has no process-global client fallback.
#[derive(Clone)]
pub struct TenantToolContext {
    pub request: TenantRequestContext,
    pub client: Arc<CoolifyClient>,
    pub audit: Option<Arc<dyn AuditHook>>,
    pub instance_registry: Option<Arc<InstanceRegistry>>,
    pub request_metadata: serde_json::Map<String, serde_json::Value>,
}
