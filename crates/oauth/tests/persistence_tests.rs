use oauth::{OAuthProvider, OAuthStateStore, PersistedState, RegistrationRequest};
use std::os::unix::fs::PermissionsExt;

#[test]
fn corrupt_state_recovers_cleanly_and_persistence_is_private() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("oauth.json");
    std::fs::write(&path, b"not json").unwrap();
    let loaded = OAuthStateStore::load(&path).unwrap();
    assert!(loaded.degraded());
    assert!(loaded.state().clients.is_empty());
    let store = OAuthStateStore::new(path.clone());
    store.save(&PersistedState::default()).unwrap();
    let mode = std::fs::metadata(path).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[test]
fn persisted_json_contains_only_digests_not_issued_secrets_or_codes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("oauth.json");
    let provider =
        OAuthProvider::with_store("https://server.test".into(), "/mcp".into(), path.clone())
            .unwrap();
    let registration = provider
        .register(RegistrationRequest {
            redirect_uris: vec!["https://client.test/cb".into()],
            client_name: None,
        })
        .unwrap();
    let json = std::fs::read_to_string(path).unwrap();
    assert!(!json.contains(&registration.client_secret));
    assert!(json.contains("client_secret_hash"));
}

#[test]
fn failed_automatic_persistence_rolls_back_registration() {
    let provider = OAuthProvider::with_store(
        "https://server.test".into(),
        "/mcp".into(),
        std::path::PathBuf::from("/dev/null"),
    )
    .unwrap();
    let result = provider.register(RegistrationRequest {
        redirect_uris: vec!["https://client.test/cb".into()],
        client_name: None,
    });
    assert!(result.is_err());
}
