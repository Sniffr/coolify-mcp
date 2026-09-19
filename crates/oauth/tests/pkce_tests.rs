use base64::Engine;
use oauth::{redirect_uri_matches, verify_pkce};
use sha2::Digest;

#[test]
fn pkce_s256_is_strict_and_redirects_are_exact() {
    let verifier = "0123456789012345678901234567890123456789012";
    let challenge =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(sha2::Sha256::digest(verifier));
    assert!(verify_pkce(verifier, &challenge));
    assert!(!verify_pkce("wrong", &challenge));
    assert!(!redirect_uri_matches(
        "https://a.test/cb",
        "https://a.test/cb/"
    ));
    assert!(redirect_uri_matches(
        "http://localhost:123/cb",
        "http://localhost:456/cb"
    ));
    assert!(!redirect_uri_matches(
        "https://localhost:123/cb",
        "https://localhost:456/cb"
    ));
    assert!(!redirect_uri_matches(
        "https://client.test/cb?x=1",
        "https://client.test/cb?x=1"
    ));
}
