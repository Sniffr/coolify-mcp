use std::fs;

use safety::CapabilityProfile;
use secrecy::{ExposeSecret, SecretString};
use tempfile::tempdir;
use tenant::{TenantError, TenantStore};
use url::Url;

const KEY: &str = "fixture-encryption-key-material-only-for-tests-0123456789";
const OTHER_KEY: &str = "different-fixture-encryption-key-material-0123456789";
const TOKEN: &str = "fixture-coolify-token-not-a-credential";

fn store_path() -> (tempfile::TempDir, std::path::PathBuf) {
    let directory = tempdir().expect("temporary directory");
    let path = directory.path().join("tenant.sqlite");
    (directory, path)
}

#[test]
fn token_is_encrypted_and_never_serialized_plaintext() {
    let (_directory, path) = store_path();
    let store = TenantStore::open(&path, KEY).expect("store opens");
    let user = store
        .upsert_user("github-user-a", "fixture-user-a")
        .expect("user persists");

    store
        .save_connection(
            user.id,
            &Url::parse("https://coolify.example.test").expect("fixture URL"),
            &SecretString::from(TOKEN.to_owned()),
            CapabilityProfile::ReadOnly,
        )
        .expect("connection persists");
    drop(store);

    let bytes = fs::read(&path).expect("database is readable");
    let database_contents = String::from_utf8_lossy(&bytes);
    assert!(!database_contents.contains(TOKEN));

    let reopened = TenantStore::open(&path, KEY).expect("store reopens");
    let connection = reopened
        .load_connection(user.id)
        .expect("connection decrypts")
        .expect("connection exists");
    assert_eq!(connection.token.expose_secret(), TOKEN);
}

#[test]
fn users_cannot_load_each_others_connection() {
    let (_directory, path) = store_path();
    let store = TenantStore::open(&path, KEY).expect("store opens");
    let user_a = store
        .upsert_user("github-user-a", "fixture-user-a")
        .expect("user A persists");
    let user_b = store
        .upsert_user("github-user-b", "fixture-user-b")
        .expect("user B persists");

    store
        .save_connection(
            user_a.id,
            &Url::parse("https://a.coolify.example.test").expect("fixture URL"),
            &SecretString::from("fixture-token-a".to_owned()),
            CapabilityProfile::ReadOnly,
        )
        .expect("connection A persists");

    assert!(
        store
            .load_connection(user_b.id)
            .expect("tenant-scoped lookup succeeds")
            .is_none()
    );
}

#[test]
fn same_key_survives_restart_and_wrong_key_fails_closed() {
    let (_directory, path) = store_path();
    let store = TenantStore::open(&path, KEY).expect("store opens");
    let user = store
        .upsert_user("github-user-a", "fixture-user-a")
        .expect("user persists");
    store
        .save_connection(
            user.id,
            &Url::parse("https://coolify.example.test").expect("fixture URL"),
            &SecretString::from(TOKEN.to_owned()),
            CapabilityProfile::ReadOnly,
        )
        .expect("connection persists");
    drop(store);

    let reopened = TenantStore::open(&path, KEY).expect("same key reopens");
    assert!(
        reopened
            .load_connection(user.id)
            .expect("same key lookup succeeds")
            .is_some()
    );
    drop(reopened);

    let wrong_key = TenantStore::open(&path, OTHER_KEY).expect("wrong key can open schema");
    let error = wrong_key
        .load_connection(user.id)
        .expect_err("wrong key refuses decryption");
    assert!(matches!(error, TenantError::Decryption));
}

#[test]
fn database_file_is_private_and_failed_save_rolls_back() {
    let (_directory, path) = store_path();
    let store = TenantStore::open(&path, KEY).expect("store opens");
    let user = store
        .upsert_user("github-user-a", "fixture-user-a")
        .expect("user persists");

    let error = store
        .save_connection(
            user.id,
            &Url::parse("https://coolify.example.test").expect("fixture URL"),
            &SecretString::from(String::new()),
            CapabilityProfile::ReadOnly,
        )
        .expect_err("empty tokens cannot save connections");
    assert!(matches!(error, TenantError::InvalidToken));
    assert!(
        store
            .load_connection(user.id)
            .expect("lookup succeeds")
            .is_none()
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&path)
            .expect("database metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }
}
