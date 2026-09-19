use base64::Engine;
use oauth::{
    AuthorizeRequest, OAuthError, OAuthProvider, RegistrationRequest, TokenRequest,
    canonical_resource, redirect_uri_matches,
};
use sha2::Digest;

#[test]
fn rejects_oauth_attack_inputs() {
    assert!(canonical_resource("https://example.test/mcp").is_ok());
    assert!(canonical_resource("https://example.test/mcp#fragment").is_err());
    assert!(!redirect_uri_matches(
        "https://client.test/cb",
        "https://client.test/other"
    ));
    assert!(redirect_uri_matches(
        "http://127.0.0.1:1234/cb",
        "http://127.0.0.1:9876/cb"
    ));
    assert!(!redirect_uri_matches(
        "http://127.0.0.1:1234/cb",
        "http://127.0.0.1:9876/other"
    ));
    assert!(!oauth::verify_pkce("weak", "not-a-valid-challenge"));
    let provider = OAuthProvider::new("https://server.test".into(), "/mcp".into());
    let client = provider
        .register(RegistrationRequest {
            redirect_uris: vec!["https://client.test/cb".into()],
            client_name: Some("test".into()),
        })
        .unwrap();
    assert!(provider.registered_client(&client.client_id).is_some());
    let err = provider
        .authorize(AuthorizeRequest {
            client_id: "unknown".into(),
            redirect_uri: "https://client.test/cb".into(),
            response_type: "code".into(),
            resource: "https://server.test/mcp".into(),
            scope: "mcp".into(),
            state: "s".into(),
            code_challenge: "bad".into(),
            code_challenge_method: "plain".into(),
        })
        .unwrap_err();
    assert!(matches!(err, OAuthError::InvalidRequest(_)));
}
#[test]
fn signed_state_rejects_tampering_and_replay() {
    let p = OAuthProvider::new("https://server.test".into(), "/mcp".into());
    let c = p
        .register(RegistrationRequest {
            redirect_uris: vec!["https://client.test/cb".into()],
            client_name: None,
        })
        .unwrap();
    let state = p
        .create_state(&c.client_id, "https://client.test/cb")
        .unwrap();
    let mut bad = state.clone();
    bad.push('x');
    let v = "a-secret-verifier-that-is-long-enough-123456789";
    let ch = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(sha2::Sha256::digest(v));
    let req = |s| AuthorizeRequest {
        client_id: c.client_id.clone(),
        redirect_uri: "https://client.test/cb".into(),
        response_type: "code".into(),
        resource: "https://server.test/mcp".into(),
        scope: "mcp".into(),
        state: s,
        code_challenge: ch.clone(),
        code_challenge_method: "S256".into(),
    };
    assert!(p.authorize(req(bad)).is_err());
    assert!(p.authorize(req(state.clone())).is_ok());
    assert!(p.authorize(req(state)).is_err());
}
#[test]
fn exact_resource_and_code_single_use_and_refresh_replay_revokes_family() {
    let p = OAuthProvider::new("https://server.test".into(), "/mcp".into());
    let c = p
        .register(RegistrationRequest {
            redirect_uris: vec!["https://client.test/cb".into()],
            client_name: None,
        })
        .unwrap();
    let v = "a-secret-verifier-that-is-long-enough-123456789";
    let ch = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(sha2::Sha256::digest(v));
    let a = p
        .authorize(AuthorizeRequest {
            client_id: c.client_id.clone(),
            redirect_uri: "https://client.test/cb".into(),
            response_type: "code".into(),
            resource: "https://server.test/mcp".into(),
            scope: "mcp".into(),
            state: p
                .create_state(&c.client_id, "https://client.test/cb")
                .unwrap(),
            code_challenge: ch,
            code_challenge_method: "S256".into(),
        })
        .unwrap();
    let t = p
        .exchange_code(TokenRequest {
            grant_type: "authorization_code".into(),
            code: a.code.clone(),
            redirect_uri: Some("https://client.test/cb".into()),
            client_id: c.client_id.clone(),
            client_secret: Some(c.client_secret.clone()),
            code_verifier: Some(v.into()),
            refresh_token: None,
            resource: Some("https://server.test/mcp".into()),
        })
        .unwrap();
    assert!(
        p.verify_bearer(&t.access_token, "https://server.test/mcp")
            .is_ok()
    );
    assert!(
        p.verify_bearer(&t.access_token, "https://evil.test/mcp")
            .is_err()
    );
    assert!(
        p.exchange_code(TokenRequest {
            grant_type: "authorization_code".into(),
            code: a.code,
            redirect_uri: Some("https://client.test/cb".into()),
            client_id: c.client_id,
            client_secret: Some(c.client_secret),
            code_verifier: Some(v.into()),
            refresh_token: None,
            resource: Some("https://server.test/mcp".into())
        })
        .is_err()
    );
    let r = p.refresh(&t.refresh_token).unwrap();
    assert!(p.refresh(&t.refresh_token).is_err());
    assert!(
        p.verify_bearer(&r.access_token, "https://server.test/mcp")
            .is_err()
    );
}
#[test]
fn expired_authorization_code_is_rejected() {
    use oauth::{AuthorizationCode, Client, OAuthStateStore, PersistedState};
    use std::collections::HashMap;
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("oauth.json");
    let id = "client-id";
    let secret = "client-secret";
    let code = "expired-code";
    let h =
        |v: &str| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(sha2::Sha256::digest(v));
    let s = PersistedState {
        clients: HashMap::from([(
            h(id),
            Client {
                client_id: id.into(),
                client_secret_hash: h(secret),
                redirect_uris: vec!["https://client.test/cb".into()],
                client_name: None,
            },
        )]),
        codes: HashMap::from([(
            h(code),
            AuthorizationCode {
                code_hash: h(code),
                client_id: id.into(),
                redirect_uri: "https://client.test/cb".into(),
                resource: "https://server.test/mcp".into(),
                scope: "mcp".into(),
                challenge: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into(),
                expires_at: 0,
                used: false,
            },
        )]),
        ..PersistedState::default()
    };
    OAuthStateStore::new(path.clone()).save(&s).unwrap();
    let p = OAuthProvider::with_store("https://server.test".into(), "/mcp".into(), path).unwrap();
    assert!(
        p.exchange_code(TokenRequest {
            grant_type: "authorization_code".into(),
            code: code.into(),
            redirect_uri: Some("https://client.test/cb".into()),
            client_id: id.into(),
            client_secret: Some(secret.into()),
            code_verifier: Some("a-secret-verifier-that-is-long-enough-123456789".into()),
            refresh_token: None,
            resource: Some("https://server.test/mcp".into())
        })
        .is_err()
    );
}
