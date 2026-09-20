use reqwest::{Client, Response, header};
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use thiserror::Error;
use url::Url;

const AUTHORIZE_ENDPOINT: &str = "https://github.com/login/oauth/authorize";
const TOKEN_ENDPOINT: &str = "https://github.com/login/oauth/access_token";
const USER_ENDPOINT: &str = "https://api.github.com/user";
const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const MAX_RESPONSE_BODY_BYTES: usize = 64 * 1024;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum IdentityError {
    #[error("GitHub identity request failed")]
    RequestFailed,
    #[error("GitHub rejected the identity exchange")]
    GitHubRejected,
    #[error("GitHub returned an invalid identity response")]
    MalformedResponse,
    #[error("GitHub response was too large")]
    ResponseTooLarge,
    #[error("GitHub identity request is invalid")]
    InvalidRequest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitHubUser {
    pub github_id: String,
    pub login: String,
    pub display_name: Option<String>,
}

pub struct GitHubIdentityProvider {
    client_id: String,
    client_secret: SecretString,
    callback_url: Url,
    http: Client,
    token_endpoint: Url,
    user_endpoint: Url,
}

impl GitHubIdentityProvider {
    /// Construct the production provider. The supplied client is accepted for
    /// compatibility with the transport's shared-client wiring; requests use a
    /// dedicated redirectless client so upstream responses cannot be followed.
    pub fn new(
        client_id: String,
        client_secret: SecretString,
        callback_url: Url,
        _http: Client,
    ) -> Self {
        Self::from_parts(
            client_id,
            client_secret,
            callback_url,
            redirectless_client(),
            Url::parse(TOKEN_ENDPOINT).expect("static GitHub token endpoint must be valid"),
            Url::parse(USER_ENDPOINT).expect("static GitHub user endpoint must be valid"),
        )
    }

    /// Construct a provider against explicit endpoints for deterministic local
    /// fixtures. Production callers should use [`Self::new`]. The supplied
    /// client keeps the constructor compatible with the transport seam; the
    /// provider always owns the redirectless client used for requests.
    pub fn with_endpoints(
        client_id: String,
        client_secret: SecretString,
        callback_url: Url,
        _http: Client,
        token_endpoint: Url,
        user_endpoint: Url,
    ) -> Self {
        Self::from_parts(
            client_id,
            client_secret,
            callback_url,
            redirectless_client(),
            token_endpoint,
            user_endpoint,
        )
    }

    fn from_parts(
        client_id: String,
        client_secret: SecretString,
        callback_url: Url,
        http: Client,
        token_endpoint: Url,
        user_endpoint: Url,
    ) -> Self {
        Self {
            client_id,
            client_secret,
            callback_url,
            http,
            token_endpoint,
            user_endpoint,
        }
    }

    pub fn authorization_url(&self, state: &str) -> Url {
        let mut url = Url::parse(AUTHORIZE_ENDPOINT).expect("static GitHub endpoint must be valid");
        url.query_pairs_mut()
            .append_pair("client_id", &self.client_id)
            .append_pair("redirect_uri", self.callback_url.as_str())
            .append_pair("scope", "read:user")
            .append_pair("state", state);
        url
    }

    pub async fn exchange_callback(&self, code: &str) -> Result<GitHubUser, IdentityError> {
        if code.is_empty() {
            return Err(IdentityError::InvalidRequest);
        }

        let token_response = self
            .http
            .post(self.token_endpoint.clone())
            .header(header::ACCEPT, "application/json")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .form(&[
                ("client_id", self.client_id.as_str()),
                ("client_secret", self.client_secret.expose_secret()),
                ("code", code),
                ("redirect_uri", self.callback_url.as_str()),
            ])
            .timeout(REQUEST_TIMEOUT)
            .send()
            .await
            .map_err(|_| IdentityError::RequestFailed)?;
        let token_status = token_response.status();
        let token_body = read_bounded(token_response).await?;
        if !token_status.is_success() {
            return Err(IdentityError::GitHubRejected);
        }
        let token: TokenResponse =
            serde_json::from_slice(&token_body).map_err(|_| IdentityError::MalformedResponse)?;
        let access_token = token
            .access_token
            .filter(|value| !value.is_empty())
            .ok_or(IdentityError::MalformedResponse)?;
        let access_token = SecretString::from(access_token);

        let user_response = self
            .http
            .get(self.user_endpoint.clone())
            .header(header::ACCEPT, "application/vnd.github+json")
            .header(header::USER_AGENT, "coolify-mcp-identity")
            .bearer_auth(access_token.expose_secret())
            .timeout(REQUEST_TIMEOUT)
            .send()
            .await
            .map_err(|_| IdentityError::RequestFailed)?;
        let user_status = user_response.status();
        let user_body = read_bounded(user_response).await?;
        if !user_status.is_success() {
            return Err(IdentityError::GitHubRejected);
        }
        let user: UserResponse =
            serde_json::from_slice(&user_body).map_err(|_| IdentityError::MalformedResponse)?;
        let github_id = user
            .id
            .as_u64()
            .filter(|id| *id > 0)
            .map(|id| id.to_string())
            .ok_or(IdentityError::MalformedResponse)?;
        if user.login.is_empty() {
            return Err(IdentityError::MalformedResponse);
        }

        Ok(GitHubUser {
            github_id,
            login: user.login,
            display_name: user.name.filter(|name| !name.is_empty()),
        })
    }
}

fn redirectless_client() -> Client {
    Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("static GitHub HTTP client configuration must be valid")
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
}

#[derive(Deserialize)]
struct UserResponse {
    id: serde_json::Value,
    login: String,
    name: Option<String>,
}

async fn read_bounded(mut response: Response) -> Result<Vec<u8>, IdentityError> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_RESPONSE_BODY_BYTES as u64)
    {
        return Err(IdentityError::ResponseTooLarge);
    }
    let mut body = Vec::with_capacity(
        response
            .content_length()
            .unwrap_or(0)
            .min(MAX_RESPONSE_BODY_BYTES as u64) as usize,
    );
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| IdentityError::RequestFailed)?
    {
        if body.len().saturating_add(chunk.len()) > MAX_RESPONSE_BODY_BYTES {
            return Err(IdentityError::ResponseTooLarge);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}
