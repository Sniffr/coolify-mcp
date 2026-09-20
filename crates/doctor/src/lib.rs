//! Side-effect-free startup and connectivity diagnostics.
mod checks;
mod report;

pub use checks::{DoctorCheck, DoctorCheckStatus, ProbeResponse};
pub use report::DoctorReport;
use std::collections::HashMap;

/// Validate hosted service configuration without inspecting or constructing a
/// process-global Coolify client.
pub fn run_hosted_doctor(env: &HashMap<String, String>, tenant_ready: bool) -> DoctorReport {
    let checks = checks::hosted_checks(env, tenant_ready);
    let ok = checks
        .iter()
        .all(|check| check.status == DoctorCheckStatus::Pass);
    DoctorReport { ok, checks }
}
use std::future::Future;

pub async fn run_doctor<F, Fut>(env: &HashMap<String, String>, fetcher: F) -> DoctorReport
where
    F: Fn(&str) -> Fut,
    Fut: Future<Output = Result<ProbeResponse, String>>,
{
    let mut checks = checks::static_checks(env);
    checks.extend(checks::network_checks(env, &fetcher).await);
    let ok = checks
        .iter()
        .all(|check| check.status == DoctorCheckStatus::Pass);
    DoctorReport { ok, checks }
}
