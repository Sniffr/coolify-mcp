use crate::http::HttpConfig;
use axum::{
    Router,
    body::Bytes,
    extract::{ConnectInfo, DefaultBodyLimit, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Json, Redirect, Response},
    routing::{get, post},
};
use mcp_tools::{McpApplication, ToolResult};
use oauth::{AuthorizeRequest, OAuthError, RegistrationRequest, TokenRequest};
use safety::{AuditEvent, AuditLogger};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    fs::{File, OpenOptions},
    io::{BufRead, BufReader},
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tenant::UserId;
use tower_http::timeout::{RequestBodyTimeoutLayer, TimeoutLayer};

#[derive(Clone)]
struct RateLimiter {
    entries: Arc<Mutex<HashMap<String, (Instant, u32)>>>,
    max_entries: usize,
    max_requests: u32,
    window: Duration,
}
impl RateLimiter {
    fn new() -> Self {
        Self {
            entries: Arc::new(Mutex::new(HashMap::new())),
            max_entries: 1024,
            max_requests: 30,
            window: Duration::from_secs(60),
        }
    }
    fn allow(&self, key: String) -> bool {
        let mut e = self.entries.lock().unwrap();
        let now = Instant::now();
        if e.len() >= self.max_entries && !e.contains_key(&key) {
            e.retain(|_, (at, _)| now.duration_since(*at) < self.window);
            if e.len() >= self.max_entries {
                return false;
            }
        }
        let slot = e.entry(key).or_insert((now, 0));
        if now.duration_since(slot.0) >= self.window {
            *slot = (now, 0)
        };
        slot.1 += 1;
        slot.1 <= self.max_requests
    }
}
#[derive(Clone)]
struct PendingAuthorization {
    request: AuthorizeRequest,
    expires_at: Instant,
}

#[derive(Clone)]
struct GithubContinuation {
    mcp_state: String,
    expires_at: Instant,
}

#[derive(Default)]
struct BrowserState {
    pending: HashMap<String, PendingAuthorization>,
    github: HashMap<String, GithubContinuation>,
    sessions: HashMap<String, (UserId, Instant)>,
}

#[derive(Clone)]
struct AppState {
    app: Arc<dyn McpApplication>,
    config: HttpConfig,
    rate: RateLimiter,
    sessions: Arc<Mutex<HashMap<String, Instant>>>,
    browser: Arc<Mutex<BrowserState>>,
    audit: Option<Arc<Mutex<AuditLogger<File>>>>,
}
fn open_audit(config: &HttpConfig) -> Option<Arc<Mutex<AuditLogger<File>>>> {
    if let Some(parent) = config.audit_path.parent() {
        std::fs::create_dir_all(parent).ok()?;
    }
    let previous_hash = File::open(&config.audit_path)
        .ok()
        .and_then(|file| {
            BufReader::new(file)
                .lines()
                .map_while(Result::ok)
                .last()
                .and_then(|line| serde_json::from_str::<Value>(&line).ok())
                .and_then(|record| {
                    record
                        .get("hash")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                })
        })
        .unwrap_or_default();
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&config.audit_path)
        .ok()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ =
            std::fs::set_permissions(&config.audit_path, std::fs::Permissions::from_mode(0o600));
    }
    Some(Arc::new(Mutex::new(AuditLogger::with_previous_hash(
        file,
        previous_hash,
    ))))
}

fn audit_event(
    state: &AppState,
    client_id: Option<&str>,
    tool: &str,
    args: Option<&Value>,
    outcome: &str,
    status: StatusCode,
    started: Instant,
) {
    let Some(logger) = &state.audit else { return };
    let resource_id = args
        .and_then(|v| v.get("uuid").or_else(|| v.get("id")))
        .and_then(Value::as_str)
        .map(|v| v.chars().take(128).collect::<String>())
        .unwrap_or_default();
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string();
    let event = AuditEvent::new(
        timestamp,
        client_id,
        tool.chars().take(128).collect::<String>(),
        resource_id,
        outcome,
        status.as_u16(),
        started.elapsed().as_millis().min(u64::MAX as u128) as u64,
    );
    let _ = logger.lock().unwrap().record(event);
}

struct EmptyApp;
impl McpApplication for EmptyApp {
    fn tools(&self) -> Vec<mcp_tools::ToolSpec> {
        vec![]
    }
    fn call<'a>(
        &'a self,
        _: &'a str,
        _: Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ToolResult> + Send + 'a>> {
        Box::pin(async {
            ToolResult {
                text: "{}".into(),
                is_error: false,
            }
        })
    }
}
pub fn router(config: HttpConfig) -> Router {
    router_with_app(config, EmptyApp).layer(axum::Extension(ConnectInfo(
        "127.0.0.1:0".parse::<SocketAddr>().unwrap(),
    )))
}
pub fn router_with_app<A: McpApplication + 'static>(config: HttpConfig, app: A) -> Router {
    let audit = open_audit(&config);
    if audit.is_none() {
        config
            .persistence_health
            .store(false, std::sync::atomic::Ordering::Release);
    }
    let state = AppState {
        app: Arc::new(app),
        config,
        rate: RateLimiter::new(),
        sessions: Arc::new(Mutex::new(HashMap::new())),
        browser: Arc::new(Mutex::new(BrowserState::default())),
        audit,
    };
    let request_timeout = state.config.request_timeout;
    Router::new()
        .route("/healthz", get(health))
        .route("/.well-known/oauth-authorization-server", get(discovery))
        .route(
            "/.well-known/oauth-protected-resource",
            get(protected_resource),
        )
        .route("/oauth/register", post(register))
        .route("/oauth/authorize", get(authorize))
        .route("/oauth/state", post(oauth_state))
        .route("/oauth/token", post(token))
        .route("/auth/github/start", get(github_start))
        .route("/auth/github/callback", get(github_callback))
        .route("/mcp", post(mcp).get(mcp_get).delete(mcp_delete))
        .layer(DefaultBodyLimit::max(state.config.max_body_bytes))
        .layer(RequestBodyTimeoutLayer::new(request_timeout))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            request_timeout,
        ))
        .with_state(state)
}
async fn health(State(s): State<AppState>) -> Response {
    let healthy = s.config.persistence_available
        && s.config
            .persistence_health
            .load(std::sync::atomic::Ordering::Acquire);
    let status = if healthy { "ok" } else { "degraded" };
    let code = if healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (code, Json(json!({"status":status,"persistence":healthy}))).into_response()
}
async fn discovery(State(s): State<AppState>) -> impl IntoResponse {
    Json(
        json!({"issuer":crate::http::public_base(&s.config.public_url),"authorization_endpoint":format!("{}/oauth/authorize",crate::http::public_base(&s.config.public_url)),"token_endpoint":format!("{}/oauth/token",crate::http::public_base(&s.config.public_url)),"registration_endpoint":format!("{}/oauth/register",crate::http::public_base(&s.config.public_url)),"response_types_supported":["code"],"code_challenge_methods_supported":["S256"]}),
    )
}
async fn protected_resource(State(s): State<AppState>) -> impl IntoResponse {
    Json(
        json!({"resource":crate::http::mcp_resource_url(&s.config.public_url),"authorization_servers":[crate::http::public_base(&s.config.public_url)]}),
    )
}
fn client_key(s: &AppState, headers: &HeaderMap, peer: Option<SocketAddr>) -> String {
    if s.config.trusted_proxy
        && let Some(value) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok())
    {
        return value.chars().take(128).collect();
    }
    peer.map(|address| address.ip().to_string())
        .unwrap_or_else(|| "unknown-peer".into())
}
fn limited(s: &AppState, headers: &HeaderMap, peer: Option<SocketAddr>, endpoint: &str) -> bool {
    s.rate
        .allow(format!("{endpoint}:{}", client_key(s, headers, peer)))
}
async fn register(
    State(s): State<AppState>,
    headers: HeaderMap,
    peer: ConnectInfo<SocketAddr>,
    Json(req): Json<RegistrationRequest>,
) -> Response {
    if !limited(&s, &headers, Some(peer.0), "register") {
        return rate_error();
    }
    match s.config.oauth.register(req) {
        Ok(v) => (StatusCode::CREATED, Json(v)).into_response(),
        Err(e) => {
            mark_persistence_error(&s, &e);
            oauth_error(e)
        }
    }
}
#[derive(Deserialize)]
struct OAuthStateRequest {
    client_id: String,
    redirect_uri: String,
}
async fn oauth_state(
    State(s): State<AppState>,
    headers: HeaderMap,
    peer: ConnectInfo<SocketAddr>,
    Json(req): Json<OAuthStateRequest>,
) -> Response {
    if !limited(&s, &headers, Some(peer.0), "authorize") {
        return rate_error();
    }
    match s
        .config
        .oauth
        .create_state(&req.client_id, &req.redirect_uri)
    {
        Ok(state) => Json(json!({"state": state})).into_response(),
        Err(e) => {
            mark_persistence_error(&s, &e);
            oauth_error(e)
        }
    }
}
#[derive(Deserialize)]
struct GithubStartQuery {
    state: String,
}

#[derive(Deserialize)]
struct GithubCallbackQuery {
    state: Option<String>,
    code: Option<String>,
}

async fn github_start(
    State(s): State<AppState>,
    headers: HeaderMap,
    peer: ConnectInfo<SocketAddr>,
    Query(query): Query<GithubStartQuery>,
) -> Response {
    if !limited(&s, &headers, Some(peer.0), "github_start") {
        return rate_error();
    }
    let Some(hosted) = s.config.hosted_auth.as_ref() else {
        return auth_failure();
    };
    cleanup_browser(&s);
    if !s.browser.lock().unwrap().pending.contains_key(&query.state) {
        return auth_failure();
    }
    let github_state = uuid::Uuid::new_v4().to_string();
    s.browser.lock().unwrap().github.insert(
        github_state.clone(),
        GithubContinuation {
            mcp_state: query.state,
            expires_at: Instant::now() + Duration::from_secs(600),
        },
    );
    let mut response =
        Redirect::to(hosted.github.authorization_url(&github_state).as_str()).into_response();
    response.headers_mut().append(
        "set-cookie",
        secure_cookie("mcp_github_state", &github_state, 600),
    );
    response
}

async fn github_callback(
    State(s): State<AppState>,
    headers: HeaderMap,
    peer: ConnectInfo<SocketAddr>,
    Query(query): Query<GithubCallbackQuery>,
) -> Response {
    if !limited(&s, &headers, Some(peer.0), "github_callback") {
        return rate_error();
    }
    let Some(hosted) = s.config.hosted_auth.as_ref() else {
        return auth_failure();
    };
    let Some(state) = query.state.filter(|value| !value.is_empty()) else {
        return auth_failure();
    };
    let Some(code) = query.code.filter(|value| !value.is_empty()) else {
        return auth_failure();
    };
    if cookie(&headers, "mcp_github_state").as_deref() != Some(state.as_str()) {
        return auth_failure();
    }
    cleanup_browser(&s);
    let Some(continuation) = s.browser.lock().unwrap().github.remove(&state) else {
        return auth_failure();
    };
    let Some(pending) = s
        .browser
        .lock()
        .unwrap()
        .pending
        .remove(&continuation.mcp_state)
    else {
        return auth_failure();
    };
    let github_user = match hosted.github.exchange_callback(&code).await {
        Ok(user) => user,
        Err(_) => return auth_failure(),
    };
    let user = match hosted
        .tenant
        .upsert_user(&github_user.github_id, &github_user.login)
    {
        Ok(user) => user,
        Err(_) => return auth_failure(),
    };
    let client_id = pending.request.client_id.clone();
    let authorization = match s.config.oauth.authorize_for_user(pending.request, user.id) {
        Ok(authorization) => authorization,
        Err(_) => return auth_failure(),
    };
    if hosted
        .tenant
        .create_grant(user.id, &client_id, &s.config.oauth.resource())
        .is_err()
    {
        return auth_failure();
    }
    let browser_session = uuid::Uuid::new_v4().to_string();
    s.browser.lock().unwrap().sessions.insert(
        browser_session.clone(),
        (user.id, Instant::now() + s.config.session_ttl),
    );
    let mut response = authorization_redirect(&s, Ok(authorization));
    response.headers_mut().append(
        "set-cookie",
        secure_cookie(
            "mcp_session",
            &browser_session,
            s.config.session_ttl.as_secs(),
        ),
    );
    response
        .headers_mut()
        .append("set-cookie", expired_cookie("mcp_github_state"));
    response
}

fn cleanup_browser(s: &AppState) {
    let now = Instant::now();
    let mut browser = s.browser.lock().unwrap();
    browser
        .pending
        .retain(|_, pending| pending.expires_at > now);
    browser
        .github
        .retain(|_, continuation| continuation.expires_at > now);
    browser
        .sessions
        .retain(|_, (_, expires_at)| *expires_at > now);
}

fn browser_user(s: &AppState, headers: &HeaderMap) -> Option<UserId> {
    let session = cookie(headers, "mcp_session")?;
    cleanup_browser(s);
    s.browser
        .lock()
        .unwrap()
        .sessions
        .get(&session)
        .map(|(user_id, _)| *user_id)
}

fn cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get("cookie")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            value.split(';').find_map(|part| {
                let (key, value) = part.trim().split_once('=')?;
                (key == name).then(|| value.to_owned())
            })
        })
}

fn secure_cookie(name: &str, value: &str, max_age: u64) -> HeaderValue {
    HeaderValue::from_str(&format!(
        "{name}={value}; Max-Age={max_age}; Path=/; Secure; HttpOnly; SameSite=Lax"
    ))
    .expect("cookie values are generated from safe identifiers")
}

fn expired_cookie(name: &str) -> HeaderValue {
    HeaderValue::from_str(&format!(
        "{name}=; Max-Age=0; Path=/; Secure; HttpOnly; SameSite=Lax"
    ))
    .expect("cookie name is static")
}

fn auth_failure() -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({"error":"authentication_failed","error_description":"authentication failed"})),
    )
        .into_response()
}

#[derive(Deserialize)]
struct AuthorizeQuery {
    client_id: String,
    redirect_uri: String,
    response_type: String,
    resource: String,
    scope: Option<String>,
    state: String,
    code_challenge: String,
    code_challenge_method: String,
}
async fn authorize(
    State(s): State<AppState>,
    headers: HeaderMap,
    peer: ConnectInfo<SocketAddr>,
    Query(q): Query<AuthorizeQuery>,
) -> Response {
    if !limited(&s, &headers, Some(peer.0), "authorize") {
        return rate_error();
    }
    let req = AuthorizeRequest {
        client_id: q.client_id,
        redirect_uri: q.redirect_uri,
        response_type: q.response_type,
        resource: q.resource,
        scope: q.scope.unwrap_or_default(),
        state: q.state,
        code_challenge: q.code_challenge,
        code_challenge_method: q.code_challenge_method,
    };
    if s.config.hosted_auth.is_some() {
        if let Some(user_id) = browser_user(&s, &headers) {
            return authorization_redirect(&s, s.config.oauth.authorize_for_user(req, user_id));
        }

        if let Err(error) = s
            .config
            .oauth
            .create_state(&req.client_id, &req.redirect_uri)
        {
            mark_persistence_error(&s, &error);
            return oauth_error(error);
        }
        let mcp_state = req.state.clone();
        cleanup_browser(&s);
        s.browser.lock().unwrap().pending.insert(
            mcp_state.clone(),
            PendingAuthorization {
                request: req,
                expires_at: Instant::now() + s.config.session_ttl.min(Duration::from_secs(600)),
            },
        );
        let mut location = crate::http::public_base(&s.config.public_url);
        location.push_str("/auth/github/start?state=");
        location.push_str(
            &url::form_urlencoded::byte_serialize(mcp_state.as_bytes()).collect::<String>(),
        );
        return Redirect::to(&location).into_response();
    }

    authorization_redirect(&s, s.config.oauth.authorize(req))
}

fn authorization_redirect(
    s: &AppState,
    result: Result<oauth::AuthorizationResponse, OAuthError>,
) -> Response {
    match result {
        Ok(v) => {
            let mut location = v.redirect_uri;
            let separator = if location.contains('?') { '&' } else { '?' };
            location.push(separator);
            location.push_str("code=");
            location.push_str(
                &url::form_urlencoded::byte_serialize(v.code.as_bytes()).collect::<String>(),
            );
            location.push_str("&state=");
            location.push_str(
                &url::form_urlencoded::byte_serialize(v.state.as_bytes()).collect::<String>(),
            );
            Redirect::to(&location).into_response()
        }
        Err(e) => {
            mark_persistence_error(s, &e);
            oauth_error(e)
        }
    }
}
#[derive(Deserialize)]
struct TokenForm {
    grant_type: String,
    code: Option<String>,
    redirect_uri: Option<String>,
    client_id: String,
    client_secret: Option<String>,
    code_verifier: Option<String>,
    refresh_token: Option<String>,
    resource: Option<String>,
}
async fn token(
    State(s): State<AppState>,
    headers: HeaderMap,
    peer: ConnectInfo<SocketAddr>,
    axum::extract::Form(f): axum::extract::Form<TokenForm>,
) -> Response {
    if !limited(&s, &headers, Some(peer.0), "token") {
        return rate_error();
    }
    let req = TokenRequest {
        grant_type: f.grant_type,
        code: f.code.unwrap_or_default(),
        redirect_uri: f.redirect_uri,
        client_id: f.client_id,
        client_secret: f.client_secret,
        code_verifier: f.code_verifier,
        refresh_token: f.refresh_token.clone(),
        resource: f.resource,
    };
    let result = if req.grant_type == "refresh_token" {
        f.refresh_token
            .as_deref()
            .ok_or(OAuthError::InvalidGrant)
            .and_then(|t| s.config.oauth.refresh(t))
    } else {
        s.config.oauth.exchange_code(req)
    };
    match result {
        Ok(v) => Json(v).into_response(),
        Err(e) => {
            mark_persistence_error(&s, &e);
            oauth_error(e)
        }
    }
}
fn rate_error() -> Response {
    (
        StatusCode::TOO_MANY_REQUESTS,
        [("retry-after", "60")],
        Json(json!({"error":"rate_limited","error_description":"rate limit exceeded"})),
    )
        .into_response()
}
fn mark_persistence_error(s: &AppState, error: &OAuthError) {
    if matches!(error, OAuthError::Persistence(_)) {
        s.config
            .persistence_health
            .store(false, std::sync::atomic::Ordering::Release);
    }
}
fn oauth_error(e: OAuthError) -> Response {
    let status = if matches!(e, OAuthError::InvalidClient) {
        StatusCode::UNAUTHORIZED
    } else {
        StatusCode::BAD_REQUEST
    };
    (
        status,
        Json(json!({"error":"invalid_request","error_description":e.to_string()})),
    )
        .into_response()
}
fn authorized(headers: &HeaderMap, s: &AppState) -> Result<String, StatusCode> {
    let value = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(StatusCode::UNAUTHORIZED)?;
    if s.config.hosted_auth.is_some() {
        return s
            .config
            .oauth
            .verify_bearer_user(value, &crate::http::mcp_resource_url(&s.config.public_url))
            .map(|user_id| user_id.to_string())
            .map_err(|_| StatusCode::UNAUTHORIZED);
    }
    s.config
        .oauth
        .verify_bearer(value, &crate::http::mcp_resource_url(&s.config.public_url))
        .map_err(|_| StatusCode::UNAUTHORIZED)
}
fn accepts_json(headers: &HeaderMap) -> bool {
    headers
        .get("accept")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| {
            v.split(',').any(|x| {
                let x = x.trim();
                x == "*/*"
                    || x.starts_with("application/json")
                    || x.starts_with("text/event-stream")
            })
        })
}
fn content_json(headers: &HeaderMap) -> bool {
    headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| {
            v.split(';')
                .next()
                .is_some_and(|x| x.trim().eq_ignore_ascii_case("application/json"))
        })
}
fn cleanup_sessions(s: &AppState) {
    let now = Instant::now();
    s.sessions
        .lock()
        .unwrap()
        .retain(|_, seen| now.duration_since(*seen) < s.config.session_ttl);
}
fn session_valid(s: &AppState, id: &str) -> bool {
    if uuid::Uuid::parse_str(id).is_err() {
        return false;
    }
    cleanup_sessions(s);
    let mut sessions = s.sessions.lock().unwrap();
    if let Some(seen) = sessions.get_mut(id) {
        *seen = Instant::now();
        true
    } else {
        false
    }
}
async fn mcp_delete(State(s): State<AppState>, headers: HeaderMap) -> Response {
    if authorized(&headers, &s).is_err() {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Some(id) = headers.get("mcp-session-id").and_then(|v| v.to_str().ok()) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if s.sessions.lock().unwrap().remove(id).is_some() {
        StatusCode::NO_CONTENT.into_response()
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}
async fn mcp_get(State(s): State<AppState>, headers: HeaderMap) -> Response {
    if authorized(&headers, &s).is_err() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":"invalid bearer token"})),
        )
            .into_response();
    }
    if !accepts_json(&headers) {
        return (
            StatusCode::NOT_ACCEPTABLE,
            Json(json!({"error":"acceptable Accept header required"})),
        )
            .into_response();
    }
    let Some(id) = headers.get("mcp-session-id").and_then(|v| v.to_str().ok()) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":"Mcp-Session-Id required"})),
        )
            .into_response();
    };
    if !session_valid(&s, id) {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"error":"unknown MCP session"})),
        )
            .into_response();
    }
    StatusCode::NO_CONTENT.into_response()
}
async fn mcp(State(s): State<AppState>, headers: HeaderMap, body: Bytes) -> Response {
    let started = Instant::now();
    let parsed_body = serde_json::from_slice::<Value>(&body).ok();
    let audit_name = parsed_body
        .as_ref()
        .and_then(|v| v.pointer("/params/name"))
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let audit_args = parsed_body
        .as_ref()
        .and_then(|v| v.pointer("/params/arguments"));
    let client = match authorized(&headers, &s) {
        Ok(client) => Some(client),
        Err(_) => {
            audit_event(
                &s,
                None,
                audit_name,
                audit_args,
                "rejected",
                StatusCode::UNAUTHORIZED,
                started,
            );
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error":"invalid bearer token"})),
            )
                .into_response();
        }
    };
    if !content_json(&headers) || !accepts_json(&headers) {
        audit_event(
            &s,
            client.as_deref(),
            audit_name,
            audit_args,
            "rejected",
            StatusCode::NOT_ACCEPTABLE,
            started,
        );
        return (
            StatusCode::NOT_ACCEPTABLE,
            Json(json!({"error":"Content-Type application/json and acceptable Accept required"})),
        )
            .into_response();
    }
    let request: Value = match parsed_body {
        Some(v) => v,
        None => {
            audit_event(
                &s,
                client.as_deref(),
                audit_name,
                None,
                "rejected",
                StatusCode::BAD_REQUEST,
                started,
            );
            return (
                StatusCode::BAD_REQUEST,
                Json(
                    json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"Parse error"}}),
                ),
            )
                .into_response();
        }
    };
    if request.get("id").is_none() {
        return StatusCode::ACCEPTED.into_response();
    }
    let provided = headers
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    if let Some(id) = &provided
        && !session_valid(&s, id)
    {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"error":"unknown MCP session"})),
        )
            .into_response();
    }
    let id = request["id"].clone();
    let method = request.get("method").and_then(Value::as_str).unwrap_or("");
    let response = match method {
        "initialize" => {
            json!({"jsonrpc":"2.0","id":id,"result":{"protocolVersion":"2025-03-26","capabilities":{"tools":{}},"serverInfo":{"name":"coolify-mcp","version":env!("CARGO_PKG_VERSION")}}})
        }
        "tools/list" => json!({"jsonrpc":"2.0","id":id,"result":{"tools":s.app.tools()}}),
        "tools/call" => {
            let name = request
                .pointer("/params/name")
                .and_then(Value::as_str)
                .unwrap_or("");
            let args = request
                .pointer("/params/arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let r = s.app.call(name, args).await;
            audit_event(
                &s,
                client.as_deref(),
                name,
                request.pointer("/params/arguments"),
                if r.is_error { "error" } else { "ok" },
                StatusCode::OK,
                started,
            );
            json!({"jsonrpc":"2.0","id":id,"result":{"content":[{"type":"text","text":r.text}],"isError":r.is_error}})
        }
        _ => json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"Method not found"}}),
    };
    let session = if let Some(id) = provided {
        id
    } else {
        cleanup_sessions(&s);
        let mut sessions = s.sessions.lock().unwrap();
        if sessions.len() >= s.config.max_sessions {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"error":"MCP session capacity reached"})),
            )
                .into_response();
        }
        let id = uuid::Uuid::new_v4().to_string();
        sessions.insert(id.clone(), Instant::now());
        id
    };
    let mut response = Json(response).into_response();
    response
        .headers_mut()
        .insert("content-type", "application/json".parse().unwrap());
    response
        .headers_mut()
        .insert("mcp-session-id", session.parse().unwrap());
    response
}
