use oauth::{OAuthStateStore, PersistedState};

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
use std::os::unix::fs::PermissionsExt;
