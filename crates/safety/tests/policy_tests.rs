use safety::{Action, CapabilityProfile, allows, default_profile_for_transport};

#[test]
fn read_actions_are_allowed_and_mutations_are_profile_controlled() {
    for profile in [
        CapabilityProfile::ReadOnly,
        CapabilityProfile::Operations,
        CapabilityProfile::Admin,
    ] {
        assert!(allows(profile, Action::Read));
    }
    assert!(!allows(CapabilityProfile::ReadOnly, Action::Deploy));
    assert!(!allows(CapabilityProfile::ReadOnly, Action::Write));
    assert!(!allows(CapabilityProfile::ReadOnly, Action::Delete));
    assert!(allows(CapabilityProfile::Operations, Action::Deploy));
    assert!(allows(CapabilityProfile::Operations, Action::Write));
    assert!(!allows(CapabilityProfile::Operations, Action::Delete));
    assert!(allows(CapabilityProfile::Admin, Action::Delete));
}

#[test]
fn transport_defaults_are_safe_for_http_and_compatible_for_stdio() {
    assert_eq!(
        default_profile_for_transport("http"),
        CapabilityProfile::ReadOnly
    );
    assert_eq!(
        default_profile_for_transport("stdio"),
        CapabilityProfile::Operations
    );
    assert_eq!(
        default_profile_for_transport("unknown"),
        CapabilityProfile::ReadOnly
    );
}
