use crate::checks::{DoctorCheck, DoctorCheckStatus};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DoctorReport {
    pub ok: bool,
    pub checks: Vec<DoctorCheck>,
}
impl DoctorReport {
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
    pub fn to_human(&self) -> String {
        let mut out = format!(
            "coolify-mcp doctor: {}\n",
            if self.ok { "ok" } else { "failed" }
        );
        for check in &self.checks {
            out.push_str(&format!(
                "{} {:<20} {}\n",
                match check.status {
                    DoctorCheckStatus::Pass => "✓",
                    DoctorCheckStatus::Fail => "✗",
                    DoctorCheckStatus::Inconclusive => "?",
                },
                check.name,
                check.detail
            ));
            if !matches!(check.status, DoctorCheckStatus::Pass) {
                out.push_str(&format!("  fix: {}\n", check.fix));
            }
        }
        out
    }
}
