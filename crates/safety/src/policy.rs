#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapabilityProfile {
    ReadOnly,
    Operations,
    Admin,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    Read,
    Deploy,
    Write,
    Delete,
}

pub fn allows(profile: CapabilityProfile, action: Action) -> bool {
    matches!(
        (profile, action),
        (_, Action::Read)
            | (CapabilityProfile::Admin, _)
            | (
                CapabilityProfile::Operations,
                Action::Deploy | Action::Write
            )
    )
}

pub fn default_profile_for_transport(transport: &str) -> CapabilityProfile {
    if transport.eq_ignore_ascii_case("stdio") {
        CapabilityProfile::Operations
    } else {
        CapabilityProfile::ReadOnly
    }
}
