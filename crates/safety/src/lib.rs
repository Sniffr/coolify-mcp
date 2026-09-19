//! Safety policies, masking, untrusted output framing, and audit foundations.

mod audit;
mod masking;
mod policy;
mod untrusted;

pub use audit::{AuditEvent, AuditLogger};
pub use masking::{sanitize_json, sanitize_text};
pub use policy::{Action, CapabilityProfile, allows, default_profile_for_transport};
pub use untrusted::frame_untrusted;
