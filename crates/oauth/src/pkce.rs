use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};
use url::Url;

pub fn verify_pkce(verifier: &str, challenge: &str) -> bool {
    if !(43..=128).contains(&verifier.len()) {
        return false;
    }
    let digest = Sha256::digest(verifier.as_bytes());
    let expected = URL_SAFE_NO_PAD.encode(digest);
    constant_time_eq(expected.as_bytes(), challenge.as_bytes())
}
pub fn redirect_uri_matches(registered: &str, requested: &str) -> bool {
    let (Ok(a), Ok(b)) = (Url::parse(registered), Url::parse(requested)) else {
        return false;
    };
    if a.fragment().is_some()
        || b.fragment().is_some()
        || !a.username().is_empty()
        || !b.username().is_empty()
        || a.password().is_some()
        || b.password().is_some()
        || a.query().is_some()
        || b.query().is_some()
        || a.host_str().is_none()
        || b.host_str().is_none()
    {
        return false;
    }
    if a.scheme() != b.scheme()
        || a.host_str() != b.host_str()
        || a.path() != b.path()
        || a.query() != b.query()
    {
        return false;
    }
    if a.scheme() == "http" && !is_loopback(a.host_str()) {
        return a.port() == b.port();
    }
    if a.scheme() == "http" && is_loopback(a.host_str()) {
        return true;
    }
    a.port() == b.port()
}
pub fn canonical_resource(value: &str) -> Result<String, crate::OAuthError> {
    let u = Url::parse(value)
        .map_err(|_| crate::OAuthError::InvalidRequest("invalid resource".into()))?;
    let loopback = matches!(
        u.host_str(),
        Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
    );
    if !(u.scheme() == "https" || (u.scheme() == "http" && loopback))
        || u.fragment().is_some()
        || u.query().is_some()
        || !u.username().is_empty()
        || u.password().is_some()
        || u.host_str().is_none()
        || u.path() != "/mcp"
    {
        return Err(crate::OAuthError::InvalidRequest(
            "resource must be an HTTPS /mcp URL (plain HTTP loopback only)".into(),
        ));
    }
    let host = u
        .host_str()
        .ok_or_else(|| crate::OAuthError::InvalidRequest("resource host required".into()))?;
    let port = u.port().map(|p| format!(":{p}")).unwrap_or_default();
    Ok(format!("{}://{}{}{}", u.scheme(), host, port, u.path()))
}
fn is_loopback(host: Option<&str>) -> bool {
    matches!(host, Some("localhost" | "127.0.0.1" | "[::1]" | "::1"))
}
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |x, (l, r)| x | (l ^ r)) == 0
}
