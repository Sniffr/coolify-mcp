//! Side-effect-free startup and connectivity diagnostics.
mod checks;
mod report;

pub use checks::{DoctorCheck, DoctorCheckStatus};
pub use report::DoctorReport;
use std::collections::HashMap;
use std::future::Future;

pub async fn run_doctor<F, Fut>(env: &HashMap<String, String>, fetcher: F) -> DoctorReport
where
    F: Fn(&str) -> Fut,
    Fut: Future<Output = Result<String, String>>,
{
    let mut checks = checks::static_checks(env);
    checks.extend(checks::network_checks(&fetcher).await);
    let ok = checks
        .iter()
        .all(|check| check.status == DoctorCheckStatus::Pass);
    DoctorReport { ok, checks }
}
