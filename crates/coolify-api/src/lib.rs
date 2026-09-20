//! Typed Coolify API client foundation.

pub mod api_shape;
mod client;
pub mod compatibility;
mod config;
mod error;
pub mod models;
mod network_policy;
pub mod resources;
pub mod route_matrix;
mod token_source;

pub use api_shape::{pagination_query, unwrap_logs};
pub use client::{CoolifyClient, ProbeOutcome};
pub use compatibility::{LegacyEndpoint, error_hint, error_hint_with_body};
pub use config::{ConfigError, CoolifyConfig, config_from_env};
pub use error::{CoolifyApiError, HttpErrorDetails, MAX_BODY_BYTES};
pub use models::{
    ActionResult, ApplicationSummary, BackupSummary, ChildSummary, DatabaseSummary,
    DeploymentSummary, DiagnosticSummary, DomainSummary, EnvironmentSummary, EnvironmentVariable,
    ProjectSummary, S3StorageSummary, ScheduledTaskSummary, ServerSummary, ServiceSummary,
    StorageSummary, SystemSummary, TagSummary, ValidationResult,
};
pub use network_policy::validate_hosted_base_url;
pub use token_source::{TokenSource, TokenSourceError};
pub(crate) fn encode_segment(value: &str) -> String {
    percent_encoding::utf8_percent_encode(value, percent_encoding::NON_ALPHANUMERIC).to_string()
}
pub fn is_running_status(status: Option<&str>) -> bool {
    let Some(status) = status else { return false };
    let lower = status.to_ascii_lowercase();
    !lower.contains("unhealthy") && (lower.starts_with("running") || lower.contains("healthy"))
}
