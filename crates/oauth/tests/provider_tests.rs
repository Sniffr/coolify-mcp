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
    let _ = TokenRequest {
        grant_type: "authorization_code".into(),
        code: "x".into(),
        redirect_uri: None,
        client_id: client.client_id.clone(),
        client_secret: Some("wrong".into()),
        code_verifier: Some("x".into()),
        refresh_token: None,
        resource: Some("https://server.test/mcp".into()),
    };
}

#[test]
fn exact_resource_and_code_single_use_and_refresh_replay_revokes_family() {
    let provider = OAuthProvider::new("https://server.test".into(), "/mcp".into());
    let c = provider
        .register(RegistrationRequest {
            redirect_uris: vec!["https://client.test/cb".into()],
            client_name: None,
        })
        .unwrap();
    let verifier = "a-secret-verifier-that-is-long-enough-123456789";
    let challenge =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(sha2::Sha256::digest(verifier));
    let a = provider
        .authorize(AuthorizeRequest {
            client_id: c.client_id.clone(),
            redirect_uri: "https://client.test/cb".into(),
            response_type: "code".into(),
            resource: "https://server.test/mcp".into(),
            scope: "mcp".into(),
            state: "state".into(),
            code_challenge: challenge,
            code_challenge_method: "S256".into(),
        })
        .unwrap();
    let t = provider
        .exchange_code(TokenRequest {
            grant_type: "authorization_code".into(),
            code: a.code,
            redirect_uri: Some("https://client.test/cb".into()),
            client_id: c.client_id.clone(),
            client_secret: Some(c.client_secret),
            code_verifier: Some(verifier.into()),
            refresh_token: None,
            resource: Some("https://server.test/mcp".into()),
        })
        .unwrap();
    assert!(
        provider
            .verify_bearer(&t.access_token, "https://server.test/mcp")
            .is_ok()
    );
    assert!(
        provider
            .verify_bearer(&t.access_token, "https://evil.test/mcp")
            .is_err()
    );
    assert!(
        provider
            .exchange_code(TokenRequest {
                grant_type: "authorization_code".into(),
                code: "already-used".into(),
                redirect_uri: None,
                client_id: c.client_id,
                client_secret: None,
                code_verifier: None,
                refresh_token: None,
                resource: None
            })
            .is_err()
    );
    let rotated = provider.refresh(&t.refresh_token).unwrap();
    assert!(provider.refresh(&t.refresh_token).is_err());
    assert!(
        provider
            .verify_bearer(&rotated.access_token, "https://server.test/mcp")
            .is_err()
    );
}
