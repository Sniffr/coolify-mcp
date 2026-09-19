use crate::{
    model::*,
    persistence::OAuthStateStore,
    pkce::{canonical_resource, redirect_uri_matches, verify_pkce},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::Rng;
use sha2::{Digest, Sha256};
use std::sync::Mutex;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OAuthError {
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("invalid grant")]
    InvalidGrant,
    #[error("invalid client")]
    InvalidClient,
    #[error("invalid token")]
    InvalidToken,
    #[error("persistence failure: {0}")]
    Persistence(String),
}
pub struct OAuthProvider {
    issuer: String,
    resource_path: String,
    state: Mutex<PersistedState>,
}
impl OAuthProvider {
    pub fn new(issuer: String, resource_path: String) -> Self {
        Self {
            issuer,
            resource_path,
            state: Mutex::new(PersistedState::default()),
        }
    }
    pub fn issuer(&self) -> &str {
        &self.issuer
    }
    pub fn resource(&self) -> String {
        format!("{}{}", self.issuer, self.resource_path)
    }
    pub fn register(&self, req: RegistrationRequest) -> Result<RegistrationResponse, OAuthError> {
        if req.redirect_uris.is_empty() || req.redirect_uris.iter().any(|u| !valid_redirect(u)) {
            return Err(OAuthError::InvalidRequest("invalid redirect URI".into()));
        }
        let id = random();
        let secret = random();
        let c = Client {
            client_id: id.clone(),
            client_secret: secret.clone(),
            redirect_uris: req.redirect_uris.clone(),
            client_name: req.client_name,
        };
        self.state.lock().unwrap().clients.insert(id.clone(), c);
        Ok(RegistrationResponse {
            client_id: id,
            client_secret: secret,
            redirect_uris: req.redirect_uris,
        })
    }
    pub fn registered_client(&self, id: &str) -> Option<Client> {
        self.state.lock().unwrap().clients.get(id).cloned()
    }
    pub fn authorize(&self, req: AuthorizeRequest) -> Result<AuthorizationResponse, OAuthError> {
        let resource = canonical_resource(&req.resource)?;
        let s = self.state.lock().unwrap();
        let c = s
            .clients
            .get(&req.client_id)
            .ok_or_else(|| OAuthError::InvalidRequest("unknown client".into()))?;
        if req.response_type != "code"
            || req.code_challenge_method != "S256"
            || base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(&req.code_challenge)
                .map_or(true, |b| b.len() != 32)
        {
            return Err(OAuthError::InvalidRequest("PKCE S256 required".into()));
        }
        if !c
            .redirect_uris
            .iter()
            .any(|x| redirect_uri_matches(x, &req.redirect_uri))
            || req.state.is_empty()
        {
            return Err(OAuthError::InvalidRequest(
                "redirect or state mismatch".into(),
            ));
        }
        drop(s);
        let code = random();
        let mut s = self.state.lock().unwrap();
        s.codes.insert(
            code.clone(),
            AuthorizationCode {
                hash: hash(&code),
                client_id: req.client_id,
                redirect_uri: req.redirect_uri.clone(),
                resource,
                scope: req.scope,
                challenge: req.code_challenge,
                expires_at: now() + 600,
                used: false,
            },
        );
        Ok(AuthorizationResponse {
            code,
            state: req.state,
            redirect_uri: req.redirect_uri,
        })
    }
    pub fn exchange_code(&self, req: TokenRequest) -> Result<TokenResponse, OAuthError> {
        if req.grant_type != "authorization_code" {
            return Err(OAuthError::InvalidGrant);
        }
        let mut s = self.state.lock().unwrap();
        let c = s
            .clients
            .get(&req.client_id)
            .ok_or(OAuthError::InvalidClient)?;
        if req.client_secret.as_deref() != Some(&c.client_secret) {
            return Err(OAuthError::InvalidClient);
        }
        let code = s
            .codes
            .get(&req.code)
            .ok_or(OAuthError::InvalidGrant)?
            .clone();
        if code.used
            || code.expires_at < now()
            || code.client_id != req.client_id
            || req.redirect_uri.as_deref() != Some(&code.redirect_uri)
            || req.resource.as_deref() != Some(&code.resource)
            || !verify_pkce(req.code_verifier.as_deref().unwrap_or(""), &code.challenge)
        {
            return Err(OAuthError::InvalidGrant);
        }
        s.codes.get_mut(&req.code).expect("code exists").used = true;
        let gid = random();
        s.grants.insert(gid.clone(), GrantFamily { revoked: false });
        Ok(issue(
            &mut s,
            &gid,
            &req.client_id,
            &code.resource,
            &code.scope,
        ))
    }
    pub fn refresh(&self, token: &str) -> Result<TokenResponse, OAuthError> {
        let mut s = self.state.lock().unwrap();
        let key = hash(token);
        let old = s
            .tokens
            .values_mut()
            .find(|x| x.hash == key && x.kind == "refresh")
            .ok_or(OAuthError::InvalidGrant)?;
        if old.rotated || old.revoked || old.expires_at < now() {
            let gid = old.grant_id.clone();
            if let Some(g) = s.grants.get_mut(&gid) {
                g.revoked = true
            };
            for t in s.tokens.values_mut().filter(|t| t.grant_id == gid) {
                t.revoked = true
            }
            return Err(OAuthError::InvalidGrant);
        }
        old.rotated = true;
        let (gid, cid, res, scope) = (
            old.grant_id.clone(),
            old.client_id.clone(),
            old.resource.clone(),
            old.scope.clone(),
        );
        if s.grants.get(&gid).is_some_and(|g| g.revoked) {
            return Err(OAuthError::InvalidGrant);
        }
        Ok(issue(&mut s, &gid, &cid, &res, &scope))
    }
    pub fn verify_bearer(&self, token: &str, resource: &str) -> Result<String, OAuthError> {
        let s = self.state.lock().unwrap();
        let t = s
            .tokens
            .values()
            .find(|x| {
                x.hash == hash(token) && x.kind == "access" && !x.revoked && x.expires_at >= now()
            })
            .ok_or(OAuthError::InvalidToken)?;
        if t.resource != resource {
            return Err(OAuthError::InvalidToken);
        }
        Ok(t.client_id.clone())
    }
    pub fn load_store(&self, store: &OAuthStateStore) {
        *self.state.lock().unwrap() = store.state().clone();
    }
    pub fn save_store(&self, store: &OAuthStateStore) -> Result<(), OAuthError> {
        store
            .save(&self.state.lock().unwrap())
            .map_err(|e| OAuthError::Persistence(e.to_string()))
    }
}
fn issue(s: &mut PersistedState, gid: &str, cid: &str, res: &str, scope: &str) -> TokenResponse {
    let a = random();
    let r = random();
    s.tokens.insert(
        hash(&a),
        TokenRecord {
            hash: hash(&a),
            grant_id: gid.into(),
            client_id: cid.into(),
            resource: res.into(),
            scope: scope.into(),
            expires_at: now() + 3600,
            revoked: false,
            rotated: false,
            kind: "access".into(),
        },
    );
    s.tokens.insert(
        hash(&r),
        TokenRecord {
            hash: hash(&r),
            grant_id: gid.into(),
            client_id: cid.into(),
            resource: res.into(),
            scope: scope.into(),
            expires_at: now() + 86400,
            revoked: false,
            rotated: false,
            kind: "refresh".into(),
        },
    );
    TokenResponse {
        access_token: a,
        refresh_token: r,
        token_type: "Bearer".into(),
        expires_in: 3600,
        scope: scope.into(),
    }
}
fn valid_redirect(v: &str) -> bool {
    url::Url::parse(v).is_ok_and(|u| {
        u.fragment().is_none()
            && (u.scheme() == "https"
                || (u.scheme() == "http"
                    && matches!(u.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))))
    })
}
fn random() -> String {
    let mut b = [0u8; 32];
    rand::rng().fill(&mut b);
    URL_SAFE_NO_PAD.encode(b)
}
fn hash(v: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(v.as_bytes()))
}
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
