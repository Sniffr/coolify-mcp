use crate::{
    model::*,
    persistence::OAuthStateStore,
    pkce::{canonical_resource, redirect_uri_matches, verify_pkce},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::Rng;
use sha2::{Digest, Sha256};
use std::{collections::HashSet, path::PathBuf, sync::Mutex};
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
#[derive(serde::Serialize, serde::Deserialize)]
struct StateClaims {
    client_id: String,
    redirect_uri: String,
    resource: String,
    exp: u64,
    nonce: String,
}
struct Inner {
    data: PersistedState,
    state_key: [u8; 32],
    used_states: HashSet<String>,
    path: Option<PathBuf>,
}
pub struct OAuthProvider {
    issuer: String,
    resource: String,
    inner: Mutex<Inner>,
}
impl OAuthProvider {
    pub fn new(issuer: String, resource_path: String) -> Self {
        let resource = format!("{}{}", issuer, resource_path);
        Self {
            issuer,
            resource,
            inner: Mutex::new(Inner {
                data: PersistedState::default(),
                state_key: random_bytes(),
                used_states: HashSet::new(),
                path: None,
            }),
        }
    }
    pub fn with_store(
        issuer: String,
        resource_path: String,
        path: PathBuf,
    ) -> Result<Self, OAuthError> {
        let resource = format!("{}{}", issuer, resource_path);
        let store =
            OAuthStateStore::load(&path).map_err(|e| OAuthError::Persistence(e.to_string()))?;
        Ok(Self {
            issuer,
            resource,
            inner: Mutex::new(Inner {
                data: store.state().clone(),
                state_key: random_bytes(),
                used_states: HashSet::new(),
                path: Some(path),
            }),
        })
    }
    pub fn issuer(&self) -> &str {
        &self.issuer
    }
    pub fn resource(&self) -> String {
        self.resource.clone()
    }
    pub fn create_state(&self, client_id: &str, redirect_uri: &str) -> Result<String, OAuthError> {
        let i = self.inner.lock().unwrap();
        let c = i
            .data
            .clients
            .get(&hash(client_id))
            .ok_or(OAuthError::InvalidClient)?;
        if !c
            .redirect_uris
            .iter()
            .any(|r| redirect_uri_matches(r, redirect_uri))
        {
            return Err(OAuthError::InvalidRequest("redirect mismatch".into()));
        }
        let nonce = random();
        let claims = StateClaims {
            client_id: client_id.into(),
            redirect_uri: redirect_uri.into(),
            resource: self.resource.clone(),
            exp: now() + 600,
            nonce,
        };
        Ok(sign_state(&i.state_key, &claims))
    }
    pub fn register(&self, req: RegistrationRequest) -> Result<RegistrationResponse, OAuthError> {
        if req.redirect_uris.is_empty() || req.redirect_uris.iter().any(|u| !valid_redirect(u)) {
            return Err(OAuthError::InvalidRequest("invalid redirect URI".into()));
        }
        let id = random();
        let secret = random();
        let c = Client {
            client_id: id.clone(),
            client_secret_hash: hash(&secret),
            redirect_uris: req.redirect_uris.clone(),
            client_name: req.client_name,
        };
        let mut i = self.inner.lock().unwrap();
        let previous = i.data.clone();
        i.data.clients.insert(hash(&id), c);
        if let Err(error) = persist(&i) {
            i.data = previous;
            return Err(error);
        }
        Ok(RegistrationResponse {
            client_id: id,
            client_secret: secret,
            redirect_uris: req.redirect_uris,
        })
    }
    pub fn registered_client(&self, id: &str) -> Option<Client> {
        self.inner
            .lock()
            .unwrap()
            .data
            .clients
            .get(&hash(id))
            .cloned()
    }
    pub fn authorize(&self, req: AuthorizeRequest) -> Result<AuthorizationResponse, OAuthError> {
        let resource = canonical_resource(&req.resource)?;
        if resource != self.resource {
            return Err(OAuthError::InvalidRequest("resource mismatch".into()));
        }
        if req.response_type != "code"
            || req.code_challenge_method != "S256"
            || URL_SAFE_NO_PAD
                .decode(&req.code_challenge)
                .map_or(true, |b| b.len() != 32)
        {
            return Err(OAuthError::InvalidRequest("PKCE S256 required".into()));
        }
        let mut i = self.inner.lock().unwrap();
        let c = i
            .data
            .clients
            .get(&hash(&req.client_id))
            .ok_or_else(|| OAuthError::InvalidRequest("unknown client".into()))?;
        if !c
            .redirect_uris
            .iter()
            .any(|r| redirect_uri_matches(r, &req.redirect_uri))
        {
            return Err(OAuthError::InvalidRequest("redirect mismatch".into()));
        }
        let previous = i.data.clone();
        let previous_states = i.used_states.clone();
        let claims = verify_state(&i.state_key, &req.state)
            .ok_or_else(|| OAuthError::InvalidRequest("invalid state".into()))?;
        if claims.exp < now()
            || claims.client_id != req.client_id
            || claims.redirect_uri != req.redirect_uri
            || claims.resource != self.resource
            || !i.used_states.insert(claims.nonce)
        {
            return Err(OAuthError::InvalidRequest("invalid state".into()));
        }
        let code = random();
        i.data.codes.insert(
            hash(&code),
            AuthorizationCode {
                code_hash: hash(&code),
                client_id: req.client_id,
                redirect_uri: req.redirect_uri.clone(),
                resource,
                scope: req.scope,
                challenge: req.code_challenge,
                expires_at: now() + 600,
                used: false,
            },
        );
        if let Err(error) = persist(&i) {
            i.data = previous;
            i.used_states = previous_states;
            return Err(error);
        }
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
        let mut i = self.inner.lock().unwrap();
        let c = i
            .data
            .clients
            .get(&hash(&req.client_id))
            .ok_or(OAuthError::InvalidClient)?;
        if !ct_eq(
            c.client_secret_hash.as_bytes(),
            hash(req.client_secret.as_deref().unwrap_or("")).as_bytes(),
        ) {
            return Err(OAuthError::InvalidClient);
        }
        let code = i
            .data
            .codes
            .values()
            .find(|code| ct_eq(code.code_hash.as_bytes(), hash(&req.code).as_bytes()))
            .ok_or(OAuthError::InvalidGrant)?
            .clone();
        if code.used
            || code.expires_at < now()
            || code.client_id != req.client_id
            || req.redirect_uri.as_deref() != Some(&code.redirect_uri)
            || req.resource.as_deref() != Some(&self.resource)
            || code.resource != self.resource
            || !verify_pkce(req.code_verifier.as_deref().unwrap_or(""), &code.challenge)
        {
            return Err(OAuthError::InvalidGrant);
        }
        let previous = i.data.clone();
        i.data.codes.get_mut(&hash(&req.code)).unwrap().used = true;
        let gid = random();
        i.data
            .grants
            .insert(gid.clone(), GrantFamily { revoked: false });
        let out = issue(
            &mut i.data,
            &gid,
            &req.client_id,
            &self.resource,
            &code.scope,
        );
        if let Err(error) = persist(&i) {
            i.data = previous;
            return Err(error);
        }
        Ok(out)
    }
    pub fn refresh(&self, token: &str) -> Result<TokenResponse, OAuthError> {
        let mut i = self.inner.lock().unwrap();
        let key = hash(token);
        let found = i
            .data
            .tokens
            .iter()
            .find(|(k, v)| ct_eq(k.as_bytes(), key.as_bytes()) && v.kind.starts_with("refresh"))
            .map(|(_, v)| {
                (
                    v.grant_id.clone(),
                    v.rotated || v.revoked || v.expires_at < now(),
                    v.client_id.clone(),
                    v.resource.clone(),
                    v.scope.clone(),
                )
            })
            .ok_or(OAuthError::InvalidGrant)?;
        let (gid, bad, cid, res, scope) = found;
        if i.data.grants.get(&gid).is_some_and(|grant| grant.revoked) {
            return Err(OAuthError::InvalidGrant);
        }
        let previous = i.data.clone();
        if bad || res != self.resource {
            if let Some(g) = i.data.grants.get_mut(&gid) {
                g.revoked = true
            }
            for t in i.data.tokens.values_mut().filter(|t| t.grant_id == gid) {
                t.revoked = true
            }
            if let Err(error) = persist(&i) {
                i.data = previous;
                return Err(error);
            }
            return Err(OAuthError::InvalidGrant);
        }
        if let Some(t) = i.data.tokens.get_mut(&key) {
            t.rotated = true;
        }
        let out = issue(&mut i.data, &gid, &cid, &res, &scope);
        if let Err(error) = persist(&i) {
            i.data = previous;
            return Err(error);
        }
        Ok(out)
    }
    pub fn verify_bearer(&self, token: &str, resource: &str) -> Result<String, OAuthError> {
        if canonical_resource(resource).ok().as_deref() != Some(&self.resource) {
            return Err(OAuthError::InvalidToken);
        }
        let i = self.inner.lock().unwrap();
        let key = hash(token);
        let t = i
            .data
            .tokens
            .values()
            .find(|t| {
                t.kind.starts_with("access")
                    && !t.revoked
                    && t.expires_at >= now()
                    && ct_eq(t.token_hash.as_bytes(), key.as_bytes())
            })
            .ok_or(OAuthError::InvalidToken)?;
        if t.resource != self.resource {
            Err(OAuthError::InvalidToken)
        } else {
            Ok(t.client_id.clone())
        }
    }
    pub fn load_store(&self, store: &OAuthStateStore) {
        self.inner.lock().unwrap().data = store.state().clone()
    }
    pub fn save_store(&self, store: &OAuthStateStore) -> Result<(), OAuthError> {
        let i = self.inner.lock().unwrap();
        store
            .save(&i.data)
            .map_err(|e| OAuthError::Persistence(e.to_string()))
    }
    pub fn flush(&self) -> Result<(), OAuthError> {
        let i = self.inner.lock().unwrap();
        persist(&i)
    }
}
fn issue(s: &mut PersistedState, gid: &str, cid: &str, res: &str, scope: &str) -> TokenResponse {
    let a = random();
    let r = random();
    s.tokens.insert(
        hash(&a),
        TokenRecord {
            token_hash: hash(&a),
            grant_id: gid.into(),
            client_id: cid.into(),
            resource: res.into(),
            scope: scope.into(),
            expires_at: now() + 3600,
            revoked: false,
            rotated: false,
            kind: format!("access:{}", hash(&a)),
        },
    );
    s.tokens.insert(
        hash(&r),
        TokenRecord {
            token_hash: hash(&r),
            grant_id: gid.into(),
            client_id: cid.into(),
            resource: res.into(),
            scope: scope.into(),
            expires_at: now() + 86400,
            revoked: false,
            rotated: false,
            kind: format!("refresh:{}", hash(&r)),
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
fn persist(i: &Inner) -> Result<(), OAuthError> {
    if let Some(p) = &i.path {
        OAuthStateStore::new(p.clone())
            .save(&i.data)
            .map_err(|e| OAuthError::Persistence(e.to_string()))?
    }
    Ok(())
}
fn valid_redirect(v: &str) -> bool {
    url::Url::parse(v).is_ok_and(|u| {
        u.host_str().is_some()
            && u.username().is_empty()
            && u.password().is_none()
            && u.query().is_none()
            && u.fragment().is_none()
            && (u.scheme() == "https"
                || (u.scheme() == "http"
                    && matches!(u.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))))
    })
}
fn sign_state(k: &[u8; 32], c: &StateClaims) -> String {
    let p = URL_SAFE_NO_PAD.encode(serde_json::to_vec(c).unwrap());
    format!("{}.{}", p, URL_SAFE_NO_PAD.encode(mac(k, p.as_bytes())))
}
fn verify_state(k: &[u8; 32], v: &str) -> Option<StateClaims> {
    let (p, m) = v.split_once('.')?;
    let got = URL_SAFE_NO_PAD.decode(m).ok()?;
    if !ct_eq(&got, &mac(k, p.as_bytes())) {
        return None;
    }
    serde_json::from_slice(&URL_SAFE_NO_PAD.decode(p).ok()?).ok()
}
fn mac(k: &[u8; 32], v: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(k);
    h.update(v);
    h.finalize().into()
}
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |x, (l, r)| x | l ^ r) == 0
}
fn hash(v: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(v.as_bytes()))
}
fn random_bytes() -> [u8; 32] {
    let mut b = [0; 32];
    rand::rng().fill(&mut b);
    b
}
fn random() -> String {
    URL_SAFE_NO_PAD.encode(random_bytes())
}
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
