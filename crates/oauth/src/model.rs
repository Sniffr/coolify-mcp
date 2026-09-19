use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Client {
    pub client_id: String,
    pub client_secret_hash: String,
    pub redirect_uris: Vec<String>,
    pub client_name: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrationRequest {
    pub redirect_uris: Vec<String>,
    pub client_name: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrationResponse {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uris: Vec<String>,
}
#[derive(Debug, Clone)]
pub struct AuthorizeRequest {
    pub client_id: String,
    pub redirect_uri: String,
    pub response_type: String,
    pub resource: String,
    pub scope: String,
    pub state: String,
    pub code_challenge: String,
    pub code_challenge_method: String,
}
#[derive(Debug, Clone)]
pub struct AuthorizationResponse {
    pub code: String,
    pub state: String,
    pub redirect_uri: String,
}
#[derive(Debug, Clone)]
pub struct TokenRequest {
    pub grant_type: String,
    pub code: String,
    pub redirect_uri: Option<String>,
    pub client_id: String,
    pub client_secret: Option<String>,
    pub code_verifier: Option<String>,
    pub refresh_token: Option<String>,
    pub resource: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
    pub expires_in: u64,
    pub scope: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PersistedState {
    pub clients: HashMap<String, Client>,
    pub codes: HashMap<String, AuthorizationCode>,
    pub tokens: HashMap<String, TokenRecord>,
    pub grants: HashMap<String, GrantFamily>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorizationCode {
    pub code_hash: String,
    pub client_id: String,
    pub redirect_uri: String,
    pub resource: String,
    pub scope: String,
    pub challenge: String,
    pub expires_at: u64,
    pub used: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenRecord {
    pub token_hash: String,
    pub grant_id: String,
    pub client_id: String,
    pub resource: String,
    pub scope: String,
    pub expires_at: u64,
    pub revoked: bool,
    pub rotated: bool,
    pub kind: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrantFamily {
    pub revoked: bool,
}
